import SwiftUI
import RodaCore

@main
struct ZoeniOSApp: App {
    @State private var model = AppModel()
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .tint(Palette.action)
                .preferredColorScheme(AppModel.launchColorScheme)
                .task { await model.applyLaunchOptions() }
                // Friend, Space and campaign links: where this install came from (ADR 0044).
                .onOpenURL { model.captureAcquisition($0) }
                .onContinueUserActivity(NSUserActivityTypeBrowsingWeb) { a in
                    if let url = a.webpageURL { model.captureAcquisition(url) }
                }
        }
        // A revoke waiting on its "Desfazer" is settled before the app can be killed.
        .onChange(of: phase) { _, p in
            if p != .active { model.commitPendingRevokes() }
            if p == .active { Task { await model.growthSync() } }
        }
    }
}

/// iPhone root: one navigation stack per destination, and a floating Liquid Glass bar:
/// Search on the left, a three-tab pill in the middle, and the dark + on the right that fans
/// out the radial menu (create and the destinations that aren't tabs).
struct RootView: View {
    @Environment(AppModel.self) private var model
    @Namespace private var zoom
    @State private var onboarding = OnboardingFlow.shouldShow
    @State private var size: CGSize = .zero

    private var barVisible: Bool { model.path(model.tab).isEmpty }

    var body: some View {
        @Bindable var model = model
        ZStack(alignment: .bottom) {
            ForEach(AppTab.allCases, id: \.self) { t in
                if (t != .search || model.tab == .search)
                    && (t != .activity || (model.tab == .activity && !model.approvalsOpen && !model.notificationsOpen)) {
                    stack(for: t)
                        .opacity(model.tab == t ? 1 : 0)
                        .allowsHitTesting(model.tab == t)
                        .accessibilityHidden(model.tab != t)
                }
            }

            if barVisible {
                // The bar keeps an empty slot for the +; the real trigger sits above the dim
                // veil so the fan and the + stay bright while everything else dims.
                ZStack(alignment: .bottomTrailing) {
                    ZoenBottomBar()
                    if !model.chatSearch {
                        RadialBackdrop(isOpen: $model.radialOpen)
                        FanMenu(isOpen: $model.radialOpen, items: model.radialItems,
                                triggerSize: ZoenBottomBar.circle, room: (left: size.width - 16 - ZoenBottomBar.circle / 2, right: 16 + ZoenBottomBar.circle / 2)) {
                            model.handleRadial($0)
                        }
                        .padding(.trailing, ZoenBottomBar.inset)
                        .padding(.bottom, ZoenBottomBar.bottom + (ZoenBottomBar.height - ZoenBottomBar.circle) / 2)
                        .transition(.scale(scale: 0.6).combined(with: .opacity))
                    }
                }
                .transition(.move(edge: .bottom).combined(with: .opacity))
            }

            if model.searchOpen {
                SearchReveal(origin: model.searchOrigin)
                    .zIndex(10)
            }

            if let toast = model.toast {
                ToastView(toast: toast, onUndo: { model.undo($0) }, onRestore: { model.restoreStanding($0) }, onClose: { withAnimation { model.toast = nil } })
                    .padding(.bottom, barVisible ? 96 : 84)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .onGeometryChange(for: CGSize.self) { $0.size } action: { size = $0 }
        .animation(.spring(duration: 0.35), value: barVisible)
        // Nothing under the approvals cover is visible: stop its ink/mascot/globe loops so
        // the cards get the whole main thread (sheets below are outside this and keep theirs).
        .environment(\.ambientPaused, model.approvalsOpen)
        .overlay {
            if onboarding || model.sync.needsAccount {
                OnboardingFlow { withAnimation(.easeInOut(duration: 0.4)) { onboarding = false } }
                    .transition(.opacity)
            }
            if UserDefaults.standard.bool(forKey: "RodaMascotGallery") { MascotGallery() }
            if UserDefaults.standard.bool(forKey: "RodaIconSheet") { ZoenIconSheet() }
            if UserDefaults.standard.bool(forKey: "RodaTopBarWidths") { TopBarWidthSheet() }
            if UserDefaults.standard.bool(forKey: "RodaIconExplore") { IconExploreSheet() }
        }
        .overlay {
            if let flip = model.appFlip {
                MiniAppFlipHost(flip: flip).id(flip.id)
            }
        }
        .environment(\.appZoom, zoom)
        .sheet(item: Binding(get: { model.appSheet == nil && model.appFlip == nil ? model.appConfirm : nil }, set: { model.appConfirm = $0 })) { req in
            AppConfirmSheet(request: req) { model.appConfirm = nil }
        }
        .sheet(isPresented: $model.newChatOpen) {
            NewChatSheet().environment(model)
        }
        .sheet(item: $model.profileSheet) { ref in
            ProfileSheet(personaId: ref.id)
                .environment(model)
                .environment(\.appZoom, zoom)
        }
        .sheet(isPresented: $model.notificationsOpen) {
            NotificationsSheet().environment(model).environment(\.appZoom, zoom)
        }
        .fullScreenCover(isPresented: $model.approvalsOpen) {
            ApprovalsStackView().environment(model).environment(\.appZoom, zoom)
        }
        .sheet(item: $model.appSheet) { ref in
            AppSheetHost(itemId: ref.id)
                .environment(model)
                .environment(\.appZoom, zoom)
        }
    }

    @ViewBuilder
    private func stack(for t: AppTab) -> some View {
        NavigationStack(path: Binding(get: { model.path(t) }, set: { model.setPath(t, $0) })) {
            root(t)
                .safeAreaInset(edge: .bottom, spacing: 0) { Color.clear.frame(height: 74) }
                .navigationDestination(for: Route.self) { route in destination(route) }
        }
    }

    @ViewBuilder
    private func root(_ t: AppTab) -> some View {
        switch t {
        case .conversations: ConversationsScreen()
        case .files: FilesScreen()
        case .activity: ActivityScreen()
        case .agents: AgentsLibraryScreen()
        case .you: YouScreen()
        case .search: SearchScreen(onOpenItem: { id in model.push(.item(id)) })
        case .store: StoreScreen()
        }
    }

    private func destination(_ route: Route) -> some View { RouteDestination(route: route) }
}

/// Every pushed screen (tab stacks and the notifications sheet share it).
struct RouteDestination: View {
    @Environment(AppModel.self) private var model
    @Environment(\.appZoom) private var zoom
    let route: Route

    /// Chat and full-screen mini-apps own their chrome; everything else gets a plain Back
    /// that only pops (never the system history menu).
    private var ownsBack: Bool {
        switch route {
        case .space, .app: true
        default: false
        }
    }

    var body: some View {
        content
            .modifier(PlainPopBack(enabled: !ownsBack))
    }

    @ViewBuilder
    private var content: some View {
        switch route {
        case .store(let id):
            StoreDetailView(listingId: id)
        case .space(let id):
            SpaceView(spaceId: id, onOpenItem: { item in model.push(.item(item)) })
        case .item(let id):
            ItemView(itemId: id)
        case .agent(let id):
            AgentDetailView(agentId: id)
        case .integrity:
            IntegrityView()
        case .log(let id):
            LogEventsView(spaceId: id)
        case .items:
            ItemsListView(onOpen: { id in model.push(.item(id)) })
        case .participants(let id):
            ParticipantsView(spaceId: id)
        case .permissions(let agent, let space):
            AgentPermissionsView(agentId: agent, spaceId: space)
        case .request(let id):
            RequestReviewView(requestId: id)
        case .folder(let id):
            FolderView(spaceId: id)
        case .app(let id):
            AppFullScreenView(itemId: id)
                .appZoomDestination(id, zoom)
        }
    }
}

/// Hides the system back (and its history menu) and puts a single-pop Back in its place.
private struct PlainPopBack: ViewModifier {
    var enabled: Bool
    @ViewBuilder
    func body(content: Content) -> some View {
        if enabled {
            content
                .navigationBarBackButtonHidden(true)
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) {
                        ZoenBackButton(chrome: .toolbar)
                    }
                }
        } else {
            content
        }
    }
}

/// Floating bottom bar: Search circle · two-tab pill · slot for the + (see `RootView`).
/// Tabs: Chats (direct conversations, groups, and communities) and Store (agents
/// and mini-apps). Notifications and approvals live behind the bell in the headers.
struct ZoenBottomBar: View {
    static let circle: CGFloat = 56
    static let height: CGFloat = 62
    static let inset: CGFloat = 16
    static let bottom: CGFloat = 2

    /// Tab pill labels: `selected` (default: only the current tab shows its word) or `none`
    /// (icons only). `-RodaTabLabels none` for the comparison screenshots.
    enum LabelStyle: String { case selected, none }
    static var labelStyle: LabelStyle { LabelStyle(rawValue: UserDefaults.standard.string(forKey: "RodaTabLabels") ?? "") ?? .selected }

    @Environment(AppModel.self) private var model
    @Environment(\.colorScheme) private var scheme
    @Namespace private var pill
    @FocusState private var fieldFocused: Bool

    var body: some View {
        @Bindable var model = model
        HStack(spacing: 10) {
            if model.chatSearch {
                // Search grows out of the circle into a full-width field; Cancel closes it.
                HStack(spacing: 8) {
                    ZoenIcon(.search, size: 19)
                        .foregroundStyle(Palette.textSecondary)
                        .matchedGeometryEffect(id: "lens", in: pill)
                    TextField("Search", text: $model.chatQuery)
                        .textFieldStyle(.plain)
                        .font(.body)
                        .autocorrectionDisabled()
                        .submitLabel(.search)
                        .focused($fieldFocused)
                    if !model.chatQuery.isEmpty {
                        Button { model.chatQuery = "" } label: {
                            ZoenIcon(.close, size: 14).foregroundStyle(Palette.textTertiary)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Clear search")
                    }
                }
                .padding(.horizontal, 18)
                .frame(height: Self.height)
                .glassEffect(.regular.tint(BarEdge.tint(scheme)).interactive(), in: .capsule)
                .barEdge(Capsule())
                .matchedGeometryEffect(id: "search", in: pill)

                Button {
                    Haptics.dismiss()
                    closeSearch()
                } label: {
                    Text("Cancel")
                        .font(.body.weight(.semibold))
                        .foregroundStyle(Palette.textPrimary)
                        .padding(.horizontal, 18)
                        .frame(height: Self.height)
                        .glassEffect(.regular.tint(BarEdge.tint(scheme)).interactive(), in: .capsule)
                        .barEdge(Capsule())
                }
                .buttonStyle(.plain)
                .transition(.move(edge: .trailing).combined(with: .opacity))
            } else {
                Button {
                    Haptics.tap()
                    openSearch()
                } label: {
                    ZoenIcon(.search, size: 23)
                        .foregroundStyle(Palette.textPrimary)
                        .matchedGeometryEffect(id: "lens", in: pill)
                        .frame(width: Self.circle, height: Self.circle)
                        .glassEffect(.regular.tint(BarEdge.tint(scheme)).interactive(), in: .circle)
                        .barEdge(Circle())
                        .matchedGeometryEffect(id: "search", in: pill)
                }
                .buttonStyle(IconPressStyle())
                .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { model.searchOrigin = CGPoint(x: $0.midX, y: $0.midY) }
                .accessibilityLabel("Search")
                .accessibilityHint("Messages, people, agents, chats and apps")
                .accessibilityIdentifier("bar-search")

                HStack(spacing: 2) {
                    tab(.conversations, badge: model.spaces.reduce(0) { $0 + Int($1.unread) })
                    tab(.store)
                }
                .padding(4)
                .frame(height: Self.height)
                .glassEffect(.regular.tint(BarEdge.tint(scheme)), in: .capsule)
                .barEdge(Capsule())
                .transition(.opacity.combined(with: .scale(scale: 0.9)))

                Color.clear.frame(width: Self.circle, height: Self.circle)
                    .accessibilityHidden(true)
            }
        }
        .padding(.horizontal, Self.inset)
        .padding(.bottom, Self.bottom)
        .onChange(of: model.chatSearch) { _, open in fieldFocused = open }
        .onAppear { if model.chatSearch { fieldFocused = true } }
    }

    private func openSearch() {
        model.radialOpen = false
        model.searchOpen = true
    }

    private func closeSearch() {
        fieldFocused = false
        withAnimation(.spring(duration: 0.38, bounce: 0.12)) {
            model.chatQuery = ""
            model.chatSearch = false
        }
    }

    private func tab(_ t: AppTab, badge: Int = 0) -> some View {
        let on = model.tab == t
        let showsLabel = on && Self.labelStyle == .selected
        return Button {
            guard model.tab != t || !model.path(t).isEmpty else { return }
            if !on { Haptics.selectionTick() }
            withAnimation(.spring(duration: 0.38, bounce: 0.22)) { model.select(t) }
        } label: {
            HStack(spacing: badge > 0 ? 13 : 6) {
                ZoenIcon(t.glyph, selected: on, size: 23)
                    .overlay(alignment: .topTrailing) {
                        if badge > 0 {
                            Text("\(min(badge, 99))")
                                .font(.system(size: 10, weight: .bold)).monospacedDigit()
                                .foregroundStyle(.white)
                                .padding(.horizontal, 4).frame(minWidth: 16, minHeight: 16)
                                .background(Palette.danger, in: .capsule)
                                .offset(x: 11, y: -6)
                        }
                    }
                if showsLabel {
                    Text(t.title)
                        .font(.system(size: 13, weight: .bold))
                        .lineLimit(1)
                        .fixedSize()
                        .transition(.opacity.combined(with: .scale(scale: 0.8, anchor: .leading)))
                }
            }
            .foregroundStyle(on ? Palette.action : Palette.textSecondary)
            .padding(.horizontal, showsLabel ? 14 : 0)
            .frame(maxWidth: showsLabel ? nil : .infinity, maxHeight: .infinity)
            .frame(minWidth: 48)
            .background {
                // The selected tab sits on a tinted pill (the ink icons' highlighter swipe is
                // only used with `-RodaInkIcons YES`).
                if on && !ZoenIconStyle.ink {
                    Capsule().fill(Palette.action.opacity(scheme == .dark ? 0.26 : 0.14))
                        .matchedGeometryEffect(id: "tab", in: pill)
                }
            }
            .contentShape(.capsule)
        }
        .buttonStyle(IconPressStyle())
        .layoutPriority(showsLabel ? 1 : 0)
        .accessibilityLabel(badge > 0 ? "\(t.title), \(badge)" : t.title)
        .accessibilityIdentifier("bar-\(t.rawValue)")
        .accessibilityAddTraits(on ? .isSelected : [])
        .accessibilityShowsLargeContentViewer {
            Label(t.title, systemImage: t.symbol)
        }
    }
}

/// The bell's sheet: mentions, tasks and approvals (the old Activity tab), with its own stack.
struct NotificationsSheet: View {
    @Environment(AppModel.self) private var model
    var body: some View {
        NavigationStack(path: Binding(get: { model.path(.activity) }, set: { model.setPath(.activity, $0) })) {
            ActivityScreen(initialList: true, onShowCards: {
                model.notificationsOpen = false
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) {
                    model.openNotifications()
                }
            })
                .navigationTitle(Text("Notifications"))
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button { model.notificationsOpen = false } label: { ZoenIcon(.close, size: 18) }
                            .accessibilityLabel(Text("Close"))
                    }
                }
                .navigationDestination(for: Route.self) { RouteDestination(route: $0) }
        }
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
    }
}

/// Bottom-bar glass that reads on near-black: in dark mode a faint light tint, a white
/// hairline (~16%) with a brighter top edge, and a deeper shadow. Light mode: a soft shadow.
enum BarEdge {
    /// Dark: a slate tint so the glass stays dark (and its light icons legible) even over
    /// bright content like the Store's paper cards, yet sits a step above the near-black bg.
    static func tint(_ scheme: ColorScheme) -> Color { scheme == .dark ? Color(hex: "#1C2433").opacity(0.62) : .clear }
}

struct BarEdgeModifier<S: InsettableShape>: ViewModifier {
    let shape: S
    var diffuse = true
    @Environment(\.colorScheme) private var scheme
    func body(content: Content) -> some View {
        let dark = scheme == .dark
        content
            .overlay {
                if dark {
                    shape.strokeBorder(LinearGradient(colors: [.white.opacity(0.28), .white.opacity(0.12)], startPoint: .top, endPoint: .bottom), lineWidth: 0.75)
                        .allowsHitTesting(false)
                }
            }
            // Diffusion inside the element: `.regular` glass alone only frosts lightly, so busy
            // content (paper-card titles) still read sharply through it and collided with the
            // icons. A material in the element's own shape, under the glass, softens what's
            // behind each piece (no band behind the bar). `-RodaBarDiffuse NO` turns it off.
            .background {
                if diffuse, UserDefaults.standard.object(forKey: "RodaBarDiffuse") == nil || UserDefaults.standard.bool(forKey: "RodaBarDiffuse") {
                    shape.fill(dark ? .thinMaterial : .ultraThinMaterial).allowsHitTesting(false)
                }
            }
            // The shadow lives on its own layer *outside* the shape (interior punched out).
            // `.shadow` on the glass view itself flattens it into an offscreen layer and the
            // glass loses its backdrop diffusion, so busy content showed through sharply.
            .background {
                if !UserDefaults.standard.bool(forKey: "RodaBarOldShadow") {
                    shape.fill(.black)
                        .shadow(color: .black.opacity(dark ? 0.55 : 0.12), radius: dark ? 14 : 10, y: dark ? 6 : 4)
                        .mask {
                            ZStack {
                                Rectangle().padding(-40)
                                shape.blendMode(.destinationOut)
                            }
                            .compositingGroup()
                        }
                        .allowsHitTesting(false)
                }
            }
            .modifier(OldShadow(dark: dark))
    }
}

/// `-RodaBarOldShadow YES` restores the previous shadow for the before/after shots.
private struct OldShadow: ViewModifier {
    let dark: Bool
    func body(content: Content) -> some View {
        if UserDefaults.standard.bool(forKey: "RodaBarOldShadow") {
            content.shadow(color: .black.opacity(dark ? 0.55 : 0.1), radius: dark ? 14 : 10, y: dark ? 6 : 4)
        } else { content }
    }
}

extension View {
    func barEdge<S: InsettableShape>(_ shape: S, diffuse: Bool = true) -> some View { modifier(BarEdgeModifier(shape: shape, diffuse: diffuse)) }
}

/// The chat hides the system navigation bar (it draws its own glass bar), and UIKit then
/// stops the edge swipe back. Keep it: the pop gesture may begin whenever there is a screen
/// to pop. Covered by UITests/SwipeBackTests.
extension UINavigationController: @retroactive UIGestureRecognizerDelegate {
    override open func viewDidLoad() {
        super.viewDidLoad()
        interactivePopGestureRecognizer?.delegate = self
    }

    public func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        guard gestureRecognizer === interactivePopGestureRecognizer else { return true }
        return viewControllers.count > 1 && transitionCoordinator == nil
    }

    /// The edge swipe wins over the chat's own drags (drag-to-reply, the scroll view): they
    /// wait for it to fail, which it does at once for a touch away from the edge.
    public func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer,
                                  shouldBeRequiredToFailBy other: UIGestureRecognizer) -> Bool {
        gestureRecognizer === interactivePopGestureRecognizer && viewControllers.count > 1
    }
}
