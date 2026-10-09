import SwiftUI
import RodaCore

@main
struct ZoenMacApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup("Zoen") {
            MacRootView()
                .environment(model)
                .tint(Palette.action)
                .preferredColorScheme(AppModel.launchColorScheme)
                .frame(minWidth: 1080, minHeight: 680)
                .task { await model.applyLaunchOptions() }
                .onOpenURL { model.captureAcquisition($0) }
        }
        .windowToolbarStyle(.unified)
        .defaultSize(width: 1380, height: 860)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("New Chat") { model.newChatOpen = true }
                    .keyboardShortcut("n", modifiers: .command)
                    .disabled(model.sync.account == nil)
            }
            CommandMenu("Zoen") {
                Button("Chat with Zoen") { if let id = model.zoenSpaceId() { model.go(.space(id)) } }
                    .keyboardShortcut("0", modifiers: .command)
                Button("Activity") { model.select(.activity) }
                    .keyboardShortcut("1", modifiers: .command)
                Button("Create and more") { withAnimation(.spring(duration: 0.45, bounce: 0.28)) { model.radialOpen.toggle() } }
                    .keyboardShortcut("k", modifiers: .command)
            }
        }
    }
}

/// Mac: 3 colunas — barra lateral | detalhe | Item (inspetor). A barra lateral
/// segue a estrutura dos posters (Conversas, Comunidades, Arquivos, Atividade) e
/// has the same + fan menu in its footer (⌘K opens/closes).
struct MacRootView: View {
    @Environment(AppModel.self) private var model
    @State private var search = ""
    @State private var onboarding = OnboardingFlow.shouldShow

    var body: some View {
        @Bindable var model = model
        NavigationSplitView {
            List(selection: Binding(get: { model.macSelection }, set: { new in
                model.macSelection = new
                model.setPath(.conversations, [])
            })) {
                Section {
                    sidebarRow(String(localized: "Activity"), symbol: AppTab.activity.symbol, item: .activity, badge: model.pendingCount)
                    sidebarRow(String(localized: "Spaces"), symbol: AppTab.communities.symbol, item: .communities)
                    sidebarRow(String(localized: "Store"), symbol: AppTab.store.symbol, item: .store)
                    sidebarRow(String(localized: "Files"), symbol: AppTab.files.symbol, item: .files)
                    sidebarRow(String(localized: "Your agents"), symbol: AppTab.agents.symbol, item: .agents)
                    sidebarRow(String(localized: "Your context"), symbol: AppTab.you.symbol, item: .you)
                }
                Section {
                    ForEach(model.spaces) { s in
                        HStack(spacing: 10) {
                            SpaceAvatar(space: s, size: 34, waiting: s.pendingRequests > 0, working: model.working[s.id] != nil)
                            VStack(alignment: .leading, spacing: 1) {
                                HStack {
                                    Text(s.title).font(.body.weight(.medium)).lineLimit(1)
                                    Spacer()
                                    Text(RodaTime.short(s.lastAtMs)).font(.caption2).foregroundStyle(.secondary)
                                }
                                Text(s.lastPreview).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                            }
                        }
                        .padding(.vertical, 3)
                        .tag(MacSidebarItem.space(s.id))
                        .badge(Int(s.unread))
                    }
                } header: {
                    HStack {
                        Text("Chats")
                        Spacer()
                        if model.sync.account != nil {
                            ConnectionDot()
                            Button { model.newChatOpen = true } label: { Image(systemName: "square.and.pencil") }
                                .buttonStyle(.borderless)
                                .help("New chat (⌘N)")
                        }
                    }
                }
            }
            .navigationSplitViewColumnWidth(min: 270, ideal: 300, max: 380)
            .searchable(text: $search, placement: .sidebar, prompt: "Search")
            .safeAreaInset(edge: .bottom, spacing: 0) {
                FanMenu(isOpen: $model.radialOpen, items: model.radialItems, triggerSize: 48, arc: 40...140) {
                    model.handleRadial($0)
                }
                .padding(.top, 8)
                .padding(.bottom, 14)
                .frame(maxWidth: .infinity)
                .allowsHitTesting(true)
            }
        } detail: {
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
        }
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
        .onKeyPress(.escape) {
            guard model.radialOpen else { return .ignored }
            withAnimation(.spring(duration: 0.35)) { model.radialOpen = false }
            return .handled
        }
        .onAppear {
            if model.macSelection == nil, let first = model.spaceId(titled: DemoSpace.paraty) ?? model.spaces.first?.id {
                model.macSelection = .space(first)
            }
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

    private func sidebarRow(_ title: String, symbol: String, item: MacSidebarItem, badge: Int = 0) -> some View {
        Label {
            HStack {
                Text(title)
                Spacer()
                if badge > 0 {
                    Text("\(badge)").font(.caption.weight(.bold)).foregroundStyle(.white)
                        .padding(.horizontal, 6).padding(.vertical, 1).background(Palette.amber, in: .capsule)
                }
            }
        } icon: { Image(systemName: symbol) }
        .tag(item)
    }

    @ViewBuilder
    private var detail: some View {
        if !search.isEmpty {
            MacSearchResults(query: search)
        } else {
            switch model.macSelection {
            case .activity: ActivityScreen()
            case .communities: CommunitiesScreen()
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

struct MacSearchResults: View {
    @Environment(AppModel.self) private var model
    let query: String
    var body: some View {
        let hits = model.core.search(query: query)
        List(hits) { h in
            Button {
                if let item = h.itemId { model.inspectorItem = item; model.macSelection = .space(h.spaceId) } else { model.macSelection = .space(h.spaceId) }
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
