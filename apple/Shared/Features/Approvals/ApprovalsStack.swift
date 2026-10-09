import SwiftUI
import RodaCore
#if os(iOS)
import CoreMotion
#endif

// Catch-up stack for approval requests (Slack's swipe cards, in Zoen's ink and glass).
// Right approves, left denies, up always approves, down always denies; tap opens the
// details, where the same swipes and buttons work. Decisions wait a few seconds behind
// an undo toast before they reach the core.

/// The four swipes and what each one means.
enum ApprovalSwipe: CaseIterable, Identifiable {
    case right, left, up, down
    var id: Self { self }

    var decision: RequestDecision {
        switch self {
        case .right: .approve
        case .left: .deny
        case .up: .alwaysApprove
        case .down: .alwaysDeny
        }
    }

    var isStanding: Bool { self == .up || self == .down }

    var label: String {
        switch self {
        case .right: String(localized: "Approve")
        case .left: String(localized: "Deny")
        case .up: String(localized: "Always approve")
        case .down: String(localized: "Always deny")
        }
    }

    /// Past tense, for the undo toast.
    var done: String {
        switch self {
        case .right: String(localized: "Approved")
        case .left: String(localized: "Denied")
        case .up: String(localized: "Always approved")
        case .down: String(localized: "Always denied")
        }
    }

    var tint: Color {
        switch self {
        case .right: Palette.action
        case .left: Palette.danger
        case .up: Color(hex: "#0F8B8D")
        case .down: InkPalette.ink
        }
    }

    var symbol: String {
        switch self {
        case .right: "checkmark"
        case .left: "xmark"
        case .up: "checkmark.seal.fill"
        case .down: "hand.raised.fill"
        }
    }

    var accessibilityId: String {
        switch self {
        case .right: "approval-approve"
        case .left: "approval-deny"
        case .up: "approval-always-approve"
        case .down: "approval-always-deny"
        }
    }

    /// Where the label sits on the card while you drag.
    var labelAlignment: Alignment {
        switch self {
        case .right: .topLeading
        case .left: .topTrailing
        case .up: .bottom
        case .down: .top
        }
    }

    /// Sideways is quick; up and down are standing decisions, so they need a longer pull.
    var threshold: CGFloat { isStanding ? 170 : 110 }
}

struct ApprovalsStackView: View {
    var showsClose = true
    var onShowList: (() -> Void)? = nil
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AppStorage("RodaApprovalsExplained") private var explained = false

    @State private var deck: [AgentRequestDto] = []
    /// Decided here but not yet sent to the core (undo window), or already sent.
    @State private var hidden: Set<String> = []
    @State private var decided = 0
    @State private var pending: Pending?
    /// The finger's raw translation (what thresholds read). The card shows `shown(_:)` of it.
    @State private var drag: CGSize = .zero
    /// Grabbed in the top half: the card pivots on its bottom edge (and the other way round).
    @State private var grabTop = true
    @State private var pressed = false
    @State private var flying: ApprovalSwipe?
    /// Where the card flies to (continues the finger) and the spin it carries.
    @State private var exit: CGSize = .zero
    @State private var exitSpin: Double = 0
    @State private var stamping: ApprovalSwipe?
    @State private var crossed: ApprovalSwipe?
    @State private var cardWidth: CGFloat = 360
    @State private var cardHeight: CGFloat = 560
    @State private var commitTick = 0
    /// Cards fall into place when the stack opens (Wallet-style: weight, then a settle).
    @State private var entered = false
    /// Ids already shown; anything new that arrives live drops in from above.
    @State private var known: Set<String> = []
    @State private var arriving: Set<String> = []
    @State private var arrivalTick = 0
    @State private var commitFeel: SensoryFeedback = .impact(weight: .light)
    @State private var expanded = false
    @State private var lockedHint = false
    @State private var explainerStep = 0
    /// Cards have been on screen at least once (a deep link can open the stack before the
    /// first sync lands; that's not "all caught up").
    @State private var seenCards = false

    struct Pending: Equatable {
        let id: String
        let ids: [String]
        let swipe: ApprovalSwipe
        let title: String
        let agent: String
        let token = UUID()
    }

    private var visible: [AgentRequestDto] { deck.filter { !hidden.contains($0.id) } }
    private var showingExplainers: Bool { !explained && explainerStep < ApprovalExplainer.all.count && !visible.isEmpty }
    private var progress: Double {
        let total = decided + visible.count
        return total == 0 ? (seenCards ? 1 : 0) : Double(decided) / Double(total)
    }

    var body: some View {
        VStack(spacing: 14) {
            header
            ZStack {
                if showingExplainers {
                    explainerDeck
                } else if !visible.isEmpty {
                    cards
                } else if seenCards {
                    AllCaughtUpView(onClose: showsClose ? { close() } : nil)
                        .transition(.opacity.combined(with: .scale(scale: 0.96)))
                } else {
                    InkEmptyState(pose: .zen, title: String(localized: "No approval requests here yet"),
                                  message: String(localized: "Requests appear here when your agents need a decision. Use List for mentions, tasks, and past requests."))
                        .accessibilityIdentifier("approvals-empty")
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .padding(.horizontal, 20)
            .padding(.top, 10)
            .onGeometryChange(for: CGSize.self) { $0.size } action: {
                cardWidth = max(200, $0.width - 40); cardHeight = max(200, $0.height - 10)
            }
            .overlay(alignment: .bottom) { toast.padding(.bottom, 6) }
            if !showingExplainers, let top = visible.first {
                buttons(for: top)
                    .transition(.opacity)
            }
        }
        .padding(.bottom, 8)
        .background(Palette.background.ignoresSafeArea())
        .animation(.spring(response: 0.55, dampingFraction: 0.66), value: visible.map(\.id))
        .onAppear(perform: sync)
        #if os(iOS) && DEBUG
        .task {
            // Frame-pacing baseline: the stack sitting still, before any drag.
            guard FramePacingProbe.enabled else { return }
            try? await Task.sleep(for: .seconds(2.5))
            FramePacingProbe.shared.begin("idle")
            try? await Task.sleep(for: .seconds(1.5))
            FramePacingProbe.shared.end()
        }
        #endif
        .onChange(of: model.revision) { _, _ in sync() }
        .onChange(of: visible.isEmpty, initial: true) { _, empty in
            if !empty {
                seenCards = true
                if !entered { DispatchQueue.main.asyncAfter(deadline: .now() + 0.12) { entered = true } }
            }
        }
        .sensoryFeedback(.impact(weight: .medium, intensity: 0.7), trigger: arrivalTick)
        .onDisappear { commitPending() }
        // Crossing a threshold (or back): a light, crisp tick. A red-line "always" warns.
        .sensoryFeedback(trigger: crossed) { old, new in
            if new == .up, let top = visible.first, !top.canAlwaysApprove { return .warning }
            return old == new ? nil : .impact(weight: .light)
        }
        .sensoryFeedback(trigger: commitTick) { _, _ in commitFeel }
        // The next card settles on top: a tiny tap.
        .sensoryFeedback(.impact(weight: .light, intensity: 0.45), trigger: visible.first?.id)
    }

    // MARK: header (no numbers: the bar alone shows progress)

    private var header: some View {
        HStack(spacing: 14) {
            if showsClose || expanded {
                Button { if expanded { withAnimation(.spring(duration: 0.4)) { expanded = false } } else { close() } } label: {
                    ZoenIcon(.back, size: 18)
                        .foregroundStyle(Palette.textPrimary)
                        .frame(width: 44, height: 44)
                        .glassEffect(.regular.interactive(), in: .circle)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(expanded ? Text("Back to the cards") : Text("Close"))
                .accessibilityIdentifier("approvals-back")
            }

            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Palette.textPrimary.opacity(0.08))
                    Capsule().fill(Palette.action)
                        .frame(width: max(8, geo.size.width * progress))
                        .animation(.spring(duration: 0.5, bounce: 0.15), value: progress)
                }
            }
            .frame(height: 6)
            .accessibilityElement()
            .accessibilityLabel(Text("Approvals"))
            .accessibilityValue(Text("\(decided) of \(decided + visible.count) decided"))
            .accessibilityIdentifier("approvals-progress")

            Button {
                if let onShowList {
                    commitPending()
                    onShowList()
                } else {
                    close()
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) {
                        model.setPath(.activity, [])
                        model.notificationsOpen = true
                    }
                }
            } label: {
                Label("List", systemImage: "list.bullet")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(Palette.textPrimary)
                    .padding(.horizontal, 14)
                    .frame(height: 44)
                    .glassEffect(.regular.interactive(), in: .capsule)
            }
            .buttonStyle(.plain)
            .disabled(flying != nil || stamping != nil)
            .accessibilityHint(Text("View mentions, tasks, and approval history"))
            .accessibilityIdentifier("notifications-list")
        }
        .padding(.horizontal, 18)
        .padding(.top, 6)
    }

    // MARK: cards

    /// Sideways is free; up and down pull against a rubber band until the "always"
    /// threshold, so a standing decision always feels deliberate.
    private func shown(_ raw: CGSize, _ r: AgentRequestDto) -> CGSize {
        if reduceMotion { return .zero }
        let a = abs(raw.height), s: CGFloat = raw.height < 0 ? -1 : 1
        let T = ApprovalSwipe.up.threshold
        let y: CGFloat
        if raw.height < 0 && !r.canAlwaysApprove {
            y = -sqrt(a) * 4.5                                   // red line: it won't go
        } else {
            y = s * (a < T ? a * 0.5 : T * 0.5 + (a - T) * 0.92)  // resistance, then it gives
        }
        return CGSize(width: raw.width, height: y)
    }

    /// Proportional to x, capped at 12°, pivoting on the edge opposite the finger.
    private var tilt: Double {
        guard !reduceMotion else { return 0 }
        let a = Double(drag.width / cardWidth) * 18
        return min(12, max(-12, a)) * (grabTop ? 1 : -1)
    }

    private var leading: (ApprovalSwipe, CGFloat)? {
        guard let d = direction(for: drag) else { return nil }
        return (d, swipeProgress(drag, d))
    }

    private var cards: some View {
        let lead = leading
        // The next card rises with the pull, live; all the way once the top one is gone.
        let lift: CGFloat = flying != nil ? 1 : min(1, (lead?.1 ?? 0) * 0.9)
        return ZStack {
            ForEach(Array(visible.prefix(3).enumerated()), id: \.element.id) { i, r in
                deckCard(r, depth: CGFloat(i), lift: lift, lead: lead)
                    .zIndex(Double(3 - i))
            }
        }
    }

    /// One card in the deck. Same view for the top and the ones behind (only values change),
    /// so a card that comes to the top keeps its identity: no cross-fade, no relayout.
    private func deckCard(_ r: AgentRequestDto, depth i: CGFloat, lift: CGFloat, lead: (ApprovalSwipe, CGFloat)?) -> some View {
        let top = i == 0
        let back = max(0, i - lift)                     // 0 = on top
        let offset = top ? (flying != nil ? exit : shown(drag, r)) : CGSize(width: 0, height: reduceMotion ? 0 : -(cardHeight * 0.0275 + 11) * back)
        let angle = top ? tilt + (flying != nil ? exitSpin : 0) : 0
        let scale = top ? (pressed && flying == nil ? 1.02 : 1) : 1 - 0.055 * back
        let locked = lead?.0 == .up && !r.canAlwaysApprove
        let drop = reduceMotion ? 0 : (entered ? 0 : -(cardHeight + 160 + i * 70))
        return TiltSheen(active: top && !pressed && flying == nil && drag == .zero && entered) {
            ApprovalCardView(request: r, expanded: top && expanded,
                             history: top ? history(for: r) : [], standing: top ? model.standing(for: r.agent.id) : [],
                             showHint: top && decided == 0)
        }
            .overlay {
                // Back cards sit a little dimmer, as if further from the light.
                RoundedRectangle(cornerRadius: ApprovalCardView.radius, style: .continuous)
                    .fill(Palette.background.opacity(top ? 0 : Double(min(0.4, 0.22 * back))))
                    .allowsHitTesting(false)
            }
            .overlay {
                if top { tintOverlay(dir: flying ?? lead?.0, progress: flying != nil ? 1 : (lead?.1 ?? 0), locked: locked) }
            }
            .overlay {
                if top, let s = stamping {
                    AlwaysStamp(tint: s.tint)
                        .transition(.scale(scale: 1.5).combined(with: .opacity))
                }
            }
            .scaleEffect(scale)
            // Lift on touch: the shadow grows, and leans away from the tilt.
            .shadow(color: .black.opacity(top ? (pressed ? 0.16 : 0.09) : 0.05),
                    radius: top && pressed ? 30 : 16,
                    x: top ? CGFloat(-angle) * 1.1 : 0, y: top && pressed ? 18 : 8)
            .rotationEffect(.degrees(angle), anchor: grabTop ? .bottom : .top)
            .offset(offset)
            .offset(y: drop)
            .rotationEffect(.degrees(entered || reduceMotion ? 0 : (i == 0 ? -5 : 4)))
            .opacity(reduceMotion && !entered ? 0 : 1)
            // Back cards land first, the top one last, each with a little bounce.
            .animation(reduceMotion ? .easeOut(duration: 0.25) : .spring(response: 0.62, dampingFraction: 0.68).delay(Double(2 - min(2, i)) * 0.07), value: entered)
            .transition(arriving.contains(r.id) && !reduceMotion
                        ? .asymmetric(insertion: .offset(y: -(cardHeight + 200)).combined(with: .scale(scale: 1.04)), removal: .opacity)
                        : .opacity)
            .opacity(top && flying != nil && reduceMotion ? 0 : 1)
            .allowsHitTesting(top)
            .gesture(dragGesture(for: r), isEnabled: top)
            .accessibilityElement(children: .combine)
            .accessibilityAddTraits(.isButton)
            .accessibilityHint(Text("Actions: approve, deny, always approve, always deny, details."))
            .accessibilityActions {
                ForEach(available(for: r)) { s in
                    Button(s.label) { decide(s) }
                }
                Button(expanded ? String(localized: "Hide details") : String(localized: "Show details")) { toggleDetails() }
            }
            .accessibilityHidden(!top)
            .accessibilityIdentifier(top ? "approval-card" : "approval-card-behind")
    }

    private func toggleDetails() {
        withAnimation(.spring(response: 0.42, dampingFraction: 0.82)) { expanded.toggle() }
    }

    @ViewBuilder
    private func tintOverlay(dir: ApprovalSwipe?, progress p: CGFloat, locked: Bool) -> some View {
        if let dir {
            let color = locked ? Palette.textTertiary : dir.tint
            // Fades in with the pull; the label grows a touch as you near the threshold.
            let ramp = Double(min(1, max(0, (p - 0.12) / 0.6)))
            ZStack(alignment: dir.labelAlignment) {
                RoundedRectangle(cornerRadius: ApprovalCardView.radius, style: .continuous)
                    .fill(color.opacity(min(0.5, Double(p) * 0.42 + (p >= 1 ? 0.08 : 0))))
                HStack(spacing: 8) {
                    Image(systemName: locked ? "lock.fill" : dir.symbol)
                        .font(.system(size: 16, weight: .heavy))
                    Text(locked ? String(localized: "Always asks") : dir.label)
                        .font(.system(size: 17, weight: .bold, design: .rounded))
                }
                .foregroundStyle(.white)
                .padding(.horizontal, 16).padding(.vertical, 11)
                .background(color, in: .capsule)
                .shadow(color: color.opacity(0.35), radius: 8, y: 3)
                .rotationEffect(.degrees(dir == .right ? -8 : dir == .left ? 8 : 0))
                .padding(24)
                .opacity(ramp)
                .scaleEffect(0.86 + 0.22 * min(1, p))
            }
            .allowsHitTesting(false)
            .accessibilityHidden(true)
        }
    }

    private func dragGesture(for r: AgentRequestDto) -> some Gesture {
        // Zero distance, so the card lifts the moment you touch it; a short tap flips it.
        DragGesture(minimumDistance: 0)
            .onChanged { v in
                guard flying == nil, stamping == nil else { return }
                if !pressed {
                    #if os(iOS) && DEBUG
                    FramePacingProbe.shared.begin("card-drag")
                    #endif
                    grabTop = v.startLocation.y < cardHeight / 2
                    withAnimation(.spring(response: 0.25, dampingFraction: 0.75)) { pressed = true }
                }
                drag = v.translation           // 1:1, no animation
                let dir = direction(for: v.translation)
                let over = dir.flatMap { swipeProgress(v.translation, $0) >= 1 ? $0 : nil }
                if over != crossed { crossed = over }
            }
            .onEnded { v in
                #if os(iOS) && DEBUG
                // Keep measuring through the settle / fly-out, then report.
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) { FramePacingProbe.shared.end() }
                #endif
                guard flying == nil, stamping == nil else { return }
                crossed = nil
                let t = v.translation
                if hypot(t.width, t.height) < 8 {
                    withAnimation(.spring(response: 0.3, dampingFraction: 0.7)) { pressed = false; drag = .zero }
                    toggleDetails()
                    return
                }
                guard let dir = direction(for: t) else { return settle(v.velocity, r) }
                let along: CGFloat = switch dir {
                case .right: v.velocity.width
                case .left: -v.velocity.width
                case .up: -v.velocity.height
                case .down: v.velocity.height
                }
                // A quick flick commits short of the line; up and down still need half the pull.
                let flick = dir.isStanding ? (along > 1100 && swipeProgress(t, dir) > 0.5) : (along > 650 && abs(t.width) > 24)
                guard swipeProgress(t, dir) >= 1 || flick else { return settle(v.velocity, r) }
                if dir == .up && !r.canAlwaysApprove {
                    withAnimation(.spring(duration: 0.3)) { lockedHint = true }
                    DispatchQueue.main.asyncAfter(deadline: .now() + 3) { withAnimation { lockedHint = false } }
                    return settle(v.velocity, r)
                }
                decide(dir, velocity: v.velocity)
            }
    }

    /// Back to the middle with an interactive spring that keeps the finger's speed.
    private func settle(_ velocity: CGSize, _ r: AgentRequestDto) {
        let at = shown(drag, r)
        let d2 = max(1, at.width * at.width + at.height * at.height)
        let toward = -(velocity.width * at.width + velocity.height * at.height) / d2
        withAnimation(.interpolatingSpring(Spring(response: 0.35, dampingRatio: 0.7), initialVelocity: min(30, max(-10, toward)))) {
            drag = .zero
            pressed = false
        }
    }

    private func direction(for t: CGSize) -> ApprovalSwipe? {
        guard hypot(t.width, t.height) > 8 else { return nil }
        // Vertical has to clearly dominate: a sloppy sideways swipe is still sideways.
        if abs(t.height) > abs(t.width) * 1.15 { return t.height < 0 ? .up : .down }
        return t.width > 0 ? .right : .left
    }

    private func swipeProgress(_ t: CGSize, _ d: ApprovalSwipe) -> CGFloat {
        let v: CGFloat = (d == .left || d == .right) ? abs(t.width) : abs(t.height)
        return v / d.threshold
    }

    private func available(for r: AgentRequestDto) -> [ApprovalSwipe] {
        ApprovalSwipe.allCases.filter { $0 != .up || r.canAlwaysApprove }
    }

    // MARK: deciding

    /// `velocity` is the finger's at release (nil for the buttons and VoiceOver).
    private func decide(_ s: ApprovalSwipe, velocity: CGSize? = nil) {
        guard flying == nil, stamping == nil, let top = visible.first else { return }
        if s == .up && !top.canAlwaysApprove {
            commitFeel = .warning; commitTick += 1
            withAnimation(.spring(duration: 0.3)) { lockedHint = true }
            DispatchQueue.main.asyncAfter(deadline: .now() + 3) { withAnimation { lockedHint = false } }
            return
        }
        // The previous card's undo window closes now, but its write to the core waits until
        // this card has flown: committing (core write + refresh) mid-throw was a 200+ ms hitch.
        let previous = pending
        if previous != nil { withAnimation(.easeOut(duration: 0.2)) { pending = nil } }
        // A standing decision settles the other cards it covers, here and in the core.
        let covered = s.isStanding ? visible.dropFirst().filter {
            $0.agent.id == top.agent.id && $0.spaceId == top.spaceId && $0.actionKey == top.actionKey
                && (s == .down || $0.canAlwaysApprove)
        }.map(\.id) : []

        let fly = {
            let start = shown(drag, top)
            // Keep going the way the finger was going (no snapping to an axis).
            var unit: CGVector = switch s {
            case .right: CGVector(dx: 1, dy: 0.12)
            case .left: CGVector(dx: -1, dy: 0.12)
            case .up: CGVector(dx: 0.05, dy: -1)
            case .down: CGVector(dx: 0.05, dy: 1)
            }
            var speed: CGFloat = 0
            if let v = velocity {
                speed = hypot(v.width, v.height)
                let along = v.width * unit.dx + v.height * unit.dy
                if speed > 350 && along > 0 { unit = CGVector(dx: v.width / speed, dy: v.height / speed) }
            }
            let distance: CGFloat = 1300
            // easeOut starts at 3·d/t: match the finger, within a sane range.
            let duration = speed > 350 ? min(0.5, max(0.26, Double(3 * distance / speed))) : 0.34
            if !s.isStanding { commitFeel = .impact(weight: .light, intensity: 0.8); commitTick += 1 }
            var t = Transaction(); t.disablesAnimations = true
            withTransaction(t) { exit = start }
            withAnimation(reduceMotion ? .easeOut(duration: 0.22) : (speed > 350 ? .easeOut(duration: duration) : .easeIn(duration: 0.3))) {
                flying = s
                exit = reduceMotion ? .zero : CGSize(width: start.width + unit.dx * distance, height: start.height + unit.dy * distance)
                exitSpin = reduceMotion ? 0 : Double(unit.dx) * 14 * (grabTop ? 1 : -1)
                pressed = false
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + (reduceMotion ? 0.23 : (speed > 350 ? min(0.42, duration) : 0.3))) {
                var t = Transaction(); t.disablesAnimations = true
                withTransaction(t) {
                    hidden.insert(top.id)
                    for id in covered { hidden.insert(id) }
                    decided += 1 + covered.count
                    flying = nil
                    stamping = nil
                    drag = .zero
                    exit = .zero
                    exitSpin = 0
                    expanded = false
                }
                if let previous { commit(previous) }
                schedule(Pending(id: top.id, ids: [top.id] + covered, swipe: s, title: top.title, agent: top.agent.name))
            }
        }
        if s.isStanding {
            // The ink "Sempre" stamp presses in first, with a firmer thud: this one sticks.
            commitFeel = .impact(weight: .medium); commitTick += 1
            withAnimation(reduceMotion ? .easeOut(duration: 0.2) : .spring(response: 0.28, dampingFraction: 0.55)) { stamping = s }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5, execute: fly)
        } else {
            fly()
        }
    }

    private func schedule(_ p: Pending) {
        withAnimation(.spring(duration: 0.35)) { pending = p }
        let token = p.token
        DispatchQueue.main.asyncAfter(deadline: .now() + 4.5) {
            if pending?.token == token { commitPending() }
        }
    }

    private func commitPending() {
        guard let p = pending else { return }
        withAnimation(.easeOut(duration: 0.25)) { pending = nil }
        commit(p)
    }

    private func commit(_ p: Pending) {
        if model.decide(p.id, p.swipe.decision) == nil {
            // The core said no (stale, already resolved…): its toast explains; the card returns.
            for id in p.ids { hidden.remove(id) }
            decided = max(0, decided - p.ids.count)
        }
    }

    private func undo() {
        guard let p = pending else { return }
        commitFeel = .impact(weight: .light, intensity: 0.6); commitTick += 1
        pending = nil
        let from: CGSize = switch p.swipe {
        case .right: CGSize(width: 500, height: 40)
        case .left: CGSize(width: -500, height: 40)
        case .up: CGSize(width: 0, height: -900)
        case .down: CGSize(width: 0, height: 900)
        }
        var t = Transaction(); t.disablesAnimations = true
        withTransaction(t) {
            drag = reduceMotion ? .zero : from
            for id in p.ids { hidden.remove(id) }
            decided = max(0, decided - p.ids.count)
        }
        withAnimation(reduceMotion ? .easeOut(duration: 0.2) : .spring(duration: 0.5, bounce: 0.25)) { drag = .zero }
    }

    /// Keep the deck in step with the core (new requests arrive live; resolved ones leave).
    private func sync() {
        let queue = model.approvalQueue
        let ids = Set(queue.map(\.id))
        let waiting = Set(pending?.ids ?? [])
        // Keep what's waiting in the undo window; drop what the core already resolved.
        let kept = deck.filter { waiting.contains($0.id) && !ids.contains($0.id) }
        // A request that shows up while the stack is open drops in with weight.
        let fresh = seenCards ? Set(queue.map(\.id)).subtracting(known) : []
        arriving = fresh
        if !fresh.isEmpty { arrivalTick += 1 }
        known.formUnion(queue.map(\.id))
        deck = queue + kept.filter { k in !queue.contains { $0.id == k.id } }
        hidden = hidden.filter { ids.contains($0) || waiting.contains($0) }
    }

    private func close() {
        commitPending()
        model.approvalsOpen = false
        dismiss()
    }

    private func history(for r: AgentRequestDto) -> [AgentRequestDto] {
        model.requests
            .filter { $0.agent.id == r.agent.id && $0.actionKey == r.actionKey && $0.id != r.id && ($0.status == .approved || $0.status == .denied) }
            .sorted { ($0.resolvedMs ?? 0) > ($1.resolvedMs ?? 0) }
            .prefix(3).map { $0 }
    }

    // MARK: buttons and toast

    private func buttons(for top: AgentRequestDto) -> some View {
        GlassEffectContainer(spacing: 14) {
            HStack(spacing: 14) {
                ForEach([ApprovalSwipe.down, .left, .right, .up]) { s in
                    let off = s == .up && !top.canAlwaysApprove
                    Button { decide(s) } label: {
                        VStack(spacing: 6) {
                            Image(systemName: off ? "lock.fill" : s.symbol)
                                .font(.system(size: s.isStanding ? 18 : 22, weight: .bold))
                                .foregroundStyle(.white)
                                .frame(width: s.isStanding ? 54 : 64, height: s.isStanding ? 54 : 64)
                                .glassEffect(.regular.tint((off ? Palette.textTertiary : s.tint).opacity(0.92)).interactive(), in: .circle)
                            Text(s.label)
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(off ? Palette.textTertiary : Palette.textSecondary)
                                .lineLimit(1)
                                .minimumScaleFactor(0.8)
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.plain)
                    .disabled(flying != nil)
                    .opacity(off ? 0.55 : 1)
                    .accessibilityLabel(Text(s.label))
                    .accessibilityHint(off ? Text("Red line: this always asks. You can approve just this once.") : Text(""))
                    .accessibilityIdentifier(s.accessibilityId)
                }
            }
            .frame(maxWidth: .infinity, alignment: .bottom)
        }
        .padding(.horizontal, 18)
    }

    @ViewBuilder
    private var toast: some View {
        if let p = pending {
            HStack(spacing: 10) {
                Image(systemName: p.swipe.symbol)
                    .font(.system(size: 13, weight: .bold))
                    .foregroundStyle(.white)
                    .frame(width: 26, height: 26)
                    .background(p.swipe.tint, in: .circle)
                VStack(alignment: .leading, spacing: 1) {
                    Text(p.swipe.done).font(.subheadline.weight(.semibold))
                    Text(p.ids.count > 1 ? String(localized: "\(p.title) · and the same from \(p.agent)") : p.title)
                        .font(.caption).foregroundStyle(Palette.textSecondary).lineLimit(1)
                }
                Spacer(minLength: 6)
                Button(String(localized: "Undo")) { undo() }
                    .font(.subheadline.weight(.bold))
                    .foregroundStyle(Palette.action)
                    .accessibilityIdentifier("approval-undo")
            }
            .padding(.horizontal, 14).padding(.vertical, 10)
            .glassEffect(.regular, in: .capsule)
            .padding(.horizontal, 8)
            .transition(.move(edge: .bottom).combined(with: .opacity))
            .accessibilityElement(children: .contain)
        } else if lockedHint {
            Label(String(localized: "Red line: this always asks. You can approve just this once."), systemImage: "lock.fill")
                .font(.footnote.weight(.medium))
                .padding(.horizontal, 14).padding(.vertical, 10)
                .glassEffect(.regular, in: .capsule)
                .transition(.opacity)
        }
    }

    // MARK: first run

    private var explainerDeck: some View {
        let e = ApprovalExplainer.all[explainerStep]
        return ApprovalExplainerCard(explainer: e) {
            withAnimation(.spring(duration: 0.4)) {
                if explainerStep + 1 >= ApprovalExplainer.all.count { explained = true } else { explainerStep += 1 }
            }
        }
        .id(explainerStep)
        .transition(.asymmetric(insertion: .scale(scale: 0.94).combined(with: .opacity),
                                removal: .move(edge: e.swipe == .left ? .leading : e.swipe == .up ? .top : e.swipe == .down ? .bottom : .trailing).combined(with: .opacity)))
    }
}

// MARK: - The card

/// One glance: who asks, what (one human sentence), where, and how risky.
struct ApprovalCardView: View {
    static let radius: CGFloat = 36
    @Environment(AppModel.self) private var model
    let request: AgentRequestDto
    var expanded: Bool
    var history: [AgentRequestDto]
    var standing: [StandingDecisionDto]
    /// "Tap for details", spelled out on the first card only.
    var showHint = false

    var body: some View {
        // Tap flips the card: the details live on its back, and the swipes still work there.
        ZStack {
            face { front }
                .modifier(FlipFace(angle: expanded ? 180 : 0, back: false, reduceMotion: reduceMotion))
            face {
                // No ScrollView here: vertical drags must stay swipes. A tighter back instead.
                ViewThatFits(in: .vertical) {
                    backSide(compact: false)
                    backSide(compact: true)
                }
            }
            .modifier(FlipFace(angle: expanded ? 180 : 0, back: true, reduceMotion: reduceMotion))
        }
    }

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private func face<C: View>(@ViewBuilder _ c: () -> C) -> some View {
        c()
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .background {
                ZStack {
                    Palette.surface
                    CardGrain()
                }
                .clipShape(.rect(cornerRadius: Self.radius, style: .continuous))
            }
            .overlay(RoundedRectangle(cornerRadius: Self.radius, style: .continuous).strokeBorder(Palette.hairline, lineWidth: 0.5))
    }

    private var front: some View {
        VStack(alignment: .leading, spacing: 0) {
            who
            Spacer(minLength: 30)
            // The hero: the action, as one short sentence.
            Text(request.title)
                .font(.system(size: 27, weight: .bold))
                .tracking(-0.3)
                .lineSpacing(1)
                .foregroundStyle(Palette.textPrimary)
                .lineLimit(3)
                .minimumScaleFactor(0.85)
                .fixedSize(horizontal: false, vertical: true)
            if !request.detail.isEmpty {
                // Why / preview, quoted like a note in the margin.
                HStack(alignment: .top, spacing: 10) {
                    Capsule().fill(Palette.textPrimary.opacity(0.14)).frame(width: 3)
                    Text(request.detail)
                        .font(.callout)
                        .foregroundStyle(Palette.textSecondary)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, 14)
            }
            FlowLayout(spacing: 8, lineSpacing: 8) {
                ForEach(chips) { ChipView(chip: $0) }
            }
            .padding(.top, 20)
            // Twice the room below: the action sits at the card's optical third.
            Spacer(minLength: 30)
            Spacer(minLength: 0)
            hint
        }
        .padding(.horizontal, 26)
        .padding(.top, 24)
        .padding(.bottom, 18)
    }

    private func backSide(compact: Bool) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            who
            Text(request.title)
                .font(.title3.weight(.bold))
                .foregroundStyle(Palette.textPrimary)
                .lineLimit(compact ? 2 : nil)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, 18)
            if !request.detail.isEmpty && !compact {
                Text(request.detail)
                    .font(.subheadline)
                    .foregroundStyle(Palette.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 6)
            }
            details(compact: compact).padding(.top, 18)
            Spacer(minLength: 18)
            hint
        }
        .padding(.horizontal, 26)
        .padding(.top, 24)
        .padding(.bottom, 18)
    }

    private var who: some View {
        HStack(spacing: 12) {
            ContactAvatar(persona: request.agent, size: 46)
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(request.agent.name)
                        .font(.headline)
                        .foregroundStyle(Palette.textPrimary)
                    Text(RodaTime.relative(request.openedMs))
                        .font(.caption.weight(.medium))
                        .foregroundStyle(Palette.textTertiary)
                }
                Text("asks in \(request.spaceTitle)")
                    .font(.footnote)
                    .foregroundStyle(Palette.textSecondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            if let s = model.space(request.spaceId) {
                ContextFan(space: s, people: Array(s.members.filter { $0.kind != .agent && !$0.isMe }.prefix(2)), key: request.id)
            }
        }
    }

    @ViewBuilder
    private var hint: some View {
        HStack(spacing: 5) {
            if showHint && !expanded {
                Text("Tap for details").font(.caption.weight(.medium))
            }
            Image(systemName: expanded ? "arrow.uturn.backward" : "chevron.compact.down")
                .font(.system(size: expanded ? 13 : 18, weight: .semibold))
        }
        .foregroundStyle(Palette.textTertiary.opacity(0.8))
        .frame(maxWidth: .infinity)
        .accessibilityHidden(true)
    }

    /// Risk and scope, quiet unless sensitive (then a warm accent, never alarm red).
    private var chips: [ApprovalChip] {
        var c: [ApprovalChip] = []
        switch request.actionKey {
        case "money":
            c.append(.init(id: "money", symbol: "banknote", text: String(localized: "Spends \(Money.format(request.costCents ?? 0))"), sensitive: true))
        case "third_party_data":
            c.append(.init(id: "data", symbol: "eye", text: String(localized: "Someone else’s data"), sensitive: true))
        case "public_audience":
            c.append(.init(id: "public", symbol: "megaphone", text: String(localized: "Wider audience"), sensitive: true))
        case "irreversible":
            c.append(.init(id: "irrev", symbol: "trash", text: String(localized: "No undo"), sensitive: true))
        case "external":
            c.append(.init(id: "out", symbol: "arrow.up.right", text: String(localized: "Leaves Zoen"), sensitive: false))
        case "reversible":
            c.append(.init(id: "undo", symbol: "arrow.uturn.backward", text: String(localized: "Can be undone"), sensitive: false))
        default:
            c.append(.init(id: "reply", symbol: "text.bubble", text: String(localized: "Replies in the chat"), sensitive: false))
        }
        if !request.audience.isEmpty {
            c.append(.init(id: "who", symbol: "person.2", text: request.audience, sensitive: false))
        }
        c.append(request.canAlwaysApprove
                 ? .init(id: "once", symbol: "1.circle", text: String(localized: "Just this once"), sensitive: false)
                 : .init(id: "red", symbol: "lock", text: String(localized: "Always asks"), sensitive: true))
        return c
    }

    @ViewBuilder
    private func details(compact: Bool) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 10) {
                fact("person.2", String(localized: "Who sees it"), request.audience)
                fact("bolt", String(localized: "Action"), request.actionLabel)
                if let c = request.costCents { fact("brazilianrealsign.circle", String(localized: "Amount"), Money.format(c)) }
                fact(request.canAlwaysApprove ? "hand.raised" : "lock", String(localized: "Why it asks"), request.reason)
            }
            section(String(localized: "What it touches")) {
                Text(touches).font(.subheadline).foregroundStyle(Palette.textPrimary)
            }
            section(String(localized: "Where")) {
                if let s = model.space(request.spaceId) {
                    HStack(spacing: 10) {
                        ChatAvatar(space: s, size: 30)
                        Text(s.title).font(.subheadline.weight(.medium))
                    }
                } else {
                    Text(request.spaceTitle).font(.subheadline)
                }
            }
            section(String(localized: "Who’s asking")) {
                Text(request.agent.isMine ? String(localized: "\(request.agent.name), your agent") : String(localized: "\(request.agent.name), \(request.agent.ownerName ?? "")’s agent"))
                    .font(.subheadline)
            }
            section(String(localized: "Past decisions")) {
                if history.isEmpty && standing.isEmpty {
                    Text("First time it asks for this.").font(.subheadline).foregroundStyle(Palette.textSecondary)
                }
                ForEach(standing, id: \.grantId) { s in
                    Label(s.allow ? String(localized: "Always approve: \(s.actionLabel) in \(s.spaceTitle)") : String(localized: "Always deny: \(s.actionLabel) in \(s.spaceTitle)"),
                          systemImage: s.allow ? "checkmark.seal.fill" : "hand.raised.fill")
                        .font(.subheadline)
                }
                ForEach(history.prefix(compact ? 1 : 3), id: \.id) { h in
                    HStack(spacing: 8) {
                        Image(systemName: h.status == .approved ? "checkmark.circle.fill" : "xmark.circle.fill")
                            .foregroundStyle(h.status == .approved ? Palette.action : Palette.danger)
                        Text(h.title).font(.subheadline).lineLimit(1)
                        Spacer(minLength: 4)
                        if h.byStanding {
                            Text("Always").font(.caption2.weight(.bold)).foregroundStyle(Palette.textSecondary)
                        }
                        if let at = h.resolvedMs {
                            Text(RodaTime.relative(at)).font(.caption).foregroundStyle(Palette.textTertiary)
                        }
                    }
                }
            }
        }
        .accessibilityIdentifier("approval-details")
    }

    private var touches: String {
        switch request.actionKey {
        case "external": String(localized: "Sends something outside Zoen, to: \(request.audience)")
        case "third_party_data": String(localized: "Someone else’s data: \(request.audience)")
        case "money": String(localized: "Real money: \(Money.format(request.costCents ?? 0))")
        case "public_audience": String(localized: "Posts to a wider audience: \(request.audience)")
        case "irreversible": String(localized: "Deletes for good. There’s no undo.")
        default: String(localized: "Changes in this chat. Versioned, so it can be undone.")
        }
    }

    private func section<C: View>(_ title: String, @ViewBuilder _ c: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.caption.weight(.semibold)).foregroundStyle(Palette.textTertiary).textCase(.uppercase)
            c()
        }
    }

    private func fact(_ symbol: String, _ label: String, _ value: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: symbol).foregroundStyle(Palette.textTertiary).frame(width: 20)
            Text(label).font(.subheadline).foregroundStyle(Palette.textSecondary)
            Spacer(minLength: 8)
            Text(value).font(.subheadline.weight(.medium)).foregroundStyle(Palette.textPrimary).multilineTextAlignment(.trailing)
        }
    }
}

struct ApprovalChip: Identifiable {
    let id: String
    let symbol: String
    let text: String
    let sensitive: Bool
}

private struct ChipView: View {
    let chip: ApprovalChip
    /// Warm, not alarming: terracotta ink on a peach wash.
    static let warm = Color.adaptive(light: "#B4532A", dark: "#F2A77E")

    var body: some View {
        Label(chip.text, systemImage: chip.symbol)
            .font(.footnote.weight(chip.sensitive ? .semibold : .medium))
            .labelStyle(ChipLabelStyle())
            .lineLimit(1)
            .foregroundStyle(chip.sensitive ? Self.warm : Palette.textSecondary)
            .padding(.horizontal, 11).padding(.vertical, 7)
            .background((chip.sensitive ? Self.warm.opacity(0.11) : Palette.textPrimary.opacity(0.05)), in: .capsule)
    }
}

private struct ChipLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 5) {
            configuration.icon.font(.system(size: 11, weight: .bold))
            configuration.title
        }
    }
}

/// A whisper of paper: faint ink specks, drawn once (offsets and rotation don't redraw it).
private struct CardGrain: View {
    var body: some View {
        Canvas { ctx, size in
            var rng = InkRNG(11)
            let count = Int(size.width * size.height / 900)
            for _ in 0..<count {
                let x = CGFloat(rng.unit()) * size.width, y = CGFloat(rng.unit()) * size.height
                let d = 0.6 + CGFloat(rng.unit()) * 1.2
                ctx.fill(Path(ellipseIn: CGRect(x: x, y: y, width: d, height: d)),
                         with: .color(Palette.textPrimary.opacity(0.025 + rng.unit() * 0.03)))
            }
        }
        .drawingGroup()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// One face of a flipping card. Animatable, so each face hides exactly at 90°; with
/// Reduce Motion it cross-fades instead of turning.
private struct FlipFace: ViewModifier, Animatable {
    var angle: Double
    let back: Bool
    let reduceMotion: Bool

    nonisolated var animatableData: Double {
        get { angle }
        set { angle = newValue }
    }

    func body(content: Content) -> some View {
        let a = back ? angle - 180 : angle
        let shown = abs(a) < 90
        content
            .rotation3DEffect(.degrees(reduceMotion ? 0 : a), axis: (x: 0, y: 1, z: 0), perspective: 0.4)
            .opacity(reduceMotion ? (back ? angle / 180 : 1 - angle / 180) : (shown ? 1 : 0))
            .accessibilityHidden(!shown)
            .allowsHitTesting(shown)
    }
}

/// Where the ask lives, at a glance: the Space's art fanned with a couple of the people in
/// it, like tiny photos. They shuffle back into place with a spring when the card changes.
private struct ContextFan: View {
    let space: SpaceSummary
    let people: [Persona]
    let key: String
    @State private var fanned = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        ZStack {
            ForEach(Array(people.enumerated().reversed()), id: \.element.id) { i, p in
                ContactAvatar(persona: p, size: 26)
                    .padding(2)
                    .background(Palette.surface, in: .circle)
                    .shadow(color: .black.opacity(0.12), radius: 3, y: 1)
                    .rotationEffect(.degrees(fanned ? (i == 0 ? -12 : 12) : 0))
                    .offset(x: fanned ? (i == 0 ? -22 : 20) : 0, y: fanned ? 6 : 2)
            }
            ChatAvatar(space: space, size: 36)
                .padding(2.5)
                .background(Palette.surface, in: .rect(cornerRadius: 12, style: .continuous))
                .clipShape(.rect(cornerRadius: 12, style: .continuous))
                .shadow(color: .black.opacity(0.14), radius: 4, y: 2)
                .rotationEffect(.degrees(fanned ? -4 : 0))
        }
        .frame(width: 78, height: 46)
        .accessibilityHidden(true)
        .onAppear { reshuffle() }
        .onChange(of: key) { _, _ in fanned = false; reshuffle() }
    }

    private func reshuffle() {
        guard !reduceMotion else { fanned = true; return }
        withAnimation(.spring(response: 0.5, dampingFraction: 0.62).delay(0.08)) { fanned = true }
    }
}

#if os(iOS)
/// The phone's lean, smoothed to -1…1. Only runs while a card shows it.
@MainActor @Observable
final class DeviceLean {
    var x: Double = 0
    var y: Double = 0
    @ObservationIgnored private let motion = CMMotionManager()
    @ObservationIgnored private var users = 0

    func start() {
        users += 1
        guard users == 1, motion.isDeviceMotionAvailable else { return }
        motion.deviceMotionUpdateInterval = 1.0 / 60
        motion.startDeviceMotionUpdates(to: .main) { [weak self] m, _ in
            guard let g = m?.gravity else { return }
            let nx = max(-1, min(1, g.x * 2.2)), ny = max(-1, min(1, (g.y + 0.55) * 2.2))
            MainActor.assumeIsolated {
                guard let self else { return }
                self.x += (nx - self.x) * 0.12
                self.y += (ny - self.y) * 0.12
            }
        }
    }

    func stop() {
        users = max(0, users - 1)
        if users == 0 { motion.stopDeviceMotionUpdates() }
    }
}
#endif

/// A few degrees of parallax with the phone's lean and a soft sheen that slides across the
/// paper as the light catches it. Paused while the card is held; off with Reduce Motion.
struct TiltSheen<Content: View>: View {
    var active: Bool
    @ViewBuilder var content: Content
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorScheme) private var scheme
    #if os(iOS)
    @State private var lean = DeviceLean()
    #endif

    var body: some View {
        #if os(iOS)
        if reduceMotion {
            content
        } else {
            leaning
        }
        #else
        content
        #endif
    }

    #if os(iOS)
    @ViewBuilder
    private var leaning: some View {
        #if DEBUG
        if UserDefaults.standard.bool(forKey: "RodaFakeLean") {
            // Recording hook: the simulator has no gyroscope, so sway slowly.
            TimelineView(.animation) { tl in
                let s = tl.date.timeIntervalSinceReferenceDate
                tilted(x: sin(s * 1.3) * 0.9, y: cos(s * 0.9) * 0.6)
            }
        } else {
            tilted(x: lean.x, y: lean.y)
                .onAppear { lean.start() }
                .onDisappear { lean.stop() }
        }
        #else
        tilted(x: lean.x, y: lean.y)
            .onAppear { lean.start() }
            .onDisappear { lean.stop() }
        #endif
    }

    private func tilted(x: Double, y: Double) -> some View {
        let k = active ? 1.0 : 0.0
        return content
            .overlay {
                // The specular highlight: a soft band that moves against the lean.
                LinearGradient(stops: [
                    .init(color: .white.opacity(0), location: 0),
                    .init(color: .white.opacity(scheme == .dark ? 0.07 : 0.32), location: 0.5),
                    .init(color: .white.opacity(0), location: 1),
                ], startPoint: UnitPoint(x: -0.4 - x * 0.6, y: -0.2 - y * 0.4), endPoint: UnitPoint(x: 0.9 - x * 0.6, y: 1.1 - y * 0.4))
                .blendMode(scheme == .dark ? .plusLighter : .softLight)
                .clipShape(.rect(cornerRadius: ApprovalCardView.radius, style: .continuous))
                .opacity(k)
                .allowsHitTesting(false)
                .accessibilityHidden(true)
            }
            .rotation3DEffect(.degrees(y * 3.5 * k), axis: (x: 1, y: 0, z: 0), perspective: 0.6)
            .rotation3DEffect(.degrees(-x * 3.5 * k), axis: (x: 0, y: 1, z: 0), perspective: 0.6)
            .animation(.spring(response: 0.35, dampingFraction: 0.8), value: active)
    }
    #endif
}

// MARK: - "Sempre" stamp

private struct AlwaysStamp: View {
    let tint: Color
    var body: some View {
        let word = String(localized: "ALWAYS")
        ZStack {
            InkStampMark(word: word, inkColor: tint)
            Text(word)
                .font(.system(size: 22, weight: .heavy, design: .rounded))
                .tracking(2)
                .foregroundStyle(tint.opacity(0.9))
                .rotationEffect(.degrees(-14))
        }
        .frame(width: 190, height: 124)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

// MARK: - End state

struct AllCaughtUpView: View {
    var onClose: (() -> Void)? = nil
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown = false

    var body: some View {
        VStack(spacing: 18) {
            Spacer(minLength: 0)
            ZStack {
                DoodleView(doodle: .allClear, drawOn: reduceMotion ? 0 : 1.2, freezeAt: reduceMotion ? 2 : nil)
                    .frame(width: 230, height: 230)
                    .opacity(0.55)
                MascotView(pose: .cheer)
                    .frame(width: 170, height: 170)
                    .scaleEffect(shown || reduceMotion ? 1 : 0.6)
                    .rotationEffect(.degrees(shown || reduceMotion ? 0 : -8))
            }
            VStack(spacing: 6) {
                Text("All caught up")
                    .font(.system(size: 30, weight: .bold, design: .rounded))
                    .foregroundStyle(Palette.textPrimary)
                Text("Nothing is waiting for you. Your agents carry on with what you allowed.")
                    .font(.subheadline)
                    .foregroundStyle(Palette.textSecondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 30)
            }
            .opacity(shown || reduceMotion ? 1 : 0)
            .offset(y: shown || reduceMotion ? 0 : 12)
            Spacer(minLength: 0)
            if let onClose {
                Button(action: onClose) {
                    Text("Done").font(.headline).frame(maxWidth: 220, minHeight: 50)
                }
                .buttonStyle(.glassProminent)
                .tint(Palette.action)
                .accessibilityIdentifier("approvals-done-button")
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("approvals-done")
        .onAppear {
            Haptics.commit()
            withAnimation(.spring(duration: 0.7, bounce: 0.45).delay(0.1)) { shown = true }
        }
    }
}

// MARK: - First-run explainers

struct ApprovalExplainer: Identifiable {
    let swipe: ApprovalSwipe
    let title: String
    let message: String
    var id: String { title }

    static var all: [ApprovalExplainer] {
        [
            .init(swipe: .right, title: String(localized: "Swipe right to approve"), message: String(localized: "Just this once. You can also use the buttons at the bottom.")),
            .init(swipe: .left, title: String(localized: "Swipe left to deny"), message: String(localized: "The agent won’t do it and says so in the chat.")),
            .init(swipe: .up, title: String(localized: "Swipe up to always approve"), message: String(localized: "This kind of request, from this agent, in this chat. Red lines still ask. Revoke it in Permissions.")),
            .init(swipe: .down, title: String(localized: "Swipe down to always deny"), message: String(localized: "It stops asking for this here. Revoke it in Permissions.")),
        ]
    }
}

struct ApprovalExplainerCard: View {
    let explainer: ApprovalExplainer
    var onNext: () -> Void
    @State private var drag: CGSize = .zero
    @State private var wiggle = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(spacing: 22) {
            Spacer(minLength: 0)
            illustration.frame(height: 190)
            Text(explainer.title)
                .font(.title2.weight(.bold))
                .multilineTextAlignment(.center)
            Text(explainer.message)
                .font(.body)
                .foregroundStyle(Palette.textSecondary)
                .multilineTextAlignment(.center)
            Spacer(minLength: 0)
            Button(String(localized: "Got it"), action: onNext)
                .buttonStyle(.glass)
                .accessibilityIdentifier("approvals-explainer-next")
        }
        .padding(28)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Palette.surface, in: .rect(cornerRadius: 32, style: .continuous))
        .shadow(color: .black.opacity(0.08), radius: 18, y: 8)
        .offset(drag)
        .gesture(DragGesture().onChanged { drag = $0.translation }.onEnded { v in
            if max(abs(v.translation.width), abs(v.translation.height)) > 90 { onNext() } else {
                withAnimation(.spring(duration: 0.4)) { drag = .zero }
            }
        })
        .onAppear {
            guard !reduceMotion else { return }
            withAnimation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true).delay(0.3)) { wiggle = true }
        }
    }

    /// A paper card being slid in its direction over another, with the swipe's tint and icon.
    private var illustration: some View {
        let s = explainer.swipe
        let move: CGSize = switch s {
        case .right: CGSize(width: wiggle ? 46 : 18, height: 0)
        case .left: CGSize(width: wiggle ? -46 : -18, height: 0)
        case .up: CGSize(width: 0, height: wiggle ? -38 : -12)
        case .down: CGSize(width: 0, height: wiggle ? 38 : 12)
        }
        return ZStack {
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .fill(InkPalette.paper)
                .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(InkPalette.ink.opacity(0.35), lineWidth: 1.5))
                .frame(width: 120, height: 150)
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .fill(s.tint)
                .overlay(
                    Image(systemName: s.symbol).font(.system(size: 40, weight: .bold)).foregroundStyle(.white)
                )
                .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(InkPalette.ink.opacity(0.5), lineWidth: 1.5))
                .frame(width: 120, height: 150)
                .rotationEffect(.degrees((s == .right ? 1 : s == .left ? -1 : 0) * (wiggle ? 9 : 4)), anchor: .bottom)
                .offset(move)
        }
        .accessibilityHidden(true)
    }
}

// MARK: - Standing decisions (Permissions and the profile sheet)

struct StandingDecisionRow: View {
    @Environment(AppModel.self) private var model
    let decision: StandingDecisionDto
    var showSpace = true

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: decision.allow ? "checkmark.seal.fill" : "hand.raised.fill")
                .foregroundStyle(decision.allow ? ApprovalSwipe.up.tint : Palette.danger)
                .frame(width: 24)
            VStack(alignment: .leading, spacing: 2) {
                Text(decision.allow ? String(localized: "Always approve: \(decision.actionLabel)") : String(localized: "Always deny: \(decision.actionLabel)"))
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(Palette.textPrimary)
                Text(showSpace ? "\(decision.spaceTitle) · \(RodaTime.relative(decision.atMs))" : RodaTime.relative(decision.atMs))
                    .font(.caption).foregroundStyle(Palette.textSecondary)
            }
            Spacer(minLength: 8)
            // One tap revokes; the toast offers "Desfazer" for a few seconds.
            Button(String(localized: "Revoke"), role: .destructive) {
                Haptics.tap()
                model.revokeStanding(decision)
            }
            .buttonStyle(.borderless)
            .font(.subheadline.weight(.semibold))
            .accessibilityIdentifier("revoke-standing")
        }
        // Contain, not combine: Revoke stays its own button for VoiceOver.
        .accessibilityElement(children: .contain)
    }
}
