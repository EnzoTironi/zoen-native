import SwiftUI
import RodaCore

/// Observes the owned native bar without replacing its delegates or changing layout.
/// Ambiguous accessibility/image identity deliberately produces no effect.
private struct NativeTabImageEffects: UIViewControllerRepresentable {
    var pendingIDs: Set<String>
    func makeUIViewController(context: Context) -> Observer { Observer() }
    func updateUIViewController(_ controller: Observer, context: Context) {
        controller.observe(pendingIDs: pendingIDs)
    }

    final class Observer: UIViewController, UIGestureRecognizerDelegate {
        private weak var observedBar: UITabBar?
        private var previousPending: Set<String>?
        private lazy var taps: UITapGestureRecognizer = {
            let recognizer = UITapGestureRecognizer(target: self, action: #selector(tapped(_:)))
            recognizer.cancelsTouchesInView = false
            recognizer.delaysTouchesBegan = false
            recognizer.delaysTouchesEnded = false
            recognizer.delegate = self
            return recognizer
        }()
        override func loadView() {
            view = UIView()
            view.isUserInteractionEnabled = false
        }
        override func viewDidLoad() {
            super.viewDidLoad()
            NotificationCenter.default.addObserver(self, selector: #selector(reduceMotionChanged),
                name: UIAccessibility.reduceMotionStatusDidChangeNotification, object: nil)
        }
        @objc private func reduceMotionChanged() {
            guard UIAccessibility.isReduceMotionEnabled, let bar = observedBar else { return }
            descendants(bar, UIImageView.self).forEach {
                $0.removeAllSymbolEffects()
                $0.transform = .identity
            }
        }
        func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer,
                               shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer) -> Bool { true }
        private func descendants<T: UIView>(_ root: UIView, _: T.Type) -> [T] {
            root.subviews.flatMap { child in
                (child as? T).map { [$0] } ?? []
            } + root.subviews.flatMap { descendants($0, T.self) }
        }
        private func bars(_ controller: UIViewController) -> [UITabBar] {
            let own = (controller as? UITabBarController).map { [$0.tabBar] } ?? []
            return own + controller.children.flatMap { bars($0) }
        }
        private func resolveBar() -> UITabBar? {
            var root: UIViewController = self
            while let parent = root.parent { root = parent }
            let visible = bars(root).filter { !$0.isHidden && $0.alpha > 0 && $0.window != nil }
            return visible.count == 1 ? visible[0] : nil
        }
        private func images(in control: UIControl) -> [UIImageView] {
            descendants(control, UIImageView.self).filter {
                !$0.isHidden && $0.alpha > 0 && $0.bounds.width > 0 && $0.bounds.height > 0 && $0.image != nil
            }
        }
        private func title(of control: UIControl, bar: UITabBar) -> String? {
            let titles = bar.items?.compactMap(\.title) ?? []
            let visibleLabels = descendants(control, UILabel.self).filter {
                !$0.isHidden && $0.alpha > 0 && $0.bounds.width > 0
            }.compactMap(\.text)
            let matches = titles.filter { $0 == control.accessibilityLabel || visibleLabels.contains($0) }
            return matches.count == 1 ? matches[0] : nil
        }
        private func logicalControls(in bar: UITabBar) -> [UIControl] {
            let candidates = descendants(bar, UIControl.self).filter { control in
                guard !control.isHidden, control.alpha > 0, control.isUserInteractionEnabled,
                      title(of: control, bar: bar) != nil else { return false }
                let point = control.convert(CGPoint(x: control.bounds.midX, y: control.bounds.midY), to: bar)
                guard let hit = bar.hitTest(point, with: nil) else { return false }
                // Liquid Glass keeps a second tab presentation for transitions.
                // Only the presentation receiving real touches owns the effect.
                return hit === control || hit.isDescendant(of: control)
            }
            // UIKit can nest a title-bearing control inside the tab's control. It
            // is one logical item, not two ambiguous destinations. Keep its outer
            // owner, which also contains the selected/unselected native SF images.
            return candidates.filter { candidate in
                !candidates.contains { other in other !== candidate && candidate.isDescendant(of: other) }
            }
        }
        private func control(title: String, bar: UITabBar) -> UIControl? {
            let matches = logicalControls(in: bar).filter { self.title(of: $0, bar: bar) == title }
            return matches.count == 1 ? matches[0] : nil
        }
        private func report(_ value: String) {
            #if DEBUG
            NotificationCenter.default.post(name: Notification.Name("NativeParityEffectDiagnostic"), object: value)
            #endif
        }
        func observe(pendingIDs: Set<String>) {
            // Establish baseline before resolving UIKit: initial fixture requests never wiggle.
            let added = previousPending.map { pendingIDs.subtracting($0) } ?? []
            previousPending = pendingIDs
            // Resolve after SwiftUI has attached the representable to its parent.
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                guard let bar = self.resolveBar() else { self.report("bar-identity-unresolved"); return }
                if self.observedBar !== bar {
                    self.observedBar?.removeGestureRecognizer(self.taps)
                    bar.addGestureRecognizer(self.taps)
                    self.observedBar = bar
                }
                guard !added.isEmpty, !UIAccessibility.isReduceMotionEnabled else { return }
                // Notifications live in the first inbox card, not in the tab bar.
                guard let title = bar.items?.first(where: { $0.title == String(localized: "Notifications") })?.title,
                      let control = self.control(title: title, bar: bar) else { return }
                let symbols = self.images(in: control).filter { $0.image?.isSymbolImage == true }
                guard !symbols.isEmpty else {
                    let artwork = self.images(in: control).map { "\($0.bounds.size)/\($0.image?.size ?? .zero)/\($0.image?.isSymbolImage == true)" }
                    let views = self.descendants(control, UIView.self).map { "\(type(of: $0)):\($0.bounds.size)" }
                    self.report("bell-no-symbol:\(artwork); views:\(views)")
                    return
                }
                symbols.forEach { $0.addSymbolEffect(.wiggle) }
                self.report("bell-wiggle-submitted-to-symbol-image")
            }
        }
        @objc private func tapped(_ recognizer: UITapGestureRecognizer) {
            guard recognizer.state == .ended, !UIAccessibility.isReduceMotionEnabled,
                  let bar = observedBar else { return }
            let point = recognizer.location(in: bar)
            let controls = logicalControls(in: bar).filter {
                !$0.isHidden && $0.alpha > 0 && $0.convert($0.bounds, to: bar).contains(point)
            }
            guard controls.count == 1, let control = controls.first else {
                report("tap-control-identity-ambiguous:\(controls.count)"); return
            }
            guard let title = title(of: control, bar: bar) else {
                report("tap-title-or-image-identity-unresolved"); return
            }
            let artwork = images(in: control)
            let symbols = artwork.filter { $0.image?.isSymbolImage == true }
            if !symbols.isEmpty {
                symbols.forEach { $0.addSymbolEffect(.bounce, options: .speed(1.5)) }
                report("\(title)-bounce-submitted-to-symbol-image")
            } else if title == String(localized: "Profile") {
                report("Profile-bitmap-feedback-submitted")
                UIView.animate(withDuration: 0.15, animations: {
                    artwork.forEach { $0.transform = CGAffineTransform(scaleX: 0.96, y: 0.96) }
                }, completion: { _ in
                    UIView.animate(withDuration: 0.15) { artwork.forEach { $0.transform = .identity } }
                })
            } else {
                report("\(title)-image-is-not-symbol")
            }
        }
    }
}

#if DEBUG
/// Explicit opt-in real-core event producer; never changes account metadata or animation counters.
private struct NativeParityMotionDriver: View {
    @Environment(AppModel.self) private var model
    @State private var failure = ""
    @State private var effectDiagnostic = "not-observed"
    @State private var effectEvents = 0
    private var enabled: Bool {
        ProcessInfo.processInfo.environment["ZOEN_PARITY_MOTION_DRIVER"] == "1"
            && NativeParityClock.appliedManifest(core: model.core) != nil
    }
    private var witness: String {
        let pending = model.requests.filter { $0.status == .pending }.map {
            ["agentId": $0.agent.id, "spaceId": $0.spaceId, "title": $0.title, "status": "pending"]
        }.sorted { ($0["spaceId"]!, $0["agentId"]!, $0["title"]!) < ($1["spaceId"]!, $1["agentId"]!, $1["title"]!) }
        let value: [String: Any] = ["notifications": ["pendingCount": pending.count, "pending": pending],
                                  "driver_error": failure, "native_effect_diagnostic": effectDiagnostic, "native_effect_event": effectEvents, "reduce_motion": UIAccessibility.isReduceMotionEnabled]
        guard let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]) else { return "{}" }
        return String(decoding: data, as: UTF8.self)
    }
    var body: some View {
        if enabled {
            VStack {
                Button {
                    guard let existing = model.requests.first(where: { $0.agent.isMine }),
                          let agent = model.agents.first(where: { $0.persona.id == existing.agent.id }) else {
                        failure = "No owned agent/space request available"; return
                    }
                    do {
                        _ = try model.core.demoOpenRequest(spaceId: existing.spaceId, agentHandle: agent.persona.handle,
                            title: "Native motion external request", detail: "Explicit parity motion event",
                            audience: "Parity recipient", action: "external", cents: 0)
                        model.refresh()
                    } catch { failure = String(describing: error) }
                } label: {
                    Color.clear.frame(width: 44, height: 44).contentShape(.rect)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Open genuine request")
                .accessibilityIdentifier("parity-open-genuine-request")
                Color.clear.frame(width: 1, height: 1)
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(witness)
                    .accessibilityIdentifier("parity-motion-state")
            }
            .onReceive(NotificationCenter.default.publisher(for: Notification.Name("NativeParityEffectDiagnostic"))) {
                effectDiagnostic = $0.object as? String ?? "invalid-diagnostic"
                effectEvents += 1
            }
        }
    }
}
#endif

@main
struct ZoeniOSApp: App {
    @UIApplicationDelegateAdaptor(ZoenAppDelegate.self) private var delegate
    @State private var model = AppModel()
    @AppStorage(AppAppearance.key) private var appearance = AppAppearance.auto.rawValue
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            AdaptiveIOSRoot()
                .environment(model)
                #if DEBUG
                .overlay(alignment: .topLeading) {
                    if let manifest = NativeParityClock.appliedManifest(core: model.core) {
                        Color.clear.frame(width: 1, height: 1)
                            .accessibilityElement(children: .ignore)
                            .accessibilityIdentifier("parity-applied-fixture")
                            .accessibilityLabel(manifest)
                            .allowsHitTesting(false)
                        NativeParityMotionDriver().environment(model)
                        NativeParityGeometryWitness()
                            .frame(width: 1, height: 1)
                            .allowsHitTesting(false)
                    }
                }
                #endif
                .tint(Palette.action)
                .preferredColorScheme(AppAppearance(rawValue: appearance)?.scheme)
                .task { await model.applyLaunchOptions(); PushBridge.shared.start(model) }
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

/// Regular-width iPad and Mac use the same native sidebar/detail shell.
/// Compact width keeps the existing iPhone stacks and floating bar unchanged.
private struct AdaptiveIOSRoot: View {
    @Environment(\.horizontalSizeClass) private var sizeClass

    var body: some View {
        if sizeClass == .regular {
            ZoenAdaptiveShell()
        } else {
            RootView()
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
    @State private var selectedTab: AppTab = .conversations
    @State private var previousTab: AppTab = .conversations

    private var barVisible: Bool { model.path(model.tab).isEmpty }

    private var searchTabRole: TabRole {
        // Runtime availability alone cannot hide a symbol from older SDKs.
        #if compiler(>=6.4)
        if #available(iOS 27.0, *) {
            return .prominent
        }
        #endif
        return .search
    }

    private var rootTabs: some View {
        TabView(selection: $selectedTab) {
                Tab(value: AppTab.conversations) { stack(for: .conversations) } label: {
                    Label("Chats", systemImage: "bubble.left.and.bubble.right")
                }
                Tab(value: AppTab.store) { stack(for: .store) } label: {
                    Label("Marketplace", systemImage: "storefront")
                }
                Tab(value: AppTab.you) { stack(for: .you) } label: {
                    Label { Text("Profile") } icon: { profileTabImage }
                }
                Tab("Search", systemImage: "magnifyingglass", value: AppTab.search, role: searchTabRole) {
                    Color.clear
                }
            }
            .tabViewStyle(.tabBarOnly)
            .background(NativeTabImageEffects(pendingIDs: Set(model.requests.filter { $0.status == .pending }.map(\.id))))
            .overlay(alignment: .bottomLeading) {
                if barVisible {
                    ConnectionDot()
                        .padding(.leading, 28)
                        .padding(.bottom, 22)
                        .allowsHitTesting(false)
                }
            }
            .onAppear {
                selectedTab = model.tab
                previousTab = model.tab
            }
            .onChange(of: selectedTab) { _, tab in
                if tab == .search {
                    selectedTab = previousTab
                    model.newChatOpen = true
                } else {
                    previousTab = tab
                    if model.tab != tab { model.select(tab) }
                }
            }
            .onChange(of: model.tab) { _, tab in
                if [.conversations, .store, .you].contains(tab) { selectedTab = tab }
            }
    }

    var body: some View {
        @Bindable var model = model
        ZStack(alignment: .bottom) {
            rootTabs

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
                .toolbar(barVisible ? .visible : .hidden, for: .tabBar)
                .toolbar(model.path(t).isEmpty ? .hidden : .visible, for: .navigationBar)
                .navigationDestination(for: Route.self) { route in destination(route) }
        }
    }

    @ViewBuilder
    private func root(_ t: AppTab) -> some View {
        switch t {
        case .conversations: ConversationsScreen()
        case .files: SpacesDirectoryScreen()
        case .activity: ActivityScreen(initialList: true, onShowCards: { model.openNotifications() })
        case .agents: AgentsLibraryScreen()
        case .you: YouScreen()
        case .search: SearchScreen(onOpenItem: { id in model.push(.item(id)) })
        case .store: StoreScreen()
        }
    }

    private var profileTabImage: Image {
        // TabView extracts an Image, not an arbitrary SwiftUI subtree. Flatten the
        // actual avatar before handing it to the system tab bar.
        let renderer = ImageRenderer(content: Group {
            if let me = model.me {
                if let photo = AvatarPhotoStore.load(me.id) {
                    Image(uiImage: photo).resizable().scaledToFill()
                        .frame(width: 26, height: 26).clipShape(.circle)
                } else {
                    PersonAvatar(persona: me, size: 26)
                }
            }
        }.frame(width: 26, height: 26))
        renderer.scale = UIScreen.main.scale
        return Image(uiImage: (renderer.uiImage ?? UIImage()).withRenderingMode(.alwaysOriginal)).renderingMode(.original)
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
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("bottom-bar")
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
