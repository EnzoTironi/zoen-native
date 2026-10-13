import SwiftUI
#if os(macOS)
import AppKit
import CoreImage
import QuartzCore
#else
import UIKit
#endif

// The corner leaves the straight edge 1.53 × 24 pt before the vertex.
enum ZoenMotion {
    static let pressScale: CGFloat = 0.97
    static let pressDuration = 0.12
    static let segmentDuration = 0.233
    static let segmentStops: [(Double, Double)] = [
        (0, 0), (0.07, 0.056), (0.14, 0.182), (0.21, 0.332), (0.29, 0.48),
        (0.36, 0.611), (0.43, 0.72), (0.5, 0.807), (0.57, 0.872), (0.64, 0.92),
        (0.71, 0.954), (0.79, 0.976), (0.86, 0.99), (0.93, 0.999), (1, 1),
    ]

    static func segment(_ time: Double) -> Double {
        let x = min(1, max(0, time))
        var previous = segmentStops[0]
        for stop in segmentStops.dropFirst() {
            if x <= stop.0 {
                let span = stop.0 - previous.0
                let u = span == 0 ? 0 : (x - previous.0) / span
                return previous.1 + (stop.1 - previous.1) * u
            }
            previous = stop
        }
        return 1
    }
}

enum ZoenHapticKind {
    case selection, send, record, lock, success, warning, deny, error
}

enum ZoenHaptic {
    static let maxPulseMs = 20

    static func prepare(_ kind: ZoenHapticKind) {
        #if os(iOS)
        generator(kind).prepare()
        #endif
    }

    static func play(_ kind: ZoenHapticKind) {
        #if os(iOS)
        switch kind {
        case .selection: Cache.selection.selectionChanged()
        case .send: Cache.light.impactOccurred()
        case .record: Cache.medium.impactOccurred()
        case .lock: Cache.rigid.impactOccurred()
        case .success: Cache.notification.notificationOccurred(.success)
        case .warning: Cache.notification.notificationOccurred(.warning)
        case .deny: Cache.soft.impactOccurred()
        case .error: Cache.notification.notificationOccurred(.error)
        }
        #else
        let pattern: NSHapticFeedbackManager.FeedbackPattern = switch kind {
        case .selection, .deny: .alignment
        default: .generic
        }
        NSHapticFeedbackManager.defaultPerformer.perform(pattern, performanceTime: .now)
        #endif
    }

    #if os(iOS)
    private enum Cache {
        static let selection = UISelectionFeedbackGenerator()
        static let light = UIImpactFeedbackGenerator(style: .light)
        static let medium = UIImpactFeedbackGenerator(style: .medium)
        static let rigid = UIImpactFeedbackGenerator(style: .rigid)
        static let soft = UIImpactFeedbackGenerator(style: .soft)
        static let notification = UINotificationFeedbackGenerator()
    }

    private static func generator(_ kind: ZoenHapticKind) -> UIFeedbackGenerator {
        switch kind {
        case .selection: Cache.selection
        case .send: Cache.light
        case .record: Cache.medium
        case .lock: Cache.rigid
        case .success, .warning, .error: Cache.notification
        case .deny: Cache.soft
        }
    }
    #endif
}

enum ZoenTokens {
    enum Motion {
        static let threadSwitchMs = 150
        static let fingerReleaseMs = 240
        static let statusLineMs = 150
        static let receiptMs = 150
        static let queueIntervalMs = 40
        static let queueMaxPerFrame = 8
        static let caretMs = 420
        static let caretHz = 8
        static let peekMs = 220
        static let threadPanelMs = 220
        static let reactionGrowMs = 150
    }

    enum Layout {
        static let composerBottomInset: CGFloat = 42
        static let threadPanelWidth: CGFloat = 390
        static let threadPanelRadius: CGFloat = 24
        static let threadPanelGap: CGFloat = 16
        static let threadPanelFrom: CGFloat = 1024
        static let threadFace: CGFloat = 16
        static let threadFaceHit: CGFloat = 44
        static let broadcastChipMs = 4000
        static let threadRootMaxLines = 3
        static let threadSwipeOpen: CGFloat = 64
        static let threadEdgeClose: CGFloat = 24
        static let summaryMinReplies = 8
        static let reactionLongPressMs = 350
        static let reactionTrayMax = 8
        static let reactionAboveFinger: CGFloat = 8
        static let editWindowMin = 15
        static let deleteForAllHours = 48
        static let pinMax = 3
        static let liveLocationMin = 15
    }

    enum Signature {
        static let composerBottomInset: CGFloat = 42
        static let bubbleAnchorGap: CGFloat = 12
        static let peekVisible: CGFloat = 8
        static let voiceArmMs = 80
        static let voiceLockTravel: CGFloat = 48
        static let voiceCancelTravel: CGFloat = 72
        static let receiptDot: CGFloat = 8
        static let receiptOffset: CGFloat = 4
        static let statusAboveComposer: CGFloat = 8
        static let menuAboveAttach: CGFloat = 8
        static let callButtonHeight: CGFloat = 48
        static let callButtonMinWidth: CGFloat = 120
        static let callButtonMinCenterDistance: CGFloat = 80
        static let terminalMaxWidth390: CGFloat = 270
        static let unreadDot: CGFloat = 8
        static let presenceDot: CGFloat = 8
        static let presenceBorder: CGFloat = 2
        static let groupIndent: CGFloat = 16
        static let signOutGap: CGFloat = 24
        static let closeBelowSafeArea: CGFloat = 14
        static let safeTopIslandApprox: CGFloat = 59
        static let searchDebounceMs = 80
        static let draftDebounceMs = 150
        static let localSkeletonAfterMs = 50
        static let clipboardClearMs = 60000
        static let caretHz = 8
        static let relinkChipS = 30
        static let notificationCollapseS = 5
        static let chipHitTarget: CGFloat = 44
        static let flingBlurResumeMs = 80
    }

    enum Feeling {
        static let coldStartIphoneMs = 400
        static let coldStartAndroid35Ms = 700
        static let coldStartAndroid28Ms = 1200
        static let coldStartWebMs = 1000
        static let openThreadMs = 100
        static let sendToBubbleMs = 16
        static let scrollP99Ms = 8.3
        static let callStageMs = 150
        static let callAudioMedianMs = 1500
        static let callAudioCeilingMs = 3000
        static let miniAppSnapshotMs = 16
        static let miniAppHtmlMacMs = 500
        static let androidBlurFallbackAlpha = 0.92
    }
}

enum ZoenAppearance: String, CaseIterable, Identifiable {
    case dark, light, system, automatic
    var id: String { rawValue }
    var title: String {
        switch self {
        case .dark: "Escuro"
        case .light: "Claro"
        case .system: "Sistema"
        case .automatic: "Automático"
        }
    }
    var colorScheme: ColorScheme? {
        switch self {
        case .dark: .dark
        case .light: .light
        case .system, .automatic: nil
        }
    }
}

enum ZoenGlassRole {
    case nav, search, composer
    var blur: CGFloat {
        switch self {
        case .nav, .composer: 50
        case .search: 8
        }
    }
    var fill: Color {
        switch self {
        case .nav, .search: Palette.thinBg
        case .composer: Palette.composerBg
        }
    }
}

struct Superellipse: Shape {
    var radius: CGFloat = 24
    var factor: CGFloat = 1.53

    func path(in rect: CGRect) -> Path {
        let s = min(radius * factor, min(rect.width, rect.height) / 2)
        let pull = s * 0.45
        let minX = rect.minX
        let minY = rect.minY
        let maxX = rect.maxX
        let maxY = rect.maxY
        var path = Path()
        path.move(to: CGPoint(x: minX + s, y: minY))
        path.addLine(to: CGPoint(x: maxX - s, y: minY))
        path.addCurve(
            to: CGPoint(x: maxX, y: minY + s),
            control1: CGPoint(x: maxX - pull, y: minY),
            control2: CGPoint(x: maxX, y: minY + pull)
        )
        path.addLine(to: CGPoint(x: maxX, y: maxY - s))
        path.addCurve(
            to: CGPoint(x: maxX - s, y: maxY),
            control1: CGPoint(x: maxX, y: maxY - pull),
            control2: CGPoint(x: maxX - pull, y: maxY)
        )
        path.addLine(to: CGPoint(x: minX + s, y: maxY))
        path.addCurve(
            to: CGPoint(x: minX, y: maxY - s),
            control1: CGPoint(x: minX + pull, y: maxY),
            control2: CGPoint(x: minX, y: maxY - pull)
        )
        path.addLine(to: CGPoint(x: minX, y: minY + s))
        path.addCurve(
            to: CGPoint(x: minX + s, y: minY),
            control1: CGPoint(x: minX, y: minY + pull),
            control2: CGPoint(x: minX + pull, y: minY)
        )
        path.closeSubpath()
        return path
    }
}

struct SuperellipseRing: Shape {
    var radius: CGFloat = 24
    var width: CGFloat = 1

    func path(in rect: CGRect) -> Path {
        var path = Superellipse(radius: radius).path(in: rect)
        let innerRect = rect.insetBy(dx: width, dy: width)
        let innerRadius = max(0, radius - width)
        path.addPath(Superellipse(radius: innerRadius).path(in: innerRect))
        return path
    }
}

struct ZoenPressStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(!reduceMotion && configuration.isPressed ? ZoenMotion.pressScale : 1)
            .animation(reduceMotion ? nil : .easeOut(duration: ZoenMotion.pressDuration), value: configuration.isPressed)
    }
}

struct ZoenSegmentPressStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .opacity(!reduceMotion && configuration.isPressed ? 0.5 : 1)
            .scaleEffect(!reduceMotion && configuration.isPressed ? ZoenMotion.pressScale : 1)
            .animation(reduceMotion ? nil : .easeOut(duration: ZoenMotion.pressDuration), value: configuration.isPressed)
    }
}

struct ZoenBackdropBlur: View {
    var radius: CGFloat

    var body: some View {
        #if os(macOS)
        MacBackdropBlur(radius: radius).allowsHitTesting(false)
        #else
        IOSBackdropBlur(radius: radius).allowsHitTesting(false)
        #endif
    }
}

struct ZoenGlassBackdrop: View {
    var role: ZoenGlassRole
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    var body: some View {
        ZStack {
            if reduceTransparency {
                Palette.background
            } else {
                ZoenBackdropBlur(radius: role.blur)
            }
            role.fill
        }
    }
}

struct SkyBackground: View {
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        let glow = scheme == .dark ? 0.08 : 0.06
        ZStack(alignment: .top) {
            Palette.background
            GeometryReader { geo in
                RadialGradient(
                    colors: [Palette.textPrimary.opacity(glow), Palette.textPrimary.opacity(0)],
                    center: .top,
                    startRadius: 0,
                    endRadius: max(geo.size.width, 1)
                )
                .frame(height: geo.size.height / 3)
            }
        }
        .ignoresSafeArea()
        .accessibilityHidden(true)
    }
}

private struct ZoenClipMotion: Equatable {
    var from: Double
    var to: Double
    var started: Date

    func index(at date: Date, reduceMotion: Bool) -> Double {
        if reduceMotion { return to }
        let elapsed = date.timeIntervalSince(started)
        let t = min(1, max(0, elapsed / ZoenMotion.segmentDuration))
        return from + (to - from) * ZoenMotion.segment(t)
    }
}

struct ZoenPill: View {
    let items: [String]
    @Binding var index: Int
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @State private var motion: ZoenClipMotion

    init(items: [String], index: Binding<Int>) {
        self.items = items
        self._index = index
        let current = Double(index.wrappedValue)
        _motion = State(initialValue: ZoenClipMotion(from: current, to: current, started: .distantPast))
    }

    var body: some View {
        let settled = reduceMotion || Date().timeIntervalSince(motion.started) >= ZoenMotion.segmentDuration
        TimelineView(.animation(minimumInterval: 1 / 60, paused: settled)) { timeline in
            let shown = motion.index(at: timeline.date, reduceMotion: reduceMotion)
            ZStack {
                ZoenGlassBackdrop(role: .nav)
                ZStack(alignment: .leading) {
                    GeometryReader { geo in
                        let count = CGFloat(max(items.count, 1))
                        let width = geo.size.width / count
                        RoundedRectangle(cornerRadius: 22, style: .continuous)
                            .fill(Palette.fillPrimary)
                            .frame(width: width, height: geo.size.height)
                            .offset(x: width * CGFloat(shown))
                    }
                    HStack(spacing: 0) {
                        ForEach(Array(items.enumerated()), id: \.offset) { pair in
                            let item = pair.offset
                            let title = pair.element
                            Button {
                                let current = motion.index(at: .now, reduceMotion: reduceMotion)
                                motion = ZoenClipMotion(from: current, to: Double(item), started: .now)
                                index = item
                            } label: {
                                Text(title)
                                    .font(.system(size: 15, weight: .medium))
                                    .foregroundStyle(item == index ? Palette.onAction : Palette.textSecondary)
                                    .frame(maxWidth: .infinity, minHeight: 44)
                                    .contentShape(Capsule())
                            }
                            .buttonStyle(ZoenSegmentPressStyle())
                            .accessibilityAddTraits(item == index ? .isSelected : [])
                        }
                    }
                }
                .padding(2)
            }
            .frame(height: 48)
            .clipShape(Capsule())
            .overlay {
                if reduceTransparency {
                    Capsule().strokeBorder(Palette.strokePrimary, lineWidth: 1)
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("primitives-pill")
    }
}

struct ZoenComposer: View {
    @Binding var text: String
    var placeholder = "Mensagem"
    var onSend: () -> Void = {}
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    private var empty: Bool {
        text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        HStack(alignment: .bottom, spacing: 0) {
            symbolButton("plus", label: "Anexar")
            TextField(
                "",
                text: $text,
                prompt: Text(placeholder).foregroundStyle(Palette.textSecondary),
                axis: .vertical
            )
            .lineLimit(1...6)
            .textFieldStyle(.plain)
            .font(.system(size: 15))
            .foregroundStyle(Palette.textPrimary)
            .padding(.init(top: 14.5, leading: 8, bottom: 14.5, trailing: 8))
            .frame(maxWidth: .infinity, minHeight: 48, alignment: .leading)
            if empty {
                symbolButton("mic", label: "Ditado")
            } else {
                Button(action: onSend) {
                    Image(systemName: "arrow.up")
                        .font(.system(size: 17, weight: .semibold))
                        .foregroundStyle(Palette.onAction)
                        .frame(width: 32, height: 32)
                        .background(Palette.fillPrimary, in: Circle())
                        .frame(width: 44, height: 44)
                }
                .buttonStyle(ZoenPressStyle())
                .accessibilityLabel("Enviar")
            }
        }
        .background { ZoenGlassBackdrop(role: .composer) }
        .clipShape(Superellipse())
        .overlay {
            SuperellipseRing().fill(Palette.strokeComposer, style: FillStyle(eoFill: true))
                .allowsHitTesting(false)
        }
        .overlay {
            if reduceTransparency {
                Superellipse().stroke(Palette.strokePrimary, lineWidth: 1).allowsHitTesting(false)
            }
        }
        .accessibilityIdentifier("primitives-composer")
    }

    private func symbolButton(_ name: String, label: String) -> some View {
        Button {} label: {
            Image(systemName: name)
                .font(.system(size: 18, weight: .regular))
                .foregroundStyle(Palette.textSecondary)
                .frame(width: 32, height: 32)
                .frame(width: 44, height: 44)
        }
        .buttonStyle(ZoenPressStyle())
        .accessibilityLabel(label)
    }
}

struct ZoenCard<Content: View>: View {
    @Environment(\.colorScheme) private var scheme
    @ViewBuilder var content: () -> Content

    var body: some View {
        content()
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.regularBg, in: Superellipse())
            .overlay {
                if scheme == .light {
                    Superellipse()
                        .stroke(
                            LinearGradient(
                                colors: [Color.white.opacity(0.9), Color(hex: "#1D1D1F").opacity(0.06)],
                                startPoint: .top,
                                endPoint: .bottom
                            ),
                            lineWidth: 1
                        )
                        .allowsHitTesting(false)
                }
            }
            .accessibilityIdentifier("primitives-card")
    }
}

struct ZoenGroup<Content: View>: View {
    @ViewBuilder var content: () -> Content

    var body: some View {
        VStack(spacing: 0) { content() }
            .background(Palette.thinBg, in: Superellipse())
            .clipShape(Superellipse())
    }
}

struct ZoenRow: View {
    var title: String
    var value: String? = nil
    var selected: Bool? = nil
    var showsDivider = false
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                Text(title)
                    .font(.system(size: 15, weight: .medium))
                    .foregroundStyle(Palette.textPrimary)
                Spacer(minLength: 12)
                if let value {
                    Text(value)
                        .font(.system(size: 15))
                        .foregroundStyle(Palette.rowValue)
                }
                if let selected {
                    Circle()
                        .strokeBorder(Palette.strokePrimary, lineWidth: 1.5)
                        .frame(width: 18, height: 18)
                        .overlay {
                            if selected {
                                Circle().fill(Palette.fillPrimary).frame(width: 10, height: 10)
                            }
                        }
                        .accessibilityHidden(true)
                }
            }
            .padding(.horizontal, 16)
            .frame(maxWidth: .infinity, minHeight: selected == nil ? 56 : 59.8, alignment: .leading)
            .contentShape(Rectangle())
            .overlay(alignment: .top) {
                if showsDivider {
                    Rectangle()
                        .fill(Palette.strokeSecondary)
                        .frame(height: 1)
                        .padding(.horizontal, 20)
                }
            }
        }
        .buttonStyle(ZoenPressStyle())
        .accessibilityAddTraits(selected == true ? .isSelected : [])
    }
}

struct ZoenDialog<Content: View>: View {
    @ViewBuilder var content: () -> Content

    var body: some View {
        content()
            .padding(16)
            .background(Palette.regularBg, in: Superellipse())
            .overlay(Superellipse().stroke(Palette.strokePrimary, lineWidth: 1))
    }
}

struct ZoenHostRing: View {
    var body: some View {
        Superellipse()
            .stroke(Palette.strokePrimary, lineWidth: 1)
            .allowsHitTesting(false)
    }
}

struct ZoenSheet<Content: View>: View {
    @Binding var isPresented: Bool
    var openerFocused: AccessibilityFocusState<Bool>.Binding
    @ViewBuilder var content: () -> Content
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AccessibilityFocusState private var closeFocused: Bool
    @State private var arrived = false

    var body: some View {
        ZStack {
            if isPresented {
                SkyBackground()
                    .overlay {
                        VStack(spacing: 16) {
                            content()
                        }
                        .padding(.horizontal, 16)
                        .padding(.top, 76)
                        .padding(.bottom, 32)
                        .frame(maxWidth: 390, maxHeight: .infinity, alignment: .top)
                        .scaleEffect(arrived || reduceMotion ? 1 : 0.96)
                        .blur(radius: arrived || reduceMotion ? 0 : 4)
                    }
                    .overlay(alignment: .topTrailing) {
                        Button(action: close) {
                            Image(systemName: "xmark")
                                .font(.system(size: 16, weight: .regular))
                                .frame(width: 48, height: 48)
                                .background { ZoenGlassBackdrop(role: .nav) }
                                .clipShape(Circle())
                        }
                        .buttonStyle(ZoenPressStyle())
                        .padding(.top, 14)
                        .padding(.trailing, 16)
                        .accessibilityLabel("Fechar")
                        .accessibilityFocused($closeFocused)
                    }
                    .transition(.opacity)
                    .onAppear {
                        if reduceMotion {
                            arrived = true
                        } else {
                            withAnimation(.timingCurve(0, 0, 0.2, 1, duration: 0.22)) { arrived = true }
                        }
                        closeFocused = true
                    }
                    .onKeyPress(.escape) {
                        close()
                        return .handled
                    }
                    .accessibilityAddTraits(.isModal)
                    .accessibilityIdentifier("primitives-sheet")
            }
        }
        .onChange(of: isPresented) { _, open in
            if !open {
                arrived = false
                openerFocused.wrappedValue = true
            }
        }
    }

    private func close() {
        if reduceMotion {
            isPresented = false
        } else {
            withAnimation(.timingCurve(0.4, 0, 1, 1, duration: 0.16)) { isPresented = false }
        }
    }
}

struct ZoenPrimitivesGallery: View {
    @State private var appearance: ZoenAppearance = .system
    @State private var segment = 0
    @State private var draft = ""
    @State private var sheet = false
    @AccessibilityFocusState private var openerFocused: Bool

    var body: some View {
        SkyBackground()
            .overlay {
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        Text("Primitivas")
                            .font(.system(size: 20, weight: .medium))
                            .foregroundStyle(Palette.textPrimary)
                        ZoenPill(items: ["Um", "Dois", "Três"], index: $segment)
                        ZoenComposer(text: $draft)
                        ZoenCard {
                            Text("Notas")
                                .font(.system(size: 14))
                                .foregroundStyle(Palette.textSecondary)
                            Text("12")
                                .font(.system(size: 24, weight: .medium))
                                .foregroundStyle(Palette.textPrimary)
                        }
                        Button {
                            sheet = true
                        } label: {
                            Text("Ajustes")
                                .font(.system(size: 15, weight: .medium))
                                .foregroundStyle(Palette.onAction)
                                .frame(maxWidth: .infinity, minHeight: 48)
                                .background(Palette.fillPrimary, in: Capsule())
                        }
                        .buttonStyle(ZoenPressStyle())
                        .accessibilityFocused($openerFocused)
                        .accessibilityIdentifier("primitives-open-sheet")
                    }
                    .padding(16)
                    .frame(maxWidth: 390)
                    .frame(maxWidth: .infinity)
                }
            }
            .overlay {
                ZoenSheet(isPresented: $sheet, openerFocused: $openerFocused) {
                    ZoenGroup {
                        ForEach(Array(ZoenAppearance.allCases.enumerated()), id: \.element.id) { offset, item in
                            ZoenRow(
                                title: item.title,
                                selected: appearance == item,
                                showsDivider: offset > 0
                            ) {
                                appearance = item
                            }
                        }
                    }
                    .frame(maxWidth: 358)
                }
            }
            .preferredColorScheme(appearance.colorScheme)
            .accessibilityIdentifier("primitives-gallery")
    }
}

#if os(macOS)
private struct MacBackdropBlur: NSViewRepresentable {
    var radius: CGFloat

    func makeNSView(context: Context) -> BackdropBlurView {
        let view = BackdropBlurView()
        view.radius = radius
        return view
    }

    func updateNSView(_ view: BackdropBlurView, context: Context) {
        view.radius = radius
    }

    final class BackdropBlurView: NSView {
        var radius: CGFloat = 50 { didSet { apply() } }

        override init(frame frameRect: NSRect) {
            super.init(frame: frameRect)
            wantsLayer = true
            layerUsesCoreImageFilters = true
            apply()
        }

        required init?(coder: NSCoder) {
            super.init(coder: coder)
            wantsLayer = true
            layerUsesCoreImageFilters = true
            apply()
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            layerUsesCoreImageFilters = true
            apply()
        }

        private func apply() {
            guard let filter = CIFilter(name: "CIGaussianBlur") else { return }
            filter.setValue(radius, forKey: kCIInputRadiusKey)
            layer?.backgroundColor = NSColor.clear.cgColor
            layer?.backgroundFilters = [filter]
        }
    }
}
#else
private struct IOSBackdropBlur: UIViewRepresentable {
    var radius: CGFloat

    func makeUIView(context: Context) -> UIVisualEffectView {
        UIVisualEffectView(effect: UIBlurEffect(style: style))
    }

    func updateUIView(_ view: UIVisualEffectView, context: Context) {
        view.effect = UIBlurEffect(style: style)
    }

    private var style: UIBlurEffect.Style {
        radius <= 12 ? .systemUltraThinMaterial : .systemThickMaterial
    }
}
#endif

#Preview("Primitivas") {
    ZoenPrimitivesGallery()
        .frame(width: 390, height: 844)
}
