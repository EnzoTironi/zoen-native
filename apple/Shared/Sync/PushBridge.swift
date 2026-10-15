import Foundation
import RodaCore
import UserNotifications
#if os(iOS)
import UIKit
#endif

/// Chats muted on this device. Push decisions are local: the relay never knows.
enum MutedChats {
    static let key = "zoen.mutedChats"
    static var all: Set<String> { Set(UserDefaults.standard.stringArray(forKey: key) ?? []) }
    static func isMuted(_ spaceId: String) -> Bool { all.contains(spaceId) }
    static func toggle(_ spaceId: String) -> Bool {
        var s = all
        let muted = !s.contains(spaceId)
        if muted { s.insert(spaceId) } else { s.remove(spaceId) }
        UserDefaults.standard.set(Array(s).sorted(), forKey: key)
        return muted
    }
}

/// What a wake turns into once this device has synced and decrypted: one local
/// notification per unmuted chat that gained unread messages. Pure, so it is unit-tested.
enum WakeNotifications {
    struct Note: Equatable { let spaceId: String; let title: String; let body: String }

    static func notes(before: [String: UInt32], after: [SpaceSummary], muted: Set<String>) -> [Note] {
        after.compactMap { s in
            guard s.unread > (before[s.id] ?? 0), !muted.contains(s.id) else { return nil }
            let who = s.lastAuthor?.name ?? s.title
            let title = s.kind == .direct ? who : s.title
            let body = s.kind == .direct || s.lastAuthor == nil ? s.lastPreview : "\(who): \(s.lastPreview)"
            return Note(spaceId: s.id, title: title, body: body)
        }
    }
}

/// Wires APNs to the core: register the token with the relay, and on a wake sync,
/// decrypt locally and show local notifications (respecting mute).
@MainActor
final class PushBridge {
    static let shared = PushBridge()
    weak var model: AppModel?
    private var lastToken: String?

    var environment: PushEnvironment {
        #if DEBUG
        .sandbox
        #else
        .production
        #endif
    }

    func start(_ model: AppModel) {
        self.model = model
        Task {
            let s = await UNUserNotificationCenter.current().notificationSettings()
            guard [.authorized, .provisional, .ephemeral].contains(s.authorizationStatus) else { return }
            #if os(iOS)
            UIApplication.shared.registerForRemoteNotifications()
            #endif
        }
    }

    func received(token: Data) {
        guard let hex = PushConfiguration.tokenHex(token), hex != lastToken, let core = model?.core else { return }
        lastToken = hex
        let platform = environment.platform
        Task { try? await core.registerPush(platform: platform, token: hex) }
    }

    func signedOut() {
        guard let core = model?.core else { return }
        lastToken = nil
        Task { try? await core.registerPush(platform: environment.platform, token: "") }
    }

    /// Up to ~25 s of background time: wait for the core to catch up, then notify.
    func wake(_ payload: [AnyHashable: Any]) async -> Bool {
        guard PushWake.decode(payload) != nil, let model else { return false }
        let before = Dictionary(model.spaces.map { ($0.id, $0.unread) }, uniquingKeysWith: { a, _ in a })
        for _ in 0..<40 {
            if model.sync.connection.synced { break }
            try? await Task.sleep(for: .milliseconds(500))
        }
        model.refresh()
        let notes = WakeNotifications.notes(before: before, after: model.spaces, muted: MutedChats.all)
        for n in notes {
            let c = UNMutableNotificationContent()
            c.title = n.title
            c.body = n.body
            c.sound = .default
            c.threadIdentifier = n.spaceId
            c.userInfo = ["space": n.spaceId]
            try? await UNUserNotificationCenter.current().add(
                UNNotificationRequest(identifier: "\(n.spaceId)-\(UUID().uuidString)", content: c, trigger: nil))
        }
        return !notes.isEmpty
    }
}

#if os(iOS)
final class ZoenAppDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        Task { @MainActor in PushBridge.shared.received(token: deviceToken) }
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {}

    func application(_ application: UIApplication, didReceiveRemoteNotification userInfo: [AnyHashable: Any]) async -> UIBackgroundFetchResult {
        let payload = userInfo
        let showed = await PushBridge.shared.wake(payload)
        return showed ? .newData : .noData
    }
}
#endif
