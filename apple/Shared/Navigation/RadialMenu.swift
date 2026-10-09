import SwiftUI

/// One destination in the fan menu.
struct RadialItem: Identifiable, Equatable {
    let id: String
    let title: String
    let symbol: String
    var badge: Int = 0
}

/// Fan geometry, derived instead of hand-tuned.
///
///     d = 48 pt              item's visual diameter (hit area 56 pt ≥ HIG 44 pt)
///     g = 8 pt               minimum gap between neighbours along the arc
///     θ                      arc span in radians (quarter arc on iPhone: π/2, straight up → straight left)
///     R_min = (n − 1)(d + g) / θ
///     R = max(R_min, triggerRadius + d/2 + 12)
///
/// For n = 4, θ = π/2 and a 56 pt trigger: R_min = 3 · 56 / 1.571 ≈ 107 pt, the clearance
/// term is 28 + 24 + 12 = 64 pt, so R ≈ 107 pt. Selection is angular: each item owns a θ/n
/// wedge; nothing highlights within 0.35 R of the origin (release there cancels), and past
/// 1.6 R the highlight sticks to the nearest wedge.
struct RadialMetrics {
    static let itemDiameter: CGFloat = 48
    static let hitDiameter: CGFloat = 56
    static let gap: CGFloat = 8
    static let triggerClearance: CGFloat = 12
    static let deadZone: CGFloat = 0.35
    static let stickyZone: CGFloat = 1.6

    let count: Int
    /// Arc span in degrees.
    let span: Double
    let triggerSize: CGFloat

    var theta: Double { span * .pi / 180 }
    var minRadius: CGFloat { CGFloat(Double(max(count - 1, 0)) * Double(Self.itemDiameter + Self.gap) / theta) }
    var radius: CGFloat { max(minRadius, triggerSize / 2 + Self.itemDiameter / 2 + Self.triggerClearance) }
}

/// Fan menu with Pinterest's shape and feel, in Zoen's own materials, anchored to its trigger (the dark + in the bottom-right corner
/// on iPhone, the sidebar footer on Mac).
///
/// - Press and drag (primary): the fan opens under the finger, a faint ring marks the press
///   origin, the item under the finger fills with moss green and grows (selection tick on every change),
///   and releasing commits (success haptic). Releasing away from every item cancels (soft).
/// - Tap (fallback): opens and stays open; tap an item to pick it, tap the × or outside to close.
/// - The arc opens up and to the left, and shifts by itself so no item leaves the screen.
/// - Reduce Motion: a plain fade, no fly-out. VoiceOver: the trigger and every item are buttons.
/// - Mac: pointer hover highlights items, ⌘K toggles, Esc closes.
struct FanMenu: View {
    @Binding var isOpen: Bool
    let items: [RadialItem]
    var triggerSize: CGFloat = 56
    /// Preferred arc in degrees (0 right, 90 up, 180 left).
    var arc: ClosedRange<Double> = 90...180
    /// Distance from the trigger's centre to the left/right edges of the container, when known.
    var room: (left: CGFloat, right: CGFloat)? = nil
    var onSelect: (RadialItem) -> Void

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorScheme) private var scheme
    @State private var hovered: String?
    @State private var dragging = false
    @State private var touching = false
    @State private var wasOpenAtTouch = false
    @State private var voiceCapture = false
    @State private var voiceFingerUp = 0
    /// The finger that armed the hold is still down and owns the voice take.
    @State private var holdOwnsVoice = false
    @State private var holdTask: Task<Void, Never>?
    @State private var armHoldAt: Date?
    @State private var labelSize = CGSize(width: 80, height: 26)
    /// The + and the items share one Liquid Glass container: as the items leave the + they
    /// pull out of its glass like drops and flow back into it on close. The merge distance is
    /// smaller than the gap between open items, so the open fan stays a row of clean circles.
    private static let glassMerge: CGFloat = 4

    private var metrics: RadialMetrics {
        RadialMetrics(count: items.count, span: arc.upperBound - arc.lowerBound, triggerSize: triggerSize)
    }
    private var radius: CGFloat { metrics.radius }
    private var itemSize: CGFloat { RadialMetrics.itemDiameter }

    var body: some View {
        GlassEffectContainer(spacing: Self.glassMerge) {
            fan
        }
        .frame(width: triggerSize, height: triggerSize)
        .onChange(of: isOpen) { _, open in
            if !open { hovered = nil; dragging = false }
        }
        .task { await runDemoIfAsked() }
    }

    private var fan: some View {
        ZStack {
            if isOpen {
                // Faint ring at the press origin, like Pinterest's ghost touch ring.
                Circle()
                    .strokeBorder(Palette.action.opacity(0.55), lineWidth: 2)
                    .background(Circle().fill(Palette.action.opacity(0.08)))
                    .frame(width: triggerSize + 26, height: triggerSize + 26)
                    .shadow(color: .black.opacity(0.12), radius: 6)
                    .allowsHitTesting(false)
                    .transition(reduceMotion ? .opacity : .scale(scale: 0.55).combined(with: .opacity))
                ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                    itemView(item)
                        .offset(point(angle(index), radius))
                        .transition(fly(index))
                }
            }
            trigger
            if isOpen, let id = hovered, let index = items.firstIndex(where: { $0.id == id }) {
                hoverLabel(items[index], index: index)
                    .transition(.opacity)
            }
        }
        .frame(width: triggerSize, height: triggerSize)
    }

    // MARK: trigger

    private var trigger: some View {
        // The + stays in the hierarchy (hidden) while recording: removing it would cancel
        // the arming touch's gesture, and its release is what sends the take.
        ZStack(alignment: .bottomTrailing) {
            ZoenIcon(.plus, size: 25)
                .foregroundStyle(scheme == .dark ? Color(hex: "#10200A") : .white)
                .rotationEffect(.degrees(isOpen ? 45 : 0))
                .frame(width: triggerSize, height: triggerSize)
                .glassEffect(.regular.tint(scheme == .dark ? Palette.action : Color(hex: "#1D2418").opacity(0.92)).interactive(), in: .circle)
                .overlay { if scheme == .dark { Circle().strokeBorder(.white.opacity(0.3), lineWidth: 0.75).allowsHitTesting(false) } }
                .shadow(color: .black.opacity(scheme == .dark ? 0.55 : 0.25), radius: isOpen ? 14 : 10, y: 4)
                .scaleEffect(touching && !reduceMotion ? 0.9 : 1)
                .animation(reduceMotion ? nil : .spring(duration: 0.38, bounce: 0.35), value: isOpen)
                .animation(.spring(duration: 0.2), value: touching)
                .contentShape(.circle)
                .gesture(dragGesture)
                .opacity(voiceCapture ? 0 : 1)
                .accessibilityHidden(voiceCapture)
                .accessibilityElement()
                .accessibilityLabel(isOpen ? String(localized: "Close menu") : String(localized: "Create and more"))
                .accessibilityHint(String(localized: "Ask Zoen, files, your agents and your context. Long-press the centre to record a voice note for Zoen."))
                .accessibilityAddTraits(.isButton)
                .accessibilityAction { toggle() }
                .accessibilityAction(named: Text(String(localized: "Record audio for Zoen"))) {
                    beginVoiceCapture()
                }
            if voiceCapture {
                PlusVoiceCapture(triggerSize: triggerSize, fingerUp: voiceFingerUp) {
                    withAnimation(.spring(duration: 0.35, bounce: 0.25)) { voiceCapture = false }
                }
            }
        }
    }

    private var dragGesture: some Gesture {
        DragGesture(minimumDistance: 0, coordinateSpace: .local)
            .onChanged { value in
                // A new touch while recording belongs to PlusVoiceCapture's own controls.
                if voiceCapture && !holdOwnsVoice { return }
                if !touching {
                    touching = true
                    wasOpenAtTouch = isOpen
                    Haptics.prepare()
                    if !isOpen { setOpen(true) }
                    // Arm long-hold for centre → Zoen voice (after fan has opened).
                    armHoldAt = Date()
                    holdTask?.cancel()
                    holdTask = Task { @MainActor in
                        try? await Task.sleep(for: .milliseconds(700))
                        guard !Task.isCancelled, touching, !dragging else { return }
                        beginVoiceCapture()
                    }
                }
                // Recording: the held finger only matters for its release.
                if voiceCapture { return }
                let v = CGSize(width: value.location.x - triggerSize / 2, height: value.location.y - triggerSize / 2)
                let dist = hypot(v.width, v.height)
                if !dragging, dist > 14 {
                    dragging = true
                    holdTask?.cancel()  // left the centre — fan wedges win
                }
                // Leaving the centre slop cancels the voice arm.
                if dist > 18 { holdTask?.cancel() }
                guard dragging else { return }
                hover(nearestItem(to: v))
            }
            .onEnded { _ in
                touching = false
                holdTask?.cancel()
                defer { dragging = false }
                if voiceCapture {
                    // Finger released after arming — hand the take to PlusVoiceCapture.
                    if holdOwnsVoice { voiceFingerUp &+= 1 }
                    holdOwnsVoice = false
                    return
                }
                if dragging {
                    if let id = hovered, let item = items.first(where: { $0.id == id }) {
                        choose(item)
                    } else {
                        Haptics.dismiss()
                        setOpen(false)
                    }
                } else if wasOpenAtTouch {
                    Haptics.dismiss()
                    setOpen(false)
                }
                // A plain tap on the closed trigger leaves the fan open (tap mode).
            }
    }

    private func beginVoiceCapture() {
        holdTask?.cancel()
        holdOwnsVoice = touching
        Haptics.recordLock()
        withAnimation(.spring(duration: 0.38, bounce: 0.3)) {
            setOpen(false)
            voiceCapture = true
        }
    }

    // MARK: geometry

    /// The preferred arc, shifted (never widened, R never grows) so every item stays inside
    /// the container's safe area.
    private var liveArc: ClosedRange<Double> {
        guard let room else { return arc }
        let margin = RadialMetrics.hitDiameter / 2 + 8
        let cosMax = min(1, max(-1, (room.right - margin) / radius))
        let cosMin = max(-1, min(1, -(room.left - margin) / radius))
        let aMin = acos(cosMax) * 180 / .pi
        let aMax = acos(cosMin) * 180 / .pi
        let span = arc.upperBound - arc.lowerBound
        var lo = max(arc.lowerBound, aMin)
        var hi = lo + span
        if hi > aMax { hi = aMax; lo = max(aMin, hi - span) }
        return lo...max(lo, hi)
    }

    private func angle(_ index: Int) -> Double {
        let a = liveArc
        guard items.count > 1 else { return (a.lowerBound + a.upperBound) / 2 }
        return a.lowerBound + Double(index) / Double(items.count - 1) * (a.upperBound - a.lowerBound)
    }

    private func point(_ deg: Double, _ r: CGFloat) -> CGSize {
        let a = deg * .pi / 180
        return CGSize(width: r * cos(a), height: -r * sin(a))
    }

    /// Item under the finger, by angle (not hit-testing): the arc is cut into n equal wedges
    /// of θ/n and each item owns one. Dead zone near the origin; sticky beyond 1.6 R.
    private func nearestItem(to v: CGSize) -> String? {
        guard !items.isEmpty else { return nil }
        let dist = hypot(v.width, v.height)
        guard dist > radius * RadialMetrics.deadZone else { return nil }
        var deg = atan2(-v.height, v.width) * 180 / .pi
        if deg < -90 { deg += 360 }
        let a = liveArc
        let wedge = (a.upperBound - a.lowerBound) / Double(items.count)
        // Inside the sticky zone, a finger well outside the arc (more than one wedge) picks nothing.
        if dist < radius * RadialMetrics.stickyZone, deg < a.lowerBound - wedge || deg > a.upperBound + wedge { return nil }
        let clamped = min(max(deg, a.lowerBound), a.upperBound)
        let index = min(items.count - 1, max(0, Int((clamped - a.lowerBound) / max(wedge, 0.001))))
        return items[index].id
    }

    // MARK: items

    private func fly(_ index: Int) -> AnyTransition {
        let a = angle(index)
        if reduceMotion {
            return .asymmetric(
                insertion: .modifier(active: ArcFly(progress: 1, alpha: 0, angle: a, radius: radius, sweep: 0),
                                     identity: ArcFly(progress: 1, alpha: 1, angle: a, radius: radius, sweep: 0))
                    .animation(.easeOut(duration: 0.18)),
                removal: .modifier(active: ArcFly(progress: 1, alpha: 0, angle: a, radius: radius, sweep: 0),
                                   identity: ArcFly(progress: 1, alpha: 1, angle: a, radius: radius, sweep: 0))
                    .animation(.easeOut(duration: 0.14)))
        }
        // Items leave the trigger together with the first one and swing out along the arc to
        // their own spot (30 ms stagger, a little overshoot); closing runs in reverse, faster.
        let sweep = (liveArc.lowerBound - a) * 0.65
        let n = items.count
        let insertion = AnyTransition.modifier(active: ArcFly(progress: 0, alpha: 0, angle: a, radius: radius, sweep: sweep),
                                               identity: ArcFly(progress: 1, alpha: 1, angle: a, radius: radius, sweep: sweep))
            .animation(.spring(response: 0.36, dampingFraction: 0.64).delay(Double(index) * 0.03))
        let removal = AnyTransition.modifier(active: ArcFly(progress: 0, alpha: 0, angle: a, radius: radius, sweep: sweep),
                                             identity: ArcFly(progress: 1, alpha: 1, angle: a, radius: radius, sweep: sweep))
            .animation(.spring(response: 0.24, dampingFraction: 0.95).delay(Double(n - 1 - index) * 0.022))
        return .asymmetric(insertion: insertion, removal: removal)
    }

    /// Dock-style magnification: the item under the finger grows, its neighbours lean in a
    /// little, the rest step back.
    private func magnification(_ item: RadialItem) -> CGFloat {
        guard !reduceMotion, let h = hovered,
              let hi = items.firstIndex(where: { $0.id == h }),
              let i = items.firstIndex(where: { $0.id == item.id }) else { return 1 }
        // Sized so the grown item still clears its neighbours by more than the glass merge
        // distance (gap 8 − 3.8 > 4): no gooey bridges between open items.
        switch abs(hi - i) {
        case 0: return 1.16
        case 1: return 1.0
        default: return 0.92
        }
    }

    @ViewBuilder
    private func itemView(_ item: RadialItem) -> some View {
        let on = hovered == item.id
        Button { choose(item) } label: {
            Group {
                if let g = ZoenGlyph.radial(item.id) { ZoenIcon(g, selected: on, size: 22) }
                else { Image(systemName: item.symbol).font(.system(size: 19, weight: on ? .semibold : .medium)) }
            }
                .foregroundStyle(on ? Color.white : Palette.textPrimary)
                .frame(width: itemSize, height: itemSize)
                // Same Liquid Glass as the bottom bar; the item under the finger fills with moss green.
                .glassEffect(on ? .regular.tint(Palette.action).interactive() : .regular.interactive(), in: .circle)
                .shadow(color: (on ? Palette.action : Color.black).opacity(on ? 0.35 : 0.12), radius: on ? 14 : 8, y: on ? 6 : 3)
                .overlay(alignment: .topTrailing) {
                    if item.badge > 0 {
                        Text("\(item.badge)")
                            .font(.caption2.weight(.bold)).monospacedDigit()
                            .foregroundStyle(.white)
                            .padding(.horizontal, 5).frame(minWidth: 18, minHeight: 18)
                            .background(Palette.danger, in: .capsule)
                            .offset(x: 3, y: -3)
                    }
                }
                .scaleEffect(magnification(item))
                .animation(reduceMotion ? nil : .spring(duration: 0.26, bounce: 0.35), value: hovered)
                .frame(width: RadialMetrics.hitDiameter, height: RadialMetrics.hitDiameter)
                .contentShape(.circle)
        }
        .buttonStyle(.plain)
        #if os(macOS)
        .onHover { inside in
            if inside { hover(item.id) } else if hovered == item.id { hover(nil) }
        }
        #endif
        .accessibilityLabel(item.badge > 0 ? String(localized: "\(item.title), \(item.badge) pending") : item.title)
    }

    /// The hovered item's name: a small bold pill just outside the arc, kept on screen.
    private func hoverLabel(_ item: RadialItem, index: Int) -> some View {
        let a = angle(index)
        let rad = a * .pi / 180
        let along = abs(cos(rad)) * labelSize.width / 2 + abs(sin(rad)) * labelSize.height / 2
        var p = point(a, radius + itemSize * 0.6 + 8 + along)
        if let room {
            let half = labelSize.width / 2 + 8
            p.width = min(max(p.width, -room.left + half), room.right - half)
        }
        return Text(item.title)
            .font(.footnote.weight(.bold))
            .foregroundStyle(Palette.textPrimary)
            .lineLimit(1)
            .fixedSize()
            .dynamicTypeSize(...DynamicTypeSize.xxLarge)
            .padding(.horizontal, 10).padding(.vertical, 5)
            .glassEffect(.regular, in: .capsule)
            .shadow(color: .black.opacity(0.10), radius: 6, y: 2)
            .onGeometryChange(for: CGSize.self) { $0.size } action: { labelSize = $0 }
            .offset(p)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }

    // MARK: actions

    private func hover(_ id: String?) {
        guard id != hovered else { return }
        withAnimation(reduceMotion ? nil : .spring(duration: 0.22, bounce: 0.35)) { hovered = id }
        if id != nil { Haptics.selectionTick() }
    }

    private func setOpen(_ open: Bool) {
        if open { Haptics.open() }
        withAnimation(reduceMotion ? .easeOut(duration: 0.18) : .spring(duration: 0.42, bounce: 0.25)) { isOpen = open }
    }

    private func toggle() {
        if isOpen { Haptics.dismiss() }
        setOpen(!isOpen)
    }

    private func choose(_ item: RadialItem) {
        Haptics.commit()
        withAnimation(reduceMotion ? nil : .spring(duration: 0.18)) { hovered = item.id }
        setOpen(false)
        onSelect(item)
    }

    /// Screenshot/recording hooks (the simulator can't inject touches):
    /// `-RodaRadialHover <id>` opens with that item highlighted; `-RodaRadialDemo YES` plays
    /// open → drag across the items → cancel → open → drag to Files → release.
    private func runDemoIfAsked() async {
        let d = UserDefaults.standard
        if let id = d.string(forKey: "RodaRadialHover") {
            try? await Task.sleep(for: .seconds(1.0))
            setOpen(true)
            try? await Task.sleep(for: .seconds(0.5))
            hover(id)
        }
        guard d.bool(forKey: "RodaRadialDemo") else { return }
        func wait(_ s: Double) async { try? await Task.sleep(for: .seconds(s)) }
        await wait(1.6)
        touching = true; setOpen(true); await wait(0.15); touching = false
        await wait(0.55)
        for item in items { hover(item.id); await wait(0.5) }
        for item in items.reversed().dropFirst() { hover(item.id); await wait(0.35) }
        hover(nil); await wait(0.35)
        Haptics.dismiss(); setOpen(false)
        await wait(1.2)
        touching = true; setOpen(true); await wait(0.15); touching = false
        await wait(0.5)
        for item in items.prefix(3) { hover(item.id); await wait(0.42) }
        await wait(0.5)
        if let files = items.first(where: { $0.id == "files" }) { choose(files) }
    }
}

/// How far a fan item is from its spot: `progress` 0 is tucked into the trigger, 1 is on the
/// arc. The angle swings by `sweep` on the way out, so the items travel along the arc. It only
/// adds a delta (zero at progress 1), because an asymmetric transition applies both of its
/// identity modifiers to the resting view.
private struct ArcFly: ViewModifier, Animatable {
    var progress: Double
    var alpha: Double
    let angle: Double
    let radius: CGFloat
    let sweep: Double

    nonisolated var animatableData: AnimatablePair<Double, Double> {
        get { AnimatablePair(progress, alpha) }
        set { progress = newValue.first; alpha = newValue.second }
    }

    func body(content: Content) -> some View {
        let a = (angle + (1 - progress) * sweep) * .pi / 180
        let a1 = angle * .pi / 180
        let r = radius * CGFloat(progress)
        content
            .scaleEffect(0.3 + 0.7 * progress)
            .opacity(min(1, max(0, alpha)))
            .offset(x: r * cos(a) - radius * cos(a1), y: -r * sin(a) + radius * sin(a1))
    }
}

/// Veil behind the open fan: a light dim (no blur), tap to close.
struct RadialBackdrop: View {
    @Binding var isOpen: Bool
    var body: some View {
        Color.black.opacity(0.18)
            .ignoresSafeArea()
            .opacity(isOpen ? 1 : 0)
            .allowsHitTesting(isOpen)
            .onTapGesture {
                Haptics.dismiss()
                withAnimation(.spring(duration: 0.35, bounce: 0.2)) { isOpen = false }
            }
            .accessibilityHidden(true)
            .animation(.easeOut(duration: 0.22), value: isOpen)
    }
}

extension AppModel {
    /// The fan's items, from the top of the arc to its left end. Create comes first (Ask Zoen
    /// starts anything: plans, mini-apps, agents), then the destinations that are not tabs.
    var radialItems: [RadialItem] {
        [
            RadialItem(id: "zoen", title: String(localized: "Ask Zoen"), symbol: "sparkles"),
            RadialItem(id: "agents", title: String(localized: "Your agents"), symbol: "person.2"),
            RadialItem(id: "files", title: String(localized: "Files"), symbol: "folder"),
            RadialItem(id: "you", title: String(localized: "Your context"), symbol: "person.crop.circle"),
        ]
    }

    func handleRadial(_ item: RadialItem) {
        switch item.id {
        case "zoen": if let id = zoenSpaceId() { go(.space(id)) }
        case "agents": select(.agents)
        case "files": select(.files)
        case "you": select(.you)
        default: break
        }
    }
}
