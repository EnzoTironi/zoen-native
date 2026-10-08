import SwiftUI
import RodaCore
#if canImport(UserNotifications)
import UserNotifications
#endif

/// Life areas for the first plan (onboarding step 2).
enum OnboardingArea: String, CaseIterable, Identifiable {
    case trips, money, home, food, friends, work, health, family
    var id: String { rawValue }
    var title: String {
        switch self {
        case .trips: String(localized: "Trips")
        case .money: String(localized: "Money")
        case .home: String(localized: "Home")
        case .food: String(localized: "Food")
        case .friends: String(localized: "Friends")
        case .work: String(localized: "Work")
        case .health: String(localized: "Health")
        case .family: String(localized: "Family")
        }
    }
    var symbol: String {
        switch self {
        case .trips: "airplane"
        case .money: "creditcard"
        case .home: "house"
        case .food: "fork.knife"
        case .friends: "person.2"
        case .work: "briefcase"
        case .health: "heart"
        case .family: "figure.2.and.child.holdinghands"
        }
    }
}

/// First open: under 10 seconds to the aha (pick life areas → Zoen makes a real plan on
/// this device), no signup. Then three optional steps: how much agents do on their own
/// (a real signed Grant for Zoen), the notification pre-prompt and location. Each step
/// has Zo, the mascot, in its own pose and idle loop.
struct OnboardingFlow: View {
    @Environment(AppModel.self) private var model
    let onFinish: () -> Void

    enum Step: Int, CaseIterable { case hello, profile, areas, plan, agents, notifications, location, done }

    @State private var step: Step = .hello
    @State private var forward = true
    @State private var areas: [OnboardingArea] = []
    @State private var plan: ItemDetail?
    @State private var planning = false
    @State private var trust: TrustLevelDto = .act
    @State private var notify: Bool?
    @State private var location: Bool?
    @State private var name = ""
    @State private var handle = ""
    @State private var handleEdited = false
    @State private var profileError: String?
    @State private var creating = false
    @State private var pendingPhoto: PlatformImage?

    var body: some View {
        VStack(spacing: 0) {
            header
            GeometryReader { geo in
            ScrollView {
                VStack(spacing: geo.size.height < 560 ? 10 : 14) {
                    MascotView(pose: pose)
                        .frame(width: mascotSize(geo.size.height), height: mascotSize(geo.size.height))
                        .padding(.top, 2)
                    VStack(spacing: 6) {
                        Text(title)
                            .font(.system(geo.size.height < 560 ? .title2 : .title, design: .rounded).weight(.heavy))
                            .minimumScaleFactor(0.8)
                            .multilineTextAlignment(.center)
                            .foregroundStyle(InkPalette.ink)
                            .contentTransition(.opacity)
                        Text(subtitle)
                            .font(.body)
                            .multilineTextAlignment(.center)
                            .foregroundStyle(InkPalette.ink.opacity(0.6))
                            .contentTransition(.opacity)
                    }
                    .padding(.horizontal, 28)
                    controls
                        .padding(.horizontal, 20)
                        .padding(.top, 6)
                }
                .frame(maxWidth: 560)
                .frame(maxWidth: .infinity)
                .padding(.bottom, 12)
                .id(step)
                .transition(.asymmetric(insertion: .offset(x: forward ? 40 : -40).combined(with: .opacity),
                                        removal: .offset(x: forward ? -40 : 40).combined(with: .opacity)))
            }
            .scrollBounceBehavior(.basedOnSize)
            // The button owns the bottom inset, so content can never sit underneath it; if a
            // huge text size makes the step scroll, it fades out softly above the button.
            .safeAreaInset(edge: .bottom, spacing: 0) {
                continueButton
                    .background(alignment: .top) {
                        LinearGradient(colors: [InkPalette.paper.opacity(0), InkPalette.paper], startPoint: .top, endPoint: .bottom)
                            .frame(height: 22).offset(y: -22).allowsHitTesting(false)
                    }
            }
            }
        }
        .background(InkPalette.paper.ignoresSafeArea())
        .environment(\.colorScheme, .light)
        .task {
            // Numbers keep their meaning from before the profile step existed; "profile" opens it.
            let raw = UserDefaults.standard.string(forKey: "RodaOnboardingStep") ?? ""
            if let st = raw == "profile" ? Step.profile : Int(raw).flatMap({ Step(rawValue: $0 >= 1 ? $0 + 1 : $0) }) {
                if st.rawValue >= Step.areas.rawValue { areas = [.trips, .food] }   // demo jump (launch arg only)
                step = st
                if st == .plan { await makePlan() }
            }
            // `-RodaOnboardingAuto YES`: plays the whole flow by itself (screen recordings, demos).
            if UserDefaults.standard.bool(forKey: "RodaOnboardingAuto") {
                for _ in 0..<Step.allCases.count {
                    try? await Task.sleep(for: .seconds(step == .plan ? 3.4 : 2.8))
                    if step == .areas && areas.isEmpty {
                        for a in [OnboardingArea.trips, .food, .friends] {
                            withAnimation(.snappy) { areas.append(a) }
                            try? await Task.sleep(for: .milliseconds(450))
                        }
                    }
                    if planning {
                        while planning { try? await Task.sleep(for: .milliseconds(200)) }
                        try? await Task.sleep(for: .seconds(2.6))   // let the plan reveal breathe
                    }
                    if step == .done { break }
                    advance()
                }
            }
        }
    }

    // MARK: top bar: back + segmented progress

    private var header: some View {
        HStack(spacing: 14) {
            Button { go(-1) } label: {
                Image(systemName: "arrow.left").font(.system(size: 18, weight: .bold)).frame(width: 40, height: 40)
            }
            .buttonStyle(.plain)
            .foregroundStyle(InkPalette.ink)
            .opacity(step == .hello || planning ? 0 : 1)
            .disabled(step == .hello || planning)
            .accessibilityLabel("Back")

            HStack(spacing: 6) {
                ForEach(Step.allCases, id: \.self) { s in
                    Capsule()
                        .fill(s.rawValue <= step.rawValue ? Mascot.body : InkPalette.ink.opacity(0.12))
                        .frame(height: 5)
                }
            }
            .animation(.spring(duration: 0.45, bounce: 0.3), value: step)
            .accessibilityElement()
            .accessibilityLabel(String(localized: "Step \(step.rawValue + 1) of \(Step.allCases.count)"))

            Button { finish() } label: { Text("Skip").font(.subheadline.weight(.semibold)) }
                .buttonStyle(.plain)
                .foregroundStyle(InkPalette.ink.opacity(0.55))
                .frame(width: 44, height: 40)
                .opacity(step.rawValue >= Step.agents.rawValue && step != .done ? 1 : 0)
                .disabled(step.rawValue < Step.agents.rawValue || step == .done)
        }
        .padding(.horizontal, 12)
        .padding(.top, 6)
    }

    // MARK: per-step content

    /// One stage for the whole flow: the mascot keeps the same size, position and baseline on
    /// every step. Its size comes from the tallest step (the agents list), per device height,
    /// so every step fits one screen from the iPhone SE up; scrolling is only an AX fallback.
    private func mascotSize(_ height: CGFloat) -> CGFloat {
        let tallestStepContent: CGFloat = 400
        return min(230, max(110, height - tallestStepContent - 112))   // 112 ≈ the Continue inset + paddings
    }

    private var pose: MascotPose {
        switch step {
        case .hello: .wave
        case .profile: .phone
        case .areas: .map
        case .plan: .run
        case .agents: .juggle
        case .notifications: .phone
        case .location: .walk
        case .done: .cheer
        }
    }

    private var title: String {
        switch step {
        case .hello: String(localized: "Hi, I’m Zoen.")
        case .profile: String(localized: "What should friends call you?")
        case .areas: String(localized: "What could use a hand?")
        case .plan: planning || plan == nil ? String(localized: "One second…") : String(localized: "Your first plan")
        case .agents: String(localized: "How much should agents do on their own?")
        case .notifications: String(localized: "Can I tap you on the shoulder?")
        case .location: String(localized: "Plans that know where you are?")
        case .done: String(localized: "All set. Don’t make it weird.")
        }
    }

    private var subtitle: String {
        switch step {
        case .hello: String(localized: "I’m the app and the agent inside it. I turn chats into plans you control.")
        case .profile: String(localized: "Your @ is how people find you. No email, no password: your keys are made on this device and stay in its Keychain.")
        case .areas: String(localized: "Pick a few. Your first plan starts there.")
        case .plan: planning || plan == nil ? String(localized: "Planning on this device. Nothing leaves it.") : String(localized: "Edit anything. Every change is a version you can undo.")
        case .agents: String(localized: "You can change it per agent and per chat, anytime.")
        case .notifications: String(localized: "Only when an agent needs your OK. Never for marketing.")
        case .location: String(localized: "Used on this device, only when a plan needs it.")
        case .done: String(localized: "Your plan is waiting in our chat. I’ll keep an eye on it.")
        }
    }

    @ViewBuilder
    private var controls: some View {
        switch step {
        case .hello:
            EmptyView()
        case .profile:
            AvatarPhotoPicker(personaId: nil, pending: $pendingPhoto, size: 104,
                               initials: initialsFrom(name), tintHex: "#6B8F71")
                .frame(maxWidth: .infinity)
                .padding(.bottom, 4)
            ProfileFields(name: $name, handle: $handle, handleEdited: $handleEdited, error: profileError)
        case .areas:
            FlowPills(items: OnboardingArea.allCases, selected: Set(areas)) { a in
                Haptics.selectionTick()
                if let i = areas.firstIndex(of: a) { areas.remove(at: i) } else { areas.append(a) }
            }
        case .plan:
            ThinkingPlanCard(plan: plan, planning: planning)
        case .agents:
            VStack(spacing: 8) {
                ForEach([TrustLevelDto.listen, .suggest, .act], id: \.self) { level in
                    ChoiceRow(title: level.label, detail: onboardingDetail(level), symbol: level.symbol,
                              badge: level == .act ? String(localized: "Recommended") : nil,
                              selected: trust == level) {
                        Haptics.selectionTick()
                        trust = level
                    }
                }
            }
        case .notifications:
            VStack(spacing: 8) {
                ChoiceRow(title: String(localized: "Notify me"), detail: String(localized: "Approvals and mentions only."), symbol: "bell.badge", selected: notify == true) {
                    notify = true
                    requestNotifications()
                }
                ChoiceRow(title: String(localized: "Not now"), detail: String(localized: "You’ll see requests in Activity."), symbol: "bell.slash", selected: notify == false) { notify = false }
            }
        case .location:
            VStack(spacing: 8) {
                ChoiceRow(title: String(localized: "When I’m planning"), detail: String(localized: "We’ll ask iOS only when a plan needs it."), symbol: "location", selected: location == true) { location = true }
                ChoiceRow(title: String(localized: "Never"), detail: String(localized: "Plans stay generic about places."), symbol: "location.slash", selected: location == false) { location = false }
            }
        case .done:
            VStack(alignment: .leading, spacing: 12) {
                recap("checkmark.circle.fill", areas.isEmpty ? String(localized: "A starter plan") : String(localized: "A plan for \(areas.map(\.title.localizedLowercase).formatted(.list(type: .and)))"))
                recap("person.badge.shield.checkmark", String(localized: "Agents may: \(trust.label)"))
                recap(notify == true ? "bell.badge" : "bell.slash", notify == true ? String(localized: "I’ll tap you only for approvals") : String(localized: "Requests wait in Activity"))
            }
            .padding(18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.white, in: .rect(cornerRadius: 22, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).strokeBorder(Mascot.line.opacity(0.18), lineWidth: 1.5))
        }
    }

    private func onboardingDetail(_ level: TrustLevelDto) -> String {
        switch level {
        case .listen: String(localized: "Answers only when you ask.")
        case .suggest: String(localized: "Drafts things. Nothing goes out without you.")
        default: String(localized: "Does reversible things, always with Undo.")
        }
    }

    private func recap(_ symbol: String, _ text: String) -> some View {
        HStack(spacing: 12) {
            Image(systemName: symbol).font(.title3).foregroundStyle(Mascot.body).frame(width: 28)
            Text(text).font(.subheadline.weight(.medium)).foregroundStyle(InkPalette.ink)
        }
    }

    // MARK: sticky Continue (solid brand color)

    private var continueEnabled: Bool {
        switch step {
        case .profile: !creating && !name.trimmingCharacters(in: .whitespaces).isEmpty && handle.count >= 3
        case .areas: !areas.isEmpty
        case .plan: !planning && plan != nil
        default: true
        }
    }

    private var continueTitle: String {
        switch step {
        case .hello: String(localized: "Hi, Zoen!")
        case .plan: String(localized: "Looks good")
        case .done: String(localized: "Open Zoen")
        default: String(localized: "Continue")
        }
    }

    private var continueButton: some View {
        Button { advance() } label: {
            Text(continueTitle)
                .font(.headline)
                .foregroundStyle(continueEnabled ? Color.white : Mascot.line.opacity(0.55))
                .frame(maxWidth: 520)
                .frame(height: 56)
                .background(continueEnabled ? Mascot.body : Mascot.belly.opacity(0.35), in: .capsule)
                .overlay(Capsule().strokeBorder(Mascot.line.opacity(continueEnabled ? 0.9 : 0.25), lineWidth: 1.5))
                .shadow(color: Mascot.body.opacity(continueEnabled ? 0.35 : 0), radius: 12, y: 6)
        }
        .buttonStyle(.plain)
        .disabled(!continueEnabled)
        .padding(.horizontal, 20)
        .padding(.top, 6)
        .padding(.bottom, 10)
        .background(InkPalette.paper)
        .animation(.snappy, value: continueEnabled)
    }

    // MARK: actions

    private func go(_ delta: Int) {
        guard var next = Step(rawValue: step.rawValue + delta) else { return }
        // Demo builds and devices that already have an account skip the profile step.
        if next == .profile && !model.sync.needsAccount, let skip = Step(rawValue: next.rawValue + delta) { next = skip }
        forward = delta > 0
        withAnimation(.spring(duration: 0.45, bounce: 0.2)) { step = next }
    }

    private func advance() {
        Haptics.action()
        switch step {
        case .profile:
            createAccount()
        case .areas:
            go(1)
            Task { await makePlan() }
        case .agents:
            if let zid = model.zoenSpaceId(), let zoen = model.space(zid)?.counterpart {
                model.perform { try model.core.setTrust(agentId: zoen.id, spaceId: zid, level: trust) }
            }
            go(1)
        case .location:
            go(1)
            Haptics.commit()
        case .done:
            finish()
        default:
            go(1)
        }
    }

    private func createAccount() {
        creating = true
        defer { creating = false }
        do {
            try model.sync.createAccount(core: model.core, name: name, handle: handle)
            model.refresh()
            if let img = pendingPhoto, let id = model.me?.id ?? model.sync.account?.identityId {
                AvatarPhotoStore.save(id, image: img)
            }
            profileError = nil
            Haptics.commit()
            go(1)
        } catch {
            profileError = (error as? CoreError)?.message ?? error.localizedDescription
        }
    }

    private func makePlan() async {
        guard plan == nil, !planning else { return }
        planning = true
        let made = await model.onboardingPlan(areas: areas)
        withAnimation(.spring(duration: 0.6, bounce: 0.25)) {
            plan = made
            planning = false
        }
        if made != nil { Haptics.commit() }
    }

    private func requestNotifications() {
        #if canImport(UserNotifications)
        Task { _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .badge, .sound]) }
        #endif
    }

    private func finish() {
        UserDefaults.standard.set(true, forKey: "RodaOnboarded")
        if let zid = model.zoenSpaceId() { model.go(.space(zid)) }
        onFinish()
    }

    /// First launch only. Launch arguments (demo shortcuts, screenshots) skip it, except
    /// `-RodaOnboarding YES`, which forces it.
    static var shouldShow: Bool {
        let d = UserDefaults.standard
        if d.bool(forKey: "RodaOnboarding") { return true }
        if ProcessInfo.processInfo.arguments.contains(where: { $0.hasPrefix("-Roda") }) { return false }
        return !d.bool(forKey: "RodaOnboarded")
    }
}

/// Wrapping pills (multi-select).
struct FlowPills: View {
    let items: [OnboardingArea]
    let selected: Set<OnboardingArea>
    let onTap: (OnboardingArea) -> Void

    var body: some View {
        WrapLayout(spacing: 10) {
            ForEach(items) { a in
                let on = selected.contains(a)
                Button { onTap(a) } label: {
                    Label(a.title, systemImage: a.symbol)
                        .font(.body.weight(.semibold))
                        .padding(.horizontal, 16)
                        .padding(.vertical, 11)
                        .foregroundStyle(on ? .white : InkPalette.ink)
                        .background(on ? Mascot.body : .white, in: .capsule)
                        .overlay(Capsule().strokeBorder(on ? Mascot.line : InkPalette.ink.opacity(0.18), lineWidth: on ? 1.5 : 1))
                        .scaleEffect(on ? 1.04 : 1)
                }
                .buttonStyle(.plain)
                .animation(.spring(duration: 0.3, bounce: 0.45), value: on)
                .accessibilityAddTraits(on ? .isSelected : [])
            }
        }
    }
}

struct WrapLayout: Layout {
    var spacing: CGFloat = 8
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? 360
        var x: CGFloat = 0, y: CGFloat = 0, row: CGFloat = 0
        for s in subviews {
            let size = s.sizeThatFits(.unspecified)
            if x > 0 && x + size.width > width { x = 0; y += row + spacing; row = 0 }
            x += size.width + spacing
            row = max(row, size.height)
        }
        return CGSize(width: width, height: y + row)
    }
    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        // Center each row.
        var rows: [[(Subviews.Element, CGSize)]] = [[]]
        var x: CGFloat = 0
        for s in subviews {
            let size = s.sizeThatFits(.unspecified)
            if x > 0 && x + size.width > bounds.width { rows.append([]); x = 0 }
            rows[rows.count - 1].append((s, size))
            x += size.width + spacing
        }
        var y = bounds.minY
        for row in rows {
            let w = row.reduce(0) { $0 + $1.1.width } + spacing * CGFloat(max(0, row.count - 1))
            var cx = bounds.minX + (bounds.width - w) / 2
            let h = row.map(\.1.height).max() ?? 0
            for (s, size) in row {
                s.place(at: CGPoint(x: cx, y: y), proposal: ProposedViewSize(size))
                cx += size.width + spacing
            }
            y += h + spacing
        }
    }
}

struct ChoiceRow: View {
    let title: String
    let detail: String
    let symbol: String
    var badge: String? = nil
    let selected: Bool
    let onTap: () -> Void
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        Button(action: onTap) {
            HStack(spacing: 12) {
                Image(systemName: symbol)
                    .font(.system(size: 16, weight: .semibold))
                    .frame(width: 34, height: 34)
                    .foregroundStyle(selected ? .white : Mascot.body)
                    .background(selected ? Mascot.body : Mascot.belly.opacity(0.35), in: .circle)
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 6) {
                        Text(title).font(.body.weight(.bold))
                        if let badge {
                            Text(badge).font(.caption2.weight(.bold)).padding(.horizontal, 7).padding(.vertical, 2)
                                .foregroundStyle(.white).background(Mascot.accent, in: .capsule)
                        }
                    }
                    Text(detail).font(.footnote).foregroundStyle(InkPalette.ink.opacity(0.6)).multilineTextAlignment(.leading)
                        .lineLimit(typeSize.isAccessibilitySize ? 4 : 2).minimumScaleFactor(0.9)
                }
                Spacer(minLength: 0)
                Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                    .font(.title3)
                    .foregroundStyle(selected ? Mascot.body : InkPalette.ink.opacity(0.25))
            }
            .foregroundStyle(InkPalette.ink)
            .padding(.horizontal, 14).padding(.vertical, 10)
            .background(.white, in: .rect(cornerRadius: 18, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(selected ? Mascot.body : InkPalette.ink.opacity(0.12), lineWidth: selected ? 2 : 1))
        }
        .buttonStyle(.plain)
        .animation(.spring(duration: 0.3, bounce: 0.3), value: selected)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// One container that thinks (shimmering brand gradient) and then smoothly resizes into
/// the plan, instead of swapping instantly.
struct ThinkingPlanCard: View {
    let plan: ItemDetail?
    let planning: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let plan, let p = plan.plan {
                HStack(alignment: .firstTextBaseline) {
                    Text(p.title).font(.headline)
                    Spacer()
                    if p.totalCents > 0 { Text(Money.format(p.totalCents)).font(.headline).monospacedDigit() }
                }
                ForEach(Array(p.sections.prefix(4).enumerated()), id: \.offset) { _, s in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(s.title.uppercased()).font(.caption2.weight(.bold)).foregroundStyle(Mascot.body)
                        ForEach(Array(s.lines.prefix(2).enumerated()), id: \.offset) { _, l in
                            HStack {
                                Image(systemName: "circle").font(.caption2).foregroundStyle(InkPalette.ink.opacity(0.35))
                                Text(l.text).font(.subheadline).lineLimit(1)
                                Spacer()
                                if l.costCents > 0 { Text(Money.format(l.costCents)).font(.subheadline).monospacedDigit().foregroundStyle(InkPalette.ink.opacity(0.6)) }
                            }
                        }
                    }
                }
                Text(plan.origin).font(.caption2).foregroundStyle(InkPalette.ink.opacity(0.5)).lineLimit(2)
                    .transition(.opacity)
            } else {
                ThinkingShimmer(phrases: [String(localized: "Reading what you picked…"), String(localized: "Thinking on this device…"), String(localized: "Putting it together…")], compact: false)
                    .frame(maxWidth: .infinity, alignment: .center)   // centered under the title
            }
        }
        .foregroundStyle(InkPalette.ink)
        .padding(plan == nil ? 6 : 16)
        // No container while thinking: just the centered pill. The white card grows in with the plan.
        .background(plan == nil ? Color.clear : Color.white, in: .rect(cornerRadius: 22, style: .continuous))
        .overlay {
            if plan != nil {
                RoundedRectangle(cornerRadius: 22, style: .continuous).strokeBorder(InkPalette.ink.opacity(0.12), lineWidth: 1)
            }
        }
        .animation(.spring(duration: 0.6, bounce: 0.25), value: plan?.id)
    }
}

// MARK: - Starter plan

extension AppModel {
    /// The aha: Zoen turns the picked areas into a real plan in its chat (signed Item).
    func onboardingPlan(areas: [OnboardingArea]) async -> ItemDetail? {
        guard let zid = zoenSpaceId(), let zoen = space(zid)?.counterpart else { return nil }
        let list = ListFormatter.localizedString(byJoining: areas.map { $0.title.lowercased() })
        let prompt = String(localized: "Make me a starter plan for the next two weeks: \(list).")
        perform { try core.sendMessage(spaceId: zid, text: prompt) }
        let draft = await planner.makeStarterPlan(areas: areas, prompt: prompt)
        let outcome = perform {
            try core.agentCreatePlan(spaceId: zid, agentId: zoen.id, prompt: prompt, plan: draft.plan, engineLabel: draft.engineLabel, aiCostCents: 0)
        }
        return outcome?.item
    }
}

extension LocalFallbackPlanner {
    /// Deterministic starter plan (no AI): one section per picked area, two concrete lines.
    static func starter(areas: [OnboardingArea]) -> PlanDto {
        let L = AppLocale.pick
        func line(_ pt: String, _ en: String, _ dollars: Int64) -> (String, Int64) {
            (L(pt, en), AppLocale.isPortuguese ? dollars * 500 : dollars * 100)
        }
        let picked = areas.isEmpty ? [OnboardingArea.trips, .food] : Array(areas.prefix(4))
        let sections: [PlanSectionDto] = picked.map { a in
            let lines: [(String, Int64)]
            switch a {
            case .trips: lines = [line("Escolher destino e datas do feriado", "Pick a destination and dates for the long weekend", 0), line("Reservar pousada bem avaliada · 2 noites", "Book a well-rated inn · 2 nights", 420)]
            case .money: lines = [line("Revisar as contas do mês", "Review this month’s bills", 0), line("Separar a reserva da viagem", "Move money into the trip fund", 200)]
            case .home: lines = [line("Consertar a torneira da cozinha", "Fix the leaky kitchen tap", 60), line("Faxina de sábado de manhã", "Deep clean on Saturday morning", 0)]
            case .food: lines = [line("Jantar em casa no sábado para 4", "Dinner at home on Saturday for 4", 120), line("Lista de mercado da semana", "This week’s grocery list", 90)]
            case .friends: lines = [line("Marcar a data do encontro da turma", "Pick a date for the group hangout", 0), line("Reservar mesa para 6", "Book a table for 6", 180)]
            case .work: lines = [line("Bloquear duas manhãs de foco", "Block two focus mornings", 0), line("Preparar a revisão de quinta", "Prep Thursday’s review", 0)]
            case .health: lines = [line("Três corridas na semana", "Three runs this week", 0), line("Marcar o check-up", "Book a check-up", 80)]
            case .family: lines = [line("Almoço de domingo com a família", "Sunday lunch with the family", 60), line("Ligar para a vó", "Call grandma", 0)]
            }
            return PlanSectionDto(title: a.title, lines: lines.map { PlanLineDto(id: "", text: $0.0, costCents: $0.1, done: false) })
        }
        return PlanDto(title: L("Suas próximas duas semanas", "Your next two weeks"),
                       summary: L("Um primeiro rascunho a partir do que você escolheu. Edite à vontade.", "A first draft from what you picked. Edit anything."),
                       budgetCents: nil, sections: sections, totalCents: 0)
    }
}

private func initialsFrom(_ name: String) -> String {
    let parts = name.split { $0.isWhitespace }.prefix(2)
    let chars = parts.compactMap({ $0.first }).map(String.init)
    return chars.isEmpty ? "?" : chars.joined().uppercased()
}

/// Name and @handle for the account (onboarding). The @ follows the name until you edit it.
struct ProfileFields: View {
    @Binding var name: String
    @Binding var handle: String
    @Binding var handleEdited: Bool
    var error: String?
    @FocusState private var focus: Field?
    enum Field { case name, handle }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            TextField(String(localized: "Your name"), text: $name)
                .textContentType(.name)
                .focused($focus, equals: .name)
                .submitLabel(.next)
                .onSubmit { focus = .handle }
                .onChange(of: name) { _, v in if !handleEdited { handle = Self.suggest(v) } }
                .modifier(OnboardingField())
            HStack(spacing: 2) {
                Text(verbatim: "@").font(.title3.weight(.semibold)).foregroundStyle(InkPalette.ink.opacity(0.45))
                TextField(String(localized: "handle"), text: Binding(get: { handle }, set: { handle = Self.clean($0); handleEdited = true }))
                    .textContentType(.username)
                    .autocorrectionDisabled()
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    #endif
                    .focused($focus, equals: .handle)
            }
            .modifier(OnboardingField())
            if let error {
                Text(error).font(.footnote.weight(.medium)).foregroundStyle(.red.opacity(0.85))
                    .transition(.opacity)
            }
        }
        .task { focus = .name }
    }

    static func clean(_ raw: String) -> String {
        let folded = raw.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: nil).lowercased()
        return String(folded.filter { $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "." || $0 == "_") }.prefix(24))
    }

    static func suggest(_ name: String) -> String {
        var h = clean(name.replacingOccurrences(of: " ", with: ""))
        while let f = h.first, !f.isLetter { h.removeFirst() }
        return String(h.prefix(20))
    }
}

private struct OnboardingField: ViewModifier {
    func body(content: Content) -> some View {
        content
            .font(.title3.weight(.medium))
            .foregroundStyle(InkPalette.ink)
            .padding(.horizontal, 16)
            .frame(height: 54)
            .background(.white, in: .rect(cornerRadius: 16, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(Mascot.line.opacity(0.18), lineWidth: 1.5))
    }
}
