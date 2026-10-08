import SwiftUI

// MARK: - Hand-drawn ink: Roda's illustration language
//
// Standing rule: every generated or decorative illustration looks hand-drawn and moves.
// • Strokes are ink ribbons with pen pressure (variable width, tapered ends) drawn
//   through slightly jittered control points, plus a faint pencil "ghost" pass.
// • Fills are marker fills: a touch off the line, like a hand that misses.
// • The jitter is re-seeded ~10×/s: the "boiling line" of hand-drawn animation.
// • Strokes draw on in order (trim), and drawings breathe.
// • Reduce Motion: one static, fully drawn frame.
// Everything is drawn on device with Canvas; no images, no external services.

struct InkRNG {
    private var state: UInt64
    init(_ seed: UInt64) { state = seed &* 0x2545F4914F6CDD1D &+ 0x9E3779B97F4A7C15 }
    mutating func unit() -> Double {
        state &+= 0x9E3779B97F4A7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58476D1CE4E5B9
        z = (z ^ (z >> 27)) &* 0x94D049BB133111EB
        z ^= z >> 31
        return Double(z >> 11) / 9_007_199_254_740_992.0
    }
    mutating func signed() -> CGFloat { CGFloat(unit() * 2 - 1) }
}

enum InkPalette {
    static let ink = Color(hex: "#2B2530")
    static let paper = Color(hex: "#FFFCF5")
    static let tomato = Color(hex: "#F07052")
    static let butter = Color(hex: "#F7CE5B")
    static let mint = Color(hex: "#5CC79E")
    static let sky = Color(hex: "#9CCBFF")
    static let lilac = Color(hex: "#C3B2FF")
    static let blush = Color(hex: "#FFB8AA")
    static let cardboard = Color(hex: "#DDAE73")
    static let steam = Color(hex: "#A79FAE")
}

/// One stroke in unit space (0…1, y down) of a square drawing.
struct InkStroke {
    var points: [CGPoint]
    var closed = false
    var width: CGFloat = 0.024
    var color: Color = InkPalette.ink
    var fill: Color? = nil
    /// When this stroke starts drawing, as a fraction of the whole draw-on (0…1).
    var start: Double = 0
    var span: Double = 0.55
    var wobble: CGFloat = 1
    var smooth = true
    var opacity: Double = 1
    var ghost = true
    /// How far the marker fill slips off the line (1 = the usual hand slip, 0 = exact).
    var misregister: CGFloat = 1

    static func line(_ pts: [(CGFloat, CGFloat)], width: CGFloat = 0.024, color: Color = InkPalette.ink, start: Double = 0, span: Double = 0.55, smooth: Bool = true, opacity: Double = 1) -> InkStroke {
        InkStroke(points: pts.map { CGPoint(x: $0.0, y: $0.1) }, width: width, color: color, start: start, span: span, smooth: smooth, opacity: opacity)
    }

    static func shape(_ pts: [(CGFloat, CGFloat)], fill: Color?, width: CGFloat = 0.024, color: Color = InkPalette.ink, start: Double = 0, span: Double = 0.55, smooth: Bool = true, opacity: Double = 1) -> InkStroke {
        InkStroke(points: pts.map { CGPoint(x: $0.0, y: $0.1) }, closed: true, width: width, color: color, fill: fill, start: start, span: span, smooth: smooth, opacity: opacity)
    }

    /// A hand-drawn ellipse. Open ones overshoot past the start, like a quick pen loop.
    static func ellipse(_ cx: CGFloat, _ cy: CGFloat, _ rx: CGFloat, _ ry: CGFloat, fill: Color? = nil, closed: Bool = true, width: CGFloat = 0.024, color: Color = InkPalette.ink, start: Double = 0, span: Double = 0.55, n: Int = 12, opacity: Double = 1) -> InkStroke {
        let turns = closed ? 1.0 : 1.1
        let count = closed ? n : Int(Double(n) * turns) + 1
        let a0 = -Double.pi * 0.6
        let pts = (0..<count).map { i -> CGPoint in
            let a = a0 + Double(i) / Double(n) * 2 * .pi
            return CGPoint(x: cx + rx * CGFloat(cos(a)), y: cy + ry * CGFloat(sin(a)))
        }
        return InkStroke(points: pts, closed: closed, width: width, color: color, fill: fill, start: start, span: span, opacity: opacity)
    }

    /// A hand-drawn rounded rectangle (r is a fraction of the shorter side).
    static func box(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat, r: CGFloat = 0.12, fill: Color? = nil, closed: Bool = true, width: CGFloat = 0.024, color: Color = InkPalette.ink, start: Double = 0, span: Double = 0.55, opacity: Double = 1) -> InkStroke {
        let rr = min(w, h) * r
        var pts: [CGPoint] = []
        let corners: [(CGFloat, CGFloat, Double)] = [(x + w - rr, y + rr, -.pi / 2), (x + w - rr, y + h - rr, 0), (x + rr, y + h - rr, .pi / 2), (x + rr, y + rr, .pi)]
        for (cx, cy, a0) in corners {
            for k in 0...2 {
                let a = a0 + Double(k) / 2 * .pi / 2
                pts.append(CGPoint(x: cx + rr * CGFloat(cos(a)), y: cy + rr * CGFloat(sin(a))))
            }
        }
        if !closed { pts.append(pts[0]); pts.append(pts[1]) }
        return InkStroke(points: pts, closed: closed, width: width, color: color, fill: fill, start: start, span: span, opacity: opacity)
    }

    func moved(_ dx: CGFloat, _ dy: CGFloat) -> InkStroke {
        var s = self; s.points = points.map { CGPoint(x: $0.x + dx, y: $0.y + dy) }; return s
    }

    func rotated(_ angle: Double, around o: CGPoint) -> InkStroke {
        var s = self
        let c = CGFloat(cos(angle)), si = CGFloat(sin(angle))
        s.points = points.map { p in
            let dx = p.x - o.x, dy = p.y - o.y
            return CGPoint(x: o.x + dx * c - dy * si, y: o.y + dx * si + dy * c)
        }
        return s
    }

    func faded(_ o: Double) -> InkStroke { var s = self; s.opacity *= o; return s }
}

enum Ink {
    /// Draws strokes in the centered square of `size`. `frame` re-seeds the jitter (boil);
    /// `progress` is the draw-on (0…1, past 1 = fully drawn).
    static func render(_ strokes: [InkStroke], ctx: GraphicsContext, size: CGSize, seed: UInt64, frame: Int, progress: Double, jitter: CGFloat = 1) {
        let side = min(size.width, size.height)
        let ox = (size.width - side) / 2, oy = (size.height - side) / 2
        for (i, s) in strokes.enumerated() where s.opacity > 0.01 {
            let local = s.span <= 0 ? 1 : min(1, max(0, (progress - s.start) / s.span))
            if local <= 0 { continue }
            var rng = InkRNG(seed &+ UInt64(i) &* 1_000_003 &+ UInt64(frame) &* 7_919)
            let amp = side * 0.0055 * jitter * s.wobble
            let pts = s.points.map { CGPoint(x: ox + $0.x * side + rng.signed() * amp, y: oy + $0.y * side + rng.signed() * amp) }
            let poly = s.smooth ? catmull(pts, closed: s.closed) : linear(pts, closed: s.closed)
            if let fill = s.fill {
                let a = min(1, max(0, (local - 0.45) / 0.4))
                if a > 0 {
                    let m = s.misregister
                    let dx = (side * 0.011 + rng.signed() * side * 0.003) * m, dy = (side * 0.009 + rng.signed() * side * 0.003) * m
                    var path = Path()
                    path.addLines(poly.map { CGPoint(x: $0.x + dx, y: $0.y + dy) })
                    path.closeSubpath()
                    ctx.fill(path, with: .color(fill.opacity(a * s.opacity)))
                }
            }
            guard s.width > 0 else { continue }
            let drawn = trim(poly, local)
            guard drawn.count > 1 else { continue }
            let w = max(0.9, s.width * side)
            ctx.fill(ribbon(drawn, width: w, tapered: !s.closed || local < 1, rng: &rng), with: .color(s.color.opacity(s.opacity)))
            if s.ghost && w > 1.4 {
                var ghost = Path()
                ghost.addLines(drawn.map { CGPoint(x: $0.x + side * 0.005, y: $0.y - side * 0.004) })
                ctx.stroke(ghost, with: .color(s.color.opacity(0.22 * s.opacity)), style: StrokeStyle(lineWidth: max(0.5, w * 0.28), lineCap: .round, lineJoin: .round))
            }
        }
    }

    /// Paper grain: a few faint specks that also boil.
    static func grain(ctx: GraphicsContext, size: CGSize, frame: Int, count: Int = 70, color: Color = InkPalette.ink) {
        var rng = InkRNG(UInt64(9_001 + frame))
        for _ in 0..<count {
            let x = CGFloat(rng.unit()) * size.width, y = CGFloat(rng.unit()) * size.height
            let r = 0.4 + CGFloat(rng.unit()) * 0.8
            ctx.fill(Path(ellipseIn: CGRect(x: x, y: y, width: r, height: r)), with: .color(color.opacity(0.07)))
        }
    }

    static func catmull(_ p: [CGPoint], closed: Bool, steps: Int = 8) -> [CGPoint] {
        let n = p.count
        guard n > 2 else { return closed && n == 2 ? p + [p[0]] : p }
        func at(_ i: Int) -> CGPoint { closed ? p[((i % n) + n) % n] : p[min(max(i, 0), n - 1)] }
        var out: [CGPoint] = []
        out.reserveCapacity(n * steps + 1)
        let segs = closed ? n : n - 1
        for i in 0..<segs {
            let p0 = at(i - 1), p1 = at(i), p2 = at(i + 1), p3 = at(i + 2)
            for s in 0..<steps {
                let t = CGFloat(s) / CGFloat(steps)
                out.append(CGPoint(x: spline(p0.x, p1.x, p2.x, p3.x, t), y: spline(p0.y, p1.y, p2.y, p3.y, t)))
            }
        }
        out.append(closed ? p[0] : p[n - 1])
        return out
    }

    private static func spline(_ a: CGFloat, _ b: CGFloat, _ c: CGFloat, _ d: CGFloat, _ t: CGFloat) -> CGFloat {
        let t2 = t * t
        let t3 = t2 * t
        let k1 = -a + c
        let k2 = 2 * a - 5 * b + 4 * c - d
        let k3 = -a + 3 * b - 3 * c + d
        return 0.5 * (2 * b + k1 * t + k2 * t2 + k3 * t3)
    }

    static func linear(_ p: [CGPoint], closed: Bool) -> [CGPoint] {
        guard closed, let first = p.first else { return p }
        // Subdivide so the ribbon gets enough samples for pressure.
        var out: [CGPoint] = []
        let ring = p + [first]
        for i in 0..<(ring.count - 1) {
            for s in 0..<4 {
                let t = CGFloat(s) / 4
                out.append(CGPoint(x: ring[i].x + (ring[i + 1].x - ring[i].x) * t, y: ring[i].y + (ring[i + 1].y - ring[i].y) * t))
            }
        }
        out.append(first)
        return out
    }

    static func trim(_ p: [CGPoint], _ f: Double) -> [CGPoint] {
        guard f < 1, p.count > 1 else { return p }
        var total: CGFloat = 0
        for i in 1..<p.count { total += hypot(p[i].x - p[i - 1].x, p[i].y - p[i - 1].y) }
        let target = total * CGFloat(f)
        var acc: CGFloat = 0
        var out = [p[0]]
        for i in 1..<p.count {
            let d = hypot(p[i].x - p[i - 1].x, p[i].y - p[i - 1].y)
            if acc + d >= target {
                let t = d > 0 ? (target - acc) / d : 0
                out.append(CGPoint(x: p[i - 1].x + (p[i].x - p[i - 1].x) * t, y: p[i - 1].y + (p[i].y - p[i - 1].y) * t))
                return out
            }
            acc += d
            out.append(p[i])
        }
        return out
    }

    /// An ink ribbon: width varies with a slow "pressure" wave and tapers at open ends.
    static func ribbon(_ p: [CGPoint], width: CGFloat, tapered: Bool, rng: inout InkRNG) -> Path {
        let n = p.count
        let k = 5 + rng.unit() * 7, phase = rng.unit() * 6.28
        var left: [CGPoint] = [], right: [CGPoint] = []
        left.reserveCapacity(n); right.reserveCapacity(n)
        for i in 0..<n {
            let a = p[max(0, i - 1)], b = p[min(n - 1, i + 1)]
            var dx = b.x - a.x, dy = b.y - a.y
            let len = max(0.0001, hypot(dx, dy)); dx /= len; dy /= len
            let u = n > 1 ? Double(i) / Double(n - 1) : 0
            let pressure = 0.82 + 0.3 * (0.5 + 0.5 * sin(u * k + phase))
            let taper = tapered ? 0.3 + 0.7 * pow(sin(Double.pi * u), 0.45) : 1
            let h = width * 0.5 * CGFloat(pressure * taper)
            left.append(CGPoint(x: p[i].x - dy * h, y: p[i].y + dx * h))
            right.append(CGPoint(x: p[i].x + dy * h, y: p[i].y - dx * h))
        }
        var path = Path()
        path.addLines(left + right.reversed())
        path.closeSubpath()
        return path
    }
}

/// `-RodaFreezeArt 2.4` freezes every drawing at that time (deterministic screenshots).
enum InkClock {
    static let freeze: Double? = {
        let v = UserDefaults.standard.double(forKey: "RodaFreezeArt")
        return v > 0 ? v : nil
    }()
}

/// A hand-drawn, animated drawing: strokes as a function of elapsed time.
struct InkDrawing: View {
    var seed: UInt64 = 7
    var drawOn: Double = 1.1
    var fps: Double = 10
    var breathe: CGFloat = 0.014
    var jitter: CGFloat = 1
    var grain = false
    var freezeAt: Double? = nil
    let strokes: (Double) -> [InkStroke]
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var start = Date()
    /// Paused while scrolled off screen (carousels, long lists).
    @State private var onScreen = true

    var body: some View {
        let frozen = freezeAt ?? InkClock.freeze
        TimelineView(.animation(minimumInterval: 1 / fps, paused: reduceMotion || frozen != nil || !onScreen)) { tl in
            let t: Double = frozen ?? (reduceMotion ? 0 : max(0, tl.date.timeIntervalSince(start)))
            let frame = reduceMotion ? 0 : Int(t * fps) % 4
            let progress = reduceMotion || drawOn <= 0 || frozen != nil ? 2 : t / drawOn
            Canvas { ctx, size in
                if grain { Ink.grain(ctx: ctx, size: size, frame: frame) }
                Ink.render(strokes(t), ctx: ctx, size: size, seed: seed, frame: frame, progress: progress, jitter: jitter)
            }
            .scaleEffect(reduceMotion ? 1 : 1 + breathe * CGFloat(sin(t * 2.1)))
        }
        .onAppear { start = .now }
        .onScrollVisibilityChange(threshold: 0.01) { onScreen = $0 }
        .accessibilityHidden(true)
    }
}

/// A boiling ink outline (agent avatars, the agent orb). `loop` turns it into a ring
/// that keeps drawing itself (the agent is working).
struct InkOutline: View {
    var corner: CGFloat = 0.3
    var color: Color = .white.opacity(0.6)
    var width: CGFloat = 0.045
    var seed: UInt64 = 3
    var loop: Double? = nil
    var fps: Double = 8
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / fps, paused: reduceMotion)) { tl in
            let t = reduceMotion ? 0 : tl.date.timeIntervalSinceReferenceDate
            Canvas { ctx, size in
                let frame = reduceMotion ? 0 : Int(t * fps) % 4
                let progress: Double = reduceMotion ? 2 : loop.map { (t / $0).truncatingRemainder(dividingBy: 1) * 1.3 } ?? 2
                let ring = InkStroke.box(0.04, 0.04, 0.92, 0.92, r: corner, closed: loop == nil, width: width, color: color, span: 1)
                Ink.render([ring], ctx: ctx, size: size, seed: seed, frame: frame, progress: progress, jitter: 0.7)
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

// MARK: - Doodles

enum Doodle {
    case pot, allClear, mentions, tasks, search, lost, trophy, ballot, notepad, roda, trip, hike
    case heart, vinyl, moss, hourglass, steps, pin

    /// Snapshot `art` names → doodles (Home strip, chat tiles, widgets and the Store share them).
    static func named(_ name: String) -> Doodle? {
        switch name {
        case "pot": .pot; case "ballot": .ballot; case "notepad": .notepad; case "trip": .trip; case "hike": .hike
        case "heart": .heart; case "vinyl": .vinyl; case "moss": .moss; case "hourglass": .hourglass
        case "steps": .steps; case "pin": .pin; case "trophy": .trophy; case "tasks": .tasks
        default: nil
        }
    }

    var seed: UInt64 {
        switch self {
        case .pot: 11; case .allClear: 12; case .mentions: 13; case .tasks: 14; case .search: 15
        case .lost: 16; case .trophy: 17; case .ballot: 18; case .notepad: 19; case .roda: 20
        case .trip: 21; case .hike: 22
        case .heart: 23; case .vinyl: 24; case .moss: 25; case .hourglass: 26; case .steps: 27; case .pin: 28
        }
    }

    private static func frac(_ x: Double) -> Double { x - x.rounded(.down) }

    static func sparkle(_ cx: CGFloat, _ cy: CGFloat, _ r: CGFloat, color: Color = InkPalette.butter, start: Double = 0.7, opacity: Double = 1) -> [InkStroke] {
        [InkStroke.line([(cx, cy - r), (cx, cy + r)], width: 0.018, color: color, start: start, span: 0.2, smooth: false, opacity: opacity),
         InkStroke.line([(cx - r, cy), (cx + r, cy)], width: 0.018, color: color, start: start + 0.05, span: 0.2, smooth: false, opacity: opacity)]
    }

    func strokes(_ t: Double) -> [InkStroke] {
        switch self {
        case .pot:
            var s: [InkStroke] = []
            for i in 0..<3 {
                let ph = Self.frac(t * 0.55 + Double(i) / 3)
                let x = 0.38 + CGFloat(i) * 0.12
                let sway = CGFloat(sin(t * 2 + Double(i))) * 0.012
                s.append(InkStroke.line([(x, 0.33), (x + 0.03 + sway, 0.27), (x - 0.03 + sway, 0.2), (x + 0.02, 0.13)], width: 0.018, color: InkPalette.steam, start: 0.75, span: 0.25, opacity: (1 - ph) * 0.9).moved(0, -CGFloat(ph) * 0.06))
            }
            let r = Self.frac(t / 2.6)
            let rattle = r < 0.16 ? -abs(CGFloat(sin(r / 0.16 * .pi * 3))) * 0.014 : 0
            s += [
                InkStroke.line([(0.06, 0.87), (0.5, 0.86), (0.94, 0.875)], width: 0.012, opacity: 0.45),
                InkStroke.shape([(0.21, 0.46), (0.79, 0.46), (0.76, 0.73), (0.66, 0.8), (0.34, 0.8), (0.24, 0.73)], fill: InkPalette.tomato, width: 0.026),
                InkStroke.line([(0.21, 0.53), (0.11, 0.52), (0.1, 0.6), (0.22, 0.61)], width: 0.024, start: 0.3),
                InkStroke.line([(0.79, 0.53), (0.89, 0.52), (0.9, 0.6), (0.78, 0.61)], width: 0.024, start: 0.35),
                InkStroke.line([(0.3, 0.55), (0.31, 0.68)], width: 0.02, color: .white, start: 0.6, span: 0.2, opacity: 0.85),
                InkStroke.shape([(0.23, 0.45), (0.3, 0.38), (0.5, 0.35), (0.7, 0.38), (0.77, 0.45)], fill: InkPalette.butter, width: 0.024, start: 0.4).moved(0, rattle),
                InkStroke.ellipse(0.5, 0.33, 0.035, 0.022, fill: InkPalette.ink, width: 0.02, start: 0.55, span: 0.2).moved(0, rattle),
                InkStroke.line([(0.16, 0.465), (0.84, 0.455)], width: 0.03, start: 0.2),
            ]
            return s
        case .allClear:
            let tw = 0.5 + 0.5 * sin(t * 3)
            return [
                InkStroke.shape([(0.14, 0.52), (0.3, 0.52), (0.36, 0.6), (0.64, 0.6), (0.7, 0.52), (0.86, 0.52), (0.86, 0.78), (0.14, 0.78)], fill: InkPalette.lilac, smooth: false),
                InkStroke.line([(0.32, 0.3), (0.45, 0.43), (0.7, 0.14)], width: 0.045, color: InkPalette.mint, start: 0.45, span: 0.35, smooth: false),
            ] + Self.sparkle(0.8, 0.24, 0.04, start: 0.8, opacity: tw) + Self.sparkle(0.2, 0.3, 0.03, color: InkPalette.blush, start: 0.85, opacity: 1 - tw)
        case .mentions:
            let dots = (0..<3).map { i in InkStroke.ellipse(0.55 + CGFloat(i) * 0.075, 0.64, 0.016, 0.016, fill: InkPalette.ink, width: 0.014, start: 0.7, span: 0.15).moved(0, -abs(CGFloat(sin(t * 5 - Double(i) * 0.7))) * 0.025) }
            return [
                InkStroke.ellipse(0.42, 0.38, 0.29, 0.2, fill: InkPalette.paper, width: 0.026),
                InkStroke.line([(0.3, 0.55), (0.24, 0.68), (0.4, 0.57)], width: 0.024, start: 0.25, smooth: false),
                InkStroke.line([(0.27, 0.34), (0.56, 0.33)], width: 0.02, start: 0.35, opacity: 0.6),
                InkStroke.line([(0.27, 0.42), (0.46, 0.42)], width: 0.02, start: 0.4, opacity: 0.6),
                InkStroke.ellipse(0.63, 0.64, 0.22, 0.14, fill: InkPalette.sky, width: 0.026, start: 0.45),
                InkStroke.line([(0.74, 0.76), (0.82, 0.86), (0.7, 0.78)], width: 0.022, start: 0.6, smooth: false),
            ] + dots
        case .tasks:
            var s: [InkStroke] = [
                InkStroke.box(0.25, 0.17, 0.5, 0.68, r: 0.08, fill: Color(hex: "#FFF1CC")),
                InkStroke.box(0.4, 0.12, 0.2, 0.09, r: 0.3, fill: InkPalette.ink, width: 0.02, start: 0.2),
            ]
            for i in 0..<3 {
                let y = 0.36 + CGFloat(i) * 0.15
                let st = 0.35 + Double(i) * 0.12
                s.append(InkStroke.box(0.32, y - 0.035, 0.07, 0.07, r: 0.2, width: 0.018, start: st, span: 0.2))
                s.append(InkStroke.line([(0.45, y), (0.67, y + 0.005)], width: 0.018, start: st + 0.05, span: 0.25, opacity: 0.7))
                s.append(InkStroke.line([(0.32, y - 0.01), (0.355, y + 0.03), (0.42, y - 0.06)], width: 0.026, color: InkPalette.mint, start: st + 0.2, span: 0.15, smooth: false))
            }
            return s
        case .search:
            let dx = CGFloat(sin(t * 1.5)) * 0.05, dy = CGFloat(cos(t * 3)) * 0.015
            return [
                InkStroke.line([(0.1, 0.82), (0.5, 0.8), (0.9, 0.83)], width: 0.012, opacity: 0.4),
                InkStroke.line([(0.58, 0.57), (0.78, 0.76)], width: 0.065, start: 0.3).moved(dx, dy),
                InkStroke.ellipse(0.45, 0.43, 0.2, 0.2, fill: InkPalette.sky.opacity(0.55), width: 0.03).moved(dx, dy),
                InkStroke.line([(0.34, 0.38), (0.38, 0.31), (0.46, 0.28)], width: 0.02, color: .white, start: 0.6, span: 0.2).moved(dx, dy),
            ]
        case .lost:
            let a = sin(t * 1.3) * 0.06
            let o = CGPoint(x: 0.5, y: 0.8)
            return [
                InkStroke.shape([(0.28, 0.16), (0.62, 0.16), (0.74, 0.28), (0.74, 0.82), (0.28, 0.82)], fill: InkPalette.paper, smooth: false),
                InkStroke.line([(0.62, 0.16), (0.62, 0.28), (0.74, 0.28)], width: 0.02, start: 0.3, smooth: false),
                InkStroke.line([(0.43, 0.42), (0.45, 0.35), (0.53, 0.33), (0.58, 0.39), (0.51, 0.47), (0.51, 0.55)], width: 0.032, color: InkPalette.tomato, start: 0.45, span: 0.35),
                InkStroke.ellipse(0.51, 0.65, 0.018, 0.018, fill: InkPalette.tomato, width: 0.016, color: InkPalette.tomato, start: 0.8, span: 0.1),
            ].map { $0.rotated(a, around: o) }
        case .trophy:
            let tw = 0.5 + 0.5 * sin(t * 2.7)
            return [
                InkStroke.line([(0.32, 0.28), (0.19, 0.28), (0.2, 0.4), (0.36, 0.44)], width: 0.022, start: 0.3),
                InkStroke.line([(0.68, 0.28), (0.81, 0.28), (0.8, 0.4), (0.64, 0.44)], width: 0.022, start: 0.35),
                InkStroke.shape([(0.31, 0.2), (0.69, 0.2), (0.66, 0.42), (0.56, 0.52), (0.44, 0.52), (0.34, 0.42)], fill: InkPalette.butter),
                InkStroke.line([(0.5, 0.52), (0.5, 0.64)], width: 0.03, start: 0.45, smooth: false),
                InkStroke.box(0.35, 0.64, 0.3, 0.12, r: 0.15, fill: InkPalette.tomato, start: 0.5),
                InkStroke.line([(0.4, 0.27), (0.41, 0.38)], width: 0.02, color: .white, start: 0.6, span: 0.2, opacity: 0.85),
            ] + Self.sparkle(0.82, 0.14, 0.045, start: 0.7, opacity: tw) + Self.sparkle(0.16, 0.6, 0.03, color: InkPalette.sky, start: 0.75, opacity: 1 - tw)
        case .ballot:
            let ph = Self.frac(t / 2.2)
            let drop = CGFloat(min(1, ph / 0.6)) * 0.2
            return [
                InkStroke.box(0.38, 0.16, 0.24, 0.3, r: 0.1, fill: InkPalette.paper, width: 0.02).moved(0, drop),
                InkStroke.line([(0.44, 0.3), (0.49, 0.35), (0.57, 0.24)], width: 0.026, color: InkPalette.mint, start: 0.3, span: 0.2, smooth: false).moved(0, drop),
                InkStroke.box(0.2, 0.5, 0.6, 0.34, r: 0.08, fill: InkPalette.sky, start: 0.2),
                InkStroke.line([(0.34, 0.5), (0.66, 0.5)], width: 0.04, start: 0.5, smooth: false),
                InkStroke.line([(0.28, 0.66), (0.5, 0.65)], width: 0.018, color: .white, start: 0.6, opacity: 0.8),
            ]
        case .notepad:
            let tick = Self.frac(t / 3)
            var s: [InkStroke] = [InkStroke.box(0.24, 0.16, 0.52, 0.7, r: 0.06, fill: InkPalette.paper)]
            for i in 0..<3 { s.append(InkStroke.ellipse(0.35 + CGFloat(i) * 0.15, 0.16, 0.025, 0.04, closed: false, width: 0.018, start: 0.25)) }
            for i in 0..<4 {
                let y = 0.33 + CGFloat(i) * 0.13
                s.append(InkStroke.box(0.31, y - 0.03, 0.06, 0.06, r: 0.2, width: 0.016, start: 0.3 + Double(i) * 0.08, span: 0.15))
                s.append(InkStroke.line([(0.42, y), (0.68 - CGFloat(i % 2) * 0.08, y)], width: 0.016, start: 0.35 + Double(i) * 0.08, opacity: 0.65))
                if Double(i) < tick * 5 { s.append(InkStroke.line([(0.31, y - 0.01), (0.34, y + 0.025), (0.39, y - 0.05)], width: 0.024, color: InkPalette.tomato, start: 0.6, span: 0.1, smooth: false)) }
            }
            return s
        case .trip:
            // A sun over the bay, two hills and a little sailboat bobbing on the waves.
            let bob = CGFloat(sin(t * 1.8)) * 0.012
            let drift = CGFloat(sin(t * 0.4)) * 0.03
            var s: [InkStroke] = [
                InkStroke.ellipse(0.7, 0.3, 0.13, 0.13, fill: InkPalette.butter, width: 0.022),
                InkStroke.shape([(0.0, 0.62), (0.18, 0.44), (0.34, 0.52), (0.5, 0.4), (0.66, 0.62)], fill: InkPalette.mint, width: 0.022, start: 0.2),
            ]
            for i in 0..<3 {
                let y = 0.68 + CGFloat(i) * 0.09
                let ph = t * 1.4 + Double(i)
                let pts = (0...6).map { k -> (CGFloat, CGFloat) in (CGFloat(k) / 6, y + CGFloat(sin(ph + Double(k) * 1.3)) * 0.012) }
                s.append(InkStroke.line(pts, width: 0.018, color: InkPalette.sky, start: 0.35 + Double(i) * 0.08, span: 0.3, opacity: 0.9 - Double(i) * 0.2))
            }
            s += [
                InkStroke.shape([(0.5, 0.7), (0.74, 0.7), (0.7, 0.76), (0.54, 0.76)], fill: InkPalette.tomato, width: 0.02, start: 0.5, smooth: false).moved(drift, bob),
                InkStroke.line([(0.62, 0.7), (0.62, 0.5)], width: 0.018, start: 0.6, smooth: false).moved(drift, bob),
                InkStroke.shape([(0.63, 0.52), (0.72, 0.67), (0.63, 0.67)], fill: InkPalette.paper, width: 0.018, start: 0.65, smooth: false).moved(drift, bob),
            ]
            return s
        case .hike:
            // Two peaks, a zigzag trail up the near one and a flag at the top.
            let wave = CGFloat(sin(t * 3)) * 0.015
            return [
                InkStroke.ellipse(0.8, 0.2, 0.07, 0.07, fill: InkPalette.butter, width: 0.02),
                InkStroke.shape([(0.42, 0.82), (0.66, 0.36), (0.96, 0.82)], fill: InkPalette.sky.opacity(0.7), width: 0.022, start: 0.1, smooth: false),
                InkStroke.shape([(0.04, 0.84), (0.38, 0.26), (0.74, 0.84)], fill: InkPalette.mint, width: 0.026, start: 0.2, smooth: false),
                InkStroke.shape([(0.31, 0.38), (0.38, 0.26), (0.45, 0.38), (0.4, 0.35), (0.36, 0.39)], fill: InkPalette.paper, width: 0.018, start: 0.35, smooth: false),
                InkStroke.line([(0.22, 0.82), (0.44, 0.7), (0.28, 0.6), (0.44, 0.5), (0.36, 0.42)], width: 0.02, color: InkPalette.tomato, start: 0.45, span: 0.35, smooth: false, opacity: 0.9),
                InkStroke.line([(0.38, 0.26), (0.38, 0.12)], width: 0.018, start: 0.75, span: 0.1, smooth: false),
                InkStroke.shape([(0.385, 0.12), (0.48 + wave, 0.145), (0.385, 0.17)], fill: InkPalette.tomato, width: 0.016, start: 0.8, span: 0.1, smooth: false),
                InkStroke.line([(0.02, 0.86), (0.5, 0.85), (0.98, 0.86)], width: 0.012, start: 0.05, opacity: 0.45),
            ]
        case .heart:
            // A fat marker heart that beats, with two little motion ticks.
            let beat = 1 + 0.035 * CGFloat(max(0, sin(t * 4.2)) * max(0, sin(t * 4.2)))
            let pts: [(CGFloat, CGFloat)] = (0..<18).map { i in
                let a = Double(i) / 18 * 2 * .pi
                let x = 16 * pow(sin(a), 3)
                let y = 13 * cos(a) - 5 * cos(2 * a) - 2 * cos(3 * a) - cos(4 * a)
                return (0.5 + CGFloat(x) / 40 * beat, 0.5 - CGFloat(y) / 40 * beat)
            }
            return [
                InkStroke.shape(pts, fill: InkPalette.tomato, width: 0.03, span: 0.6),
                InkStroke.line([(0.36, 0.38), (0.33, 0.45)], width: 0.022, color: InkPalette.paper, start: 0.6, span: 0.15, opacity: 0.9),
                InkStroke.line([(0.12, 0.3), (0.18, 0.34)], width: 0.018, start: 0.75, span: 0.12, smooth: false),
                InkStroke.line([(0.86, 0.28), (0.8, 0.33)], width: 0.018, start: 0.8, span: 0.12, smooth: false),
            ]
        case .vinyl:
            // A record that spins (the highlight and the label notch go round).
            let a = t * 1.6
            let hx = 0.5 + 0.24 * CGFloat(cos(a)), hy = 0.5 + 0.24 * CGFloat(sin(a))
            let hx2 = 0.5 + 0.3 * CGFloat(cos(a + 0.5)), hy2 = 0.5 + 0.3 * CGFloat(sin(a + 0.5))
            return [
                InkStroke.ellipse(0.5, 0.5, 0.4, 0.4, fill: Color(hex: "#3A3440"), width: 0.03, span: 0.5, n: 16),
                InkStroke.ellipse(0.5, 0.5, 0.31, 0.31, closed: false, width: 0.012, color: InkPalette.paper, start: 0.35, span: 0.3, n: 16, opacity: 0.35),
                InkStroke.ellipse(0.5, 0.5, 0.22, 0.22, closed: false, width: 0.012, color: InkPalette.paper, start: 0.4, span: 0.3, n: 14, opacity: 0.3),
                InkStroke.ellipse(0.5, 0.5, 0.13, 0.13, fill: InkPalette.mint, width: 0.022, start: 0.5, span: 0.25),
                InkStroke.ellipse(0.5, 0.5, 0.025, 0.025, fill: InkPalette.paper, width: 0.012, start: 0.65, span: 0.1, n: 8),
                InkStroke.line([(hx, hy), (hx2, hy2)], width: 0.02, color: InkPalette.paper, start: 0.7, span: 0.15, opacity: 0.7),
            ]
        case .moss:
            // Moss, the garden spirit: a fuzzy green ball with a sprout that sways and blinks.
            let sway = CGFloat(sin(t * 1.7)) * 0.03
            let blink = Self.frac(t / 3.4) > 0.94
            let fuzz: [(CGFloat, CGFloat)] = (0..<22).map { i in
                let a = Double(i) / 22 * 2 * .pi
                let r = 0.3 + (i % 2 == 0 ? 0.025 : -0.01)
                return (0.5 + CGFloat(cos(a) * r), 0.58 + CGFloat(sin(a) * r * 0.92))
            }
            var s: [InkStroke] = [
                InkStroke.shape(fuzz, fill: InkPalette.mint, width: 0.026, span: 0.55),
                InkStroke.line([(0.5, 0.31), (0.5 + sway * 0.5, 0.22), (0.52 + sway, 0.14)], width: 0.022, start: 0.45, span: 0.2),
                InkStroke.shape([(0.52 + sway, 0.15), (0.64 + sway, 0.08), (0.66 + sway, 0.17), (0.56 + sway, 0.2)], fill: Color(hex: "#7FD38A"), width: 0.018, start: 0.55, span: 0.15),
                InkStroke.shape([(0.5 + sway * 0.6, 0.2), (0.38 + sway * 0.6, 0.13), (0.36 + sway * 0.6, 0.21), (0.46 + sway * 0.6, 0.24)], fill: Color(hex: "#7FD38A"), width: 0.018, start: 0.6, span: 0.15),
                InkStroke.ellipse(0.36, 0.66, 0.05, 0.03, fill: InkPalette.blush.opacity(0.8), width: 0.0, start: 0.8, span: 0.1),
                InkStroke.ellipse(0.64, 0.66, 0.05, 0.03, fill: InkPalette.blush.opacity(0.8), width: 0.0, start: 0.8, span: 0.1),
            ]
            if blink {
                s += [InkStroke.line([(0.36, 0.56), (0.44, 0.56)], width: 0.022, start: 0.7, span: 0.1),
                      InkStroke.line([(0.56, 0.56), (0.64, 0.56)], width: 0.022, start: 0.7, span: 0.1)]
            } else {
                s += [InkStroke.ellipse(0.4, 0.55, 0.055, 0.065, fill: InkPalette.ink, width: 0.016, start: 0.7, span: 0.15),
                      InkStroke.ellipse(0.6, 0.55, 0.055, 0.065, fill: InkPalette.ink, width: 0.016, start: 0.72, span: 0.15),
                      InkStroke.ellipse(0.415, 0.53, 0.015, 0.015, fill: InkPalette.paper, width: 0.0, start: 0.85, span: 0.05),
                      InkStroke.ellipse(0.615, 0.53, 0.015, 0.015, fill: InkPalette.paper, width: 0.0, start: 0.85, span: 0.05)]
            }
            s.append(InkStroke.line([(0.45, 0.68), (0.5, 0.71), (0.55, 0.68)], width: 0.016, start: 0.8, span: 0.1))
            return s
        case .hourglass:
            // Sand runs from the top bulb to the bottom one, then the glass flips.
            let ph = Self.frac(t / 6)
            let top = 0.36 - CGFloat(ph) * 0.12, bot = 0.76 - CGFloat(ph) * 0.12
            return [
                InkStroke.line([(0.24, 0.14), (0.76, 0.14)], width: 0.034, smooth: false),
                InkStroke.line([(0.24, 0.86), (0.76, 0.86)], width: 0.034, start: 0.05, smooth: false),
                InkStroke.shape([(0.5, 0.49), (0.38, top + 0.03), (0.62, top + 0.03)], fill: InkPalette.butter, width: 0.0, start: 0.4, span: 0.2, smooth: false),
                InkStroke.shape([(0.32, 0.82), (0.68, 0.82), (0.6, bot), (0.4, bot)], fill: InkPalette.butter, width: 0.0, start: 0.45, span: 0.2, smooth: false),
                InkStroke.line([(0.5, 0.5), (0.5, 0.8)], width: 0.008, color: InkPalette.cardboard, start: 0.6, span: 0.2, smooth: false, opacity: 0.9),
                InkStroke.shape([(0.3, 0.16), (0.7, 0.16), (0.68, 0.3), (0.54, 0.46), (0.54, 0.54), (0.68, 0.7), (0.7, 0.84), (0.3, 0.84), (0.32, 0.7), (0.46, 0.54), (0.46, 0.46), (0.32, 0.3)], fill: InkPalette.sky.opacity(0.35), width: 0.026, start: 0.1, span: 0.5),
                InkStroke.line([(0.27, 0.15), (0.27, 0.85)], width: 0.02, start: 0.3, span: 0.2, smooth: false),
                InkStroke.line([(0.73, 0.15), (0.73, 0.85)], width: 0.02, start: 0.32, span: 0.2, smooth: false),
            ]
        case .steps:
            // A 4×4 grid of days; filled ones pop in one after another.
            var s: [InkStroke] = []
            let lit = Int(Self.frac(t / 5) * 18)
            let colors = [InkPalette.mint, InkPalette.tomato, InkPalette.mint, InkPalette.butter]
            for r in 0..<4 { for c in 0..<4 {
                let i = r * 4 + c
                let on = [0, 1, 3, 5, 6, 9, 10, 11, 14].contains(i) && i <= max(lit, 9)
                s.append(InkStroke.box(0.14 + CGFloat(c) * 0.185, 0.14 + CGFloat(r) * 0.185, 0.15, 0.15, r: 0.25,
                                       fill: on ? colors[(r + c) % 4] : nil, width: 0.018, start: Double(i) * 0.03, span: 0.25, opacity: on ? 1 : 0.55))
            } }
            return s
        case .pin:
            // A map pin that drops and bounces on a dotted path.
            let ph = Self.frac(t / 2.8)
            let bounce = ph < 0.25 ? -CGFloat(sin(ph / 0.25 * .pi)) * 0.05 : 0
            return [
                InkStroke.line([(0.08, 0.86), (0.3, 0.8), (0.52, 0.88), (0.74, 0.8), (0.94, 0.86)], width: 0.014, color: InkPalette.steam, span: 0.4, opacity: 0.8),
                InkStroke.ellipse(0.5, 0.84, 0.12, 0.03, fill: InkPalette.ink.opacity(0.12), width: 0.0, start: 0.3, span: 0.2),
                InkStroke.shape([(0.5, 0.82), (0.33, 0.52), (0.3, 0.38), (0.36, 0.24), (0.5, 0.17), (0.64, 0.24), (0.7, 0.38), (0.67, 0.52)], fill: InkPalette.tomato, width: 0.028, start: 0.2, span: 0.5).moved(0, bounce),
                InkStroke.ellipse(0.5, 0.38, 0.075, 0.075, fill: InkPalette.paper, width: 0.02, start: 0.55, span: 0.2).moved(0, bounce),
            ]
        case .roda:
            var s: [InkStroke] = [InkStroke.ellipse(0.5, 0.5, 0.3, 0.3, closed: false, width: 0.03, span: 0.5, n: 16)]
            let colors = [InkPalette.blush, InkPalette.sky, InkPalette.butter, InkPalette.mint, InkPalette.lilac]
            for i in 0..<5 {
                let a = Double(i) / 5 * 2 * .pi - .pi / 2 + t * 0.18
                let x = 0.5 + 0.3 * CGFloat(cos(a)), y = 0.5 + 0.3 * CGFloat(sin(a))
                let bob = CGFloat(sin(t * 2.4 + Double(i))) * 0.008
                let st = 0.4 + Double(i) * 0.08
                if i == 4 {
                    s.append(InkStroke.box(x - 0.075, y - 0.075 + bob, 0.15, 0.15, r: 0.3, fill: colors[i], width: 0.024, start: st, span: 0.25))
                    s += Self.sparkle(x, y + bob, 0.035, color: InkPalette.ink, start: st + 0.15)
                } else {
                    s.append(InkStroke.ellipse(x, y + bob, 0.075, 0.075, fill: colors[i], width: 0.024, start: st, span: 0.25))
                    s.append(InkStroke.line([(x - 0.03, y + 0.015 + bob), (x, y + 0.035 + bob), (x + 0.03, y + 0.015 + bob)], width: 0.014, start: st + 0.15, span: 0.15))
                }
            }
            return s
        }
    }
}

struct DoodleView: View {
    let doodle: Doodle
    var drawOn: Double = 1.1
    var freezeAt: Double? = nil
    var body: some View {
        InkDrawing(seed: doodle.seed, drawOn: drawOn, freezeAt: freezeAt) { doodle.strokes($0) }
    }
}

// MARK: - Paper

/// Content paper for pen-and-paper art: warm off-white with a still grain and a few fibres
/// (procedural, no image assets). In dark mode it's a dimmer, warmer sheet so ink still reads.
enum Paper {
    static let sheet = Color.adaptive(light: "#FBF8F1", dark: "#DCD4C4")
    static let edge = Color.adaptive(light: "#EFE9DC", dark: "#CFC6B4")
}

struct PaperBackground: View {
    var seed: UInt64 = 5
    var wash: Color? = nil
    var body: some View {
        Canvas { ctx, size in
            ctx.fill(Path(CGRect(origin: .zero, size: size)), with: .color(Paper.sheet))
            var rng = InkRNG(seed)
            if let wash {
                // A watercolour bloom behind the drawing: a few overlapping soft blobs.
                for k in 0..<4 {
                    let w = size.width * (0.55 + CGFloat(rng.unit()) * 0.3)
                    let r = CGRect(x: size.width * (0.1 + CGFloat(rng.unit()) * 0.25), y: size.height * (0.05 + CGFloat(rng.unit()) * 0.2) + CGFloat(k) * 4, width: w, height: w * 0.85)
                    ctx.fill(Path(ellipseIn: r), with: .color(wash.opacity(0.16)))
                }
            }
            let count = Int(size.width * size.height / 260)
            for _ in 0..<count {
                let x = CGFloat(rng.unit()) * size.width, y = CGFloat(rng.unit()) * size.height
                let d = 0.5 + CGFloat(rng.unit()) * 1.1
                ctx.fill(Path(ellipseIn: CGRect(x: x, y: y, width: d, height: d)), with: .color(InkPalette.ink.opacity(0.05 + rng.unit() * 0.05)))
            }
            for _ in 0..<Int(size.width / 18) {
                var p = Path()
                let x = CGFloat(rng.unit()) * size.width, y = CGFloat(rng.unit()) * size.height
                p.move(to: CGPoint(x: x, y: y))
                p.addQuadCurve(to: CGPoint(x: x + 6 + CGFloat(rng.unit()) * 10, y: y + rng.signed() * 4), control: CGPoint(x: x + 4, y: y + rng.signed() * 5))
                ctx.stroke(p, with: .color(InkPalette.ink.opacity(0.05)), lineWidth: 0.5)
            }
        }
        .drawingGroup()
        .accessibilityHidden(true)
    }
}

/// Empty state: Zo, Roda's hand-drawn mascot, in a pose that fits the moment.
struct InkEmptyState: View {
    let pose: MascotPose
    let title: String
    var message: String? = nil
    var size: CGFloat = 130

    var body: some View {
        VStack(spacing: 8) {
            MascotView(pose: pose).frame(width: size, height: size)
            Text(title).font(.headline).foregroundStyle(Palette.textPrimary).multilineTextAlignment(.center)
            if let message {
                Text(message).font(.subheadline).foregroundStyle(Palette.textSecondary).multilineTextAlignment(.center)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 24)
        .padding(.horizontal, 20)
        .accessibilityElement(children: .combine)
    }
}

// MARK: - Unboxing reveal (the group's new donkey arrives in a hand-drawn box)

struct UnboxingReveal: View {
    let name: String
    var freezeAt: Double? = nil
    let onDone: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var start = Date()
    @State private var finished = false

    private static let length = 4.4

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 30, paused: reduceMotion || freezeAt != nil)) { tl in
            let t: Double = freezeAt ?? (reduceMotion ? 3.2 : tl.date.timeIntervalSince(start))
            let frame = reduceMotion ? 0 : Int(t * 10) % 4
            GeometryReader { g in
                let side = min(g.size.width, 420)
                ZStack {
                    InkPalette.paper.opacity(0.97)
                    Canvas { ctx, size in Ink.grain(ctx: ctx, size: size, frame: frame, count: 160) }
                    VStack(spacing: 18) {
                        ZStack {
                            Canvas { ctx, size in Ink.render(back(t), ctx: ctx, size: size, seed: 41, frame: frame, progress: t / 0.9) }
                            PetSprite(bounce: t > 2.4)
                                .frame(width: side * 0.4, height: side * 0.4)
                                .offset(y: side * donkeyY(t))
                                .opacity(t > 1.75 ? 1 : 0)
                                .mask(alignment: .top) { Rectangle().frame(height: side * 0.58) }
                            Canvas { ctx, size in Ink.render(front(t), ctx: ctx, size: size, seed: 42, frame: frame, progress: t / 0.9) }
                        }
                        .frame(width: side, height: side)
                        .rotationEffect(.radians(shake(t)), anchor: UnitPoint(x: 0.5, y: 0.82))

                        VStack(spacing: 6) {
                            Text("Meet \(name)!")
                                .font(.system(.largeTitle, design: .rounded).weight(.heavy))
                                .foregroundStyle(InkPalette.ink)
                                .rotationEffect(.degrees(-3))
                            Text("The group’s new member. Look after him together.")
                                .font(.subheadline)
                                .foregroundStyle(InkPalette.ink.opacity(0.65))
                                .multilineTextAlignment(.center)
                        }
                        .opacity(min(1, max(0, (t - 2.3) / 0.4)))
                        .offset(y: CGFloat(max(0, 1 - (t - 2.3) / 0.4)) * 12)
                    }
                    .padding(.horizontal, 24)
                }
                .frame(width: g.size.width, height: g.size.height)
            }
            .onChange(of: Int(t * 10)) { _, tick in
                if freezeAt == nil && Double(tick) / 10 >= (reduceMotion ? 2.2 : Self.length) { finish() }
            }
        }
        .ignoresSafeArea()
        .contentShape(Rectangle())
        .onTapGesture { finish() }
        .onAppear { start = .now }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
        .accessibilityHint("Closes the reveal")
    }

    private func finish() {
        guard !finished else { return }
        finished = true
        onDone()
    }

    private func donkeyY(_ t: Double) -> CGFloat {
        let x = max(0, t - 1.75)
        let p = 1 - exp(-5.5 * x) * cos(9 * x)
        return CGFloat(0.12 - p * 0.33)
    }

    private func shake(_ t: Double) -> Double {
        guard t > 0.9, t < 1.55 else { return 0 }
        let env = sin((t - 0.9) / 0.65 * .pi)
        return sin(t * 38) * 0.045 * env
    }

    private func flapAngle(_ t: Double) -> Double {
        let p = min(1, max(0, (t - 1.5) / 0.45))
        let e = 1 - pow(1 - p, 3)
        return e * 2.25
    }

    /// Behind the donkey: the box's open mouth and the far flaps.
    private func back(_ t: Double) -> [InkStroke] {
        [InkStroke.shape([(0.24, 0.5), (0.3, 0.44), (0.7, 0.44), (0.76, 0.5)], fill: Color(hex: "#9A6A3A"), width: 0.022, smooth: false).faded(t > 1.5 ? 1 : 0)]
    }

    /// In front of the donkey: the box face, tape, a heart, the flaps and the burst.
    private func front(_ t: Double) -> [InkStroke] {
        let a = flapAngle(t)
        let lh = CGPoint(x: 0.24, y: 0.5), rh = CGPoint(x: 0.76, y: 0.5)
        var s: [InkStroke] = [
            InkStroke.line([(0.1, 0.84), (0.5, 0.83), (0.9, 0.845)], width: 0.012, opacity: 0.4),
            InkStroke.shape([(0.24, 0.5), (0.76, 0.5), (0.76, 0.82), (0.24, 0.82)], fill: InkPalette.cardboard, width: 0.028, smooth: false),
            InkStroke.shape([(0.465, 0.5), (0.535, 0.5), (0.535, 0.66), (0.465, 0.66)], fill: Color(hex: "#F1DDAF"), width: 0.014, start: 0.4, span: 0.3, smooth: false),
            InkStroke.shape([(0.36, 0.7), (0.33, 0.66), (0.36, 0.63), (0.39, 0.66)], fill: InkPalette.blush, width: 0.014, color: InkPalette.tomato, start: 0.6, span: 0.25),
            InkStroke.line([(0.6, 0.73), (0.7, 0.73)], width: 0.014, start: 0.7, span: 0.2, opacity: 0.55),
            InkStroke.line([(0.6, 0.77), (0.67, 0.77)], width: 0.014, start: 0.75, span: 0.2, opacity: 0.55),
            InkStroke.shape([(0.24, 0.5), (0.5, 0.5), (0.5, 0.47), (0.24, 0.47)], fill: Color(hex: "#E8BE86"), width: 0.022, start: 0.3, smooth: false).rotated(-a, around: lh),
            InkStroke.shape([(0.76, 0.5), (0.5, 0.5), (0.5, 0.47), (0.76, 0.47)], fill: Color(hex: "#E8BE86"), width: 0.022, start: 0.35, smooth: false).rotated(a, around: rh),
        ]
        if t > 2.0 {
            let p = (t - 2.0) / 0.5
            let fade = t > 3.4 ? max(0, 1 - (t - 3.4) / 0.6) : 1
            let c = CGPoint(x: 0.5, y: 0.3)
            for i in 0..<9 {
                let ang = Double(i) / 9 * 2 * .pi + 0.2
                let r0: CGFloat = 0.25, r1: CGFloat = 0.25 + 0.07 * CGFloat(min(1, p))
                let col = [InkPalette.butter, InkPalette.tomato, InkPalette.sky][i % 3]
                var ray = InkStroke.line([(c.x + r0 * CGFloat(cos(ang)), c.y + r0 * CGFloat(sin(ang))), (c.x + r1 * CGFloat(cos(ang)), c.y + r1 * CGFloat(sin(ang)))], width: 0.02, color: col, smooth: false, opacity: fade)
                ray.span = 0
                s.append(ray)
            }
        }
        return s
    }
}
