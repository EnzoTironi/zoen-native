import SwiftUI
import RodaCore

/// Tela 2: a conversa com pessoas e agentes. O Item nasce da mensagem do agente.
struct SpaceView: View {
    @Environment(AppModel.self) private var model
    let spaceId: String
    var onOpenItem: (String) -> Void

    @State private var entries: [TimelineEntry] = []
    @State private var sharedBackdrop: (ChatBackground, PhotoBackgroundLayout) = (.none, .default)
    @State private var draft = ""
    @State private var pinnedApps: [ItemDetail] = []
    /// The floating title, plan and widgets reserve room for the initial reading position.
    @State private var chromeHeight: CGFloat = 0
    @State private var scrollPos = ScrollPosition(edge: .bottom)
    @State private var offsetY: CGFloat = 0
    /// Reading position: new messages only pull the chat down while you're at the end.
    @State private var nearBottom = true
    /// Messages that arrived while you were reading further up ("↓ N novas mensagens").
    @State private var unseen = 0
    /// First message you hadn't read when the chat opened; the divider sits above it.
    @State private var firstUnread: String?
    /// Set by `reload()` for the next scroll decision: did *I* just send something?
    @State private var appendedMine = false
    @State private var appendedOthers = 0
    #if DEBUG
    @State private var demoIncomingSent = false
    #endif
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// Top of the composer bar, in global coordinates (the message fade ends there).
    @State private var composerTop: CGFloat = .infinity
    /// Where the header (and any pinned card) ends: messages never read through it.
    @State private var chromeBottom: CGFloat = 0
    @Environment(\.dismiss) private var dismiss
    @FocusState private var composerFocused: Bool
    @State private var backgroundPicker = false
    /// A message a search result jumped to (briefly highlighted).
    @State private var highlighted: String?
    /// Drag-left on a message: the composer answers it inline (quoted).
    @State private var replyTo: TimelineEntry?
    /// The header menu (HeaderMenu): the title grows into it.
    @State private var headerMenu = false
    /// Drag-right on a message (or its "N respostas"): its thread.
    @State private var thread: ThreadRef?
    /// Long-pressed message (actions overlay), and the follow-ups it can start.
    @State private var actionTarget: TimelineEntry?
    @State private var forwardTarget: ThreadRef?
    @State private var editTarget: TimelineEntry?
    @State private var deleteTarget: TimelineEntry?
    /// Thread-only replies stay in the thread; broadcasts also appear in the chat.
    private var visible: [TimelineEntry] { ChatTimeline.visible(entries) }

    private var space: SpaceSummary? { model.space(spaceId) }
    private var workingAgent: Persona? { model.working[spaceId] }
    /// "Just for me" wins over the shared one (the latest `BackgroundSet` in the log).
    private var backgroundState: (background: ChatBackground, layout: PhotoBackgroundLayout, isLocal: Bool) {
        let store = ChatBackgroundStore.shared
        if let local = store.local(spaceId) { return (local, store.layout(for: spaceId), true) }
        return (sharedBackdrop.0, sharedBackdrop.1, false)
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
                    Color.clear.frame(height: 1).id("chat-top")
                    if let space { SpaceHeaderCard(space: space).padding(.bottom, 12) }
                    let shown = visible
                    ForEach(Array(shown.enumerated()), id: \.element.id) { idx, entry in
                        if entry.id == firstUnread {
                            UnreadDivider().id("unread-divider")
                        }
                        EntryView(spaceId: spaceId, entry: entry,
                                  previous: idx > 0 ? shown[idx - 1] : nil,
                                  next: idx + 1 < shown.count ? shown[idx + 1] : nil,
                                  isDirect: space?.kind == .direct,
                                  onOpenItem: onOpenItem,
                                  threadFaces: entry.threadReplies > 0 ? threadFaces(entry.id) : [],
                                  onSwipe: { intent in
                                      switch intent {
                                      case .reply:
                                          withAnimation(reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.8)) { replyTo = entry }
                                          composerFocused = true
                                      case .thread:
                                          thread = ThreadRef(id: entry.inThread ?? entry.id)
                                      }
                                  },
                                  onLongPress: { actionTarget = entry },
                                  onJump: { jump(to: $0, proxy: proxy) })
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
                            .transition(reduceMotion ? .opacity : .asymmetric(insertion: .scale(scale: 0.92, anchor: .bottom).combined(with: .opacity), removal: .opacity))
                    }
                    if model.browser.session?.spaceId == spaceId {
                        AgentBrowserCard()
                            .id("agent-browser")
                            .transition(reduceMotion ? .opacity : .scale(scale: 0.94, anchor: .bottom).combined(with: .opacity))
                    }
                    if let agent = workingAgent {
                        WorkingRow(agent: agent, label: model.planner.availability == .onDevice ? String(localized: "on device") : String(localized: "local planner"))
                            .id("working")
                            .transition(reduceMotion ? .opacity : .asymmetric(insertion: .scale(scale: 0.6, anchor: .bottomLeading).combined(with: .opacity),
                                                    removal: .scale(scale: 0.85, anchor: .leading).combined(with: .opacity)))
                    }
                    Color.clear.frame(height: 8).id("bottom")
                }
                .padding(.horizontal, 14)
                .padding(.top, 8)
            }
            .accessibilityIdentifier("chat-message-scroll")
            // Screenshot journeys can start at the top; ordinary chats keep their reading position.
            .defaultScrollAnchor(UserDefaults.standard.bool(forKey: "RodaChatScrollTop") ? .top : .bottom)
            .scrollEdgeEffectHidden(true, for: .top)
            .scrollPosition($scrollPos)
            .onScrollGeometryChange(for: CGFloat.self) { $0.contentOffset.y } action: { _, y in offsetY = y }
            .onScrollGeometryChange(for: Bool.self) { g in
                g.visibleRect.maxY >= g.contentSize.height - 140
            } action: { _, near in
                nearBottom = near
                if near && unseen > 0 { withAnimation(reduceMotion ? nil : .smooth(duration: 0.3)) { unseen = 0 } }
            }
            // History scrolls behind the transparent header; its starting position clears the widgets.
            .safeAreaPadding(.top, chromeHeight + 8)
            .scrollDismissesKeyboard(.interactively)
            .environment(\.chatBackdrop, !background.isNone)
            // Messages fade out just above the composer instead of ghosting through its
            // glass; the chat background (drawn below this mask) stays whole.
            .mask { ChatFadeMask(composerTop: composerTop, chromeBottom: chromeBottom) }
            .background(ChatBackdropView(background: background, spaceId: spaceId, layout: backgroundState.layout).ignoresSafeArea())
            // A bar, not an inset: iOS 26 fades and blurs what scrolls under the composer,
            // so message text doesn't ghost through its glass.
            #if os(iOS)
            .scrollEdgeEffectStyle(.soft, for: .bottom)
            #endif
            .safeAreaBar(edge: .bottom, spacing: 0) {
              VStack(spacing: 6) {
                if unseen > 0 {
                    NewMessagesPill(count: unseen) {
                        Haptics.selectionTick()
                        withAnimation(reduceMotion ? nil : .smooth(duration: 0.45)) {
                            proxy.scrollTo("bottom", anchor: .bottom)
                        }
                        withAnimation(reduceMotion ? nil : .smooth(duration: 0.3)) { unseen = 0 }
                    }
                    .transition(reduceMotion ? .opacity : .scale(scale: 0.85, anchor: .bottom).combined(with: .opacity))
                }
                if let r = replyTo {
                    ReplyComposerBar(entry: r) { withAnimation(reduceMotion ? nil : .spring(response: 0.3, dampingFraction: 0.85)) { replyTo = nil } }
                        .transition(reduceMotion ? .opacity : .move(edge: .bottom).combined(with: .opacity))
                }
                Composer(text: $draft,
                         placeholder: space?.counterpart?.kind == .agent ? String(localized: "What do you want to get done?") : String(localized: "Message"),
                         focused: $composerFocused,
                         busy: workingAgent != nil,
                         attachmentSpaceId: spaceId,
                         onVoice: voiceHandler) {
                    let text = draft
                    draft = ""
                    model.sync.stoppedTyping(spaceId)
                    if let r = replyTo {
                        withAnimation(reduceMotion ? nil : .spring(response: 0.3, dampingFraction: 0.85)) { replyTo = nil }
                        model.sendReply(text, to: r.id, thread: false, in: spaceId)
                    } else {
                        Task { await model.send(text, in: spaceId) }
                    }
                }
              }
              .animation(reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.85), value: replyTo?.id)
              .animation(reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.8), value: unseen > 0)
              .onGeometryChange(for: CGFloat.self) { $0.frame(in: .global).minY } action: { composerTop = $0 }
            }
            #if os(iOS)
            .fullScreenCover(isPresented: Bindable(model.browser).screenOpen) { LiveViewScreen() }
            #else
            .sheet(isPresented: Bindable(model.browser).screenOpen) { LiveViewScreen().frame(minWidth: 420, minHeight: 720) }
            #endif
            .overlay {
                if let target = actionTarget {
                    MessageActionsOverlay(
                        spaceId: spaceId, entry: target,
                        onReply: {
                            withAnimation(reduceMotion ? nil : Motion.wabi) { replyTo = target }
                            composerFocused = true
                        },
                        onForward: { forwardTarget = ThreadRef(id: target.id) },
                        onEdit: {
                            editTarget = target
                        },
                        onDelete: { deleteTarget = target },
                        onClose: { actionTarget = nil })
                }
            }
            .sheet(item: $forwardTarget) { t in
                ForwardSheet(fromSpaceId: spaceId, messageId: t.id).presentationDetents([.medium, .large])
            }
            .sheet(item: $editTarget) { target in
                if case .message(let text, _) = target.kind {
                    MessageMutationSheet(spaceId: spaceId, messageId: target.id, text: text)
                }
            }
            .sheet(item: $deleteTarget) { target in
                MessageMutationSheet(spaceId: spaceId, messageId: target.id, deleting: true)
            }
            .threadPanel(item: $thread) { t in ThreadSheet(spaceId: spaceId, rootId: t.id, focusedEntryId: t.entryId) }
            .onChange(of: draft) { _, text in model.sync.typingChanged(spaceId, text: text) }
            // The agent's browser card shows up (or asks for you): bring it into view.
            .onChange(of: model.browser.phase) { _, _ in
                guard model.browser.session?.spaceId == spaceId else { return }
                followOrCount(1, proxy: proxy)
            }
            .onChange(of: model.browser.session?.spaceId) { _, id in
                guard id == spaceId else { return }
                followOrCount(1, proxy: proxy)
            }
            #if DEBUG
            .overlay(alignment: .topTrailing) {
                if SyncModel.mode == .demo,
                   UserDefaults.standard.bool(forKey: "RodaIncomingManual"),
                   !demoIncomingSent {
                    Button("Receive demo messages") {
                        demoIncomingSent = true
                        Task { await model.receiveIncomingStoryMessages() }
                    }
                    .buttonStyle(.wabiGlassProminent)
                    .accessibilityIdentifier("demo-incoming-trigger")
                    .padding(.top, chromeHeight + 8)
                    .padding(.trailing, 14)
                }
            }
            #endif
            .onDisappear { model.sync.stoppedTyping(spaceId) }
            .onChange(of: visible.map(\.id)) { old, _ in
                // Screenshot journeys can open at the beginning of message history.
                if UserDefaults.standard.bool(forKey: "RodaChatScrollTop") {
                    if old.isEmpty {
                        Task { @MainActor in
                            try? await Task.sleep(for: .milliseconds(80))
                            proxy.scrollTo("chat-top", anchor: .top)
                            scrollPos.scrollTo(edge: .top)
                        }
                    }
                    return
                }
                if old.isEmpty {
                    // Primeira carga: sem animação, direto na primeira mensagem não lida
                    // (ou no fim, quando está tudo lido).
                    let target = firstUnread == nil ? "bottom" : "unread-divider"
                    let anchor: UnitPoint = firstUnread == nil ? .bottom : .top
                    proxy.scrollTo(target, anchor: anchor)
                    Task { @MainActor in
                        try? await Task.sleep(for: .milliseconds(60))
                        proxy.scrollTo(target, anchor: anchor)
                    }
                } else if appendedMine {
                    // I just sent it: always follow, even from further up.
                    withAnimation(reduceMotion ? nil : .snappy) { proxy.scrollTo("bottom", anchor: .bottom) }
                    unseen = 0
                } else if appendedOthers > 0 {
                    followOrCount(appendedOthers, proxy: proxy)
                }
            }
            .task(id: "\(model.jumpTarget?.space ?? "")|\(model.jumpTarget?.entry ?? "")|\(entries.contains { $0.id == model.jumpTarget?.entry })") {
                // The target can arrive after the initial history. Hidden replies open
                // their thread instead of scrolling to an absent chat anchor.
                guard let j = model.jumpTarget, j.space == spaceId,
                      ChatTimeline.location(of: j.entry, in: entries) != nil else { return }
                do { try await Task.sleep(for: .milliseconds(450)) } catch { return }
                jump(to: j.entry, proxy: proxy)
                Haptics.selectionTick()
                model.jumpTarget = nil
            }
            .onChange(of: workingAgent?.id) { _, id in
                // Only follow the "working" row while you're at the end; never yank you down.
                guard nearBottom, id != nil else { return }
                withAnimation(reduceMotion ? nil : .snappy) { proxy.scrollTo("bottom", anchor: .bottom) }
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
            // Screenshot journeys choose an initial reading position after the story loads.
            let wait = delay > 0 ? delay : (top ? 1.2 : 9)
            try? await Task.sleep(for: .seconds(wait))
            withAnimation(reduceMotion ? nil : .smooth(duration: 0.45)) {
                if top { scrollPos.scrollTo(edge: .top) } else { scrollPos.scrollTo(y: max(0, offsetY - up)) }
            }
        }
        .task {
            // Screenshots: `-RodaVoiceSeed YES` drops one voice note (synthetic audio) in.
            if let d = UserDefaults.standard.string(forKey: "RodaDraft") { draft = d }
            guard UserDefaults.standard.bool(forKey: "RodaVoiceSeed") else { return }
            let wait = UserDefaults.standard.double(forKey: "RodaVoiceSeedDelay")
            try? await Task.sleep(for: .seconds(wait > 0 ? wait : 1.5))
            let has = entries.contains { e in
                if e.voice != nil { return true }
                if case .message(let t, _) = e.kind { return VoiceNoteRef.parse(t) != nil }
                return false
            }
            guard !has, let clip = VoiceStore.makeDemoClip() else { return }
            let ref = VoiceNoteRef(id: clip.id, ms: clip.ms, levels: clip.levels,
                                   transcript: String(localized: "Leaving at eight! I'll bring snacks and the good thermos, can someone grab the map?"))
            await model.sendVoice(clip, in: spaceId, transcript: ref.transcript)
        }
        .onAppear {
            // Capture the opening unread boundary before markRead refreshes its count to zero.
            reload()
            model.perform { try model.core.markRead(spaceId: spaceId) }
        }
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
        #endif
        .overlay {
            if headerMenu {
                // Everything under the menu dims; a tap anywhere folds it back.
                Color.black.opacity(0.18)
                    .ignoresSafeArea()
                    .onTapGesture { setHeaderMenu(false) }
                    .transition(.opacity)
                    .accessibilityHidden(true)
            }
        }
        .overlay(alignment: .top) {
            VStack(spacing: 8) {
                if let space {
                    ChatTopBar(space: space, subtitle: subtitle(space), status: status(space),
                               onBack: { _ = model.pop() },
                               showsBack: showsBack,
                               onOpen: {
                                   setHeaderMenu(false)
                                   if let who = space.counterpart { model.openProfile(who.id) } else { model.go(.participants(spaceId)) }
                               },
                               onTitle: { setHeaderMenu(!headerMenu) },
                               menuOpen: headerMenu)
                    if headerMenu {
                        HeaderMenuPanel(space: space, about: subtitle(space).isEmpty ? space.title : "\(space.title) · \(subtitle(space))") { pick in
                            headerPick(pick)
                        }
                        // Grows out of the title (not from nothing), folds back a touch quicker.
                        .transition(reduceMotion ? .opacity : .asymmetric(
                            insertion: .scale(scale: 0.88, anchor: .top).combined(with: .offset(y: -12)).combined(with: .opacity),
                            removal: .scale(scale: 0.95, anchor: .top).combined(with: .opacity)))
                    }
                }
                pinnedAppStrip
            }
            .padding(.top, headerTopPadding)
            .padding(.bottom, 6)
            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { if !headerMenu { chromeHeight = $0 } }
            .onGeometryChange(for: CGFloat.self) { $0.frame(in: .global).maxY } action: { if !headerMenu { chromeBottom = $0 } }
        }
    }

    private var headerTopPadding: CGFloat {
        #if os(macOS)
        return 10
        #else
        return 2
        #endif
    }

    private var showsBack: Bool {
        #if os(iOS)
        return true
        #else
        return !model.path(.conversations).isEmpty
        #endif
    }

    @ViewBuilder
    private var pinnedAppStrip: some View {
        if !pinnedApps.isEmpty {
            ChatPinStrip(apps: pinnedApps, onOpenItem: onOpenItem)
                .padding(.vertical, 8)
                .clipped()
                .transition(.opacity)
        }
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

    private func setHeaderMenu(_ open: Bool) {
        guard open != headerMenu else { return }
        open ? Haptics.menuOpen() : Haptics.menuClose()
        let anim: Animation? = reduceMotion ? nil
            : open ? .spring(response: 0.38, dampingFraction: 0.84) : .spring(response: 0.26, dampingFraction: 0.95)
        withAnimation(anim) { headerMenu = open }
    }

    private func headerPick(_ pick: HeaderMenuPanel.Pick) {
        setHeaderMenu(false)
        switch pick {
        case .members, .agents, .settings: model.go(.participants(spaceId))
        case .files, .pages: model.go(.folder(spaceId))
        case .mute:
            let muted = model.toggleMute(spaceId)
            model.show(.init(kind: .info, text: muted ? String(localized: "Muted on this device.") : String(localized: "Notifications on.")))
        }
    }

    /// Who answered in a message's thread (newest first, each once).
    private func threadFaces(_ root: String) -> [Persona] {
        var seen = Set<String>(), out: [Persona] = []
        for e in entries.reversed() where e.inThread == root && seen.insert(e.author.id).inserted { out.append(e.author) }
        return out
    }

    /// At the end: follow new content. Reading further up: keep your place and count it.
    private func followOrCount(_ n: Int, proxy: ScrollViewProxy) {
        if nearBottom {
            withAnimation(reduceMotion ? nil : .snappy) { proxy.scrollTo("bottom", anchor: .bottom) }
        } else {
            withAnimation(reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.8)) { unseen += n }
            Haptics.selectionTick()
            AccessibilityNotification.Announcement(NewMessagesPill.label(unseen)).post()
        }
    }

    private func jump(to id: String, proxy: ScrollViewProxy) {
        guard let location = ChatTimeline.location(of: id, in: entries) else { return }
        switch location {
        case .thread(let root, let entry):
            thread = ThreadRef(id: root, entryId: entry)
        case .chat(let entry):
            withAnimation(reduceMotion ? nil : .smooth(duration: 0.45)) { proxy.scrollTo(entry, anchor: .center) }
            withAnimation(.easeOut(duration: 0.25)) { highlighted = entry }
            Task { @MainActor in
                try? await Task.sleep(for: .seconds(1.8))
                withAnimation(.easeOut(duration: 0.6)) { highlighted = nil }
            }
        }
    }

    private func reload() {
        let new = (try? model.core.timeline(spaceId: spaceId)) ?? []
        let shared = model.sharedBackgroundState(spaceId)
        if shared.0 != sharedBackdrop.0 || shared.1 != sharedBackdrop.1 { sharedBackdrop = shared }
        let plan = model.items.first {
            $0.spaceId == spaceId && $0.plan != nil && !model.chatAppsUnpinned.contains($0.id)
        }
        // Live mini-apps of this chat, newest first (the hike leads when there is one).
        let liveApps = model.liveApps.filter { $0.spaceId == spaceId && $0.app != nil && !model.chatAppsUnpinned.contains($0.id) }
            .sorted { ($0.app?.appId == "hike" ? 1 : 0, $0.versions.first?.atMs ?? 0) > ($1.app?.appId == "hike" ? 1 : 0, $1.versions.first?.atMs ?? 0) }
        let apps = (plan.map { [$0] } ?? []) + liveApps.filter { $0.id != plan?.id }
        let firstLoad = entries.isEmpty
        if apps != pinnedApps {
            if firstLoad || apps.map(\.id) == pinnedApps.map(\.id) {
                pinnedApps = apps
            } else {
                withAnimation(reduceMotion ? nil : .spring(duration: 0.5, bounce: 0.2)) { pinnedApps = apps }
            }
        }
        // Another chat changed (or nothing did): leave this one alone, no re-render, no motion.
        guard new != entries else { return }
        if firstLoad {
            // Opening the chat: remember where unread starts, then show everything at once.
            // Match the core's chat count before taking its unread suffix: hidden
            // replies do not consume that count, while broadcast replies do.
            firstUnread = ChatTimeline.firstUnread(in: new, unread: space?.unread ?? 0)
            appendedMine = false
            appendedOthers = 0
            var t = Transaction()
            t.disablesAnimations = true
            withTransaction(t) { entries = new }
        } else {
            let added = ChatTimeline.added(in: new, after: entries)
            appendedMine = added.contains { $0.author.isMe }
            appendedOthers = added.filter { !$0.author.isMe }.count
            if added.isEmpty {
                // Edits, delivery ticks, reactions: update in place, nothing flies in.
                entries = new
            } else {
                // Only the new bubbles animate in (their own insertion transition).
                withAnimation(reduceMotion ? nil : .spring(duration: 0.4, bounce: 0.15)) { entries = new }
            }
        }
        if !new.isEmpty { try? model.core.markRead(spaceId: spaceId) }
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
    let spaceId: String
    let entry: TimelineEntry
    let previous: TimelineEntry?
    var next: TimelineEntry? = nil
    var isDirect: Bool
    var onOpenItem: (String) -> Void
    var threadFaces: [Persona] = []
    /// Drag left (reply) / right (thread); nil = no message gestures here.
    var onSwipe: ((MessageSwipeIntent) -> Void)? = nil
    /// Long-press a message: its actions and reactions.
    var onLongPress: (() -> Void)? = nil
    /// Tap a quote: scroll to the message it answers.
    var onJump: ((String) -> Void)? = nil
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// Same author within 5 minutes → continuation (no name).
    private var grouped: Bool {
        guard let previous, previous.author.id == entry.author.id else { return false }
        guard case .message = previous.kind else { return false }
        return entry.atMs - previous.atMs < 5 * 60_000
    }

    /// Last bubble in a run: avatar sits here (WhatsApp-style).
    private var endsRun: Bool {
        guard let next, next.author.id == entry.author.id else { return true }
        guard case .message = next.kind else { return true }
        return next.atMs - entry.atMs >= 5 * 60_000
    }

    var body: some View {
        switch entry.kind {
        case .background(let dto):
            BackgroundChangeRow(author: entry.author, background: ChatBackground(dto: dto),
                                spaceId: entry.id, layout: PhotoBackgroundLayout(dto: dto))
        case .message(let text, _) where model.ownerAgentNote(entry, text: text) != nil:
            OwnerAgentNoteCard(agent: entry.author, note: model.ownerAgentNote(entry, text: text)!,
                               fresh: Date().timeIntervalSince1970 * 1000 - Double(entry.atMs) < 15_000)
                .padding(.top, 10)
                .padding(.leading, 44)
                .frame(maxWidth: .infinity, alignment: .leading)
        case .message where entry.deleted:
            DeletedBubble(mine: entry.author.isMe)
                .frame(maxWidth: .infinity, alignment: entry.author.isMe ? .trailing : .leading)
                .padding(.top, grouped ? 0 : 10)
        case .message(let text, let card):
            VStack(alignment: entry.author.isMe ? .trailing : .leading, spacing: 3) {
                if entry.forwarded {
                    Label("Forwarded", systemImage: "arrowshape.turn.up.right")
                        .font(.caption2.italic())
                        .foregroundStyle(Palette.textTertiary)
                        .padding(.leading, entry.author.isMe ? 0 : 44)
                }
                MessageRow(author: entry.author, text: text, card: card, voice: entry.voice.map(VoiceNoteRef.init), atMs: entry.atMs, delivery: entry.delivery,
                           grouped: grouped && entry.replyTo == nil, endsRun: endsRun, isDirect: isDirect, onOpenItem: onOpenItem,
                           quote: entry.replyTo, onQuote: { onJump?($0) })
                    .modifier(MessageSwipe(enabled: onSwipe != nil, onActions: onLongPress) { onSwipe?($0) })
                    .onLongPressGesture(minimumDuration: 0.35) { onLongPress?() }
                if !entry.reactions.isEmpty {
                    ReactionChips(spaceId: spaceId, entry: entry)
                        .padding(.leading, entry.author.isMe ? 0 : 44)
                        .padding(.top, -8)
                }
                if entry.edited {
                    Text("Edited").font(.caption2).foregroundStyle(Palette.textTertiary)
                        .padding(.leading, entry.author.isMe ? 0 : 44)
                }
                if entry.threadReplies > 0 {
                    ThreadRepliesChip(count: entry.threadReplies, faces: entry.threadParticipants.isEmpty ? threadFaces : entry.threadParticipants,
                                      lastReplyMs: entry.threadLastReplyMs, unread: entry.threadFollowing ? entry.threadUnread : 0) { onSwipe?(.thread) }
                        .padding(.leading, entry.author.isMe ? 0 : 44)
                        .transition(reduceMotion ? .opacity : .scale(scale: 0.6, anchor: .topLeading).combined(with: .opacity))
                }
                if entry.author.isMe, entry.delivery == .failed { DeliveryMark(delivery: entry.delivery) }
            }
            .frame(maxWidth: .infinity, alignment: entry.author.isMe ? .trailing : .leading)
            .padding(.top, grouped && entry.replyTo == nil ? 0 : 12)
        case .itemEdited(let itemId, _, _, _, _) where model.miniAppItemIds.contains(itemId):
            // Mini-apps: cada toque atualiza o widget ao vivo (ponto vermelho e coração no
            // cartão); o histórico fica na folha, não enche a conversa.
            EmptyView()
        case .itemEdited(let itemId, _, let version, let note, let isUndo):
            Button { onOpenItem(itemId) } label: {
                SystemRow(glyph: isUndo ? .back : .list, text: "\(entry.author.isMe ? "Você" : entry.author.name) · \(note)", trailing: "v\(version)")
            }
            .buttonStyle(.plain)
            .padding(.vertical, 6)
        case .request(let requestId, let title, let approved):
            if let r = model.requests.first(where: { $0.id == requestId && $0.spaceId == spaceId }) {
                InlineRequestRow(request: r).padding(.vertical, 6)
            } else if !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                RequestHistoryRow(author: entry.author, title: title, approved: approved)
                    .padding(.vertical, 6)
                    .accessibilityIdentifier("request-history-\(entry.id)")
            }
        case .system(let text):
            SystemRow(glyph: text.hasPrefix(String(localized: "Approved")) ? .check : .close, text: text, trailing: nil)
                .padding(.vertical, 6)
        }
    }
}

/// Time and delivery tick inside a bubble's bottom-right corner (WhatsApp). Sending is a
/// clock, sent one tick; the receipts work adds the double tick and read colour here.
struct BubbleMeta: View {
    /// Hark: bubbles radius 24.
    static let radius: CGFloat = 24
    let atMs: Int64
    let delivery: Delivery?
    let mine: Bool

    var body: some View {
        HStack(spacing: 3) {
            Text(RodaTime.clock(atMs))
            if let delivery {
                switch delivery {
                case .sending: Image(systemName: "clock").imageScale(.small)
                case .failed: Image(systemName: "exclamationmark.circle.fill").foregroundStyle(Palette.danger)
                // ✓ sent, ✓✓ delivered, blue ✓✓ read (aggregated "read up to").
                default: ReceiptTicks(delivery: delivery, base: Palette.myBubbleText.opacity(0.62))
                }
            }
        }
        .font(.caption2.monospacedDigit())
        .foregroundStyle(mine ? Palette.myBubbleText.opacity(0.62) : Palette.textTertiary)
        .contentTransition(.symbolEffect(.replace))
        .animation(.timingCurve(0.25, 1, 0.5, 1, duration: 0.22), value: delivery)
        .accessibilityHidden(true)
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

/// ✓ / ✓✓ / read ✓✓: the second tick draws in and the pair turns blue with the Wabi curve.
struct ReceiptTicks: View {
    let delivery: Delivery
    /// Unread ticks: muted on my bubble by default; the chat list passes its own grey.
    var base: Color = Palette.myBubbleText.opacity(0.62)

    private var double: Bool { delivery == .delivered || delivery == .read }

    var body: some View {
        switch delivery {
        case .sending, .local:
            Image(systemName: "clock").font(.system(size: 10, weight: .semibold))
                .accessibilityLabel("Sending")
        case .failed:
            Image(systemName: "exclamationmark.circle.fill").font(.system(size: 11, weight: .semibold))
                .foregroundStyle(Palette.danger)
                .accessibilityLabel("Not delivered")
        default:
            ZStack(alignment: .leading) {
                Image(systemName: "checkmark")
                Image(systemName: "checkmark")
                    .offset(x: 5)
                    .opacity(double ? 1 : 0)
                    .scaleEffect(double ? 1 : 0.4, anchor: .leading)
            }
            .font(.system(size: 10, weight: .bold))
            .padding(.trailing, 5)
            .foregroundStyle(delivery == .read ? Palette.receiptRead : base)
            .accessibilityLabel(delivery == .read ? "Read" : double ? "Delivered" : "Sent")
        }
    }
}

enum Motion {
    /// Wabi's ease: cubic-bezier(.25, 1, .5, 1), standard length.
    static let wabi = Animation.timingCurve(0.25, 1, 0.5, 1, duration: 0.28)
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
    /// A voice note sent as an audio file (older ones arrive as a text marker in `text`).
    var voice: VoiceNoteRef? = nil
    let atMs: Int64
    var delivery: Delivery = .sent
    let grouped: Bool
    /// Avatar aligns to the last bubble of a run (WhatsApp-style).
    var endsRun: Bool = true
    let isDirect: Bool
    var onOpenItem: (String) -> Void
    /// Inline reply: the message this one answers, quoted at the top of the bubble.
    var quote: ReplyQuote? = nil
    var onQuote: ((String) -> Void)? = nil
    @Environment(\.chatBackdrop) private var backdrop
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

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
                    }
                    .padding(.horizontal, backdrop ? 7 : 0)
                    .padding(.vertical, backdrop ? 2 : 0)
                    .background { if backdrop { Capsule().fill(.regularMaterial) } }
                    .padding(.leading, 2)
                }
                if let voice = voice ?? VoiceNoteRef.parse(text) {
                    VoiceBubble(ref: voice, mine: mine, grouped: grouped)
                        .alignmentGuide(.messageFace) { $0[.bottom] }
                } else if !text.isEmpty {
                    if author.kind == .agent && !mine, let plan = Itinerary(text) {
                        ItineraryCard(itinerary: plan)
                    } else {
                        // Me in black (4 pt corner bottom right); everyone else, agents
                        // included, in the same grey bubble (tight corner bottom left).
                        VStack(alignment: .leading, spacing: 6) {
                            if let quote {
                                if let onQuote {
                                    Button { onQuote(quote.id) } label: {
                                        ReplyQuoteView(quote: quote, mine: mine)
                                    }
                                    .buttonStyle(.plain)
                                } else {
                                    ReplyQuoteView(quote: quote, mine: mine)
                                }
                            }
                            BubbleText(text: Text(attributed(text))) {
                                BubbleMeta(atMs: atMs, delivery: mine ? delivery : nil, mine: mine)
                            }
                                .font(.body)
                                .foregroundStyle(mine ? Palette.myBubbleText : Palette.textPrimary)
                        }
                            .padding(.horizontal, quote == nil ? 14 : 10)
                            .padding(.top, quote == nil ? 9 : 8)
                            .padding(.bottom, 9)
                            .background {
                                if mine {
                                    UnevenRoundedRectangle(topLeadingRadius: BubbleMeta.radius, bottomLeadingRadius: BubbleMeta.radius, bottomTrailingRadius: grouped ? BubbleMeta.radius : 6, topTrailingRadius: BubbleMeta.radius, style: .continuous)
                                        .fill(Palette.myBubble)
                                } else {
                                    UnevenRoundedRectangle(topLeadingRadius: BubbleMeta.radius, bottomLeadingRadius: grouped ? BubbleMeta.radius : 6, bottomTrailingRadius: BubbleMeta.radius, topTrailingRadius: BubbleMeta.radius, style: .continuous)
                                        .fill(backdrop ? Palette.otherBubbleOnBackdrop : Palette.otherBubble)
                                }
                            }
                            .shadow(color: .black.opacity(backdrop ? 0.1 : 0), radius: 1.5, y: 0.5)
                            .alignmentGuide(.messageFace) { $0[.bottom] }
                        if mine && !grouped {
                            FirstBubbleFlourish(key: "\(author.id)-\(atMs)")
                        }
                    }
                }
                if let card, let app = card.app, app.appId == "hike" {
                    HikeChatCard(card: card, app: app)
                        .transition(reduceMotion ? .opacity : .scale(scale: 0.85, anchor: .topLeading).combined(with: .opacity))
                } else if let card, let app = card.app {
                    AppOutputBlock(card: card, app: app)
                        .transition(reduceMotion ? .opacity : .scale(scale: 0.85, anchor: .topLeading).combined(with: .opacity))
                } else if let card {
                    Button { onOpenItem(card.itemId) } label: { ItemCardView(card: card) }
                        .buttonStyle(.plain)
                        .transition(reduceMotion ? .opacity : .scale(scale: 0.85, anchor: .topLeading).combined(with: .opacity))
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
                Button { model.approve(request) } label: {
                    Text("Approve").foregroundStyle(ActionGlassForeground())
                }
                    .buttonStyle(.wabiGlassProminent)
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

private struct RequestHistoryRow: View {
    let author: Persona
    let title: String
    let approved: Bool?

    var body: some View {
        HStack(spacing: 10) {
            ContactAvatar(persona: author, size: 26)
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.subheadline.weight(.semibold))
                    .fixedSize(horizontal: false, vertical: true)
                Group {
                    if let approved {
                        Label(approved ? String(localized: "Approved") : String(localized: "Declined"),
                              systemImage: approved ? "checkmark.circle" : "xmark.circle")
                    } else {
                        Label("Awaiting decision", systemImage: "clock")
                    }
                }
                .font(.caption).foregroundStyle(Palette.textSecondary)
            }
            Spacer(minLength: 4)
        }
        .padding(.horizontal, 12).padding(.vertical, 10)
        .background(Palette.surface, in: .rect(cornerRadius: 16, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(Palette.hairline, lineWidth: 0.8))
        .padding(.leading, 38)
        .accessibilityElement(children: .combine)
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
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var body: some View {
        TimelineView(.animation(paused: ambientPaused || reduceMotion)) { ctx in
            let t = ctx.date.timeIntervalSinceReferenceDate
            HStack(spacing: 4) {
                ForEach(0..<3, id: \.self) { i in
                    let phase = reduceMotion ? 0.5 : (sin((t * 2 * .pi / 1.1) - Double(i) * 0.9) + 1) / 2
                    Circle().fill(Palette.textSecondary)
                        .frame(width: 7, height: 7)
                        .opacity(0.35 + 0.65 * phase)
                        .offset(y: reduceMotion ? 0 : -2.5 * phase)
                }
            }
        }
    }
}

// MARK: - Composer (cápsula de vidro)

struct Composer: View {
    static let height: CGFloat = 48
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Binding var text: String
    var placeholder: String
    var focused: FocusState<Bool>.Binding
    var busy: Bool
    var attachmentSpaceId: String? = nil
    /// A finished voice note (iOS) and its transcript if already known (the editor sends
    /// the kept words). Nil hides the mic.
    var onVoice: ((VoiceClip, String?) -> Void)? = nil
    var onSend: () -> Void
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
        bar
        .overlay(alignment: .topTrailing) {
            if tooltip {
                Text("Hold to record, release to send")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(Palette.textPrimary)
                    .padding(.horizontal, 12).padding(.vertical, 8)
                    .glassEffect(.regular, in: .capsule)
                    .offset(x: -12, y: -38)
                    .transition(reduceMotion ? .opacity : .scale(scale: 0.8, anchor: .bottomTrailing).combined(with: .opacity))
            }
        }
        .overlay(alignment: .bottomTrailing) {
            if recorder.phase == .recording {
                LockHint(dragY: drag.height)
                    .padding(.trailing, 14)
                    .padding(.bottom, 66)
                    .transition(reduceMotion ? .opacity : .move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(reduceMotion ? nil : .spring(duration: 0.35, bounce: 0.25), value: recorder.phase)
        .animation(reduceMotion ? nil : .snappy, value: tooltip)
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
                        .glassEffect(.regular, in: .rect(cornerRadius: 24, style: .continuous))
                        .transition(reduceMotion ? .opacity : .opacity.combined(with: .scale(scale: 0.96, anchor: .trailing)))
                } else {
                    leading
                    TextField(placeholder, text: $text, axis: .vertical)
                        .lineLimit(1...5)
                        .focused(focused)
                        .textFieldStyle(.plain)
                        // Hark: 15/19 text in a field at least 48 tall, radius 24.
                        .font(.system(size: 15))
                        .lineSpacing(4)
                        .padding(.horizontal, 18)
                        .padding(.vertical, 14)
                        .frame(minHeight: Composer.height)
                        .glassEffect(.regular.interactive(), in: .rect(cornerRadius: 24, style: .continuous))
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
        if let attachmentSpaceId {
            ChatAttachmentButton(spaceId: attachmentSpaceId)
        }
    }

    @ViewBuilder private var trailing: some View {
        if recorder.phase == .locked {
            Button(action: finishRecording) {
                ZoenIcon(.send, size: 21).frame(width: Composer.height, height: Composer.height)
            }
            .buttonStyle(IconPressStyle())
            .foregroundStyle(ActionGlassForeground())
            .glassEffect(.regular.tint(Palette.action).interactive(), in: .circle)
            .accessibilityLabel("Send voice message")
            .transition(reduceMotion ? .opacity : .scale.combined(with: .opacity))
        } else if micMode || recorder.phase == .recording {
            mic
        } else {
            Button(action: send) {
                ZoenIcon(busy ? .more : .send, size: 21)
                    .frame(width: Composer.height, height: Composer.height)
            }
            .buttonStyle(IconPressStyle())
            .foregroundStyle(ActionGlassForeground())
            .glassEffect(.regular.tint(Palette.action).interactive(), in: .circle)
            .disabled(empty || busy)
            .opacity(empty ? 0.55 : 1)
            .accessibilityLabel("Send")
            .accessibilityIdentifier("send")
            .keyboardShortcut(.return, modifiers: .command)
            .transition(reduceMotion ? .opacity : .scale(scale: 0.6).combined(with: .opacity))
        }
    }

    /// Hold to record · slide left to cancel · slide up to lock · release to send.
    private var mic: some View {
        let recording = recorder.phase == .recording
        return ZoenIcon(.mic, selected: recording, size: 21)
            .foregroundStyle(recording ? AnyShapeStyle(Color.white) : AnyShapeStyle(ActionGlassForeground()))
            .frame(width: Composer.height, height: Composer.height)
            // While recording it's a solid red disc outside the glass (so it doesn't melt
            // into the recording row next to it).
            .background { if recording { Circle().fill(Palette.danger).shadow(color: Palette.danger.opacity(0.35), radius: 8, y: 2) } }
            .glassEffect(recording ? .identity : .regular.tint(Palette.action).interactive(), in: .circle)
            .scaleEffect(reduceMotion ? 1 : (recording ? 1.3 : (pressing ? 0.92 : 1)), anchor: .trailing)
            .offset(x: recording && !reduceMotion ? min(0, drag.width) * 0.35 : 0, y: recording && !reduceMotion ? min(0, drag.height) * 0.35 : 0)
            .animation(reduceMotion ? nil : .spring(duration: 0.3, bounce: 0.3), value: recording)
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
                            withAnimation(reduceMotion ? nil : .spring(duration: 0.35, bounce: 0.25)) { recorder.lock() }
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
            .transition(reduceMotion ? .opacity : .scale(scale: 0.6).combined(with: .opacity))
    }

    /// Locked → stop: review (Delete · Edit · Send) instead of sending right away.
    private func stopForReview() {
        guard let clip = recorder.finish() else { Haptics.warning(); return }
        Haptics.tap()
        review = VoiceEditorModel(clip: clip, transcript: nil)
    }

    private func cancelRecording() {
        Haptics.dismiss()
        withAnimation(reduceMotion ? nil : .snappy) { recorder.cancel() }
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

/// "Mensagens não lidas": where you stopped reading when the chat opened.
struct UnreadDivider: View {
    var body: some View {
        HStack(spacing: 10) {
            Rectangle().fill(Palette.action.opacity(0.35)).frame(height: 1)
            Text("Unread messages")
                .font(.caption.weight(.semibold))
                .foregroundStyle(Palette.action)
                .fixedSize()
            Rectangle().fill(Palette.action.opacity(0.35)).frame(height: 1)
        }
        .padding(.vertical, 10)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("unread-divider")
    }
}

/// Glass capsule above the composer while new messages wait below you.
struct NewMessagesPill: View {
    let count: Int
    var onTap: () -> Void

    static func label(_ n: Int) -> String {
        n == 1 ? String(localized: "1 new message") : String(localized: "\(n) new messages")
    }

    var body: some View {
        Button(action: onTap) {
            HStack(spacing: 6) {
                Image(systemName: "arrow.down").font(.caption.weight(.bold))
                Text(Self.label(count)).font(.subheadline.weight(.semibold)).monospacedDigit()
                    .contentTransition(.numericText(value: Double(count)))
            }
            .foregroundStyle(Palette.textPrimary)
            .padding(.horizontal, 14)
            .padding(.vertical, 8)
            .glassEffect(.regular.interactive(), in: .capsule)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("new-messages-pill")
        .accessibilityHint(Text("Jumps to the latest message"))
    }
}

/// Hides messages under the header/pinned card and fades them out above the composer.
private struct ChatFadeMask: View {
    let composerTop: CGFloat
    let chromeBottom: CGFloat

    var body: some View {
        GeometryReader { g in
            let fade: CGFloat = 28
            let cut = composerTop.isFinite ? max(0, composerTop - g.frame(in: .global).minY) : g.size.height
            // Top: hidden under the header and pinned card, then a short fade in.
            let top = min(cut, max(0, chromeBottom - g.frame(in: .global).minY - 6))
            let topFade = min(18, max(0, cut - top))
            VStack(spacing: 0) {
                Color.clear.frame(height: top)
                LinearGradient(colors: [.black.opacity(0), .black], startPoint: .top, endPoint: .bottom)
                    .frame(height: topFade)
                Color.black.frame(height: max(0, cut - fade - top - topFade))
                LinearGradient(colors: [.black, .black.opacity(0)], startPoint: .top, endPoint: .bottom)
                    .frame(height: min(fade, cut))
                Color.clear
            }
        }
        .ignoresSafeArea()
    }
}
