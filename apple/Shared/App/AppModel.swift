import SwiftUI
import Observation
import RodaCore

/// Top-level destinations. On iPhone, three live in the bottom bar's pill (`barTabs`),
/// Search has its own button, and the rest open from the + fan menu.
enum AppTab: String, Hashable, CaseIterable {
    case conversations, communities, files, activity
    case agents, you, search, store

    var title: String {
        switch self {
        case .conversations: String(localized: "Chats")
        case .communities: String(localized: "Spaces")
        case .files: String(localized: "Files")
        case .activity: String(localized: "Activity")
        case .agents: String(localized: "Your agents")
        case .you: String(localized: "Your context")
        case .search: String(localized: "Search")
        case .store: String(localized: "Store")
        }
    }

    var symbol: String {
        switch self {
        case .conversations: "bubble.left.and.bubble.right.fill"
        case .communities: "person.3.fill"
        case .files: "folder.fill"
        case .activity: "bell.fill"
        case .agents: "sparkles.rectangle.stack.fill"
        case .you: "person.crop.circle.fill"
        case .search: "magnifyingglass"
        case .store: "storefront"
        }
    }

    /// Zoen's own pen-and-paper icon for the destination.
    var glyph: ZoenGlyph {
        switch self {
        case .conversations: .chats
        case .communities: .spaces
        case .files: .folder
        case .activity: .bell
        case .agents: .agents
        case .you: .you
        case .search: .search
        case .store: .store
        }
    }

    /// Selected state in the tab pill (the outline symbols fill in).
    var selectedSymbol: String { self == .store ? "storefront.fill" : symbol }

    /// Chats, Spaces and Store. Notifications (and approvals) moved to the bell in the headers.
    static let barTabs: [AppTab] = [.conversations, .communities, .store]
}

enum Route: Hashable {
    case store(String)
    case space(String)
    case item(String)
    case agent(String)
    case integrity
    case log(String)
    case items
    case participants(String)
    case permissions(agent: String, space: String)
    case request(String)
    case folder(String)
    /// Mini-app (MCP App) em tela cheia.
    case app(String)
}

/// Store @Observable fino sobre o núcleo Rust. Não guarda verdade própria: lê do
/// núcleo depois de cada escrita (`refresh`) e incrementa `revision` para as telas
/// recarregarem o que precisam.
@MainActor
@Observable
final class AppModel {
    let core: RodaEngine
    let planner = AgentPlanner()
    /// Account, relay connection and the live signals (typing, presence).
    let sync = SyncModel()

    private(set) var me: Persona?
    private(set) var spaces: [SpaceSummary] = []
    private(set) var requests: [AgentRequestDto] = []
    /// Standing "always approve / always deny" decisions you gave your agents.
    private(set) var standing: [StandingDecisionDto] = []
    /// Revoked on screen, still undoable (see `revokeStanding`).
    private(set) var pendingRevokes: Set<String> = []
    @ObservationIgnored private var revokeTasks: [String: Task<Void, Never>] = [:]
    private(set) var agents: [AgentProfile] = []
    private(set) var mentions: [Mention] = []
    private(set) var stats: CoreStats?
    private(set) var revision = 0
    /// Mini-apps vivos (um por Espaço, o mais recente) para os blocos da tela Conversas.
    private(set) var liveApps: [ItemDetail] = []
    /// Confirmação nativa pedida por um mini-app (irreversível / sai do Espaço).
    var appConfirm: AppConfirmRequest?
    /// Zoen's consent sheet for a mini-app capability (layer 1; iOS asks after, once).
    var consent: ConsentRequest?
    /// Mini-app aberto em folha (mola a partir do cartão).
    var appSheet: AppSheetRef?
    var petTab: PetTab = .care
    /// Item → última versão vista por mim (ponto vermelho quando outro membro muda).
    private var seenAppVersion: [String: UInt32] = [:]
    /// `ui/update-model-context` de cada mini-app: o que o View quer que o agente saiba.
    @ObservationIgnored var appModelContext: [String: [String: Any]] = [:]

    var tab: AppTab = .conversations
    /// Uma pilha de navegação por destino (preserva onde você estava em cada um).
    var paths: [AppTab: [Route]] = [:]
    /// Menu radial do botão central (aberto por toque ou por pressionar e arrastar).
    var radialOpen = false
    /// iPhone: the bar's search field is open (it filters Chats live).
    var chatSearch = false
    var chatQuery = ""
    /// Pre-filled query for the search screen (`-RodaSearch <query>`).
    var searchSeed: String?
    /// iPhone: the universal search screen is open (it grows out of the bar's circle).
    var searchOpen = false
    /// Centre of the bar's search circle (global coordinates), where search grows from.
    var searchOrigin: CGPoint = CGPoint(x: 42, y: 820)
    /// A search result asked to open a chat at this message (scroll + brief highlight).
    var jumpTarget: JumpTarget?
    /// Home strip: pinned mini-app order and the ones you unpinned (UserDefaults).
    private(set) var homePins: [String] = UserDefaults.standard.stringArray(forKey: "RodaHomePins") ?? []
    private(set) var homeUnpinned: Set<String> = Set(UserDefaults.standard.stringArray(forKey: "RodaHomeUnpinned") ?? [])
    /// Mini-apps you unpinned from a chat's tile row (they stay in the chat as cards).
    private(set) var chatAppsUnpinned: Set<String> = Set(UserDefaults.standard.stringArray(forKey: "RodaChatAppsUnpinned") ?? [])
    func unpinChatApp(_ itemId: String) {
        chatAppsUnpinned.insert(itemId)
        UserDefaults.standard.set(Array(chatAppsUnpinned), forKey: "RodaChatAppsUnpinned")
        revision &+= 1
    }
    /// Pinned chats (Zoen is pinned until you unpin it).
    private(set) var chatPins: [String] = UserDefaults.standard.stringArray(forKey: "RodaChatPins") ?? ["zoen"]
    /// Mac: Espaço selecionado na barra lateral e Item no inspetor.
    var macSelection: MacSidebarItem? = nil
    var inspectorItem: String? = nil

    /// Espaço → agente trabalhando agora (estado efêmero, não vai para o log).
    var working: [String: Persona] = [:]
    /// The notifications sheet (bell in the headers): mentions, tasks and approvals.
    var notificationsOpen = false
    /// The approvals catch-up stack (bell with pending approvals).
    var approvalsOpen = false
    /// A mini-app flipping open from its tile (see MiniAppFlipHost).
    var appFlip: AppFlip?
    /// New chat / group / join sheet (real accounts only).
    var newChatOpen = false
    /// Profile sheet (medium → large) opened from any avatar/name tap.
    var profileSheet: ProfileSheetRef?
    /// Clip id of a just-sent +→Zoen voice note; SpaceView plays the land animation.
    var pendingVoiceFlight: String?
    /// Store installs (seed data stub: remembered locally, nothing downloaded).
    var storeInstalled: Set<String> = []
    /// What the working agent is doing (title status): processing, building an app, typing.
    var activity: [String: ChatStatus.Kind] = [:]
    var toast: ToastModel?
    var bootError: String?

    init() {
        let (engine, failure) = Self.boot()
        core = engine
        bootError = failure
        if failure != nil {
            toast = ToastModel(kind: .error, text: String(localized: "Another Zoen window is already using the database. This one opened a temporary copy that won’t be saved."))
        }
        sync.app = self
        #if DEBUG
        // Tests and demos of the real thing: `-RodaFreshStart YES` forgets this device's
        // account and chats; `-RodaAccount "Ana:ana"` signs up without onboarding.
        let d = UserDefaults.standard
        if d.bool(forKey: "RodaWowReset") {
            // Proof videos: replay first-time WOW moments (stamp, flourish).
            for k in d.dictionaryRepresentation().keys where k.hasPrefix("RodaWow.") { d.removeObject(forKey: k) }
        }
        if d.bool(forKey: "RodaFreshStart") {
            try? core.eraseDevice(vault: sync.vault)
            // Pins and their order are per device too: start from the default strip.
            for k in ["RodaHomePins", "RodaHomeUnpinned", "RodaChatAppsUnpinned", "RodaChatTileOrder", "RodaPendingRevokes"] { d.removeObject(forKey: k) }
            homePins = []; homeUnpinned = []; chatAppsUnpinned = []; chatTileOrder = []
            if SyncModel.mode == .demo { _ = try? core.seedDemoIfEmpty() }
        }
        if let spec = d.string(forKey: "RodaAccount"), core.account() == nil {
            let parts = spec.split(separator: ":", maxSplits: 1).map(String.init)
            try? sync.createAccount(core: core, name: parts[0], handle: parts.count > 1 ? parts[1] : parts[0])
        }
        #endif
        sync.start(core: core)
        refresh()
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(1.5))
            MiniAppWebPool.shared.prewarm()
        }
    }

    private static func boot() -> (RodaEngine, String?) {
        do {
            Money.locale = AppLocale.tag
            let engine = try RodaEngine.openDefault(locale: AppLocale.tag)
            // The demo story only behind the dev flag (`-RodaDemo YES`); a real install
            // starts empty and creates its account in onboarding.
            if SyncModel.mode == .demo && engine.account() == nil {
                // Showcase always reseeds so investor shots get the rich pinned apps.
                let reset = UserDefaults.standard.bool(forKey: "RodaResetDemo")
                    || UserDefaults.standard.bool(forKey: "RodaShowcase")
                if reset {
                    try engine.resetDemo()
                } else {
                    _ = try engine.seedDemoIfEmpty()
                }
            }
            return (engine, nil)
        } catch {
            // Último recurso: banco em memória, para o app nunca abrir vazio.
            let message = (error as? CoreError)?.message ?? error.localizedDescription
            let engine = try! RodaEngine.open(path: ":memory:", locale: AppLocale.tag)
            _ = try? engine.seedDemoIfEmpty()
            return (engine, message)
        }
    }

    // MARK: leitura

    func refresh() {
        me = try? core.me()
        spaces = core.spaces()
        requests = core.requests()
        standing = core.standingDecisions()
        agents = core.agents()
        mentions = core.mentions()
        stats = core.stats()
        liveApps = core.items().filter { item in
            guard let app = item.app else { return false }
            return !AppView(app).bool("released")
        }
        revision &+= 1
    }

    var pendingRequests: [AgentRequestDto] { requests.filter { $0.status == .pending || $0.status == .stale } }
    var pendingCount: Int { requests.filter { $0.status == .pending }.count }
    /// What the approvals stack deals: pending requests from your own agents (only an
    /// agent's owner decides), oldest first.
    var approvalQueue: [AgentRequestDto] {
        requests.filter { $0.status == .pending && $0.agent.isMine }.sorted { $0.openedMs < $1.openedMs }
    }
    func standing(for agentId: String, space: String? = nil) -> [StandingDecisionDto] {
        standing.filter { $0.agent.id == agentId && (space == nil || $0.spaceId == space) && !pendingRevokes.contains($0.grantId) }
    }

    func space(_ id: String) -> SpaceSummary? { spaces.first { $0.id == id } }
    func agentProfile(_ id: String) -> AgentProfile? { agents.first { $0.id == id } }
    var zoen: Persona? { agents.first { $0.persona.handle == "zoen" }?.persona }
    func zoenSpaceId() -> String? { spaces.first { $0.counterpart?.handle == "zoen" }?.id }

    func openProfile(_ personaId: String) {
        profileSheet = ProfileSheetRef(id: personaId)
    }

    /// Open (or create) the 1:1 chat with this person/agent, then dismiss the profile sheet.
    func openChat(with persona: Persona) {
        profileSheet = nil
        if let existing = spaces.first(where: { $0.counterpart?.id == persona.id }) {
            go(.space(existing.id))
            return
        }
        // Demo/seed: prefer a space that already includes them as the only other person.
        if let group = spaces.first(where: { $0.members.contains { $0.id == persona.id } && $0.counterpart == nil }) {
            go(.space(group.id))
        }
    }

    /// After the + long-hold voice note: open Zoen's 1:1 so the bubble is visible.
    func openZoenChat() {
        guard let id = zoenSpaceId() else { return }
        withAnimation(.spring(duration: 0.45, bounce: 0.22)) {
            go(.space(id))
        }
        revision &+= 1
    }

    func spaceId(titled title: String) -> String? { spaces.first { $0.title == title }?.id }

    // MARK: escrita (sempre via núcleo)

    @discardableResult
    func perform<T>(_ work: () throws -> T) -> T? {
        do {
            let value = try work()
            refresh()
            return value
        } catch let e as CoreError {
            show(.init(kind: .error, text: e.message))
        } catch {
            show(.init(kind: .error, text: error.localizedDescription))
        }
        refresh()
        return nil
    }

    func show(_ t: ToastModel, seconds: Double = 5) {
        withAnimation(.spring(duration: 0.45, bounce: 0.25)) { toast = t }
        let id = t.id
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(seconds))
            if toast?.id == id { withAnimation(.easeOut(duration: 0.3)) { toast = nil } }
        }
    }

    func undo(_ token: UndoToken) {
        if perform({ try core.undo(token: token) }) != nil {
            show(.init(kind: .info, text: String(localized: "Undone. The previous version is back (and the history is still there).")), seconds: 3)
        }
    }

    /// Toda edição é uma versão nova; mostramos "Desfazer". A reação do agente (se houver)
    /// já entrou na conversa como evento assinado e é devolvida para a tela mostrar.
    @discardableResult
    func handleEdit(_ outcome: EditOutcome?) -> String? {
        guard let outcome else { return nil }
        show(.init(kind: .undo(outcome.undo), text: outcome.undo.label), seconds: 6)
        return outcome.reaction
    }

    func approve(_ r: AgentRequestDto) {
        if let out = perform({ try core.approveRequest(requestId: r.id) }) {
            show(.init(kind: .agent(out.request.agent), text: out.message))
        }
    }

    func deny(_ r: AgentRequestDto) {
        if let out = perform({ try core.denyRequest(requestId: r.id) }) {
            show(.init(kind: .agent(out.request.agent), text: out.message), seconds: 3)
        }
    }

    /// One swipe (or button) on the approvals stack. Messages go into the chats as signed
    /// events; the stack shows its own undo toast, so no app toast here.
    @discardableResult
    func decide(_ requestId: String, _ decision: RequestDecision) -> DecideOutcome? {
        perform { try core.decideRequest(requestId: requestId, decision: decision) }
    }

    /// One tap revokes, with "Desfazer" for a few seconds (like the rest of the app). The row
    /// goes at once; the core only forgets the grant when the undo window closes.
    func revokeStanding(_ s: StandingDecisionDto) {
        let id = s.grantId
        withAnimation(.snappy) { _ = pendingRevokes.insert(id) }
        savePendingRevokes()
        show(.init(kind: .revoked(grantId: id, agent: s.agent), text: String(localized: "\(s.agent.name) will ask again.")),
             seconds: Self.revokeUndoSeconds)
        revokeTasks[id]?.cancel()
        revokeTasks[id] = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(Self.revokeUndoSeconds + 0.3))
            guard !Task.isCancelled else { return }
            self?.commitRevoke(id)
        }
    }

    /// "Desfazer" on the revoke toast: the decision comes back as it was.
    func restoreStanding(_ grantId: String) {
        guard pendingRevokes.contains(grantId) else { return }
        revokeTasks.removeValue(forKey: grantId)?.cancel()
        Haptics.selectionTick()
        withAnimation(.snappy) {
            _ = pendingRevokes.remove(grantId)
            toast = nil
        }
        savePendingRevokes()
    }

    /// Leaving the app (or the window closing) settles any revoke still waiting on its undo.
    func commitPendingRevokes() {
        for id in pendingRevokes { commitRevoke(id) }
    }

    private func commitRevoke(_ id: String) {
        revokeTasks.removeValue(forKey: id)?.cancel()
        guard pendingRevokes.contains(id) else { return }
        _ = perform { try core.revokeStanding(grantId: id) }
        pendingRevokes.remove(id)
        savePendingRevokes()
        standing = core.standingDecisions()
    }

    /// The pending revokes are written down as they happen, so a kill inside the undo window
    /// still revokes: the next launch applies whatever is left (`applyPendingRevokesFromLastRun`).
    private static let pendingRevokesKey = "RodaPendingRevokes"
    private func savePendingRevokes() {
        UserDefaults.standard.set(Array(pendingRevokes), forKey: Self.pendingRevokesKey)
    }

    func applyPendingRevokesFromLastRun() {
        let d = UserDefaults.standard
        let left = d.stringArray(forKey: Self.pendingRevokesKey) ?? []
        guard !left.isEmpty else { return }
        // A grant that is already gone (or a device that was erased) is fine: nothing to do.
        for id in left where !pendingRevokes.contains(id) { try? core.revokeStanding(grantId: id) }
        d.set(Array(pendingRevokes), forKey: Self.pendingRevokesKey)
        standing = core.standingDecisions()
    }

    static let revokeUndoSeconds: Double = 5

    func approveAll(agent: Persona) {
        if let n = perform({ try core.approveAll(agentId: agent.id) }) {
            show(.init(kind: .agent(agent), text: n == 1 ? String(localized: "1 request approved.") : String(localized: "\(n) requests approved. (Simulated: nothing was really paid.)")))
        }
    }

    // MARK: o agente na conversa

    /// The chat's shared background: the latest typed `BackgroundSet` event in its log
    /// (falls back to a legacy `⟦bg:⟧` marker from older builds).
    func sharedBackground(_ spaceId: String) -> ChatBackground { sharedBackgroundState(spaceId).0 }

    func sharedBackgroundState(_ spaceId: String) -> (ChatBackground, PhotoBackgroundLayout) {
        if let dto = try? core.background(spaceId: spaceId) {
            return (ChatBackground(dto: dto), PhotoBackgroundLayout(dto: dto))
        }
        let entries = (try? core.timeline(spaceId: spaceId)) ?? []
        for e in entries.reversed() {
            if case .message(let t, _) = e.kind, let bg = ChatBackgroundMarker.parse(t) { return (bg, .default) }
        }
        return (.none, .default)
    }

    /// What the chat shows: "Just for me" wins over the shared one.
    func backgroundState(_ spaceId: String) -> (background: ChatBackground, layout: PhotoBackgroundLayout, isLocal: Bool) {
        let store = ChatBackgroundStore.shared
        if let local = store.local(spaceId) { return (local, store.layout(for: spaceId), true) }
        let shared = sharedBackgroundState(spaceId)
        return (shared.0, shared.1, false)
    }

    /// Caches a shared photo for drawing once the core has it. In shared chats the core
    /// fetches the encrypted copy from the relay and reports a change, which re-runs this.
    func ensureBackgroundMedia(_ background: ChatBackground) {
        guard case .photo(let sha?) = background, !ChatBackgroundStore.shared.hasMedia(sha),
              let bytes = try? core.media(sha256: sha) else { return }
        ChatBackgroundStore.shared.cacheMedia(sha, bytes: bytes)
    }

    /// A voice note: transcribed on this device first (never in the cloud), then sent as a
    /// marker plus the transcript. Agents don't answer voice notes yet.
    func sendVoice(_ clip: VoiceClip, in spaceId: String, transcript known: String? = nil) async {
        var text = known ?? ""
        if known == nil { text = await VoiceTranscriber.transcribe(clip.url)?.text ?? "" }
        let ref = VoiceNoteRef(id: clip.id, ms: clip.ms, levels: clip.levels, transcript: text)
        if perform({ try core.sendMessage(spaceId: spaceId, text: ref.marker) }) == nil {
            VoiceStore.delete(clip.url)
        }
    }

    /// Mensagem do usuário → (talvez) o agente age. O plano vem do Foundation Models
    /// no aparelho ou do planejador local; o núcleo decide se o agente pode e assina.
    /// Inline reply (quoted in the chat) or a reply in the message's thread.
    @discardableResult
    func sendReply(_ text: String, to entryId: String, thread: Bool, in spaceId: String) -> Bool {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return false }
        guard perform({ try core.sendReply(spaceId: spaceId, text: trimmed, to: entryId, thread: thread) }) != nil else { return false }
        Haptics.send()
        return true
    }

    func send(_ text: String, in spaceId: String) async {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        guard perform({ try core.sendMessage(spaceId: spaceId, text: trimmed) }) != nil else { return }
        Haptics.send()
        guard let space = space(spaceId), let agent = respondingAgent(for: trimmed, in: space) else { return }

        withAnimation(.snappy) { working[spaceId] = agent; activity[spaceId] = .processing }
        defer { stopWorking(spaceId) }

        if let pet = livePet(in: spaceId), let name = AppChooser.renameTarget(trimmed) {
            // "vamos chamar ele de Paçoca": o grupo renomeia; o widget muda para todos e o
            // agente comenta (o núcleo assina a versão e a fala).
            stopWorking(spaceId)
            callAppTool(pet.id, "pet_rename", args: "{\"name\":\"\(name.replacingOccurrences(of: "\"", with: ""))\"}")
        } else if AppChooser.looksLikeAppRequest(trimmed) {
            withAnimation(.snappy) { activity[spaceId] = .building }
            // MCP App local: o modelo (ou a regra) escolhe o mini-app; o núcleo decide se o
            // agente pode criar, cria o Item compartilhado e concede ao mini-app "Agir" nele.
            let choice = await AppChooser.choose(prompt: trimmed, planner: planner)
            stopWorking(spaceId)
            if let choice {
                let outcome = perform {
                    try core.agentCreateApp(spaceId: spaceId, agentId: agent.id, startTool: choice.startTool, argsJson: choice.argsJSON, engineLabel: choice.engineLabel, prompt: trimmed)
                }
                if let outcome {
                    if outcome.kind == .actWithUndo { Haptics.commit() } else { show(.init(kind: .agent(agent), text: outcome.message)) }
                }
            }
        } else if AgentPlanner.looksLikePlanRequest(trimmed) {
            let people = space.members.filter { $0.kind == .person }.map(\.name)
            let draft = await planner.makePlan(prompt: trimmed, people: people, context: plannerContext(spaceId))
            stopWorking(spaceId)
            let outcome = perform {
                try core.agentCreatePlan(spaceId: spaceId, agentId: agent.id, prompt: trimmed, plan: draft.plan, engineLabel: draft.engineLabel, aiCostCents: 0)
            }
            if let outcome, outcome.kind != .actWithUndo {
                show(.init(kind: .agent(agent), text: outcome.message))
            }
        } else {
            withAnimation(.snappy) { activity[spaceId] = .typing }
            let reply = await planner.reply(to: trimmed, agentName: agent.name, context: plannerContext(spaceId))
            stopWorking(spaceId)
            perform { try core.agentSay(spaceId: spaceId, agentId: agent.id, text: reply, aiCostCents: 0) }
        }
    }

    /// The thinking capsule shrinks away with a spring while the answer grows in.
    private func stopWorking(_ spaceId: String) {
        guard working[spaceId] != nil else { return }
        withAnimation(.spring(duration: 0.45, bounce: 0.2)) { working[spaceId] = nil; activity[spaceId] = nil }
    }

    /// What the agent can see without being told: the chat, who's in it, the card that's
    /// open and the last few messages. Passed to the on-device model only.
    func plannerContext(_ spaceId: String) -> PlannerContext {
        let s = space(spaceId)
        var open: String?
        if let a = appSheet, let it = try? core.item(itemId: a.id) { open = it.title }
        #if os(macOS)
        if open == nil, let id = inspectorItem, let it = try? core.item(itemId: id) { open = it.title }
        #else
        if open == nil, case .item(let id)? = paths[tab]?.last, let it = try? core.item(itemId: id) { open = it.title }
        #endif
        if open == nil { open = core.items().first { $0.spaceId == spaceId && $0.plan != nil }?.title }
        let names = (s?.members ?? []).filter { !$0.isMe }.map(\.name)
        let recent = ((try? core.timeline(spaceId: spaceId)) ?? []).suffix(6).compactMap { e -> String? in
            if case .message(let text, _) = e.kind { return "\(e.author.name): \(text)" }
            return nil
        }
        return PlannerContext(spaceTitle: s?.title ?? "", names: names, openCard: open, recent: Array(recent))
    }

    func livePet(in spaceId: String) -> ItemDetail? {
        core.items().first { $0.spaceId == spaceId && $0.app?.appId == "pet" && !($0.app.map { AppView($0).bool("released") } ?? true) }
    }

    /// Quem responde: na DM com um agente, ele; em grupo, o agente mencionado (@Zoen)
    /// ou o Zoen quando é claramente um pedido de plano.
    private func respondingAgent(for text: String, in space: SpaceSummary) -> Persona? {
        if let c = space.counterpart { return c.kind == .agent && c.isMine ? c : nil }
        let lower = text.lowercased()
        let mine = space.members.filter { $0.kind == .agent && $0.isMine }
        if let mentioned = mine.first(where: { lower.contains("@\($0.name.lowercased())") || lower.contains($0.name.lowercased()) }) {
            return mentioned
        }
        // Em grupo, o Zoen participa no ambiente: uma frase que pede um mini-app basta.
        if AppChooser.looksLikeAppRequest(text) || (AppChooser.renameTarget(text) != nil && livePet(in: space.id) != nil), let zoen = mine.first(where: { $0.handle == "zoen" }) {
            return zoen
        }
        return nil
    }

    // MARK: atalhos de demonstração (launch arguments, para screenshots e demos)

    /// `-RodaAppearance dark|light` força o tema só neste app (demos e screenshots);
    /// sem o argumento, segue o sistema.
    nonisolated static var launchColorScheme: ColorScheme? {
        switch UserDefaults.standard.string(forKey: "RodaAppearance") {
        case "dark": .dark
        case "light": .light
        default: nil
        }
    }


    /// Deep-link into a seeded space / screen (`-RodaOpen`). Called after showcase polish.
    static let samplePage = """
    # Roteiro: Paraty

    Três dias de **barco**, trilha e *centro histórico*.

    ## Antes de ir

    - [x] Reservar a pousada
    - [ ] Alugar o barco
    - [ ] Comprar protetor

    > Muitos lugares não aceitam cartão: leve dinheiro.

    | Dia | Plano |
    |-----|-------|
    | 1   | Centro histórico |
    | 2   | Barco pelas ilhas |

    ```
    saída: sexta 7h
    ```
    """

    /// A small drawn PNG (a map-like doodle) to show Quick Look and Markup.
    static func sampleImage(side: CGFloat = 900) -> Data {
        let size = CGSize(width: side, height: side * 0.75)
        #if canImport(UIKit)
        let img = UIGraphicsImageRenderer(size: size).image { ctx in
            UIColor(red: 0.85, green: 0.93, blue: 0.97, alpha: 1).setFill()
            ctx.fill(CGRect(origin: .zero, size: size))
            UIColor(red: 0.55, green: 0.78, blue: 0.45, alpha: 1).setFill()
            UIBezierPath(ovalIn: CGRect(x: side * 0.1, y: side * 0.15, width: side * 0.45, height: side * 0.35)).fill()
            UIBezierPath(ovalIn: CGRect(x: side * 0.6, y: side * 0.4, width: side * 0.25, height: side * 0.2)).fill()
        }
        return img.pngData() ?? Data()
        #else
        let img = NSImage(size: size)
        img.lockFocus()
        NSColor(red: 0.85, green: 0.93, blue: 0.97, alpha: 1).setFill()
        NSRect(origin: .zero, size: size).fill()
        NSColor(red: 0.55, green: 0.78, blue: 0.45, alpha: 1).setFill()
        NSBezierPath(ovalIn: NSRect(x: side * 0.1, y: side * 0.15, width: side * 0.45, height: side * 0.35)).fill()
        img.unlockFocus()
        guard let tiff = img.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff) else { return Data() }
        return rep.representation(using: .png, properties: [:]) ?? Data()
        #endif
    }

    private func applyRodaOpen(_ d: UserDefaults) {
        guard let open = d.string(forKey: "RodaOpen") else { return }
        switch open {
        case "paraty":
            if let id = spaceId(titled: DemoSpace.paraty) { go(.space(id)) }
        case "paraty-plan":
            if let id = spaceId(titled: DemoSpace.paraty) {
                go(.space(id))
                if let plan = core.items().first(where: { $0.spaceId == id && $0.kindId == "plan" }) { go(.item(plan.id)) }
            }
        case "zoen":
            if let id = zoenSpaceId() { go(.space(id)) }
        case "new-page", "page-sample", "file-sample":
            // Files and pages (ADR 0040) for UI journeys and proof videos.
            guard let id = spaceId(titled: DemoSpace.paraty) else { break }
            select(.files)
            let made: ItemDetail? = switch open {
            case "new-page": try? core.pageCreate(spaceId: id, title: "")
            case "page-sample": try? core.pageImportMarkdown(spaceId: id, path: "roteiro.md", markdown: Self.samplePage)
            default: try? core.fileAdd(spaceId: id, path: "mapa.png", name: "mapa.png", mime: "image/png",
                                       bytes: Self.sampleImage(), thumbnail: Self.sampleImage(side: 120))
            }
            refresh()
            if let made { push(.item(made.id)) }
        case "turma", "saturday", "crew":
            if let id = spaceId(titled: DemoSpace.saturdayCrew) { go(.space(id)) }
        case "coastal", "litoral", "viajantes":
            if let id = spaceId(titled: DemoSpace.coastalTravelers) { go(.space(id)) }
        case "financeiro":
            if let a = agents.first(where: { $0.persona.handle == "financeiro" }) {
                select(.agents); push(.agent(a.id))
            }
        case "guia":
            if let a = agents.first(where: { $0.persona.handle == "guia" }) {
                select(.agents); push(.agent(a.id))
            }
        case "aprovacoes", "approvals":
            #if os(iOS)
            approvalsOpen = true
            #endif
        case "integridade":
            select(.you); push(.integrity)
        case "participantes":
            if let id = spaceId(titled: DemoSpace.paraty) { go(.space(id)); go(.participants(id)) }
        case "permissoes":
            if let id = spaceId(titled: DemoSpace.paraty), let a = agents.first(where: { $0.persona.handle == "financeiro" }) {
                go(.space(id)); go(.participants(id)); go(.permissions(agent: a.id, space: id))
            }
        case "pedido":
            if let r = requests.first(where: { $0.status == .pending }) {
                // The review screen itself, so the plain list (not the swipe stack).
                notificationsOpen = true; paths[.activity] = [.request(r.id)]
            }
        default: break
        }
    }

    func applyLaunchOptions() async {
        let d = UserDefaults.standard
        applyPendingRevokesFromLastRun()
        if let tab = d.string(forKey: "RodaTab") {
            self.tab = switch tab {
            case "atividade", "activity": .activity
            case "loja", "store", "explore": .store
            case "comunidades", "communities", "espacos", "spaces": .communities
            case "arquivos", "files": .files
            case "agentes", "agents": .agents
            case "voce", "you", "contexto": .you
            case "busca", "search": .search
            default: .conversations
            }
            macSelection = MacSidebarItem(tab: self.tab) ?? macSelection
        }
        // With a story, search opens once the story has played (see below).
        if let q = d.string(forKey: "RodaSearch"), d.string(forKey: "RodaStory") == nil {
            let extra = d.double(forKey: "RodaSearchDelay")
            try? await Task.sleep(for: .seconds(0.8 + extra))
            #if os(iOS)
            searchSeed = q == "YES" ? nil : q
            searchOpen = true
            #else
            searchSeed = q == "YES" ? nil : q
            select(.search)
            #endif
        }
        if d.bool(forKey: "RodaRadial") {
            try? await Task.sleep(for: .seconds(0.8))
            withAnimation(.spring(duration: 0.5, bounce: 0.3)) { radialOpen = true }
        }
        #if DEBUG
        if d.bool(forKey: "RodaShowcase") {
            await prepareShowcase()
        }
        #endif
        // After showcase seed polish so `-RodaOpen` is not wiped by `paths = []`.
        applyRodaOpen(d)
        if let story = d.string(forKey: "RodaStory") {
            await runStory(story)
            if let q = d.string(forKey: "RodaSearch") {
                try? await Task.sleep(for: .seconds(0.6))
                searchSeed = q == "YES" ? nil : q
                #if os(iOS)
                searchOpen = true
                #else
                select(.search)
                #endif
            }
            if d.string(forKey: "RodaTab") == "conversas" {
                try? await Task.sleep(for: .seconds(0.5))
                appSheet = nil
                paths[.conversations] = []
            }
        }
        // `-RodaGroupPrompt "e se a gente adotasse um burro?||…"`: manda no grupo de Paraty.
        // `-RodaAppActions "pet_feed,pet_play"` toca no mini-app criado; `-RodaOpenApp YES`
        // abre em tela cheia; `-RodaAppConfirm pet_release` mostra a folha nativa.
        if let prompts = d.string(forKey: "RodaGroupPrompt"), let id = spaceId(titled: DemoSpace.paraty) {
            go(.space(id))
            for prompt in prompts.components(separatedBy: "||") {
                try? await Task.sleep(for: .seconds(1.0))
                await send(prompt, in: id)
            }
            let app = core.items().first { $0.app != nil && $0.spaceId == id }
            if let app, let actions = d.string(forKey: "RodaAppActions") {
                for a in actions.split(separator: ",") {
                    try? await Task.sleep(for: .seconds(0.4))
                    let parts = a.split(separator: ":", maxSplits: 1)
                    callAppTool(app.id, String(parts[0]), args: parts.count > 1 ? String(parts[1]) : "{}")
                }
            }
            if let app, d.bool(forKey: "RodaOpenApp") {
                try? await Task.sleep(for: .seconds(0.8))
                openApp(app.id)
            }
            if let app, let tool = d.string(forKey: "RodaAppConfirm") {
                try? await Task.sleep(for: .seconds(1.2))
                callAppTool(app.id, tool)
            }
            if d.string(forKey: "RodaTab") == "conversas" {
                try? await Task.sleep(for: .seconds(0.6))
                paths[.conversations] = []
            }
        }
        if let prompt = d.string(forKey: "RodaPrompt"), let id = zoenSpaceId() {
            go(.space(id))
            try? await Task.sleep(for: .seconds(1.2))
            await send(prompt, in: id)
            if d.bool(forKey: "RodaOpenResult"), let item = core.items().first {
                try? await Task.sleep(for: .seconds(0.6))
                go(.item(item.id))
            }
        }
    }

    // MARK: mini-apps

    /// Abre o mini-app em tela cheia. Da tela Conversas, entra no Espaço antes (voltar
    /// leva à conversa, onde o cartão mora).
    /// Opens a mini-app from its tile with the flip: the tile turns over into the app, and
    /// closing turns it back. Stays where you are (Home or the chat) underneath.
    func flipOpenApp(_ itemId: String, from: CGRect, sourceKey: String, front: AnyView) {
        guard appFlip == nil, appSheet == nil else { return }
        if let item = try? core.item(itemId: itemId), item.app?.appId == "pet" { petTab = .care }
        appFlip = AppFlip(itemId: itemId, from: from, sourceKey: sourceKey, front: front)
    }

    func openApp(_ itemId: String, fromHome: Bool = false) {
        if fromHome, let item = try? core.item(itemId: itemId) {
            go(.space(item.spaceId))
        }
        if let item = try? core.item(itemId: itemId), item.app?.appId == "pet", appSheet == nil { petTab = .care }
        appSheet = AppSheetRef(id: itemId)
    }

    /// Outro membro mexeu no mini-app desde a última vez que eu vi.
    func unseenAppChange(_ itemId: String) -> Bool {
        _ = revision
        guard let item = try? core.item(itemId: itemId), let latest = item.versions.first else { return false }
        if latest.author.isMe || item.version <= 1 { return false }
        return (seenAppVersion[itemId] ?? 1) < item.version
    }

    func markAppSeen(_ itemId: String) {
        if let v = (try? core.item(itemId: itemId))?.version, seenAppVersion[itemId] != v { seenAppVersion[itemId] = v }
    }

    func personaNamed(_ name: String) -> Persona? {
        core.personas().first { $0.name == name }
    }

    func openExternal(_ url: URL) {
        #if os(iOS)
        UIApplication.shared.open(url)
        #else
        NSWorkspace.shared.open(url)
        #endif
    }

    /// Chamada nativa a uma ferramenta de mini-app (atalhos de demo), com a mesma folha
    /// de confirmação que o cartão usa.
    /// After the group locks in a hike, Zoen reads the chat (who drives, who's in) and posts
    /// the day as a structured itinerary. Runs once: a hike with a plan is left alone.
    func planHikeIfReady(_ itemId: String) {
        guard let item = try? core.item(itemId: itemId), let app = item.app, app.appId == "hike" else { return }
        let v = AppView(app)
        guard v.string("decided") != nil, (v["itinerary"] as? [Any]) == nil else { return }
        let since = item.versions.first?.atMs ?? 0
        let entries = (try? core.timeline(spaceId: item.spaceId)) ?? []
        let voters = v.array("trails").flatMap { ($0["votes"] as? [String]) ?? [] }
        guard let args = HikePlanner.plan(entries: entries, since: since, voters: voters) else { return }
        callAppTool(itemId, "hike_set_itinerary", args: args)
    }

    func callAppTool(_ itemId: String, _ tool: String, args: String = "{}") {
        guard let out = perform({ try core.appCallTool(itemId: itemId, tool: tool, argsJson: args, confirmed: false) }) else { return }
        if out.status == .needsConfirmation {
            Haptics.warning()
            appConfirm = AppConfirmRequest(title: out.confirmTitle ?? tool, detail: out.confirmDetail ?? out.message,
                                           appName: (try? core.item(itemId: itemId))?.app?.name ?? String(localized: "The mini-app"), destructive: true) { [weak self] ok in
                guard let self, ok else { return }
                self.perform { try self.core.appCallTool(itemId: itemId, tool: tool, argsJson: args, confirmed: true) }
            }
        } else if out.status == .denied {
            show(.init(kind: .error, text: out.message))
        }
    }

    /// Histórias do Wabi, reproduzidas de verdade no núcleo (para demos e capturas).
    /// As falas e ações de Marina, Lucas e Ana são simuladas como se chegassem pela
    /// sincronização (`demo_member_*`); as do Zoen e as suas passam pelo caminho real.
    /// Debug-only investor seed polish: clear local unpins, order Home tiles, pin key chats.
    /// The mini-apps themselves come from the core demo seed (real Items per Space).
    @MainActor
    func prepareShowcase() async {
        homeUnpinned = []
        chatAppsUnpinned = []
        UserDefaults.standard.set([] as [String], forKey: "RodaChatAppsUnpinned")
        refresh()
        let preferred = ["hike", "pet", "maptap", "countdown"]
        var pins: [String] = []
        for appId in preferred {
            if let id = liveApps.first(where: { $0.app?.appId == appId })?.id, !pins.contains(id) {
                pins.append(id)
            }
        }
        for item in liveApps where WidgetSnapshot.from(item) != nil {
            if !pins.contains(item.id) { pins.append(item.id) }
        }
        homePins = pins
        saveHome()
        // Pin Zoen + the trip/crew chats on the list.
        for title in [DemoSpace.saturdayCrew, DemoSpace.paraty, DemoSpace.coastalTravelers] {
            if let s = spaces.first(where: { $0.title == title }), !isPinned(s) {
                togglePin(s)
            }
        }
        if let z = spaces.first(where: { $0.counterpart?.handle == "zoen" }), !isPinned(z) {
            togglePin(z)
        }
        // AvatarV1 art: clear stale picks, then pin distinct themed defaults for the investor home.
        for s in spaces { UserDefaults.standard.removeObject(forKey: "RodaGroupAvatar.\(s.id)") }
        for a in agents { UserDefaults.standard.removeObject(forKey: "RodaAgentAvatar.\(a.persona.id)") }
        let artPins: [(String, String)] = [
            (DemoSpace.coastalTravelers, "GroupBeachTrip"),
            (DemoSpace.saturdayCrew, "GroupMountainHike"),
            (DemoSpace.paraty, "GroupRoadTrip"),
            (AppLocale.pick("Zoen · Produto", "Zoen · Product"), "GroupWorkLaptop"),
        ]
        for (title, asset) in artPins {
            if let s = spaces.first(where: { $0.title == title }) {
                HandDrawnAvatarAsset.saveGroup(s.id, assetName: asset)
            }
        }
        #if DEBUG
        seedShowcaseApprovals()
        #endif
        // Keep an intentional `-RodaTab` (Spaces / Store investor shots).
        if UserDefaults.standard.string(forKey: "RodaTab") == nil { tab = .conversations }
        // Keep an intentional `-RodaOpen` navigation (investor chat shots).
        if UserDefaults.standard.string(forKey: "RodaOpen") == nil {
            paths[.conversations] = []
        }
        revision &+= 1
    }

    #if DEBUG
    /// Debug showcase only: a few more of your agents' requests for the approvals stack
    /// (the core seed already has Financeiro's three in Paraty). Each goes through the core
    /// evaluator; one pairs with the Marina payment link so "Sempre aprovar" settles two.
    private func seedShowcaseApprovals() {
        guard let paraty = spaceId(titled: DemoSpace.paraty),
              let turma = spaceId(titled: DemoSpace.saturdayCrew) else { return }
        let product = spaces.first { $0.title == AppLocale.pick("Zoen · Produto", "Zoen · Product") }?.id
        let asks: [(String?, String, String, String, String, String)] = [
            (paraty, "financeiro",
             AppLocale.pick("Mandar o roteiro de Paraty pro e-mail da Marina", "Email Marina the Paraty itinerary"),
             AppLocale.pick("PDF com horários, endereços e o total por pessoa.", "PDF with times, addresses and the per-person total."),
             AppLocale.pick("Marina, por e-mail", "Marina, by email"), "external"),
            (turma, "zoen",
             AppLocale.pick("Ler a agenda da Lúcia pra achar um horário", "Read Lucia's calendar to find a time"),
             AppLocale.pick("Só livre/ocupado de sábado e domingo, pra marcar a trilha.", "Just free/busy for Saturday and Sunday, to set the hike."),
             AppLocale.pick("Agenda da Lúcia", "Lucia's calendar"), "third_party_data"),
            (product, "zoen",
             AppLocale.pick("Instalar o gancho “Resumo diário” no Slack do time", "Install the “Daily digest” hook in the team Slack"),
             AppLocale.pick("Todo dia às 18h, posta no #produto o que foi entregue e o que travou.", "Every day at 6 pm, posts to #product what shipped and what's stuck."),
             AppLocale.pick("Canal #produto no Slack", "#product channel on Slack"), "external"),
        ]
        for (space, handle, title, detail, audience, action) in asks {
            guard let space, !requests.contains(where: { $0.title == title }) else { continue }
            _ = try? core.demoOpenRequest(spaceId: space, agentHandle: handle, title: title, detail: detail,
                                          audience: audience, action: action, cents: 0)
        }
        refresh()
    }
    #endif

    func runStory(_ story: String) async {
        guard let turma = spaceId(titled: DemoSpace.saturdayCrew) else { return }
        func wait(_ s: Double) async { try? await Task.sleep(for: .seconds(s)) }
        func say(_ handle: String, _ text: String) { perform { try core.demoMemberSay(spaceId: turma, memberHandle: handle, text: text) } }
        func act(_ item: String, _ handle: String, _ tool: String, _ args: String = "{}") { perform { try core.demoMemberAppCall(itemId: item, memberHandle: handle, tool: tool, argsJson: args) } }
        func app(_ id: String) -> ItemDetail? { core.items().first { $0.spaceId == turma && $0.app?.appId == id } }
        func d(_ k: String) -> Bool { UserDefaults.standard.bool(forKey: k) }
        // `home`: the group adopts the donkey and starts a MapTap, then back to Home, so the
        // strip shows several live cards (for screenshots).
        if story == "home" {
            await runStory("pet-chat-later")
            await runStory("maptap")
            await wait(0.4)
            tab = .conversations
            paths[.conversations] = []
            return
        }
        go(.space(turma))
        await wait(0.8)
        switch story {
        case let s where s.hasPrefix("pet"):
            await send(AppLocale.pick("e se a gente adotasse um jumento pro grupo?", "what if we adopted a donkey for the group?"), in: turma)
            guard let pet = app("pet") else { return }
            await wait(0.5)
            say("marina", AppLocale.pick("dei uma cenoura pra ele", "gave him a carrot"))
            act(pet.id, "marina", "pet_feed")
            say("lucas", AppLocale.pick("finalmente um membro responsável nesse grupo", "finally a responsible member in this group"))
            if s == "pet" { return }
            if s == "pet-chat-later" || s == "pet-sleep" || s == "pet-board" {
                act(pet.id, "lucas", "pet_dash_score", "{\"meters\":142,\"carrots\":7}")
                act(pet.id, "ana", "pet_dash_score", "{\"meters\":96,\"carrots\":4}")
                say("ana", AppLocale.pick("ele tem um JOGO??", "he has a GAME??"))
                await send(AppLocale.pick("vamos chamar ele de Paçoca", "let’s call him Peanut"), in: turma)
                say("lucas", AppLocale.pick("Paçoca é perfeito", "Peanut is perfect"))
            }
            if s == "pet-sleep" { act(pet.id, "ana", "pet_nap") }
            if s == "pet-chat-later" { act(pet.id, "ana", "pet_nap"); return }
            await wait(0.6)
            openApp(pet.id)
            if s == "pet-dash" { petTab = .play }
            if s == "pet-board" { petTab = .board }
            if s == "pet-live" {
                await wait(1.6)
                act(pet.id, "ana", "pet_feed")
            }
        case let s where s.hasPrefix("maptap"):
            await send(AppLocale.pick("Zoen, dá um jogo de geografia pra gente", "Zoen, give us a geography game"), in: turma)
            guard let game = app("maptap") else { return }
            let places = (try? JSONSerialization.jsonObject(with: Data((game.app?.viewJson ?? "{}").utf8))) as? [String: Any]
            if let first = (places?["places"] as? [[String: Any]])?.first, let lat = first["lat"] as? Double, let lon = first["lon"] as? Double {
                act(game.id, "lucas", "maptap_guess", "{\"round\":0,\"lat\":\(lat + 1.4),\"lon\":\(lon - 2.1)}")
            }
            if s == "maptap" { return }
            await wait(0.6)
            openApp(game.id)
        case let s where s.hasPrefix("recipe"):
            say("ana", AppLocale.pick("quem cozinha sábado? algo vegetariano e rápido pra 3", "who’s cooking Saturday? something vegetarian and quick for 3"))
            await send(AppLocale.pick("Zoen, acha uma receita de jantar vegetariano rápido pra 3", "Zoen, find a quick vegetarian dinner recipe for 3"), in: turma)
            guard let r = app("recipe") else { return }
            act(r.id, "marina", "recipe_check", "{\"id\":\"i2\"}")
            if s == "recipe" { return }
            await wait(0.6)
            openApp(r.id)
        case let s where s.hasPrefix("hike"):
            await send(AppLocale.pick("planeja uma trilha pra gente no sábado", "plan us a hike on saturday"), in: turma)
            guard let h = app("hike") else { return }
            await wait(0.8)
            act(h.id, "marina", "hike_vote", "{\"trail\":\"tomales\"}")
            say("marina", AppLocale.pick("Pedra Grande!! que vista 🌄", "Tomales!! the elk 🦌"))
            await wait(0.4)
            act(h.id, "lucas", "hike_vote", "{\"trail\":\"tomales\"}")
            say("lucas", AppLocale.pick("Tô dentro. Eu dirijo, cabem mais 3 no carro", "I’m in. I can drive, room for 3 in the car"))
            await wait(0.4)
            act(h.id, "ana", "hike_vote", "{\"trail\":\"steep\"}")
            say("ana", AppLocale.pick("tô dentro! mas a Steep Ravine tem sombra…", "in! Steep Ravine has shade though…"))
            if s == "hike" { return }
            if s == "hike-plan" || s == "hike-done" || s == "hike-ride" {
                await wait(0.4)
                callAppTool(h.id, "hike_vote", args: "{\"trail\":\"tomales\"}")
                await wait(0.4)
                act(h.id, "lucas", "hike_decide")
                await wait(0.8)
                planHikeIfReady(h.id)
                if s == "hike-plan" { return }
                if s == "hike-ride" {
                    // Stays in the chat with the day planned (the ride tile appears).
                    say("lucas", AppLocale.pick("Eu dirijo! Pego o Enzo no Noe Valley e a Ana e a Marina na Mission", "I’ll drive! Picking up Enzo in Noe Valley, Ana and Marina in the Mission"))
                    await wait(0.6)
                    if (AppView((try? core.item(itemId: h.id))?.app ?? h.app!)["itinerary"] as? [Any]) == nil {
                        callAppTool(h.id, "hike_set_itinerary", args: "{\"driver\":\"Lucas\",\"pickups\":[{\"names\":[\"Enzo\"],\"place\":\"Noe Valley\"},{\"names\":[\"Ana\",\"Marina\"],\"place\":\"Mission\"}]}")
                    }
                    return
                }
            }
            await wait(0.6)
            openApp(h.id)
        case "poll":
            await send(AppLocale.pick("Zoen, enquete: praia ou cachoeira no sábado?", "Zoen, poll: beach or waterfall on Saturday?"), in: turma)
            if let poll = app("poll") {
                act(poll.id, "marina", "poll_vote", "{\"option\":\"o1\"}")
                act(poll.id, "lucas", "poll_vote", "{\"option\":\"o2\"}")
                act(poll.id, "ana", "poll_vote", "{\"option\":\"o1\"}")
                await wait(0.6)
                if d("RodaOpenApp") { openApp(poll.id) }
            }
        default: break
        }
    }

    func path(_ t: AppTab) -> [Route] { paths[t] ?? [] }

    func setPath(_ t: AppTab, _ p: [Route]) { paths[t] = p }

    /// Pops one screen on the current tab's stack. Returns false if already at the root
    /// (callers can fall back to `dismiss()` for sheets).
    @discardableResult
    func pop() -> Bool {
        #if os(iOS)
        let key: AppTab = notificationsOpen ? .activity : tab
        #else
        let key: AppTab = .conversations
        #endif
        var p = paths[key] ?? []
        guard !p.isEmpty else { return false }
        p.removeLast()
        paths[key] = p
        return true
    }

    /// Empilha uma rota no destino atual.
    func push(_ route: Route) {
        #if os(macOS)
        // No Mac há uma pilha só, a da coluna de detalhe.
        paths[.conversations, default: []].append(route)
        #else
        // While the notifications sheet is up, routes stack inside it.
        paths[notificationsOpen ? .activity : tab, default: []].append(route)
        #endif
    }

    /// The bell: on iPhone a sheet over whatever you're on; on Mac the Activity pane.
    func openNotifications() {
        #if os(iOS)
        // Approvals waiting: the catch-up stack. Otherwise the plain list.
        if !approvalQueue.isEmpty {
            approvalsOpen = true
            return
        }
        paths[.activity] = []
        notificationsOpen = true
        #else
        select(.activity)
        #endif
    }

    // MARK: Home strip and pins

    /// The mini-apps on the Home strip: your order first, then the rest oldest first; unpinned ones hidden.
    var homeApps: [ItemDetail] {
        // Oldest first: a new mini-app joins at the end of the strip instead of shoving the rest.
        let live = liveApps.reversed().filter { !homeUnpinned.contains($0.id) && WidgetSnapshot.from($0) != nil }
        let ordered = homePins.compactMap { id in live.first { $0.id == id } }
        return ordered + live.filter { !homePins.contains($0.id) }
    }

    func isOnHome(_ itemId: String) -> Bool { homeApps.contains { $0.id == itemId } }

    func pinToHome(_ itemId: String) {
        homeUnpinned.remove(itemId)
        homePins.removeAll { $0 == itemId }
        homePins.insert(itemId, at: 0)
        saveHome()
    }

    func unpinFromHome(_ itemId: String) {
        homeUnpinned.insert(itemId)
        homePins.removeAll { $0 == itemId }
        saveHome()
    }

    /// Home edit mode: the strip's new order after a drag.
    func setHomeOrder(_ ids: [String]) {
        homePins = ids
        saveHome()
    }

    /// Chat pins edit mode: tile order (tile ids are "<item>#<n>") and its persistence.
    private(set) var chatTileOrder: [String] = UserDefaults.standard.stringArray(forKey: "RodaChatTileOrder") ?? []
    func setChatTileOrder(_ ids: [String]) {
        chatTileOrder = ids + chatTileOrder.filter { !ids.contains($0) }
        UserDefaults.standard.set(chatTileOrder, forKey: "RodaChatTileOrder")
    }

    /// Moves a Home card one step left (-1) or right (+1).
    func moveOnHome(_ itemId: String, by step: Int) {
        var ids = homeApps.map(\.id)
        guard let i = ids.firstIndex(of: itemId) else { return }
        let j = min(max(i + step, 0), ids.count - 1)
        guard i != j else { return }
        ids.swapAt(i, j)
        homePins = ids
        saveHome()
    }

    private func saveHome() {
        UserDefaults.standard.set(homePins, forKey: "RodaHomePins")
        UserDefaults.standard.set(Array(homeUnpinned), forKey: "RodaHomeUnpinned")
        revision &+= 1
    }

    /// Pin key for a chat: "zoen" for your agent's DM (stable across reseeds), else the id.
    func pinKey(_ space: SpaceSummary) -> String { space.counterpart?.handle == "zoen" ? "zoen" : space.id }
    func isPinned(_ space: SpaceSummary) -> Bool { chatPins.contains(pinKey(space)) }
    func togglePin(_ space: SpaceSummary) {
        let k = pinKey(space)
        if let i = chatPins.firstIndex(of: k) { chatPins.remove(at: i) } else { chatPins.append(k) }
        UserDefaults.standard.set(chatPins, forKey: "RodaChatPins")
        revision &+= 1
    }

    /// Chats in Home order: pinned first (in pin order), then by recency.
    var orderedSpaces: [SpaceSummary] {
        let pinned = chatPins.compactMap { k in spaces.first { pinKey($0) == k } }
        return pinned + spaces.filter { !chatPins.contains(pinKey($0)) }
    }

    /// Troca de destino (barra ou menu radial). Tocar no destino atual volta à raiz.
    func select(_ t: AppTab) {
        #if os(iOS)
        // On iPhone, search is a full screen over whatever tab you're on, not a tab.
        if t == .search { searchOpen = true; return }
        #endif
        if tab == t { paths[t] = [] } else { tab = t }
        #if os(macOS)
        paths[.conversations] = []
        #endif
        macSelection = MacSidebarItem(tab: t) ?? macSelection
    }

    func go(_ route: Route) {
        #if os(iOS)
        // Jumping to a chat or an item from the notifications sheet closes it.
        switch route {
        case .space, .item: notificationsOpen = false
        default: break
        }
        #endif
        switch route {
        case .space(let id):
            tab = .conversations
            #if os(macOS)
            paths[.conversations] = []
            #else
            paths[.conversations] = [.space(id)]
            #endif
            macSelection = .space(id)
        case .item(let id):
            #if os(macOS)
            inspectorItem = id
            #else
            push(.item(id))
            #endif
        default:
            #if os(macOS)
            paths[.conversations, default: []].append(route)
            #else
            push(route)
            #endif
        }
    }
}

enum MacSidebarItem: Hashable {
    case activity, communities, files, agents, you, search, store
    case space(String)

    init?(tab: AppTab) {
        switch tab {
        case .conversations: return nil
        case .communities: self = .communities
        case .files: self = .files
        case .activity: self = .activity
        case .agents: self = .agents
        case .you: self = .you
        case .search: self = .search
        case .store: self = .store
        }
    }
}
