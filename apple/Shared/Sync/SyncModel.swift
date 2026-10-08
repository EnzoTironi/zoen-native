import SwiftUI
import Observation
import RodaCore

/// The app's side of Zoen Sync: the account on this device, the relay connection, and
/// the ephemeral signals (typing, what someone is doing, who's online) that never touch
/// the log. The core does the real work (outbox, catch-up, signatures); this only turns
/// its callbacks into observable state on the main actor.
@MainActor
@Observable
final class SyncModel {
    enum Mode { case demo, real }

    /// Demo data only behind a dev flag: `-RodaDemo YES` (screenshot stories imply it).
    /// Without it the app is real: an account on this device, chats through the relay.
    nonisolated static var mode: Mode {
        let d = UserDefaults.standard
        if d.bool(forKey: "RodaDemo") || d.bool(forKey: "RodaResetDemo") || d.bool(forKey: "RodaShowcase") || d.string(forKey: "RodaStory") != nil { return .demo }
        return .real
    }

    /// The relay this device signs up with: `-RodaRelay URL`, else the build's
    /// `ZoenRelayURL`, else a relay on this Mac (`scripts/dev-stack.sh`).
    nonisolated static var defaultRelayURL: String {
        if let s = UserDefaults.standard.string(forKey: "RodaRelay"), !s.isEmpty { return s }
        if let s = Bundle.main.object(forInfoDictionaryKey: "ZoenRelayURL") as? String, !s.isEmpty { return s }
        return "http://127.0.0.1:8787"
    }

    private(set) var account: AccountDto?
    private(set) var connection = ConnectionDto(state: "offline", synced: false, pending: 0, error: nil)
    /// Signed in on this device but the key isn't in the Keychain (restored backup, wiped keychain).
    private(set) var keyMissing = false
    /// Space → who → what they're doing, until when (typing fades on its own if the stop is lost).
    private(set) var activity: [String: [String: (kind: ChatStatus.Kind, until: Date)]] = [:]
    private(set) var online: Set<String> = []
    /// Bumped when someone's encrypted profile changes on this device (name, bio or photo
    /// arrived); a profile screen reads it to re-fetch `core.getProfile(identityId:)`.
    private(set) var profileRevisions: [String: Int] = [:]

    @ObservationIgnored let vault = KeychainVault()
    @ObservationIgnored private var bridge: SyncBridge?
    @ObservationIgnored weak var app: AppModel?
    @ObservationIgnored private var core: RodaEngine?
    @ObservationIgnored private var lastTypingSent: [String: Date] = [:]
    @ObservationIgnored private var refreshQueued = false
    @ObservationIgnored private var started = false

    var needsAccount: Bool { Self.mode == .real && account == nil }
    var isOnline: Bool { connection.state == "online" }

    /// Opens the account (if any) and starts syncing. Safe to call more than once.
    func start(core: RodaEngine) {
        self.core = core
        account = core.account()
        guard let acct = account, !started else { return }
        do {
            if !acct.unlocked {
                keyMissing = !(try core.unlock(vault: vault))
                if keyMissing { return }
            }
            let bridge = self.bridge ?? SyncBridge(self)
            self.bridge = bridge
            try core.startSync(listener: bridge)
            started = true
            connection = core.connection()
            account = core.account()
        } catch {
            app?.show(.init(kind: .error, text: (error as? CoreError)?.message ?? error.localizedDescription))
        }
    }

    func stop() {
        core?.stopSync()
        started = false
    }

    /// Signs this device out and deletes its chats and keys (the account stays on the relay).
    func eraseDevice() throws {
        guard let core else { return }
        try core.eraseDevice(vault: vault)
        started = false
        account = nil
        connection = ConnectionDto(state: "offline", synced: false, pending: 0, error: nil)
        activity = [:]
        online = []
        app?.refresh()
    }

    /// Creates this device's identity (keys go to the Keychain) and starts syncing. Works
    /// offline: the relay learns about you the first time it's reachable.
    func createAccount(core: RodaEngine, name: String, handle: String, relay: String = SyncModel.defaultRelayURL) throws {
        account = try core.createAccount(name: name, handle: handle, relayUrl: relay, vault: vault)
        start(core: core)
        app?.refresh()
    }

    func updateProfile(name: String, handle: String, bio: String = "") throws {
        guard let core else { return }
        account = try core.updateProfile(name: name, handle: handle, bio: bio)
        if !bio.isEmpty, let id = account?.identityId {
            UserDefaults.standard.set(bio, forKey: "RodaBio.\(id)")
        }
        app?.refresh()
    }

    // MARK: what the UI asks

    func isSynced(_ spaceId: String) -> Bool { core?.isSynced(spaceId: spaceId) ?? false }

    /// The live status for a chat's title from other people (typing, processing…).
    func remoteStatus(_ spaceId: String) -> (who: String, kind: ChatStatus.Kind)? {
        guard let entries = activity[spaceId] else { return nil }
        let now = Date()
        guard let (id, v) = entries.first(where: { $0.value.until > now }) else { return nil }
        let name = app?.space(spaceId)?.members.first { $0.id == id }?.name ?? String(localized: "Someone")
        return (name, v.kind)
    }

    /// Composer changed: tell the others, at most every 3 s while typing.
    func typingChanged(_ spaceId: String, text: String) {
        guard let core, isSynced(spaceId) else { return }
        let typing = !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        if typing {
            if let last = lastTypingSent[spaceId], Date().timeIntervalSince(last) < 3 { return }
            lastTypingSent[spaceId] = Date()
            core.setTyping(spaceId: spaceId, typing: true)
        } else if lastTypingSent.removeValue(forKey: spaceId) != nil {
            core.setTyping(spaceId: spaceId, typing: false)
        }
    }

    func stoppedTyping(_ spaceId: String) {
        guard let core, lastTypingSent.removeValue(forKey: spaceId) != nil else { return }
        core.setTyping(spaceId: spaceId, typing: false)
    }

    // MARK: from the core (via SyncBridge, already on the main actor)

    fileprivate func changed(_ spaceIds: [String]) {
        // A message from someone clears their typing.
        for s in spaceIds { activity[s] = activity[s]?.filter { $0.value.kind != .typing } }
        guard !refreshQueued else { return }
        refreshQueued = true
        Task { @MainActor in
            refreshQueued = false
            account = core?.account()
            app?.refresh()
        }
    }

    fileprivate func ephemeral(space: String, from: String, kind: String, detail: String) {
        var entries = activity[space] ?? [:]
        switch kind {
        case "typing": entries[from] = (.typing, Date().addingTimeInterval(6))
        case "stopped": entries[from] = nil
        case "status":
            switch detail {
            case "processing": entries[from] = (.processing, Date().addingTimeInterval(60))
            case "building": entries[from] = (.building, Date().addingTimeInterval(120))
            case "in_call", "call": entries[from] = (.call, Date().addingTimeInterval(3600))
            default: entries[from] = nil
            }
        default: return
        }
        withAnimation(.snappy) { activity[space] = entries.isEmpty ? nil : entries }
        if kind == "typing" {
            // Fade out if the "stopped" never comes (they closed the app mid-sentence).
            Task { @MainActor in
                try? await Task.sleep(for: .seconds(6.2))
                let now = Date()
                if let e = activity[space]?[from], e.until <= now {
                    withAnimation(.snappy) { activity[space]?[from] = nil }
                }
            }
        }
    }

    fileprivate func presence(_ who: String, _ isOnline: Bool) {
        if isOnline { online.insert(who) } else {
            online.remove(who)
            for s in activity.keys { activity[s]?[who] = nil }
        }
    }

    fileprivate func connectionChanged(_ c: ConnectionDto) {
        connection = c
        if c.state == "offline" { online.removeAll() }
    }

    fileprivate func profileChanged(_ identityId: String) {
        profileRevisions[identityId, default: 0] &+= 1
        changed([])
    }

    fileprivate func refused(_ message: String) {
        app?.show(.init(kind: .error, text: String(localized: "Couldn’t send. Check your connection and try again.")))
    }
}

/// Core → app. The core calls from its network thread; everything hops to the main actor.
final class SyncBridge: CoreListener, @unchecked Sendable {
    private weak var target: SyncModel?

    init(_ target: SyncModel) { self.target = target }

    func onChange(spaceIds: [String]) {
        Task { @MainActor [weak target] in target?.changed(spaceIds) }
    }

    func onEphemeral(spaceId: String, fromId: String, kind: String, detail: String) {
        Task { @MainActor [weak target] in target?.ephemeral(space: spaceId, from: fromId, kind: kind, detail: detail) }
    }

    func onPresence(identityId: String, online: Bool) {
        Task { @MainActor [weak target] in target?.presence(identityId, online) }
    }

    func onConnection(status: ConnectionDto) {
        Task { @MainActor [weak target] in target?.connectionChanged(status) }
    }

    func onError(message: String) {
        Task { @MainActor [weak target] in target?.refused(message) }
    }

    func onProfileChanged(identityId: String) {
        Task { @MainActor [weak target] in target?.profileChanged(identityId) }
    }
}
