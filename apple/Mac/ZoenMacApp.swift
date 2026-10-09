import SwiftUI
import RodaCore

private struct MacSearchFocusKey: FocusedValueKey {
    typealias Value = @MainActor () -> Void
}

private struct MacSearchResetKey: FocusedValueKey {
    typealias Value = @MainActor () -> Void
}

private extension FocusedValues {
    var focusMacSearch: MacSearchFocusKey.Value? {
        get { self[MacSearchFocusKey.self] }
        set { self[MacSearchFocusKey.self] = newValue }
    }

    var resetMacSearch: MacSearchResetKey.Value? {
        get { self[MacSearchResetKey.self] }
        set { self[MacSearchResetKey.self] = newValue }
    }
}

@main
struct ZoenMacApp: App {
    @State private var model = AppModel()
    @AppStorage("RodaAppearance") private var appearance = "system"
    @FocusedValue(\.resetMacSearch) private var resetSearch
    @FocusedValue(\.focusMacSearch) private var focusSearch

    private var colorScheme: ColorScheme? {
        switch appearance {
        case "light": .light
        case "dark": .dark
        default: nil
        }
    }

    var body: some Scene {
        WindowGroup("Zoen") {
            MacRootView()
                .environment(model)
                .tint(Palette.action)
                .preferredColorScheme(colorScheme)
                .frame(minWidth: 1080, minHeight: 680)
                .task { await model.applyLaunchOptions() }
                .onOpenURL { model.captureAcquisition($0) }
        }
        .windowStyle(.hiddenTitleBar)
        .windowToolbarStyle(.unifiedCompact(showsTitle: false))
        .windowBackgroundDragBehavior(.enabled)
        .defaultSize(width: 1380, height: 860)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("New Chat") { model.newChatOpen = true }
                    .keyboardShortcut("n", modifiers: .command)
                    .disabled(model.sync.account == nil && SyncModel.mode != .demo)
            }
            CommandMenu("Zoen") {
                Button("Search") { focusSearch?() }
                    .keyboardShortcut("f", modifiers: .command)
                Button("Back") { _ = model.pop() }
                    .keyboardShortcut("[", modifiers: .command)
                    .disabled(model.path(.conversations).isEmpty)
                Button("Chat with Zoen") { resetSearch?(); if let id = model.zoenSpaceId() { model.go(.space(id)) } }
                    .keyboardShortcut("0", modifiers: .command)
                Button("Activity") { resetSearch?(); model.select(.activity) }
                    .keyboardShortcut("1", modifiers: .command)
                Button("Create and more") { withAnimation(.spring(duration: 0.45, bounce: 0.28)) { model.radialOpen.toggle() } }
                    .keyboardShortcut("k", modifiers: .command)
            }
        }
    }
}

/// The rail stays available when the chat card is closed. Detail navigation and the
/// Item inspector keep their existing routes; the + fan remains available with ⌘K.
struct MacRootView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AppStorage("ZoenMacSidebarExpanded") private var sidebarExpanded = true
    @AppStorage("RodaAppearance") private var appearance = "system"
    @State private var search = ""
    @FocusState private var searchFocused: Bool
    @State private var lastChat: String?
    @State private var onboarding = OnboardingFlow.shouldShow

    var body: some View {
        @Bindable var model = model
        HStack(spacing: 0) {
            rail.padding(.top, 32).zIndex(2)
            HStack(spacing: 12) {
                if sidebarExpanded {
                    chatSidebar
                        .frame(width: 280)
                        .background(Palette.surface, in: .rect(cornerRadius: 20))
                        .overlay(RoundedRectangle(cornerRadius: 20).strokeBorder(Palette.hairline.opacity(0.6), lineWidth: 0.5))
                        .clipShape(RoundedRectangle(cornerRadius: 20))
                        .transition(reduceMotion ? .opacity : .move(edge: .leading).combined(with: .opacity))
                }
                navigationContent
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Palette.surface, in: .rect(cornerRadius: 24))
                    .clipShape(RoundedRectangle(cornerRadius: 24))
                    .overlay(RoundedRectangle(cornerRadius: 24).strokeBorder(Palette.hairline.opacity(0.6), lineWidth: 0.5))
                    .shadow(color: .black.opacity(0.04), radius: 16, y: 4)
            }
            .padding(.trailing, 16)
            .padding(.bottom, 16)
        }
        .background(Palette.background)
        .ignoresSafeArea(.container, edges: .top)
        .toolbarBackground(.hidden, for: .windowToolbar)
        .focusedSceneValue(\.focusMacSearch, { setSidebar(true); searchFocused = true })
        .focusedSceneValue(\.resetMacSearch, { search = ""; searchFocused = false })
        .inspector(isPresented: Binding(get: { model.inspectorItem != nil }, set: { if !$0 { model.inspectorItem = nil } })) {
            if let id = model.inspectorItem {
                NavigationStack { ItemView(itemId: id) }
                    .inspectorColumnWidth(min: 360, ideal: 440, max: 560)
            }
        }
        .overlay(alignment: .bottom) {
            if let toast = model.toast {
                ToastView(toast: toast, onUndo: { model.undo($0) }, onRestore: { model.restoreStanding($0) }, onClose: { withAnimation { model.toast = nil } })
                    .frame(maxWidth: 560)
                    .padding(.bottom, 90)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .sheet(item: Binding(get: { model.appSheet == nil ? model.appConfirm : nil }, set: { model.appConfirm = $0 })) { req in
            AppConfirmSheet(request: req) { model.appConfirm = nil }
                .frame(width: 420)
        }
        .sheet(item: $model.appSheet) { ref in
            AppSheetHost(itemId: ref.id).environment(model)
        }
        .sheet(item: $model.profileSheet) { ref in
            ProfileSheet(personaId: ref.id).environment(model)
        }
        .onKeyPress(.escape) {
            guard model.radialOpen else { return .ignored }
            withAnimation(reduceMotion ? nil : .spring(duration: 0.35)) { model.radialOpen = false }
            return .handled
        }
        .onAppear {
            if model.macSelection == nil, let first = model.spaceId(titled: DemoSpace.paraty) ?? model.spaces.first?.id {
                model.macSelection = .space(first)
            }
            rememberChat(model.macSelection)
        }
        .onChange(of: model.macSelection) { _, selection in
            rememberChat(selection)
            search = ""
            searchFocused = false
        }
        .sheet(isPresented: $model.newChatOpen) {
            NewChatSheet().environment(model)
        }
        .overlay {
            if onboarding || model.sync.needsAccount {
                OnboardingFlow { withAnimation(.easeInOut(duration: 0.4)) { onboarding = false } }
                    .transition(.opacity)
            }
            if UserDefaults.standard.bool(forKey: "RodaMascotGallery") { MascotGallery() }
        }
    }

    private var activeTab: AppTab {
        switch model.macSelection {
        case .activity: .activity
        case .store: .store
        case .files: .files
        case .agents: .agents
        case .you: .you
        case .search: .search
        case .space(_), nil: .conversations
        }
    }

    private var rail: some View {
        @Bindable var model = model
        return VStack(spacing: 6) {
            sidebarToggle
                .padding(.bottom, 4)
            ForEach([AppTab.conversations, .activity, .store, .files, .agents, .you, .search], id: \.self) { tab in
                railButton(tab)
            }
            Spacer(minLength: 8)
            appearanceControl
            FanMenu(isOpen: $model.radialOpen, items: model.radialItems, triggerSize: 44, arc: 8...88) {
                search = ""
                model.handleRadial($0)
            }
            .padding(.top, 8)
        }
        .padding(.vertical, 12)
        .frame(width: 80)
    }

    private func railButton(_ tab: AppTab) -> some View {
        let selected = activeTab == tab
        let badge = tab == .activity ? model.pendingCount : tab == .conversations ? model.spaces.reduce(0) { $0 + Int($1.unread) } : 0
        return Button { select(tab) } label: {
            Image(systemName: tab.symbol)
                .font(.system(size: 17, weight: .medium))
                .foregroundStyle(selected ? Palette.action : Palette.textSecondary)
                .frame(width: 40, height: 36)
                .background(selected ? Palette.action.opacity(0.11) : .clear, in: .rect(cornerRadius: 12))
                .overlay(alignment: .topTrailing) {
                    if badge > 0 {
                        Text(badge > 99 ? "99+" : "\(badge)")
                            .font(.system(size: 9, weight: .bold))
                            .foregroundStyle(.white)
                            .padding(.horizontal, 4).padding(.vertical, 2)
                            .background(Palette.action, in: .capsule)
                            .offset(x: 3, y: -2)
                    }
                }
        }
        .buttonStyle(.plain)
        .help(tab.title)
        .accessibilityLabel(tab.title)
        .accessibilityValue(badge > 0 ? "\(badge)" : "")
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("mac-rail-\(tab.rawValue)")
    }

    private var sidebarToggle: some View {
        Button { setSidebar(!sidebarExpanded) } label: {
            Image(systemName: "sidebar.left")
                .font(.system(size: 16))
                .frame(width: 40, height: 32)
        }
        .buttonStyle(.plain)
        .foregroundStyle(Palette.textSecondary)
        .help(sidebarExpanded ? "Hide sidebar" : "Show sidebar")
        .accessibilityLabel(sidebarExpanded ? "Hide sidebar" : "Show sidebar")
        .accessibilityValue(sidebarExpanded ? "Expanded" : "Collapsed")
        .accessibilityIdentifier("mac-sidebar-toggle")
    }

    private var searchField: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").font(.system(size: 13))
            TextField("Search", text: $search)
                .textFieldStyle(.plain)
                .focused($searchFocused)
                .accessibilityIdentifier("mac-search-field")
            if !search.isEmpty {
                Button { search = "" } label: { Image(systemName: "xmark.circle.fill") }
                    .buttonStyle(.plain)
                    .accessibilityLabel(Text("Clear search"))
            }
        }
        .font(.system(size: 13))
        .foregroundStyle(Palette.textSecondary)
        .padding(.horizontal, 10)
        .frame(height: 32)
        .background(Palette.surfaceMuted, in: .rect(cornerRadius: 10))
        .padding(.horizontal, 12)
        .padding(.bottom, 8)
    }

    private var appearanceControl: some View {
        MacAppearanceControl(appearance: $appearance)
    }

    private var chatSidebar: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Chats").font(.system(size: 14, weight: .semibold))
                Spacer()
                ChatInboxFilterMenu(showsLabel: false)
                if model.sync.account != nil { ConnectionDot() }
                if case .space(let id)? = model.macSelection {
                    Button { model.go(.participants(id)) } label: {
                        ZoenIcon(.info, size: 16).frame(width: 28, height: 28)
                    }
                    .buttonStyle(.plain)
                    .help("Participants")
                    .accessibilityLabel("Participants")
                }
                Button { model.newChatOpen = true } label: {
                    Image(systemName: "square.and.pencil").frame(width: 28, height: 28)
                }
                .buttonStyle(.plain)
                .disabled(model.sync.account == nil && SyncModel.mode != .demo)
                .help("New chat (⌘N)")
                .accessibilityLabel("New Chat")
            }
            .foregroundStyle(Palette.textPrimary)
            .padding(.horizontal, 16).padding(.top, 12).padding(.bottom, 8)
            searchField
            List(selection: Binding(get: { model.macSelection }, set: { selection in
                if case .space(let id)? = selection {
                    search = ""
                    model.go(.space(id))
                }
            })) {
                ForEach(model.orderedSpaces.filter { model.chatFilter.includes($0) }) { s in
                    Button {
                        search = ""
                        searchFocused = false
                        model.go(.space(s.id))
                    } label: {
                        HStack(spacing: 10) {
                            SpaceAvatar(space: s, size: 34, waiting: s.pendingRequests > 0, working: model.working[s.id] != nil)
                            VStack(alignment: .leading, spacing: 3) {
                                HStack {
                                    Text(s.title).font(.system(size: 13, weight: .medium)).lineLimit(1)
                                    Spacer(minLength: 4)
                                    Text(RodaTime.short(s.lastAtMs)).font(.caption2).foregroundStyle(.secondary)
                                }
                                Text(s.lastPreview).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 5)
                        .background(model.macSelection == .space(s.id) ? Palette.action.opacity(0.1) : .clear,
                                    in: .rect(cornerRadius: 10))
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .tag(MacSidebarItem.space(s.id))
                    .badge(Int(s.unread))
                    .accessibilityLabel(s.title)
                    .accessibilityAddTraits(model.macSelection == .space(s.id) ? .isSelected : [])
                    .accessibilityIdentifier("mac-chat-\(s.id)")
                    .contextMenu {
                        Button("Participants") { model.go(.space(s.id)); model.go(.participants(s.id)) }
                    }
                }
            }
            .listStyle(.sidebar)
            .scrollContentBackground(.hidden)
            .accessibilityIdentifier("mac-chat-sidebar")
        }
    }

    private var navigationContent: some View {
        NavigationStack(path: Binding(get: { model.path(.conversations) }, set: { model.setPath(.conversations, $0) })) {
            detail
                .navigationDestination(for: Route.self) { route in
                    switch route {
                    case .store(let id): StoreDetailView(listingId: id)
                    case .agent(let id): AgentDetailView(agentId: id)
                    case .integrity: IntegrityView()
                    case .log(let id): LogEventsView(spaceId: id)
                    case .items: ItemsListView(onOpen: { model.inspectorItem = $0 })
                    case .item(let id): ItemView(itemId: id)
                    case .space(let id): SpaceView(spaceId: id, onOpenItem: { model.inspectorItem = $0 })
                    case .participants(let id): ParticipantsView(spaceId: id)
                    case .permissions(let agent, let space): AgentPermissionsView(agentId: agent, spaceId: space)
                    case .request(let id): RequestReviewView(requestId: id)
                    case .folder(let id): FolderView(spaceId: id)
                    case .app(let id): AppFullScreenView(itemId: id)
                    }
                }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            if needsDetailBack {
                HStack {
                    Button { _ = model.pop() } label: { Label("Back", systemImage: "chevron.left") }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("mac-detail-back")
                    Spacer()
                }
                .font(.system(size: 13))
                .padding(.horizontal, 16).padding(.vertical, 10)
            }
        }
    }

    private var needsDetailBack: Bool {
        guard let route = model.path(.conversations).last else { return false }
        if case .space = route { return false }
        return true
    }

    private func setSidebar(_ expanded: Bool) {
        withAnimation(reduceMotion ? nil : .easeInOut(duration: 0.22)) { sidebarExpanded = expanded }
    }

    private func rememberChat(_ selection: MacSidebarItem?) {
        if case .space(let id)? = selection { lastChat = id }
    }

    private func select(_ tab: AppTab) {
        search = ""
        searchFocused = false
        if tab == .conversations {
            setSidebar(true)
            if let id = lastChat, model.space(id) != nil { model.go(.space(id)) }
            else if let id = model.spaces.first?.id { model.go(.space(id)) }
            else { model.select(.conversations); model.macSelection = nil }
        } else { model.select(tab) }
    }

    @ViewBuilder
    private var detail: some View {
        if !search.isEmpty {
            MacSearchResults(query: search) { search = ""; searchFocused = false }
        } else {
            switch model.macSelection {
            case .activity: ActivityScreen()
            case .store: StoreScreen()
            case .files: FilesScreen()
            case .agents: AgentsLibraryScreen()
            case .you: YouScreen()
            case .search: UniversalSearchView(embedded: true)
            case .space(let id): SpaceView(spaceId: id, onOpenItem: { model.inspectorItem = $0 }).id(id)
            case nil: ContentUnavailableView("Pick a chat", systemImage: "bubble.left.and.bubble.right")
            }
        }
    }
}

private struct MacAppearanceControl: View {
    @Binding var appearance: String
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorScheme) private var colorScheme
    @State private var position: Int
    @State private var hoveredSlot: Int?

    private static let modes = ["dark", "system", "light"]

    init(appearance: Binding<String>) {
        _appearance = appearance
        _position = State(initialValue: Self.modes.firstIndex(of: appearance.wrappedValue) ?? 1)
    }

    var body: some View {
        ZStack {
            ZStack {
                RoundedRectangle(cornerRadius: 11)
                    .fill(Palette.textSecondary.opacity(hoveredSlot == 0 ? 0.12 : 0.06))
                    .frame(width: 28, height: 36)
                ForEach((position - 3)...(position + 3), id: \.self) { index in
                    let theme = mode(at: index)
                    icon(for: theme)
                        .font(.system(size: 16, weight: .medium))
                        .foregroundStyle(Palette.textPrimary)
                        .opacity(theme == mode(at: position) || theme == hoveredMode ? 1 : 0.38)
                        .frame(width: 24, height: 40)
                        .offset(x: CGFloat(index - position) * 24)
                }
            }
            .allowsHitTesting(false)
            .accessibilityHidden(true)
            HStack(spacing: 0) {
                ForEach(-1...1, id: \.self) { slot in
                    let theme = mode(at: position + slot)
                    Button { select(theme) } label: {
                        Color.clear
                            .frame(width: slot == 0 ? 28 : 18, height: 40)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .onHover { hovering in
                        if hovering { hoveredSlot = slot }
                        else if hoveredSlot == slot { hoveredSlot = nil }
                    }
                    .onKeyPress(.leftArrow) { select(mode(at: position - 1)); return .handled }
                    .onKeyPress(.rightArrow) { select(mode(at: position + 1)); return .handled }
                    .help("Use \(theme.capitalized) appearance")
                    .accessibilityLabel("\(theme.capitalized) appearance")
                    .accessibilityValue(slot == 0 ? "Selected" : "")
                    .accessibilityAddTraits(slot == 0 ? .isSelected : [])
                    .accessibilityIdentifier("mac-appearance-\(theme)")
                }
            }
        }
        .frame(width: 64, height: 40)
        .clipped()
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text("Appearance"))
        .accessibilityValue(mode(at: position).capitalized)
        .accessibilityIdentifier("mac-appearance-control")
        .contextMenu {
            Picker("Appearance", selection: $appearance) {
                Label("Dark", systemImage: "moon").tag("dark")
                Label("System", systemImage: "desktopcomputer").tag("system")
                Label("Light", systemImage: "sun.max").tag("light")
            }
        }
        .onChange(of: appearance) { _, mode in roll(to: mode) }
    }

    private var hoveredMode: String? {
        hoveredSlot.map { mode(at: position + $0) }
    }

    private func mode(at index: Int) -> String {
        Self.modes[(index % 3 + 3) % 3]
    }

    private func select(_ mode: String) {
        guard appearance != mode else { return }
        appearance = mode
        roll(to: mode)
    }

    private func roll(to mode: String) {
        let target = Self.modes.firstIndex(of: mode) ?? 1
        let current = (position % 3 + 3) % 3
        let step = (target - current + 3) % 3
        guard step != 0 else { return }
        withAnimation(reduceMotion ? nil : .spring(duration: 0.28, bounce: 0.08)) {
            position += step == 2 ? -1 : 1
        }
    }

    @ViewBuilder
    private func icon(for mode: String) -> some View {
        if mode == "system" {
            Image(systemName: "desktopcomputer")
                .overlay {
                    Image(systemName: colorScheme == .dark ? "moon.fill" : "sun.max.fill")
                        .font(.system(size: 6, weight: .semibold))
                        .offset(y: -2)
                }
        } else {
            Image(systemName: mode == "dark" ? "moon" : "sun.max")
        }
    }
}

struct MacSearchResults: View {
    @Environment(AppModel.self) private var model
    let query: String
    var onOpen: () -> Void = {}
    var body: some View {
        let hits = model.core.search(query: query)
        List(hits) { h in
            Button {
                model.go(.space(h.spaceId))
                if let item = h.itemId { model.inspectorItem = item }
                onOpen()
            } label: {
                VStack(alignment: .leading, spacing: 2) {
                    Text(h.title).font(.headline)
                    Text(h.snippet).foregroundStyle(.secondary).lineLimit(2)
                    Text(h.spaceTitle).font(.caption).foregroundStyle(.tertiary)
                }
            }
            .buttonStyle(.plain)
        }
        .overlay { if hits.isEmpty { ContentUnavailableView.search(text: query) } }
        .navigationTitle("Search “\(query)”")
    }
}
