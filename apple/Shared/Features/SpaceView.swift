import SwiftUI
import RodaCore

/// Tela 2: a conversa com pessoas e agentes. O Item nasce da mensagem do agente.
struct SpaceView: View {
    @Environment(AppModel.self) private var model
    let spaceId: String
    var onOpenItem: (String) -> Void

    @State private var entries: [TimelineEntry] = []
    @State private var draft = ""
    @State private var pinned: ItemDetail?
    @State private var pinnedApps: [ItemDetail] = []
    /// Height of the glass top bar (plus the plan bar), and the status-bar inset, for the scroll insets and fade.
    @State private var chromeHeight: CGFloat = 0
    @State private var safeTop: CGFloat = 0
    @State private var scrollPos = ScrollPosition(edge: .bottom)
    @State private var offsetY: CGFloat = 0
    /// Top of the composer bar, in global coordinates (the message fade ends there).
    @State private var composerTop: CGFloat = .infinity
    @Environment(\.dismiss) private var dismiss
    @FocusState private var composerFocused: Bool
    @State private var backgroundPicker = false
    /// A message a search result jumped to (briefly highlighted).
    @State private var highlighted: String?

    private var space: SpaceSummary? { model.space(spaceId) }
    private var workingAgent: Persona? { model.working[spaceId] }
    /// "Just for me" wins over the shared one (the latest `BackgroundSet` in the log).
    private var backgroundState: (background: ChatBackground, layout: PhotoBackgroundLayout, isLocal: Bool) {
        _ = entries.count
        return model.backgroundState(spaceId)
    }
    private var background: ChatBackground { backgroundState.background }
    /// A photo background can pin the chat to light or dark.
    private var forcedScheme: ColorScheme? {
        let st = backgroundState
        guard st.background.isPhotoLike else { return nil }
        switch st.layout.appearance {
        case "light": return .light
        case "dark": return .dark
        default: return nil
        }
    }
    @Environment(\.colorScheme) private var systemScheme

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 4) {
                    // Wabi pattern: the pinned tiles are the chat's first section, right under
                    // the top bar, and scroll away with the messages.
                    Color.clear.frame(height: 1).id("chat-top")
                    if !pinnedApps.isEmpty {
                        ChatPinStrip(apps: pinnedApps)
                            .padding(.horizontal, -14)
                            .padding(.bottom, 8)
                            .transition(.opacity)
                    }
                    if let space { SpaceHeaderCard(space: space).padding(.bottom, 12) }
                    ForEach(Array(entries.enumerated()), id: \.element.id) { idx, entry in
                        EntryView(entry: entry,
                                  previous: idx > 0 ? entries[idx - 1] : nil,
                                  next: idx + 1 < entries.count ? entries[idx + 1] : nil,
                                  isDirect: space?.kind == .direct,
                                  onOpenItem: onOpenItem)
                            .background {
                                if highlighted == entry.id {
                                    RoundedRectangle(cornerRadius: 18, style: .continuous)
                                        .fill(Palette.action.opacity(0.18))
                                        .padding(.horizontal, -8)
                                        .padding(.vertical, -2)
                                        .transition(.opacity)
                                }
                            }
                            .id(entry.id)
                            .transition(.asymmetric(insertion: .scale(scale: 0.92, anchor: .bottom).combined(with: .opacity), removal: .opacity))
                    }
                    if let agent = workingAgent {
                        WorkingRow(agent: agent, label: model.planner.availability == .onDevice ? String(localized: "on device") : String(localized: "local planner"))
                            .id("working")
                            .transition(.asymmetric(insertion: .scale(scale: 0.6, anchor: .bottomLeading).combined(with: .opacity),
                                                    removal: .scale(scale: 0.85, anchor: .leading).combined(with: .opacity)))
                    }
                    Color.clear.frame(height: 8).id("bottom")
                }
                .padding(.horizontal, 14)
                .padding(.top, 8)
            }
            // Investor shots pin the top; content growth (tiles, images) must not drag it down.
            .defaultScrollAnchor(UserDefaults.standard.bool(forKey: "RodaChatScrollTop") ? .top : .bottom)
            #if os(iOS)
            // iOS 26 adds a soft blur where content meets a bar; none here, content stays crisp.
            .scrollEdgeEffectHidden(true, for: .top)
            #endif
            // Fold the tiles only once *you* scroll back through history (never from layout
            // changes, which would feed back into the insets and loop).
            .scrollPosition($scrollPos)
            .onScrollGeometryChange(for: CGFloat.self) { $0.contentOffset.y } action: { _, y in offsetY = y }
            // Content starts just under the glass bar (bar height + 8pt): at rest nothing sits
            // behind the glass or the status bar; content passes under the bar only while
            // scrolling. No mask, blur or band.
            .safeAreaPadding(.top, chromeHeight + 8)
            .scrollDismissesKeyboard(.interactively)
            .environment(\.chatBackdrop, !background.isNone)
            // Messages fade out just above the composer instead of ghosting through its
            // glass; the chat background (drawn below this mask) stays whole.
            .mask {
                GeometryReader { g in
                    let fade: CGFloat = 28
                    let cut = composerTop.isFinite ? max(0, composerTop - g.frame(in: .global).minY) : g.size.height
                    VStack(spacing: 0) {
                        Color.black.frame(height: max(0, cut - fade))
                        LinearGradient(colors: [.black, .black.opacity(0)], startPoint: .top, endPoint: .bottom)
                            .frame(height: min(fade, cut))
                        Color.clear
                    }
                }
                .ignoresSafeArea()
            }
            .background(ChatBackdropView(background: background, spaceId: spaceId, layout: backgroundState.layout).ignoresSafeArea())
            // A bar, not an inset: iOS 26 fades and blurs what scrolls under the composer,
            // so message text doesn't ghost through its glass.
            #if os(iOS)
            .scrollEdgeEffectStyle(.soft, for: .bottom)
            #endif
            .safeAreaBar(edge: .bottom, spacing: 0) {
                Composer(text: $draft,
                         placeholder: space?.counterpart?.kind == .agent ? String(localized: "What do you want to get done?") : String(localized: "Message"),
                         focused: $composerFocused,
                         busy: workingAgent != nil,
                         onCapture: { model.show(.init(kind: .info, text: String(localized: "Photo taken. Sending photos comes in a later build, so it stays on this device."))) },
                         onVoice: voiceHandler) {
                    let text = draft
                    draft = ""
                    model.sync.stoppedTyping(spaceId)
                    Task { await model.send(text, in: spaceId) }
                }
                .onGeometryChange(for: CGFloat.self) { $0.frame(in: .global).minY } action: { composerTop = $0 }
            }
            .onChange(of: draft) { _, text in model.sync.typingChanged(spaceId, text: text) }
            .onDisappear { model.sync.stoppedTyping(spaceId) }
            .onChange(of: entries.count) { old, _ in
                // Investor shots: `-RodaChatScrollTop` keeps the pin strip framed (don't jump to bottom).
                if UserDefaults.standard.bool(forKey: "RodaChatScrollTop") {
                    if old == 0 {
                        Task { @MainActor in
                            try? await Task.sleep(for: .milliseconds(80))
                            proxy.scrollTo("chat-top", anchor: .top)
                            scrollPos.scrollTo(edge: .top)
                        }
                    }
                    return
                }
                if old == 0 {
                    // Primeira carga: vai direto para o fim, sem animação.
                    proxy.scrollTo("bottom", anchor: .bottom)
                    Task { @MainActor in
                        try? await Task.sleep(for: .milliseconds(60))
                        proxy.scrollTo("bottom", anchor: .bottom)
                    }
                } else {
                    withAnimation(.snappy) { proxy.scrollTo("bottom", anchor: .bottom) }
                }
            }
            .task(id: "\(model.jumpTarget?.entry ?? "")|\(entries.isEmpty)") {
                // Search jumped here: scroll to the exact message and highlight it briefly.
                guard let j = model.jumpTarget, j.space == spaceId, entries.contains(where: { $0.id == j.entry }) else { return }
                try? await Task.sleep(for: .milliseconds(450))
                withAnimation(.smooth(duration: 0.5)) { proxy.scrollTo(j.entry, anchor: .center) }
                withAnimation(.easeOut(duration: 0.25)) { highlighted = j.entry }
                Haptics.selectionTick()
                Task { @MainActor in
                    try? await Task.sleep(for: .seconds(1.8))
                    withAnimation(.easeOut(duration: 0.7)) { highlighted = nil }
                }
                model.jumpTarget = nil
            }
            .onChange(of: workingAgent?.id) { _, _ in
                withAnimation(.snappy) { proxy.scrollTo("bottom", anchor: .bottom) }
            }
        }
        .task(id: model.revision) { reload() }
        .task(id: "\(background.token)|\(model.revision)") { model.ensureBackgroundMedia(background) }
        .task {
            // Screenshots: `-RodaBgSeed file:/path.jpg|asset:<name>` runs the real shared-photo
            // path once (resize → HEIC → put_media → signed BackgroundSet event).
            guard let seed = UserDefaults.standard.string(forKey: "RodaBgSeed") else { return }
            try? await Task.sleep(for: .seconds(1.2))
            if let cur = try? model.core.background(spaceId: spaceId), cur.media != nil { return }
            let data: Data? = {
                if seed.hasPrefix("file:") { return FileManager.default.contents(atPath: String(seed.dropFirst(5))) }
                if seed.hasPrefix("asset:"), let img = PlatformImage.named(String(seed.dropFirst(6))) {
                    #if os(iOS)
                    return img.jpegData(compressionQuality: 0.95)
                    #else
                    return img.tiffRepresentation
                    #endif
                }
                return nil
            }()
            guard let data, let enc = BackgroundImage.encode(data, maxSide: 2048),
                  let ref = model.perform({ try model.core.putMedia(bytes: enc.data, mime: enc.mime, width: UInt32(enc.size.width), height: UInt32(enc.size.height)) })
            else { return }
            ChatBackgroundStore.shared.cacheMedia(ref.sha256, bytes: enc.data)
            var layout = PhotoBackgroundLayout()
            if let a = UserDefaults.standard.string(forKey: "RodaBgAppearance") { layout.appearance = a }
            _ = model.perform { try model.core.setBackground(spaceId: spaceId, background: layout.dto(style: "photo", media: ref)) }
        }
        .environment(\.colorScheme, forcedScheme ?? systemScheme)
        .task {
            // Screenshots: scroll up N pt once the story settles (`-RodaChatScrollUp 260`), so
            // a message sits between the glass buttons.
            let up = UserDefaults.standard.double(forKey: "RodaChatScrollUp")
            let top = UserDefaults.standard.bool(forKey: "RodaChatScrollTop")
            guard up > 0 || top else { return }
            let delay = UserDefaults.standard.double(forKey: "RodaChatScrollDelay")
            // Investor shots: wait for timeline + pins to land, then pin the top.
            let wait = delay > 0 ? delay : (top ? 1.2 : 9)
            try? await Task.sleep(for: .seconds(wait))
            withAnimation(.smooth(duration: 0.45)) {
                if top { scrollPos.scrollTo(edge: .top) } else { scrollPos.scrollTo(y: max(0, offsetY - up)) }
            }
            if top {
                // Late tiles, images and app cards can still grow the content: keep re-pinning
                // for a few seconds until nothing moves it any more.
                for _ in 0..<6 {
                    try? await Task.sleep(for: .seconds(1))
                    withAnimation(.smooth(duration: 0.3)) { scrollPos.scrollTo(edge: .top) }
                }
            }
        }
        .task {
            // Screenshots: `-RodaVoiceSeed YES` drops one voice note (synthetic audio) in.
            if let d = UserDefaults.standard.string(forKey: "RodaDraft") { draft = d }
            guard UserDefaults.standard.bool(forKey: "RodaVoiceSeed") else { return }
            let wait = UserDefaults.standard.double(forKey: "RodaVoiceSeedDelay")
            try? await Task.sleep(for: .seconds(wait > 0 ? wait : 1.5))
            let has = entries.contains { if case .message(let t, _) = $0.kind { return VoiceNoteRef.parse(t) != nil }; return false }
            guard !has, let clip = VoiceStore.makeDemoClip() else { return }
            let ref = VoiceNoteRef(id: clip.id, ms: clip.ms, levels: clip.levels,
                                   transcript: String(localized: "Leaving at eight! I'll bring snacks and the good thermos, can someone grab the map?"))
            model.perform { try model.core.sendMessage(spaceId: spaceId, text: ref.marker) }
        }
        .onAppear { model.perform { try model.core.markRead(spaceId: spaceId) } }
        .sheet(isPresented: $backgroundPicker) {
            ChatBackgroundPicker(spaceId: spaceId, current: background, isLocal: backgroundState.isLocal)
        }
        .task {
            // Screenshots: `-RodaBackgroundPicker YES` opens the picker.
            guard UserDefaults.standard.bool(forKey: "RodaBackgroundPicker") else { return }
            try? await Task.sleep(for: .seconds(2.5))
            backgroundPicker = true
        }
        .navigationTitle(space?.title ?? "")
        #if os(iOS)
        // Our own glass bar replaces the system one (the edge swipe back still works).
        .toolbar(.hidden, for: .navigationBar)
        .toolbarBackground(.hidden, for: .navigationBar)
        .onGeometryChange(for: CGFloat.self) { $0.safeAreaInsets.top } action: { safeTop = $0 }
        // No veil, blur or fade anywhere: content runs crisp to the screen edge and only the
        // glass elements have a surface.
        .overlay(alignment: .top) {
            VStack(spacing: 8) {
                if let space {
                    ChatTopBar(space: space, subtitle: subtitle(space), status: status(space),
                               onBack: { _ = model.pop() },
                               onOpen: { model.go(.participants(spaceId)) })
                }
                if let pinned { PinnedItemBar(item: pinned) { onOpenItem(pinned.id) } }
            }
            .padding(.top, 2)
            .padding(.bottom, 6)
            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { chromeHeight = $0 }
        }
        #else
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { model.go(.participants(spaceId)) } label: { ZoenIcon(.info, size: 18) }
                    .accessibilityLabel("Participants")
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            if let pinned { PinnedItemBar(item: pinned) { onOpenItem(pinned.id) } }
        }
        #endif
    }

    /// Voice notes record on iPhone; the Mac app has no mic entitlement yet.
    private var voiceHandler: ((VoiceClip, String?) -> Void)? {
        #if os(iOS)
        return { clip, transcript in _ = Task { await model.sendVoice(clip, in: spaceId, transcript: transcript) } }
        #else
        return nil
        #endif
    }

    /// Clear space between the glass bar and the pinned row.
    #if os(iOS)
    static let pinGap: CGFloat = 10
    #else
    static let pinGap: CGFloat = 0
    #endif

    /// Live title status: who's doing what. `-RodaTitleStatus typing|processing|building|call`
    /// forces one for screenshots (typing in a group = the first other person).
    private func status(_ s: SpaceSummary) -> ChatStatus? {
        let group = s.counterpart == nil
        if let forced = UserDefaults.standard.string(forKey: "RodaTitleStatus").flatMap(ChatStatus.Kind.init(rawValue:)) {
            let who: Persona? = forced == .typing && group
                ? s.members.first { $0.kind != .agent && !$0.isMe }
                : (s.counterpart ?? model.zoen)
            return ChatStatus(kind: forced, who: group ? who?.name : nil)
        }
        if let agent = workingAgent {
            return ChatStatus(kind: model.activity[spaceId] ?? .processing, who: group ? agent.name : nil)
        }
        // People in shared chats: typing and what they're doing come from the relay.
        if let remote = model.sync.remoteStatus(spaceId) {
            return ChatStatus(kind: remote.kind, who: group ? remote.who : nil)
        }
        return nil
    }

    /// Agents are contacts: the title never counts or labels them.
    /// No encryption / sync jargon in the chrome — just presence or a people count.
    private func subtitle(_ s: SpaceSummary) -> String {
        if let c = s.counterpart {
            if model.sync.isSynced(s.id), c.kind == .person {
                return model.sync.online.contains(c.id)
                    ? String(localized: "online")
                    : ""
            }
            return ""
        }
        let people = s.members.filter { $0.kind != .agent }.count
        return String(localized: "\(people) people")
    }

    private func reload() {
        let new = (try? model.core.timeline(spaceId: spaceId)) ?? []
        pinned = model.core.items().first { $0.spaceId == spaceId && $0.plan != nil }
        // Live mini-apps of this chat, newest first (the hike leads when there is one).
        let apps = model.liveApps.filter { $0.spaceId == spaceId && $0.app != nil && !model.chatAppsUnpinned.contains($0.id) }
            .sorted { ($0.app?.appId == "hike" ? 1 : 0, $0.versions.first?.atMs ?? 0) > ($1.app?.appId == "hike" ? 1 : 0, $1.versions.first?.atMs ?? 0) }
        withAnimation(.spring(duration: 0.5, bounce: 0.2)) { pinnedApps = apps }
        withAnimation(.spring(duration: 0.5, bounce: 0.2)) { entries = new }
        if !new.isEmpty { try? model.core.markRead(spaceId: spaceId) }
    }
}

/// Item fixado no topo da conversa (poster #015: "Lançamento de outubro").
struct PinnedItemBar: View {
    let item: ItemDetail
    var onOpen: () -> Void
    var body: some View {
        Button(action: onOpen) {
            HStack(spacing: 10) {
                ZoenIcon(.pin, size: 15).foregroundStyle(Palette.action)
                VStack(alignment: .leading, spacing: 0) {
                    Text(item.title).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.textPrimary).lineLimit(1)
                    Text(meta).font(.caption2).foregroundStyle(Palette.textSecondary).lineLimit(1)
                }
                Spacer()
                ZoenIcon(.chevron, size: 13).foregroundStyle(Palette.textTertiary)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 8)
            .glassEffect(.regular.interactive(), in: .rect(cornerRadius: 16))
            .padding(.horizontal, 12)
            .padding(.bottom, 4)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Pinned: \(item.title)")
    }
    private var meta: String {
        let lines = item.plan?.sections.flatMap(\.lines) ?? []
        let total = lines.reduce(Int64(0)) { $0 + $1.costCents }
        if lines.isEmpty { return Money.format(total) }
        return String(localized: "\(lines.count) items · \(Money.format(total))")
    }
}

/// Who's in this chat — avatar only. No privacy jargon.
struct SpaceHeaderCard: View {
    let space: SpaceSummary
    var body: some View {
        VStack(spacing: 10) {
            if let c = space.counterpart {
                ContactAvatar(persona: c, size: 64)
            } else {
                // Group hero: its drawing (loops when motion is on), then who's in it.
                GroupAvatar(space: space, size: 72)
                FacePile(members: space.members.filter { $0.kind != .agent }, size: 30)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 8)
    }
}

// MARK: - Linhas da conversa

struct EntryView: View {
    let entry: TimelineEntry
    let previous: TimelineEntry?
    var next: TimelineEntry? = nil
    var isDirect: Bool
    var onOpenItem: (String) -> Void
    @Environment(AppModel.self) private var model

    /// Same author within 5 minutes → continuation (no name).
    private var grouped: Bool {
        guard let previous, previous.author.id == entry.author.id else { return false }
        guard case .message(let pt, _) = previous.kind, ChatBackgroundMarker.parse(pt) == nil else { return false }
        return entry.atMs - previous.atMs < 5 * 60_000
    }

    /// Last bubble in a run: avatar sits here (WhatsApp-style).
    private var endsRun: Bool {
        guard let next, next.author.id == entry.author.id else { return true }
        guard case .message(let nt, _) = next.kind, ChatBackgroundMarker.parse(nt) == nil else { return true }
        return next.atMs - entry.atMs >= 5 * 60_000
    }

    var body: some View {
        switch entry.kind {
        case .message(let text, _) where ChatBackgroundMarker.parse(text) != nil:
            BackgroundChangeRow(author: entry.author, background: ChatBackgroundMarker.parse(text) ?? .none, spaceId: entry.id)
        case .background(let dto):
            BackgroundChangeRow(author: entry.author, background: ChatBackground(dto: dto),
                                spaceId: entry.id, layout: PhotoBackgroundLayout(dto: dto))
        case .message(let text, let card):
            VStack(alignment: .trailing, spacing: 3) {
                MessageRow(author: entry.author, text: text, card: card, atMs: entry.atMs,
                           grouped: grouped, endsRun: endsRun, isDirect: isDirect, onOpenItem: onOpenItem)
                if entry.author.isMe { DeliveryMark(delivery: entry.delivery) }
            }
            .padding(.top, grouped ? 0 : 10)
        case .itemEdited(let itemId, _, _, _, _) where (try? model.core.item(itemId: itemId))?.app != nil:
            // Mini-apps: cada toque atualiza o widget ao vivo (ponto vermelho e coração no
            // cartão); o histórico fica na folha, não enche a conversa.
            EmptyView()
        case .itemEdited(let itemId, _, let version, let note, let isUndo):
            SystemRow(glyph: isUndo ? .back : .list, text: "\(entry.author.isMe ? "Você" : entry.author.name) · \(note)", trailing: "v\(version)")
                .onTapGesture { onOpenItem(itemId) }
                .padding(.vertical, 6)
        case .request(let requestId, _, _):
            if let r = model.requests.first(where: { $0.id == requestId }) {
                InlineRequestRow(request: r).padding(.vertical, 6)
            }
        case .system(let text):
            SystemRow(glyph: text.hasPrefix(String(localized: "Approved")) ? .check : .close, text: text, trailing: nil)
                .padding(.vertical, 6)
        }
    }
}

/// Under my own messages in shared chats: queued (offline or on its way) or refused by
/// the relay. Delivered and on-device messages show nothing.
struct DeliveryMark: View {
    let delivery: Delivery
    @Environment(\.chatBackdrop) private var backdrop

    var body: some View {
        switch delivery {
        case .sending:
            Label { Text("Sending…") } icon: { Image(systemName: "clock") }
                .font(.caption2.weight(.medium))
                .foregroundStyle(backdrop ? Palette.textSecondary : Palette.textTertiary)
                .padding(.trailing, 6)
                .transition(.opacity)
                .accessibilityLabel("Sending")
        case .failed:
            Label { Text("Not delivered") } icon: { Image(systemName: "exclamationmark.circle.fill") }
                .font(.caption2.weight(.semibold))
                .foregroundStyle(Palette.danger)
                .padding(.trailing, 6)
        default:
            EmptyView()
        }
    }
}

private extension VerticalAlignment {
    /// Where a sender's face sits beside a message: the text bubble's bottom when there is
    /// one, otherwise the bottom of the row.
    enum MessageFace: AlignmentID {
        static func defaultValue(in d: ViewDimensions) -> CGFloat { d[.bottom] }
    }
    static let messageFace = VerticalAlignment(MessageFace.self)
}

struct MessageRow: View {
    let author: Persona
    let text: String
    let card: ItemCard?
    let atMs: Int64
    let grouped: Bool
    /// Avatar aligns to the last bubble of a run (WhatsApp-style).
    var endsRun: Bool = true
    let isDirect: Bool
    var onOpenItem: (String) -> Void
    @Environment(\.chatBackdrop) private var backdrop

    private static let face: CGFloat = 36

    var body: some View {
        let mine = author.isMe
        // The face lines up with the run's last text bubble, not the bottom of a tall card
        // under it (an agent's trail card pushed its face out of view).
        HStack(alignment: .messageFace, spacing: 8) {
            if mine { Spacer(minLength: 48) }
            if !mine {
                if endsRun || author.kind == .agent {
                    // ContactAvatar: paper disc + Zoen MascotHead / AvatarV1 (AgentAvatar path
                    // was reading as an empty gutter next to agent bubbles in groups).
                    Group {
                        if author.kind == .agent {
                            ContactAvatar(persona: author, size: Self.face)
                        } else {
                            Avatar(persona: author, size: Self.face)
                        }
                    }
                    .profileLink(author)
                } else {
                    Color.clear.frame(width: Self.face, height: 1)
                }
            }
            VStack(alignment: mine ? .trailing : .leading, spacing: 4) {
                // Groups: sender name + time above the first bubble of a run (plain text, no icons).
                if !mine && !grouped && !isDirect {
                    HStack(spacing: 6) {
                        Text(author.name)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(Color(hex: author.tintHex))
                            .profileLink(author)
                        Text(RodaTime.short(atMs))
                            .font(.caption2)
                            .foregroundStyle(backdrop ? Palette.textSecondary : Palette.textTertiary)
                    }
                    .padding(.horizontal, backdrop ? 7 : 0)
                    .padding(.vertical, backdrop ? 2 : 0)
                    .background { if backdrop { Capsule().fill(.regularMaterial) } }
                    .padding(.leading, 2)
                }
                if let voice = VoiceNoteRef.parse(text) {
                    VoiceBubble(ref: voice, mine: mine, grouped: grouped)
                        .alignmentGuide(.messageFace) { $0[.bottom] }
                } else if !text.isEmpty {
                    if author.kind == .agent && !mine, let plan = Itinerary(text) {
                        ItineraryCard(itinerary: plan)
                    } else {
                        // Me in black (4 pt corner bottom right); everyone else, agents
                        // included, in the same grey bubble (tight corner bottom left).
                        Text(attributed(text))
                            .font(.body)
                            .foregroundStyle(mine ? Palette.myBubbleText : Palette.textPrimary)
                            .padding(.horizontal, 14)
                            .padding(.vertical, 9)
                            .background {
                                if mine {
                                    UnevenRoundedRectangle(topLeadingRadius: 20, bottomLeadingRadius: 20, bottomTrailingRadius: grouped ? 20 : 4, topTrailingRadius: 20, style: .continuous)
                                        .fill(Palette.myBubble)
                                } else {
                                    UnevenRoundedRectangle(topLeadingRadius: 20, bottomLeadingRadius: grouped ? 20 : 4, bottomTrailingRadius: 20, topTrailingRadius: 20, style: .continuous)
                                        .fill(backdrop ? Palette.otherBubbleOnBackdrop : Palette.otherBubble)
                                }
                            }
                            .shadow(color: .black.opacity(backdrop ? 0.1 : 0), radius: 1.5, y: 0.5)
                            .textSelection(.enabled)
                            .alignmentGuide(.messageFace) { $0[.bottom] }
                        if mine && !grouped {
                            FirstBubbleFlourish(key: "\(author.id)-\(atMs)")
                        }
                    }
                }
                if let card, let app = card.app, app.appId == "hike" {
                    HikeChatCard(card: card, app: app)
                        .transition(.scale(scale: 0.85, anchor: .topLeading).combined(with: .opacity))
                } else if let card, let app = card.app {
                    AppOutputBlock(card: card, app: app)
                        .transition(.scale(scale: 0.85, anchor: .topLeading).combined(with: .opacity))
                } else if let card {
                    Button { onOpenItem(card.itemId) } label: { ItemCardView(card: card) }
                        .buttonStyle(.plain)
                        .transition(.scale(scale: 0.85, anchor: .topLeading).combined(with: .opacity))
                }
            }
            if !mine { Spacer(minLength: card?.app != nil ? 0 : 32) }
        }
    }

    /// Realça @menções.
    private func attributed(_ s: String) -> AttributedString {
        var a = AttributedString(s)
        var search = a.startIndex
        while let r = a[search...].range(of: "@") {
            var end = r.upperBound
            while end < a.endIndex, a.characters[end].isLetter { end = a.index(afterCharacter: end) }
            if end > r.upperBound {
                a[r.lowerBound..<end].font = .body.weight(.semibold)
                if !author.isMe { a[r.lowerBound..<end].foregroundColor = Palette.action }
            }
            search = end
        }
        return a
    }
}

struct SystemRow: View {
    let glyph: ZoenGlyph
    let text: String
    let trailing: String?
    var body: some View {
        HStack(spacing: 6) {
            ZoenIcon(glyph, size: 13)
            Text(text).lineLimit(1)
            if let trailing { Text(trailing).foregroundStyle(Palette.textTertiary) }
        }
        .font(.caption.weight(.medium))
        .foregroundStyle(Palette.textSecondary)
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(Palette.surfaceMuted.opacity(0.7), in: .capsule)
        .frame(maxWidth: .infinity)
    }
}

struct InlineRequestRow: View {
    let request: AgentRequestDto
    @Environment(AppModel.self) private var model

    var body: some View {
        HStack(spacing: 10) {
            AgentAvatar(persona: request.agent, size: 26, state: request.status == .pending ? .waiting : .idle, showsOwner: false)
            VStack(alignment: .leading, spacing: 2) {
                Text(request.title).font(.subheadline.weight(.semibold)).lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 4) {
                    Text("\(request.agent.name) asked")
                    if let c = request.costCents {
                        Text("·")
                        Text(Money.format(c)).monospacedDigit().foregroundStyle(Palette.textPrimary)
                    }
                }
                .font(.caption).foregroundStyle(Palette.textSecondary)
            }
            Spacer(minLength: 4)
            switch request.status {
            case .pending:
                Button("Approve") { model.approve(request) }
                    .buttonStyle(.glassProminent)
                    .controlSize(.small)
            case .stale:
                ZoenIcon(.back, size: 17).scaleEffect(x: -1).foregroundStyle(Palette.amber)
                    .accessibilityLabel(Text("Out of date"))
            case .approved:
                ZoenIcon(.check, size: 18).foregroundStyle(Palette.success)
            case .denied:
                ZoenIcon(.close, size: 16).foregroundStyle(Palette.textTertiary)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(Palette.surface, in: .rect(cornerRadius: 16, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(request.status == .pending ? Palette.amber.opacity(0.5) : Palette.hairline.opacity(0.6), lineWidth: 0.8))
        .padding(.leading, 38)
    }
}

struct WorkingRow: View {
    let agent: Persona
    let label: String
    var body: some View {
        // Same as anyone typing: their face and a grey bubble with three dots.
        HStack(alignment: .bottom, spacing: 8) {
            ContactAvatar(persona: agent, size: 26)
            TypingDots()
                .padding(.horizontal, 14)
                .padding(.vertical, 13)
                .background(UnevenRoundedRectangle(topLeadingRadius: 20, bottomLeadingRadius: 4, bottomTrailingRadius: 20, topTrailingRadius: 20, style: .continuous).fill(Palette.otherBubble))
            Spacer()
        }
        .padding(.top, 10)
        .accessibilityElement()
        .accessibilityLabel(Text("\(agent.name) is typing"))
    }
}

struct TypingDots: View {
    @Environment(\.ambientPaused) private var ambientPaused
    var body: some View {
        TimelineView(.animation(paused: ambientPaused)) { ctx in
            let t = ctx.date.timeIntervalSinceReferenceDate
            HStack(spacing: 4) {
                ForEach(0..<3, id: \.self) { i in
                    let phase = (sin((t * 2 * .pi / 1.1) - Double(i) * 0.9) + 1) / 2
                    Circle().fill(Palette.textSecondary)
                        .frame(width: 7, height: 7)
                        .opacity(0.35 + 0.65 * phase)
                        .offset(y: -2.5 * phase)
                }
            }
        }
    }
}

// MARK: - Composer (cápsula de vidro)

struct Composer: View {
    @Binding var text: String
    var placeholder: String
    var focused: FocusState<Bool>.Binding
    var busy: Bool
    var onCapture: (() -> Void)? = nil
    /// A finished voice note (iOS) and its transcript if already known (the editor sends
    /// the kept words). Nil hides the mic.
    var onVoice: ((VoiceClip, String?) -> Void)? = nil
    var onSend: () -> Void
    @State private var cameraOpen = false
    @State private var recorder = VoiceRecorder()
    @State private var drag: CGSize = .zero
    @State private var pressing = false
    @State private var startedThisPress = false
    @State private var holdTask: Task<Void, Never>?
    @State private var tooltip = false
    @State private var review: VoiceEditorModel?

    private var empty: Bool { text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    /// Mic instead of send: the field is empty and a voice handler exists.
    private var micMode: Bool { onVoice != nil && empty && !busy }

    var body: some View {
        VStack(spacing: 8) {
            #if os(iOS)
            if cameraOpen {
                InlineCameraPanel(onClose: { withAnimation(.spring(duration: 0.4, bounce: 0.2)) { cameraOpen = false } },
                                  onCapture: { onCapture?() })
                    .padding(.horizontal, 12)
                    .transition(.asymmetric(insertion: .scale(scale: 0.4, anchor: .bottomLeading).combined(with: .opacity),
                                            removal: .scale(scale: 0.6, anchor: .bottomLeading).combined(with: .opacity)))
            }
            #endif
            bar
        }
        .overlay(alignment: .topTrailing) {
            if tooltip {
                Text("Hold to record, release to send")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(Palette.textPrimary)
                    .padding(.horizontal, 12).padding(.vertical, 8)
                    .glassEffect(.regular, in: .capsule)
                    .offset(x: -12, y: -38)
                    .transition(.scale(scale: 0.8, anchor: .bottomTrailing).combined(with: .opacity))
            }
        }
        .overlay(alignment: .bottomTrailing) {
            if recorder.phase == .recording {
                LockHint(dragY: drag.height)
                    .padding(.trailing, 14)
                    .padding(.bottom, 66)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(.spring(duration: 0.35, bounce: 0.25), value: recorder.phase)
        .animation(.snappy, value: tooltip)
        .onChange(of: micMode) { _, _ in Haptics.selectionTick() }
        .sheet(item: $review) { m in
            VoiceReviewSheet(model: m) { clip, text in onVoice?(clip, text) }
        }
        .task {
            // Screenshots: `-RodaVoiceEditor review|edit` opens the review sheet on a demo clip.
            guard let mode = UserDefaults.standard.string(forKey: "RodaVoiceEditor"), onVoice != nil else { return }
            try? await Task.sleep(for: .seconds(2.5))
            guard let clip = VoiceStore.makeDemoClip() else { return }
            let m = VoiceEditorModel(clip: clip, transcript: .demo(seconds: 7))
            switch mode {
            case "edit":
                m.toggleWord(3)  // "think" / "que"
                m.selection = 6.1...6.7
                m.editing = true
            case "edit-clean":  // UI test starts from no edits
                m.editing = true
            case "multi":  // several separate cuts at once
                m.toggleWord(3)
                m.setWords(m.fillerWords, removed: true)
                m.selection = 5.6...6.1
                m.cutSelection()
                m.apply { $0.shortenedPauses = Set(m.longPauses.indices) }
                m.applyLive("trimEnd") { $0.trimEnd = 6.75 }
                m.endLive()
                m.editing = true
            default: break
            }
            review = m
        }
        .task {
            // Screenshots: `-RodaVoiceDemo recording|locked` shows the recorder with fake input.
            guard let demo = UserDefaults.standard.string(forKey: "RodaVoiceDemo"), ["recording", "locked"].contains(demo),
                  onVoice != nil else { return }
            try? await Task.sleep(for: .seconds(2))
            recorder.demo = true
            _ = await recorder.start()
            if demo == "locked" { recorder.lock() }
            if demo == "recording" { drag = CGSize(width: -24, height: 0) }
        }
    }

    private var bar: some View {
        GlassEffectContainer(spacing: 10) {
            HStack(alignment: .bottom, spacing: 10) {
                if recorder.phase != .idle {
                    RecordingRow(recorder: recorder, dragX: drag.width, onDelete: cancelRecording, onStop: stopForReview)
                        .glassEffect(.regular, in: .rect(cornerRadius: 22, style: .continuous))
                        .transition(.opacity.combined(with: .scale(scale: 0.96, anchor: .trailing)))
                } else {
                    leading
                    TextField(placeholder, text: $text, axis: .vertical)
                        .lineLimit(1...5)
                        .focused(focused)
                        .textFieldStyle(.plain)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 12)
                        .frame(minHeight: 44)
                        .glassEffect(.regular.interactive(), in: .rect(cornerRadius: 22, style: .continuous))
                        .onSubmit(send)
                        .submitLabel(.send)
                        .accessibilityIdentifier("composer")
                }
                trailing
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 8)
        .padding(.bottom, 8)
    }

    @ViewBuilder private var leading: some View {
        #if os(iOS)
        Button {
            Haptics.selectionTick()
            withAnimation(.spring(duration: 0.45, bounce: 0.25)) { cameraOpen.toggle() }
        } label: {
            ZoenIcon(.camera, selected: cameraOpen, size: 21).frame(width: 44, height: 44)
                .foregroundStyle(Palette.textPrimary)
        }
        .buttonStyle(IconPressStyle())
        .glassEffect(.regular.interactive(), in: .circle)
        .accessibilityLabel(cameraOpen ? String(localized: "Close camera") : String(localized: "Camera"))
        #else
        Button {} label: {
            ZoenIcon(.plus, size: 19).frame(width: 44, height: 44)
        }
        .buttonStyle(.plain)
        .glassEffect(.regular.interactive(), in: .circle)
        .accessibilityLabel("Attach (soon)")
        .disabled(true)
        .opacity(0.7)
        #endif
    }

    @ViewBuilder private var trailing: some View {
        if recorder.phase == .locked {
            Button(action: finishRecording) {
                ZoenIcon(.send, size: 21).frame(width: 44, height: 44)
            }
            .buttonStyle(IconPressStyle())
            .foregroundStyle(.white)
            .glassEffect(.regular.tint(Palette.action).interactive(), in: .circle)
            .accessibilityLabel("Send voice message")
            .transition(.scale.combined(with: .opacity))
        } else if micMode || recorder.phase == .recording {
            mic
        } else {
            Button(action: send) {
                ZoenIcon(busy ? .more : .send, size: 21)
                    .frame(width: 44, height: 44)
            }
            .buttonStyle(IconPressStyle())
            .foregroundStyle(.white)
            .glassEffect(.regular.tint(Palette.action).interactive(), in: .circle)
            .disabled(empty || busy)
            .opacity(empty ? 0.55 : 1)
            .accessibilityLabel("Send")
            .accessibilityIdentifier("send")
            .keyboardShortcut(.return, modifiers: .command)
            .transition(.scale(scale: 0.6).combined(with: .opacity))
        }
    }

    /// Hold to record · slide left to cancel · slide up to lock · release to send.
    private var mic: some View {
        let recording = recorder.phase == .recording
        return ZoenIcon(.mic, selected: recording, size: 21)
            .foregroundStyle(.white)
            .frame(width: 44, height: 44)
            // While recording it's a solid red disc outside the glass (so it doesn't melt
            // into the recording row next to it).
            .background { if recording { Circle().fill(Palette.danger).shadow(color: Palette.danger.opacity(0.35), radius: 8, y: 2) } }
            .glassEffect(recording ? .identity : .regular.tint(Palette.action).interactive(), in: .circle)
            .scaleEffect(recording ? 1.3 : (pressing ? 0.92 : 1), anchor: .trailing)
            .offset(x: recording ? min(0, drag.width) * 0.35 : 0, y: recording ? min(0, drag.height) * 0.35 : 0)
            .animation(.spring(duration: 0.3, bounce: 0.3), value: recording)
            .contentShape(.circle)
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { v in
                        if !pressing {
                            pressing = true
                            startedThisPress = false
                            Haptics.prepare()
                            holdTask = Task { @MainActor in
                                try? await Task.sleep(for: .milliseconds(230))
                                guard !Task.isCancelled, pressing else { return }
                                startedThisPress = true
                                if await recorder.start() {
                                    Haptics.commit()
                                    // Released while permission was being asked.
                                    if !pressing { cancelRecording() }
                                }
                            }
                        }
                        guard recorder.phase == .recording else { return }
                        drag = v.translation
                        if v.translation.width < -110 {
                            cancelRecording()
                        } else if v.translation.height < -80 {
                            Haptics.action()
                            withAnimation(.spring(duration: 0.35, bounce: 0.25)) { recorder.lock() }
                            drag = .zero
                        }
                    }
                    .onEnded { _ in
                        pressing = false
                        holdTask?.cancel()
                        drag = .zero
                        if recorder.phase == .recording {
                            finishRecording()
                        } else if recorder.phase == .idle && !startedThisPress {
                            Haptics.tap()
                            tooltip = true
                            Task { try? await Task.sleep(for: .seconds(1.8)); tooltip = false }
                        }
                    }
            )
            .accessibilityElement()
            .accessibilityLabel(Text("Record voice message"))
            .accessibilityHint(Text("Hold to record, release to send. Double-tap to start a locked recording."))
            .accessibilityAddTraits(.isButton)
            .accessibilityAction {
                Task {
                    if recorder.phase == .idle, await recorder.start() { recorder.lock(); Haptics.commit() }
                }
            }
            .transition(.scale(scale: 0.6).combined(with: .opacity))
    }

    /// Locked → stop: review (Delete · Edit · Send) instead of sending right away.
    private func stopForReview() {
        guard let clip = recorder.finish() else { Haptics.warning(); return }
        Haptics.tap()
        review = VoiceEditorModel(clip: clip, transcript: nil)
    }

    private func cancelRecording() {
        Haptics.dismiss()
        withAnimation(.snappy) { recorder.cancel() }
        drag = .zero
    }

    private func finishRecording() {
        let clip = recorder.finish()
        drag = .zero
        if let clip, let onVoice {
            Haptics.commit()
            onVoice(clip, nil)
        } else {
            Haptics.warning()
        }
    }

    private func send() {
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, !busy else { return }
        onSend()
    }
}


/// Underline flourish under the first bubble of a chat (once).
private struct FirstBubbleFlourish: View {
    let key: String
    @State private var show = false
    var body: some View {
        Group {
            if show { InkFlourish(width: 132) }
        }
        .onAppear {
            if WowGate.once("flourish-\(key)") {
                show = true
            }
        }
    }
}
