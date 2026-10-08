import SwiftUI

// MARK: - Zoen icons: pen-and-paper glyphs for the UI chrome
//
// Vector only (SwiftUI Shapes). Drawn on a 24-unit grid with a ~2pt ink stroke (scaled with
// size), round caps and joins, and a tiny hand wobble baked into each path (deterministic per
// glyph, so they're static at rest). The selected variant adds a marker wash: the same shape
// filled at low opacity and slipped a touch off the line. Tints with foregroundStyle and
// scales with Dynamic Type.
// Motion only on interaction: draw-on (trim) + a small bounce when selected, a wiggle on tap,
// and a line boil while pressed (`\.zoenIconBoil`, set by our button styles). Reduce Motion:
// none of it.

enum ZoenGlyph: String, CaseIterable, Identifiable {
    case chats, spaces, store, search, plus, back, phone, video, bell, camera, send, mic, info
    case pin, settings, share, close, check, more, sparkle, agents, folder, you
    case chevron, crown, sun, heart, game, list, note, download
    case lock, trash, play, pause
    var id: String { rawValue }
}

/// One glyph as ink strokes (open or closed polylines) and wash regions, on the 24 grid.
struct GlyphDrawing {
    struct Stroke { var points: [CGPoint]; var closed = false; var smooth = false }
    var strokes: [Stroke] = []
    var fills: [Stroke] = []
}

extension ZoenGlyph {
    private typealias S = GlyphDrawing.Stroke
    private static func p(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: x, y: y) }
    private static func line(_ pts: (CGFloat, CGFloat)...) -> S { S(points: pts.map { p($0.0, $0.1) }) }
    private static func curve(_ pts: (CGFloat, CGFloat)...) -> S { S(points: pts.map { p($0.0, $0.1) }, smooth: true) }
    private static func closed(_ pts: (CGFloat, CGFloat)..., smooth: Bool = false) -> S { S(points: pts.map { p($0.0, $0.1) }, closed: true, smooth: smooth) }
    private static func circle(_ cx: CGFloat, _ cy: CGFloat, _ r: CGFloat, n: Int = 12) -> S {
        S(points: (0..<n).map { i in let a = Double(i) / Double(n) * 2 * .pi - .pi / 2; return p(cx + r * CGFloat(cos(a)), cy + r * CGFloat(sin(a))) }, closed: true, smooth: true)
    }
    private static func rrect(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat, _ r: CGFloat) -> S {
        var pts: [CGPoint] = []
        for (cx, cy, a0) in [(x + w - r, y + r, -Double.pi / 2), (x + w - r, y + h - r, 0), (x + r, y + h - r, Double.pi / 2), (x + r, y + r, Double.pi)] {
            for k in 0...3 { let a = a0 + Double(k) / 3 * .pi / 2; pts.append(p(cx + r * CGFloat(cos(a)), cy + r * CGFloat(sin(a)))) }
        }
        return S(points: pts, closed: true)
    }

    var drawing: GlyphDrawing {
        typealias G = ZoenGlyph
        switch self {
        case .chats:
            let back = G.closed((3, 5.5), (3, 4.2), (4.2, 3), (14.8, 3), (16, 4.2), (16, 11.8), (14.8, 13), (8, 13), (4.5, 16), (5, 13), (4.2, 13), (3, 11.8))
            let front = G.rrect(9, 9, 12.5, 9.5, 3)
            return GlyphDrawing(strokes: [back, front, G.line((18, 18.5), (19.5, 21), (15.5, 18.5))], fills: [front])
        case .spaces:
            let h1 = G.circle(9, 8, 3.4), h2 = G.circle(16.6, 9, 2.6)
            let s1 = G.curve((3.2, 20), (4, 15.6), (9, 13.6), (14, 15.6), (14.8, 20))
            let s2 = G.curve((15.2, 13.9), (18.8, 14.6), (20.8, 19))
            return GlyphDrawing(strokes: [h1, s1, h2, s2], fills: [h1, G.closed((3.2, 20), (4, 15.6), (9, 13.6), (14, 15.6), (14.8, 20), smooth: true)])
        case .store:
            let awning = G.closed((4.2, 4), (19.8, 4), (21, 9), (17.5, 10.2), (14.5, 9), (12, 10.2), (9.5, 9), (6.5, 10.2), (3, 9))
            return GlyphDrawing(strokes: [awning, G.line((4.5, 10.5), (4.5, 20.5), (19.5, 20.5), (19.5, 10.5)), G.line((10, 20.5), (10, 15), (14, 15), (14, 20.5))], fills: [awning])
        case .search:
            let lens = G.circle(10.5, 10.5, 6.6, n: 14)
            return GlyphDrawing(strokes: [lens, G.line((15.4, 15.4), (20.6, 20.6))], fills: [lens])
        case .plus:
            return GlyphDrawing(strokes: [G.line((12, 4.8), (12, 19.2)), G.line((4.8, 12), (19.2, 12))])
        case .back:
            return GlyphDrawing(strokes: [G.line((15, 4.5), (7.8, 12), (15, 19.5))])
        case .phone:
            let h = G.closed((5.5, 3.5), (8.8, 3.2), (10.6, 7.6), (8.6, 9.6), (14.4, 15.4), (16.4, 13.4), (20.8, 15.2), (20.5, 18.5), (18, 20.8), (12, 19), (5, 12), (3.2, 6), smooth: true)
            return GlyphDrawing(strokes: [h], fills: [h])
        case .video:
            let body = G.rrect(2.5, 6.5, 13, 11, 3)
            let lens = G.closed((15.5, 10.5), (21, 7.5), (21, 16.5), (15.5, 13.5))
            return GlyphDrawing(strokes: [body, lens], fills: [body])
        case .bell:
            let dome = G.curve((5.8, 17), (6.4, 10.6), (8.8, 6.6), (12, 5.4), (15.2, 6.6), (17.6, 10.6), (18.2, 17))
            return GlyphDrawing(strokes: [dome, G.line((4.2, 17.2), (19.8, 17.2)), G.curve((9.8, 19.6), (12, 21.2), (14.2, 19.6)), G.line((12, 3.2), (12, 5.4))],
                                fills: [G.closed((5.8, 17), (6.4, 10.6), (8.8, 6.6), (12, 5.4), (15.2, 6.6), (17.6, 10.6), (18.2, 17), smooth: true)])
        case .camera:
            let body = G.closed((2.5, 9.5), (3.5, 7.2), (8, 7.2), (9.6, 4.6), (14.4, 4.6), (16, 7.2), (20.5, 7.2), (21.5, 9.5), (21.5, 18.2), (20.2, 19.8), (3.8, 19.8), (2.5, 18.2))
            return GlyphDrawing(strokes: [body, G.circle(12, 13.4, 3.8)], fills: [body])
        case .send:
            return GlyphDrawing(strokes: [G.line((12, 19.5), (12, 4.8)), G.line((6, 10.6), (12, 4.6), (18, 10.6))])
        case .mic:
            let cap = G.rrect(9, 2.8, 6, 11.4, 3)
            return GlyphDrawing(strokes: [cap, G.curve((5.4, 11), (6.4, 15), (12, 17.6), (17.6, 15), (18.6, 11)), G.line((12, 17.6), (12, 21)), G.line((8.6, 21.2), (15.4, 21.2))], fills: [cap])
        case .info:
            let ring = G.circle(12, 12, 9.4, n: 16)
            return GlyphDrawing(strokes: [ring, G.circle(12, 7.6, 0.45, n: 6), G.line((11.4, 11), (12.2, 11), (12.2, 17))], fills: [ring])
        case .pin:
            let body = G.closed((8.4, 3.4), (15.6, 3.4), (14.6, 9.6), (17.8, 13.8), (6.2, 13.8), (9.4, 9.6))
            return GlyphDrawing(strokes: [body, G.line((12, 13.8), (12, 21.2))], fills: [body])
        case .settings:
            var cog: [CGPoint] = []
            for k in 0..<8 {
                let a = Double(k) / 8 * 2 * .pi
                for (da, r) in [(-0.36, 7.0), (-0.17, 9.4), (0.17, 9.4), (0.36, 7.0)] as [(Double, CGFloat)] {
                    cog.append(G.p(12 + r * CGFloat(cos(a + da)), 12 + r * CGFloat(sin(a + da))))
                }
            }
            let gear = S(points: cog, closed: true)
            return GlyphDrawing(strokes: [gear, G.circle(12, 12, 3)], fills: [gear])
        case .share:
            return GlyphDrawing(strokes: [G.line((8.4, 9), (5, 9), (5, 20.6), (19, 20.6), (19, 9), (15.6, 9)), G.line((12, 14.6), (12, 3)), G.line((8.4, 6.6), (12, 3), (15.6, 6.6))],
                                fills: [G.closed((5, 9), (19, 9), (19, 20.6), (5, 20.6))])
        case .close:
            return GlyphDrawing(strokes: [G.line((6, 6), (18, 18)), G.line((18, 6), (6, 18))])
        case .check:
            return GlyphDrawing(strokes: [G.line((4.4, 12.6), (9.6, 17.6), (19.6, 6.4))])
        case .more:
            return GlyphDrawing(strokes: [G.circle(5.5, 12, 0.7, n: 6), G.circle(12, 12, 0.7, n: 6), G.circle(18.5, 12, 0.7, n: 6)])
        case .sparkle:
            let star = G.closed((12, 2.8), (13.6, 9.6), (20.6, 11.6), (13.6, 13.6), (12, 21), (10.4, 13.6), (3.4, 11.6), (10.4, 9.6))
            return GlyphDrawing(strokes: [star, G.line((19, 2.8), (19, 6.2)), G.line((17.3, 4.5), (20.7, 4.5))], fills: [star])
        case .agents:
            let h = G.circle(10, 8.4, 3.6)
            let s = G.curve((3.6, 20.2), (4.4, 15.8), (10, 13.8), (15.6, 15.8), (16.4, 20.2))
            return GlyphDrawing(strokes: [h, s, G.line((19.4, 3.4), (19.4, 8.2)), G.line((17, 5.8), (21.8, 5.8))], fills: [h])
        case .folder:
            let f = G.closed((3, 6.4), (3.6, 5), (9.2, 5), (11.2, 7.6), (20.4, 7.6), (21, 9), (21, 18.6), (20, 19.6), (4, 19.6), (3, 18.6))
            return GlyphDrawing(strokes: [f, G.line((3.2, 10.4), (20.8, 10.4))], fills: [f])
        case .you:
            let ring = G.circle(12, 12, 9.4, n: 16)
            return GlyphDrawing(strokes: [ring, G.circle(12, 10, 3.2), G.curve((6.4, 18.6), (8.6, 15.6), (12, 14.8), (15.4, 15.6), (17.6, 18.6))], fills: [ring])
        default:
            return GlyphDrawing()  // v2-only glyphs (see ZoenInkIcons.swift)
        }
    }

    var seed: UInt64 { UInt64(abs(rawValue.hashValueStable)) }
}

private extension String {
    /// Stable across launches (Swift's hashValue is randomized).
    var hashValueStable: Int { unicodeScalars.reduce(5381) { ($0 &* 33) &+ Int($1.value) } }
}

/// The glyph's ink (or wash) as a Shape, with the hand wobble baked in. `frame` > 0 re-seeds
/// the wobble (line boil while pressed).
struct GlyphShape: Shape {
    let glyph: ZoenGlyph
    var wash = false
    var frame = 0

    func path(in rect: CGRect) -> Path {
        let d = glyph.drawing
        let k = min(rect.width, rect.height) / 24
        let ox = rect.midX - 12 * k, oy = rect.midY - 12 * k
        var rng = InkRNG(glyph.seed &+ UInt64(frame) &* 977 &+ (wash ? 31 : 0))
        var path = Path()
        for s in wash ? d.fills : d.strokes {
            let amp: CGFloat = 0.22
            // Wash slips a touch down-right, like a marker that misses the line.
            let slip = wash ? CGPoint(x: 0.7, y: 0.6) : .zero
            var pts = s.points.map { CGPoint(x: ox + ($0.x + slip.x + rng.signed() * amp) * k, y: oy + ($0.y + slip.y + rng.signed() * amp) * k) }
            if !s.smooth { pts = Self.subdivide(pts, closed: s.closed, rng: &rng, amp: amp * k * 0.5) }
            let poly = s.smooth ? Ink.catmull(pts, closed: s.closed, steps: 6) : (s.closed ? pts + [pts[0]] : pts)
            guard let first = poly.first else { continue }
            path.move(to: first)
            for q in poly.dropFirst() { path.addLine(to: q) }
            if s.closed { path.closeSubpath() }
        }
        return path
    }

    /// Splits straight segments so a faint wobble lives along them (not just at corners).
    private static func subdivide(_ p: [CGPoint], closed: Bool, rng: inout InkRNG, amp: CGFloat) -> [CGPoint] {
        guard p.count > 1 else { return p }
        var out: [CGPoint] = []
        let ring = closed ? p + [p[0]] : p
        for i in 0..<(ring.count - 1) {
            let a = ring[i], b = ring[i + 1]
            out.append(a)
            let len = hypot(b.x - a.x, b.y - a.y)
            if len > 6 * amp * 10 {
                let m = CGPoint(x: (a.x + b.x) / 2 + rng.signed() * amp, y: (a.y + b.y) / 2 + rng.signed() * amp)
                out.append(m)
            }
        }
        if !closed, let last = p.last { out.append(last) }
        return out
    }

}

extension EnvironmentValues {
    /// True while the hosting button is pressed: icons boil their line.
    @Entry var zoenIconBoil: Bool = false
}

/// A Zoen icon (v2 ink, Direction 1 "Doodle objects"). `selected` adds the highlighter swipe
/// and draws the icon on; taps (`wiggle`) play its signature motion. Scales with Dynamic Type.
struct ZoenIcon: View {
    let glyph: ZoenGlyph
    var selected = false
    /// Base point size at the default text size; scales with Dynamic Type.
    @ScaledMetric private var size: CGFloat
    /// Changes to this value play the icon's signature motion (taps).
    var wiggle: Int = 0

    init(_ glyph: ZoenGlyph, selected: Bool = false, size: CGFloat = 22, relativeTo style: Font.TextStyle = .body, wiggle: Int = 0) {
        self.glyph = glyph
        self.selected = selected
        self._size = ScaledMetric(wrappedValue: size, relativeTo: style)
        self.wiggle = wiggle
    }

    var body: some View {
        // Chrome uses Apple's SF Symbols for now (user call, Oct 8). The hand-drawn sets stay
        // in the code behind flags: `-RodaInkIcons YES` (ink v2) and `-RodaIconsV1 YES`.
        if ZoenIconStyle.ink {
            InkIcon(glyph: glyph, direction: .shipping, selected: selected, size: size, trigger: wiggle)
        } else if UserDefaults.standard.bool(forKey: "RodaIconsV1") {
            ZoenIconV1(glyph, selected: selected, size: size, wiggle: wiggle)
        } else {
            SymbolIcon(glyph: glyph, selected: selected, size: size, wiggle: wiggle)
        }
    }
}

enum ZoenIconStyle {
    /// Hand-drawn ink icons instead of SF Symbols (off by default).
    static var ink: Bool { UserDefaults.standard.bool(forKey: "RodaInkIcons") }
}

/// SF Symbol for each glyph: outline at rest, `.fill` when selected; bounces on taps and
/// swaps with a replace transition when the selection changes.
struct SymbolIcon: View {
    let glyph: ZoenGlyph
    var selected = false
    var size: CGFloat
    var wiggle: Int = 0

    var body: some View {
        Image(systemName: glyph.symbol(selected: selected))
            .font(.system(size: size * 0.82, weight: selected ? .semibold : .medium))
            .contentTransition(.symbolEffect(.replace))
            .symbolEffect(.bounce, value: wiggle)
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

extension ZoenGlyph {
    /// The SF Symbol name (`.fill` variant when selected, if there is one).
    func symbol(selected: Bool = false) -> String {
        let (outline, fill): (String, String?) = switch self {
        case .chats: ("bubble.left.and.bubble.right", "bubble.left.and.bubble.right.fill")
        case .spaces: ("person.3", "person.3.fill")
        case .store: ("bag", "bag.fill")
        case .search: ("magnifyingglass", nil)
        case .plus: ("plus", nil)
        case .back: ("chevron.left", nil)
        case .phone: ("phone", "phone.fill")
        case .video: ("video", "video.fill")
        case .bell: ("bell", "bell.fill")
        case .camera: ("camera", "camera.fill")
        case .send: ("arrow.up", nil)
        case .mic: ("mic", "mic.fill")
        case .info: ("info.circle", "info.circle.fill")
        case .pin: ("pin", "pin.fill")
        case .settings: ("gearshape", "gearshape.fill")
        case .share: ("square.and.arrow.up", nil)
        case .close: ("xmark", nil)
        case .check: ("checkmark", nil)
        case .more: ("ellipsis", nil)
        case .sparkle: ("sparkles", nil)
        case .agents: ("person.2.wave.2", "person.2.wave.2.fill")
        case .folder: ("folder", "folder.fill")
        case .you: ("person.crop.circle", "person.crop.circle.fill")
        case .chevron: ("chevron.right", nil)
        case .crown: ("crown", "crown.fill")
        case .sun: ("sun.max", "sun.max.fill")
        case .heart: ("heart", "heart.fill")
        case .game: ("gamecontroller", "gamecontroller.fill")
        case .list: ("list.bullet", nil)
        case .note: ("doc.text", "doc.text.fill")
        case .download: ("arrow.down.circle", "arrow.down.circle.fill")
        case .lock: ("lock", "lock.fill")
        case .trash: ("trash", "trash.fill")
        case .play: ("play.fill", nil)
        case .pause: ("pause.fill", nil)
        }
        return selected ? (fill ?? outline) : outline
    }
}

/// v1 outline icons, kept for the before/after shots (`-RodaIconsV1 YES`).
struct ZoenIconV1: View {
    let glyph: ZoenGlyph
    var selected = false
    var size: CGFloat
    var wiggle: Int = 0

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.zoenIconBoil) private var boil
    @State private var draw: CGFloat = 1
    @State private var bounce = false
    @State private var shake: Double = 0

    init(_ glyph: ZoenGlyph, selected: Bool = false, size: CGFloat = 22, wiggle: Int = 0) {
        self.glyph = glyph; self.selected = selected; self.size = size; self.wiggle = wiggle
    }

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 10, paused: !boil || reduceMotion)) { tl in
            let frame = boil && !reduceMotion ? Int(tl.date.timeIntervalSinceReferenceDate * 10) % 4 + 1 : 0
            ZStack {
                if selected {
                    GlyphShape(glyph: glyph, wash: true, frame: frame)
                        .fill(.foreground.opacity(0.28))
                        .opacity(Double(draw))
                }
                GlyphShape(glyph: glyph, frame: frame)
                    .trim(from: 0, to: draw)
                    .stroke(.foreground, style: StrokeStyle(lineWidth: max(1.2, size * (selected ? 2.1 : 1.85) / 24), lineCap: .round, lineJoin: .round))
            }
        }
        .frame(width: size, height: size)
        .scaleEffect(bounce ? 1.14 : 1)
        .rotationEffect(.degrees(shake))
        .onChange(of: selected) { _, on in
            guard on, !reduceMotion else { return }
            draw = 0
            withAnimation(.easeOut(duration: 0.38)) { draw = 1 }
            withAnimation(.spring(response: 0.22, dampingFraction: 0.5)) { bounce = true }
            withAnimation(.spring(response: 0.3, dampingFraction: 0.6).delay(0.16)) { bounce = false }
        }
        .onChange(of: wiggle) { _, _ in
            guard !reduceMotion else { return }
            withAnimation(.spring(response: 0.12, dampingFraction: 0.3)) { shake = 9 }
            withAnimation(.spring(response: 0.25, dampingFraction: 0.45).delay(0.1)) { shake = 0 }
        }
        .accessibilityHidden(true)
    }
}

/// Our buttons press their icons: a slight sink and the line boils while held.
struct IconPressStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .environment(\.zoenIconBoil, configuration.isPressed)
            .scaleEffect(configuration.isPressed ? 0.94 : 1)
            .animation(.spring(response: 0.25, dampingFraction: 0.6), value: configuration.isPressed)
    }
}

extension ZoenGlyph {
    /// Radial menu items by id.
    static func radial(_ id: String) -> ZoenGlyph? {
        switch id { case "zoen": .sparkle; case "agents": .agents; case "files": .folder; case "you": .you; default: nil }
    }
}

/// `-RodaIconSheet YES`: every icon at 20/24/32pt plus the selected state (24pt), fixed
/// cells so nothing overlaps.
struct ZoenIconSheet: View {
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 4) {
                Text(verbatim: "Zoen icons · 20 / 24 / 32 · selected 24").font(.caption.weight(.semibold)).foregroundStyle(Palette.textSecondary)
                LazyVGrid(columns: [GridItem(.fixed(186), spacing: 6), GridItem(.fixed(186), spacing: 6)], alignment: .leading, spacing: 2) {
                    ForEach(ZoenGlyph.allCases) { g in
                        HStack(spacing: 0) {
                            Text(g.rawValue).font(.system(size: 8).monospaced()).foregroundStyle(Palette.textTertiary)
                                .frame(width: 42, alignment: .leading).lineLimit(1).minimumScaleFactor(0.7)
                            ForEach([20, 24, 32] as [CGFloat], id: \.self) { s in ZoenIcon(g, size: s).frame(width: 36, height: 36) }
                            ZoenIcon(g, selected: true, size: 24).frame(width: 36, height: 36).foregroundStyle(Palette.action)
                        }
                        .frame(width: 186, height: 36, alignment: .leading)
                        .clipped()
                    }
                }
            }
            .padding(.horizontal, 12)
            .padding(.top, 52)
            .foregroundStyle(Palette.textPrimary)
        }
        .background(Palette.background.ignoresSafeArea())
    }
}

@MainActor
extension ZoenGlyph {
    private static var menuCache: [ZoenGlyph: Image] = [:]
    /// The icon as a template Image, for places that only take images (context menus).
    var menuImage: Image {
        guard ZoenIconStyle.ink else { return Image(systemName: symbol()) }
        if let i = Self.menuCache[self] { return i }
        let r = ImageRenderer(content: InkIcon(glyph: self, size: 22).foregroundStyle(.black).frame(width: 24, height: 24))
        r.scale = 3
        #if os(iOS)
        let img = r.uiImage.map { Image(uiImage: $0.withRenderingMode(.alwaysTemplate)) } ?? Image(systemName: "circle")
        #else
        let img: Image = {
            guard let ns = r.nsImage else { return Image(systemName: "circle") }
            ns.isTemplate = true
            return Image(nsImage: ns)
        }()
        #endif
        Self.menuCache[self] = img
        return img
    }
}
