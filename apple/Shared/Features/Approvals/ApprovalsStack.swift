import SwiftUI
import RodaCore

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
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AppStorage("RodaApprovalsExplained") private var explained = false

    @State private var deck: [AgentRequestDto] = []
    /// Decided here but not yet sent to the core (undo window), or already sent.
    @State private var hidden: Set<String> = []
    @State private var decided = 0
    @State private var pending: Pending?
    @State private var drag: CGSize = .zero
    @State private var flying: ApprovalSwipe?
    @State private var stamping: ApprovalSwipe?
    @State private var crossed: ApprovalSwipe?
    @State private var expanded = false
    @State private var lockedHint = false
    @State private var explainerStep = 0

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
        return total == 0 ? 1 : Double(decided) / Double(total)
    }

    var body: some View {
        VStack(spacing: 14) {
            header
            ZStack {
                if showingExplainers {
                    explainerDeck
                } else if let top = visible.first {
                    cards(top: top)
                } else {
                    AllCaughtUpView { close() }
                        .transition(.opacity.combined(with: .scale(scale: 0.96)))
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .padding(.horizontal, 18)
            .overlay(alignment: .bottom) { toast.padding(.bottom, 6) }
            if !showingExplainers, let top = visible.first {
                buttons(for: top)
                    .transition(.opacity)
            }
        }
        .padding(.bottom, 8)
        .background(Palette.background.ignoresSafeArea())
        .animation(.spring(duration: 0.4, bounce: 0.2), value: visible.map(\.id))
        .onAppear(perform: sync)
        .onChange(of: model.revision) { _, _ in sync() }
        .onDisappear { commitPending() }
    }

    // MARK: header (no numbers: the bar alone shows progress)

    private var header: some View {
        HStack(spacing: 14) {
            Button { if expanded { withAnimation(.spring(duration: 0.4)) { expanded = false } } else { close() } } label: {
                ZoenIcon(.back, size: 18)
                    .foregroundStyle(Palette.textPrimary)
                    .frame(width: 44, height: 44)
                    .glassEffect(.regular.interactive(), in: .circle)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(expanded ? Text("Back to the cards") : Text("Close"))
            .accessibilityIdentifier("approvals-back")

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
                close()
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) {
                    model.setPath(.activity, [])
                    model.notificationsOpen = true
                }
            } label: {
                Image(systemName: "list.bullet")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(Palette.textPrimary)
                    .frame(width: 44, height: 44)
                    .glassEffect(.regular.interactive(), in: .circle)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text("All notifications"))
        }
        .padding(.horizontal, 18)
        .padding(.top, 6)
    }

    // MARK: cards

    @ViewBuilder
    private func cards(top: AgentRequestDto) -> some View {
        let dir = direction(for: drag)
        let p = dir.map { swipeProgress(drag, $0) } ?? 0
        ForEach(Array(visible.prefix(3).enumerated().reversed()), id: \.element.id) { i, r in
            if i == 0 {
                ApprovalCardView(request: r, expanded: expanded, history: history(for: r), standing: model.standing(for: r.agent.id))
                    .overlay { tintOverlay(dir: flying ?? dir, progress: flying != nil ? 1 : p, locked: dir == .up && !r.canAlwaysApprove) }
                    .overlay {
                        if let s = stamping {
                            AlwaysStamp(tint: s.tint)
                                .transition(.scale(scale: 1.4).combined(with: .opacity))
                        }
                    }
                    .offset(cardOffset(r))
                    .rotationEffect(cardRotation, anchor: .bottom)
                    .opacity(reduceMotion && flying != nil ? 0 : 1)
                    .gesture(dragGesture(for: r))
                    .onTapGesture {
                        Haptics.tap()
                        withAnimation(.spring(duration: 0.45, bounce: 0.18)) { expanded.toggle() }
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityAddTraits(.isButton)
                    .accessibilityHint(Text("Actions: approve, deny, always approve, always deny, details."))
                    .accessibilityActions {
                        ForEach(available(for: r)) { s in
                            Button(s.label) { decide(s) }
                        }
                        Button(expanded ? String(localized: "Hide details") : String(localized: "Show details")) {
                            withAnimation(.spring(duration: 0.4)) { expanded.toggle() }
                        }
                    }
                    .accessibilityIdentifier("approval-card")
                    .zIndex(3)
            } else {
                let lift: CGFloat = flying != nil ? 1 : min(1, p)
                ApprovalCardView(request: r, expanded: false, history: [], standing: [])
                    .scaleEffect(1 - 0.05 * (CGFloat(i) - lift))
                    .offset(y: -14 * (CGFloat(i) - lift))
                    .opacity(i == 2 ? 0.6 : 1)
                    .allowsHitTesting(false)
                    .accessibilityHidden(true)
                    .zIndex(Double(3 - i))
            }
        }
    }

    private func cardOffset(_ r: AgentRequestDto) -> CGSize {
        if let f = flying, !reduceMotion {
            switch f {
            case .right: return CGSize(width: 700, height: drag.height + 80)
            case .left: return CGSize(width: -700, height: drag.height + 80)
            case .up: return CGSize(width: drag.width, height: -1100)
            case .down: return CGSize(width: drag.width, height: 1100)
            }
        }
        var d = drag
        // "Always approve" on a red line: rubber band, it won't go.
        if direction(for: drag) == .up && !r.canAlwaysApprove {
            d.height = -sqrt(abs(drag.height)) * 5
        }
        return reduceMotion ? CGSize(width: d.width * 0.25, height: d.height * 0.25) : d
    }

    private var cardRotation: Angle {
        guard !reduceMotion else { return .zero }
        let w = flying == .right ? 700 : flying == .left ? -700 : drag.width
        guard abs(w) > abs(drag.height) || flying == .left || flying == .right else { return .zero }
        return .degrees(Double(w) / 22)
    }

    @ViewBuilder
    private func tintOverlay(dir: ApprovalSwipe?, progress p: CGFloat, locked: Bool) -> some View {
        if let dir {
            let color = locked ? Palette.textTertiary : dir.tint
            ZStack(alignment: dir.labelAlignment) {
                RoundedRectangle(cornerRadius: 32, style: .continuous)
                    .fill(color.opacity(Double(min(0.82, p * 0.82))))
                HStack(spacing: 8) {
                    Image(systemName: locked ? "lock.fill" : dir.symbol)
                        .font(.system(size: 17, weight: .bold))
                        .foregroundStyle(color)
                        .frame(width: 38, height: 38)
                        .background(.white, in: .circle)
                    Text(locked ? String(localized: "Always asks") : dir.label)
                        .font(.headline)
                        .foregroundStyle(.white)
                }
                .padding(22)
                .opacity(Double(min(1, p * 1.6)))
                .scaleEffect(0.85 + 0.15 * min(1, p))
            }
            .allowsHitTesting(false)
            .accessibilityHidden(true)
        }
    }

    private func dragGesture(for r: AgentRequestDto) -> some Gesture {
        DragGesture(minimumDistance: 10)
            .onChanged { v in
                guard flying == nil else { return }
                drag = v.translation
                let dir = direction(for: v.translation)
                let over = dir.flatMap { swipeProgress(v.translation, $0) >= 1 ? $0 : nil }
                if over != crossed {
                    crossed = over
                    if let over {
                        if over == .up && !r.canAlwaysApprove { Haptics.warning() }
                        else if over.isStanding { Haptics.open() } else { Haptics.selectionTick() }
                    }
                }
            }
            .onEnded { v in
                guard flying == nil else { return }
                crossed = nil
                // Sideways also flies on a quick flick; up/down need the full, deliberate pull.
                var t = v.translation
                if abs(v.predictedEndTranslation.width) > abs(t.width) && abs(t.width) > abs(t.height) {
                    t.width = v.predictedEndTranslation.width
                }
                if let dir = direction(for: t), swipeProgress(t, dir) >= 1 {
                    if dir == .up && !r.canAlwaysApprove {
                        lockedHint = true
                        withAnimation(.spring(duration: 0.45, bounce: 0.35)) { drag = .zero }
                        DispatchQueue.main.asyncAfter(deadline: .now() + 3) { lockedHint = false }
                        return
                    }
                    decide(dir)
                } else {
                    withAnimation(.spring(duration: 0.45, bounce: 0.3)) { drag = .zero }
                }
            }
    }

    private func direction(for t: CGSize) -> ApprovalSwipe? {
        guard abs(t.width) > 6 || abs(t.height) > 6 else { return nil }
        if abs(t.width) >= abs(t.height) { return t.width > 0 ? .right : .left }
        return t.height < 0 ? .up : .down
    }

    private func swipeProgress(_ t: CGSize, _ d: ApprovalSwipe) -> CGFloat {
        let v: CGFloat = (d == .left || d == .right) ? abs(t.width) : abs(t.height)
        return v / d.threshold
    }

    private func available(for r: AgentRequestDto) -> [ApprovalSwipe] {
        ApprovalSwipe.allCases.filter { $0 != .up || r.canAlwaysApprove }
    }

    // MARK: deciding

    private func decide(_ s: ApprovalSwipe) {
        guard flying == nil, let top = visible.first else { return }
        if s == .up && !top.canAlwaysApprove {
            Haptics.warning()
            lockedHint = true
            DispatchQueue.main.asyncAfter(deadline: .now() + 3) { lockedHint = false }
            return
        }
        commitPending()
        // A standing decision settles the other cards it covers, here and in the core.
        let covered = s.isStanding ? visible.dropFirst().filter {
            $0.agent.id == top.agent.id && $0.spaceId == top.spaceId && $0.actionKey == top.actionKey
                && (s == .down || $0.canAlwaysApprove)
        }.map(\.id) : []
        let fly = {
            if s.isStanding { Haptics.strongCommit() } else { Haptics.commit() }
            withAnimation(reduceMotion ? .easeOut(duration: 0.2) : .easeIn(duration: 0.26)) { flying = s }
            DispatchQueue.main.asyncAfter(deadline: .now() + (reduceMotion ? 0.22 : 0.28)) {
                var t = Transaction(); t.disablesAnimations = true
                withTransaction(t) {
                    hidden.insert(top.id)
                    for id in covered { hidden.insert(id) }
                    decided += 1 + covered.count
                    flying = nil
                    stamping = nil
                    drag = .zero
                    expanded = false
                }
                schedule(Pending(id: top.id, ids: [top.id] + covered, swipe: s, title: top.title, agent: top.agent.name))
            }
        }
        if s.isStanding && !reduceMotion {
            // The ink "Sempre" stamp presses in first: this one sticks.
            withAnimation(.spring(duration: 0.3, bounce: 0.3)) { stamping = s }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.55, execute: fly)
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
        if model.decide(p.id, p.swipe.decision) == nil {
            // The core said no (stale, already resolved…): its toast explains; the card returns.
            for id in p.ids { hidden.remove(id) }
            decided = max(0, decided - p.ids.count)
        }
    }

    private func undo() {
        guard let p = pending else { return }
        Haptics.dismiss()
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

struct ApprovalCardView: View {
    @Environment(AppModel.self) private var model
    let request: AgentRequestDto
    var expanded: Bool
    var history: [AgentRequestDto]
    var standing: [StandingDecisionDto]

    var body: some View {
        ViewThatFits(in: .vertical) {
            content
            ScrollView { content }.scrollBounceBehavior(.basedOnSize)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Palette.surface, in: .rect(cornerRadius: 32, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 32, style: .continuous).strokeBorder(Palette.hairline, lineWidth: 0.5))
        .shadow(color: .black.opacity(0.08), radius: 18, y: 8)
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 12) {
                ContactAvatar(persona: request.agent, size: 52)
                VStack(alignment: .leading, spacing: 2) {
                    Text(request.agent.name).font(.headline).foregroundStyle(Palette.textPrimary)
                    Text("asks in \(request.spaceTitle)").font(.subheadline).foregroundStyle(Palette.textSecondary).lineLimit(1)
                }
                Spacer(minLength: 0)
                Text(RodaTime.relative(request.openedMs)).font(.caption).foregroundStyle(Palette.textTertiary)
            }
            Text(request.title)
                .font(.title2.weight(.bold))
                .foregroundStyle(Palette.textPrimary)
                .fixedSize(horizontal: false, vertical: true)
            if !request.detail.isEmpty {
                Text(request.detail).font(.body).foregroundStyle(Palette.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            VStack(alignment: .leading, spacing: 10) {
                fact("person.2", String(localized: "Who sees it"), request.audience)
                fact("bolt", String(localized: "Action"), request.actionLabel)
                if let c = request.costCents { fact("brazilianrealsign.circle", String(localized: "Amount"), Money.format(c)) }
            }
            reasonPill
            if expanded { details.transition(.opacity.combined(with: .move(edge: .top))) }
            Spacer(minLength: 0)
            if !expanded {
                Label(String(localized: "Tap for details"), systemImage: "hand.tap")
                    .font(.caption).foregroundStyle(Palette.textTertiary)
                    .frame(maxWidth: .infinity)
            }
        }
        .padding(22)
    }

    private var reasonPill: some View {
        let red = !request.canAlwaysApprove
        return Label(request.reason, systemImage: red ? "exclamationmark.shield.fill" : "hand.raised.fill")
            .font(.footnote.weight(.medium))
            .foregroundStyle(red ? Palette.danger : Palette.textSecondary)
            .padding(.horizontal, 12).padding(.vertical, 8)
            .background((red ? Palette.danger : Palette.textPrimary).opacity(0.08), in: .capsule)
    }

    @ViewBuilder
    private var details: some View {
        VStack(alignment: .leading, spacing: 14) {
            Divider().opacity(0.5)
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
                ForEach(history, id: \.id) { h in
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
    var onClose: () -> Void
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
            Button(action: onClose) {
                Text("Done").font(.headline).frame(maxWidth: 220, minHeight: 50)
            }
            .buttonStyle(.glassProminent)
            .tint(Palette.action)
            .accessibilityIdentifier("approvals-done-button")
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
            Button(String(localized: "Revoke"), role: .destructive) {
                Haptics.tap()
                withAnimation(.snappy) { model.revokeStanding(decision) }
            }
            .buttonStyle(.borderless)
            .font(.subheadline.weight(.semibold))
            .accessibilityIdentifier("revoke-standing")
        }
        .accessibilityElement(children: .combine)
    }
}
