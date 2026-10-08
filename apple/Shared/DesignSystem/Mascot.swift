import SwiftUI

// MARK: - Zoen, Roda's mascot
//
// A round, fuzzy, moss-green furball: an almost spherical body, tiny stubby arms and
// feet, big glossy dark eyes, pink blush and a grumpy-cute attitude (furrowed brows,
// pout). He looks reluctant but is secretly helpful. He's also Zoen's face: the agent's
// avatar and the "agent is here" marker in the chat.
//
// Drawn with the ink renderer, so every line boils at ~12 fps:
// • fur = many short jittered tufts around the edge (plus an inner ring and texture
//   flecks), shimmering slowly;
// • felt-green fill with soft shading, pink blush dabs;
// • a rig, not pictures: each pose sets the same parameters (squash and stretch, lean,
//   gaze, lids, brows, mouth, arms, steps, cowlick lag) and adds props.

enum MascotPose: String, CaseIterable, Identifiable {
    case wave, map, juggle, run, phone, walk, zen, roar, shield, coin, cheer
    var id: String { rawValue }
}

struct MascotRig {
    enum Mouth { case pout, smirk, o, flat, roar, grit }

    var cx: CGFloat = 0.5
    var ground: CGFloat = 0.86
    var lift: CGFloat = 0
    var squash: CGFloat = 0
    var tilt: Double = 0
    var look = CGPoint.zero
    var lid: CGFloat = 0.2          // grumpy by default
    var slant: CGFloat = 0.5        // lid slant: inner corners lower = grumpy
    var brow: CGFloat = 0.8         // furrow
    var wide = false
    var closed = false
    var happy = false               // closed eyes curve up (^^)
    var mouth: Mouth = .pout
    var armL: CGPoint?              // nub targets; nil = resting at the sides
    var armR: CGPoint?
    var stepL = CGPoint.zero
    var stepR = CGPoint.zero
    var cowlick: Double = 0         // lag of the top tuft (secondary motion)
    var headband = false
    var furCount = 64
    var limbs = true
    /// Turn around the vertical axis: 0 = facing the viewer, ±1 = full side profile (right/left),
    /// beyond ±1 the face turns away (over the shoulder), ±2 = back view, 4 = a full spin.
    var yaw: CGFloat = 0
    /// Fur streams back while moving (sign = direction of travel).
    var wind: CGFloat = 0
    var glance: CGFloat = 0         // 0…1 while glancing back at the viewer
    var phi: Double { Double(yaw) * .pi / 2 }
    /// x of a point on the body's surface at angle `theta` around the vertical axis.
    func around(_ theta: Double, _ k: CGFloat) -> CGFloat { cx + rx * k * CGFloat(sin(theta + phi)) }
    /// How much a surface point faces the viewer (1 = straight on, ≤ 0 = hidden).
    func facing(_ theta: Double) -> CGFloat { CGFloat(cos(theta + phi)) }

    var rx: CGFloat { 0.205 * (1 + squash) }
    var ry: CGFloat { 0.19 * (1 - squash) }
    var cy: CGFloat { ground - 0.03 - ry - lift }
    var top: CGFloat { cy - ry }
    var shoulderL: CGPoint { CGPoint(x: around(-.pi / 2, 0.9), y: cy + ry * 0.1) }
    var shoulderR: CGPoint { CGPoint(x: around(.pi / 2, 0.9), y: cy + ry * 0.1) }
    var eyeL: CGPoint { CGPoint(x: around(-0.38, 0.98), y: cy - ry * 0.1) }
    var eyeR: CGPoint { CGPoint(x: around(0.38, 0.98), y: cy - ry * 0.1) }
    var mouthY: CGFloat { cy + ry * 0.26 }
}

enum Mascot {
    // Palette from the references: moss-green felt, soft pink blush, dark glossy eyes.
    static let body = Color(hex: "#6A9A36")
    static let bodyDark = Color(hex: "#4C7524")
    static let belly = Color(hex: "#A9CC6E")
    static let fur = Color(hex: "#3E6120")
    static let line = Color(hex: "#2B4019")
    static let accent = Color(hex: "#E0533E")     // the headband red
    static let blush = Color(hex: "#F49A9A")
    static let iris = Color(hex: "#1B2213")
    static let mouthIn = Color(hex: "#5A1F22")
    static let tongue = Color(hex: "#E8707A")
    static let leaf = Color(hex: "#5E9B3A")

    static func frac(_ x: Double) -> Double { x - x.rounded(.down) }
    static func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: x, y: y) }
    static func polar(_ o: CGPoint, _ r: CGFloat, _ a: Double) -> CGPoint {
        CGPoint(x: o.x + r * CGFloat(cos(a)), y: o.y + r * CGFloat(sin(a)))
    }

    static func curve(_ p: [CGPoint], width: CGFloat = 0.016, color: Color = line, start: Double = 0.5, span: Double = 0.25, opacity: Double = 1) -> InkStroke {
        InkStroke(points: p, width: width, color: color, start: start, span: span, opacity: opacity)
    }
    static func blob(_ p: [CGPoint], fill: Color, width: CGFloat = 0.016, color: Color = line, start: Double = 0.5, span: Double = 0.25, smooth: Bool = true, misregister: CGFloat = 1, opacity: Double = 1) -> InkStroke {
        InkStroke(points: p, closed: true, width: width, color: color, fill: fill, start: start, span: span, smooth: smooth, opacity: opacity, misregister: misregister)
    }
    static func dot(_ c: CGPoint, _ r: CGFloat, fill: Color, start: Double = 0.55, opacity: Double = 1) -> InkStroke {
        var s = InkStroke.ellipse(c.x, c.y, r, r, fill: fill, width: 0.001, color: .clear, start: start, span: 0.15, n: 8, opacity: opacity)
        s.misregister = 0.2
        return s
    }
    static func oval(_ c: CGPoint, _ rx: CGFloat, _ ry: CGFloat, fill: Color?, width: CGFloat = 0.014, color: Color = line, start: Double = 0.5, n: Int = 10, misregister: CGFloat = 0.3, opacity: Double = 1) -> InkStroke {
        var s = InkStroke.ellipse(c.x, c.y, rx, ry, fill: fill, width: width, color: color, start: start, span: 0.2, n: n, opacity: opacity)
        s.misregister = misregister
        return s
    }

    /// Fur: short jittered tufts around an ellipse, leaning down a little (gravity) and
    /// shimmering slowly. Lines also boil through the renderer.
    static func fur(_ c: CGPoint, _ rx: CGFloat, _ ry: CGFloat, count: Int, length: CGFloat, t: Double, seed: UInt64, color: Color = fur, width: CGFloat = 0.011, inset: CGFloat = 0.93, start: Double = 0.1, opacity: Double = 1, lean: CGFloat = 0, gravity: CGFloat = 0.35, curl: CGFloat = 0.5) -> [InkStroke] {
        var rng = InkRNG(seed)
        var out: [InkStroke] = []
        out.reserveCapacity(count)
        for k in 0..<count {
            let a = Double(k) / Double(count) * 2 * .pi + rng.unit() * 0.09
            let nx = CGFloat(cos(a)), ny = CGFloat(sin(a))
            let side: CGFloat = rng.unit() < 0.5 ? -1 : 1
            var dx = nx - ny * lean * side, dy = ny + nx * lean * side + gravity
            let dl = max(0.001, hypot(dx, dy)); dx /= dl; dy /= dl
            let shimmer = 1 + 0.14 * CGFloat(sin(t * 2.6 + Double(k) * 1.7))
            let l = length * (0.55 + 0.9 * CGFloat(rng.unit())) * shimmer
            let base = pt(c.x + rx * inset * nx, c.y + ry * inset * ny)
            let tip = pt(base.x + dx * l, base.y + dy * l)
            let bend = (CGFloat(rng.unit()) - 0.5 + 0.35 * side) * l * curl
            let mid = pt((base.x + tip.x) / 2 - dy * bend, (base.y + tip.y) / 2 + dx * bend)
            out.append(InkStroke(points: [base, mid, tip], width: width * (0.7 + 0.6 * CGFloat(rng.unit())), color: color, start: start + Double(k) / Double(count) * 0.3, span: 0.2, opacity: opacity, ghost: false))
        }
        return out
    }

    /// Fur clumps: pointed, slightly curved tufts that overlap around the silhouette, so the
    /// edge reads as a plush pelt instead of a circle. Tips sway; the upper-left catches light.
    static func clumps(_ c: CGPoint, _ rx: CGFloat, _ ry: CGFloat, count: Int, length: CGFloat, t: Double, seed: UInt64, fill: Color, lit: Color? = nil, ink: Bool, inset: CGFloat = 0.88, start: Double = 0.03, wind: CGFloat = 0) -> [InkStroke] {
        var rng = InkRNG(seed)
        var out: [InkStroke] = []
        let half = Double.pi / Double(count) * 1.45
        func edge(_ a: Double, _ s: CGFloat) -> CGPoint { pt(c.x + rx * s * CGFloat(cos(a)), c.y + ry * s * CGFloat(sin(a))) }
        for k in 0..<count {
            let a = Double(k) / Double(count) * 2 * .pi + rng.unit() * 0.14
            let nx = CGFloat(cos(a)), ny = CGFloat(sin(a))
            let trailing = wind == 0 ? 0 : max(0, -nx * (wind > 0 ? 1 : -1)) * abs(wind)
            let l = length * (0.65 + 0.7 * CGFloat(rng.unit())) * (ny < -0.5 ? 1.15 : 1) * (1 + 0.6 * trailing)
            let lean = (CGFloat(rng.unit()) - 0.5) * 0.9
            var dx = nx - ny * lean - wind * 0.8, dy = ny + nx * lean + 0.32
            let dl = max(0.001, hypot(dx, dy)); dx /= dl; dy /= dl
            let sway = CGFloat(sin(t * 1.9 + Double(k) * 0.9)) * 0.006
            let root = edge(a, inset)
            let tip = pt(root.x + dx * l - dy * sway, root.y + dy * l + dx * sway)
            let bl = edge(a - half, inset), br = edge(a + half, inset)
            let ml = pt((bl.x + tip.x) / 2 + nx * l * 0.14, (bl.y + tip.y) / 2 + ny * l * 0.14)
            let mr = pt((br.x + tip.x) / 2 + nx * l * 0.1, (br.y + tip.y) / 2 + ny * l * 0.1)
            let isLit = lit != nil && ny < -0.15 && nx < 0.45
            out.append(blob([bl, ml, tip, mr, br, root], fill: isLit ? lit! : fill, width: 0.001, color: .clear, start: start + Double(k) / Double(count) * 0.25, span: 0.15, misregister: 0))
            if ink {
                // Ink one flank of the clump, heavier on the shadow side, tapering into the tip.
                let shadow = max(0, ny * 0.6 + nx * 0.4)
                let flank = rng.unit() < 0.5 ? [ml, tip] : [mr, tip]
                let from = pt((flank[0].x * 2 + root.x) / 3, (flank[0].y * 2 + root.y) / 3)
                out.append(InkStroke(points: [from, flank[0], flank[1]], width: 0.0055 + 0.004 * shadow, color: line.opacity(0.55 + 0.35 * Double(shadow)), start: start + 0.1 + Double(k) / Double(count) * 0.25, span: 0.15, opacity: 1, ghost: false))
            }
        }
        return out
    }

    /// A fuzzy little ball (arms, feet): fill plus a few tufts.
    static func nub(_ c: CGPoint, _ rx: CGFloat, _ ry: CGFloat, t: Double, seed: UInt64, start: Double = 0.45) -> [InkStroke] {
        var s = [oval(c, rx, ry, fill: body, width: 0.001, color: .clear, start: start, misregister: 0)]
        s += fur(c, rx, ry, count: 16, length: 0.016, t: t, seed: seed, color: body, width: 0.018, inset: 0.8, start: start, lean: 0.5)
        s.append(oval(pt(c.x - rx * 0.2, c.y - ry * 0.25), rx * 0.45, ry * 0.35, fill: belly.opacity(0.4), width: 0.001, color: .clear, start: start, n: 8, misregister: 0))
        s += fur(c, rx, ry, count: 20, length: 0.012, t: t, seed: seed &+ 5, color: line.opacity(0.75), width: 0.006, inset: 0.98, start: start, lean: 0.8, gravity: 0.2)
        return s
    }

    static func leaf(_ c: CGPoint, size: CGFloat, angle: Double, start: Double = 0.6, opacity: Double = 1) -> [InkStroke] {
        let pts = [pt(-1, 0), pt(-0.4, -0.32), pt(0.45, -0.22), pt(1, 0), pt(0.45, 0.2), pt(-0.4, 0.3)].map { pt(c.x + $0.x * size, c.y + $0.y * size) }
        let rib = curve([pt(c.x - size * 0.9, c.y), pt(c.x + size * 0.85, c.y - size * 0.02)], width: 0.007, color: line, start: start + 0.1, span: 0.1, opacity: 0.6 * opacity)
        return [blob(pts, fill: leaf, width: 0.009, start: start, span: 0.15, misregister: 0.4, opacity: opacity), rib].map { $0.rotated(angle, around: c) }
    }

    static func eye(_ p: CGPoint, _ r: MascotRig, left: Bool) -> [InkStroke] {
        if r.closed {
            let d: CGFloat = r.happy ? -0.022 : 0.02
            return [curve([pt(p.x - 0.04, p.y + (r.happy ? 0.008 : 0)), pt(p.x, p.y + d), pt(p.x + 0.04, p.y + (r.happy ? 0.008 : 0))], width: 0.017, start: 0.4)]
        }
        // Big glossy eyes like the plush: a thin white rim, a grey-green iris ring, a deep
        // pupil, one big highlight and a small bounce light.
        let w: CGFloat = r.wide ? 0.062 : 0.056, h: CGFloat = r.wide ? 0.066 : 0.06
        var out = [oval(p, w, h, fill: Color(hex: "#F4F6EC"), width: 0.008, color: line.opacity(0.8), start: 0.35, n: 12, misregister: 0.2)]
        let ir: CGFloat = r.wide ? 0.04 : 0.047
        let ix = p.x + r.look.x * (w - ir) * 0.9 + (left ? 0.003 : -0.003)
        let iy = p.y + r.look.y * (h - ir) * 0.8 + 0.003
        out.append(oval(pt(ix, iy), ir, ir * 1.05, fill: Color(hex: "#56634A"), width: 0.005, color: line, start: 0.4, misregister: 0.1))
        out.append(oval(pt(ix, iy + 0.002), ir * 0.8, ir * 0.84, fill: iris, width: 0.001, color: .clear, start: 0.42, misregister: 0))
        out.append(oval(pt(ix - ir * 0.15, iy + ir * 0.45), ir * 0.5, ir * 0.22, fill: Color(hex: "#6E7F5C").opacity(0.55), width: 0.001, color: .clear, start: 0.43, n: 8, misregister: 0))
        out.append(dot(pt(ix + ir * 0.32, iy - ir * 0.38), ir * 0.3, fill: .white))
        out.append(dot(pt(ix - ir * 0.05, iy - ir * 0.52), ir * 0.11, fill: .white, opacity: 0.9))
        out.append(dot(pt(ix - ir * 0.42, iy + ir * 0.36), ir * 0.12, fill: .white, opacity: 0.75))
        // Lids stay light: the grump lives in the brows, the eyes stay round and glossy.
        let lid = r.lid >= 0.99 ? 1 : r.lid * 0.45
        if lid > 0.02 {
            // Upper lid in fur green, slanted (inner corner lower) for the grumpy look.
            let s: CGFloat = left ? 1 : -1
            func ly(_ x: CGFloat) -> CGFloat { p.y - h * 1.1 + 2.2 * h * min(1, lid) + r.slant * (lid >= 1 ? 0 : 0.6) * (x - p.x) * s * 0.55 }
            var pts: [CGPoint] = []
            for k in 0...10 {
                let a = Double.pi + Double(k) / 10 * .pi
                let x = p.x + w * 1.15 * CGFloat(cos(a)), y = p.y + h * 1.15 * CGFloat(sin(a))
                pts.append(pt(x, min(y, ly(x))))
            }
            pts.append(pt(p.x + w * 1.15, ly(p.x + w * 1.15))); pts.append(pt(p.x - w * 1.15, ly(p.x - w * 1.15)))
            out.append(blob(pts, fill: body, width: 0.001, color: .clear, start: 0.45, span: 0.15, smooth: false, misregister: 0))
            out.append(curve([pt(p.x - w * 1.02, ly(p.x - w * 1.02)), pt(p.x, ly(p.x) + 0.003), pt(p.x + w * 1.02, ly(p.x + w * 1.02))], width: lid >= 1 ? 0.012 : 0.006, color: lid >= 1 ? line : bodyDark, start: 0.48, span: 0.15))
        }
        if r.brow > 0.02 {
            // Furry brow: a few thick tufts angled down toward the nose.
            let s: CGFloat = left ? 1 : -1
            let by = p.y - h - 0.008 + (r.wide ? -0.016 : 0)
            for k in 0..<5 {
                let x0 = p.x - w * 1.05 + CGFloat(k) * w * 0.44
                let y0 = by + r.brow * 0.026 * (x0 - p.x) / w * s
                let tone = k % 2 == 0 ? bodyDark : fur
                out.append(curve([pt(x0, y0 + 0.004), pt(x0 + w * 0.2, y0 - 0.004), pt(x0 + w * 0.46, y0 + r.brow * 0.012 * s + 0.004)], width: 0.017, color: tone, start: 0.5, span: 0.15, opacity: 0.95))
            }
        }
        return out
    }

    static func mouth(_ r: MascotRig) -> [InkStroke] {
        let x = r.cx, y = r.mouthY
        switch r.mouth {
        case .pout:
            return [curve([pt(x - 0.036, y + 0.014), pt(x - 0.014, y - 0.005), pt(x + 0.014, y - 0.005), pt(x + 0.036, y + 0.014)], width: 0.016, start: 0.55)]
        case .smirk:
            return [curve([pt(x - 0.026, y), pt(x + 0.004, y + 0.012), pt(x + 0.03, y - 0.006)], width: 0.015, start: 0.55)]
        case .o:
            return [oval(pt(x, y + 0.004), 0.014, 0.017, fill: mouthIn, width: 0.011, start: 0.55, n: 9, misregister: 0.1)]
        case .flat:
            return [curve([pt(x - 0.026, y + 0.004), pt(x + 0.026, y + 0.002)], width: 0.015, start: 0.55)]
        case .grit:
            return [blob([pt(x - 0.036, y - 0.006), pt(x + 0.036, y - 0.006), pt(x + 0.032, y + 0.014), pt(x - 0.032, y + 0.014)], fill: .white, width: 0.012, start: 0.55, misregister: 0.1),
                    curve([pt(x - 0.034, y + 0.004), pt(x + 0.034, y + 0.004)], width: 0.008, start: 0.6, span: 0.1),
                    curve([pt(x - 0.008, y - 0.006), pt(x - 0.008, y + 0.014)], width: 0.007, start: 0.6, span: 0.1),
                    curve([pt(x + 0.012, y - 0.006), pt(x + 0.012, y + 0.014)], width: 0.007, start: 0.6, span: 0.1)]
        case .roar:
            let w: CGFloat = 0.07, top = y - 0.035, bot = y + 0.06
            var s: [InkStroke] = [blob([pt(x - w, top + 0.004), pt(x - w * 0.4, top - 0.006), pt(x + w * 0.4, top - 0.006), pt(x + w, top + 0.004), pt(x + w * 0.8, bot - 0.02), pt(x, bot), pt(x - w * 0.8, bot - 0.02)], fill: mouthIn, width: 0.016, start: 0.55, misregister: 0.15)]
            s.append(blob([pt(x - w * 0.6, bot - 0.022), pt(x, bot - 0.04), pt(x + w * 0.6, bot - 0.022), pt(x, bot - 0.004)], fill: tongue, width: 0.001, color: .clear, start: 0.6, misregister: 0))
            for (fx, dir) in [(x - w * 0.62, 1.0), (x + w * 0.62, 1.0), (x - w * 0.5, -1.0), (x + w * 0.5, -1.0)] {
                let base = dir > 0 ? top + 0.002 : bot - 0.012
                let tipY = base + CGFloat(dir) * 0.026
                s.append(blob([pt(fx - 0.011, base), pt(fx + 0.011, base), pt(fx, tipY)], fill: .white, width: 0.008, start: 0.65, span: 0.1, smooth: false, misregister: 0))
            }
            s.append(blob([pt(x - w * 0.42, top), pt(x + w * 0.42, top), pt(x + w * 0.38, top + 0.012), pt(x - w * 0.38, top + 0.012)], fill: .white, width: 0.006, start: 0.62, span: 0.1, smooth: false, misregister: 0))
            return s
        }
    }

    static func hop(_ w: Double, _ height: CGFloat) -> (lift: CGFloat, squash: CGFloat) {
        let h = abs(sin(w))
        let k = min(1.5, height / 0.03)
        return (CGFloat(h) * height, CGFloat(pow(1 - h, 6) * 0.12 - h * 0.05) * k)
    }

    /// Each pose starts facing the viewer and turns into its angle (smooth between steps);
    /// `glanceEvery` makes him look back at the viewer now and then. Returns (yaw, glance 0…1).
    static func turn(_ target: CGFloat, t: Double, glanceEvery: Double? = nil, glanceTo: CGFloat = 0.12) -> (CGFloat, CGFloat) {
        let e = CGFloat(1 - pow(1 - min(1, t / 0.9), 3))
        var y = target * e, g: CGFloat = 0
        if let every = glanceEvery {
            let p = frac(t / every)
            if t > 1.2 && p > 0.74 { g = CGFloat(sin((p - 0.74) / 0.26 * .pi)); y += (glanceTo - y) * g }
        }
        return (y, g)
    }

    static func blinking(_ t: Double) -> Bool {
        let p = frac(t / 3.9)
        return p > 0.955 || (frac(t / 7.8) > 0.5 && p > 0.9 && p < 0.925)
    }

    // MARK: drawing

    static func draw(_ r: MascotRig, t: Double, behind: [InkStroke] = [], held: [InkStroke] = [], world: [InkStroke] = []) -> [InkStroke] {
        var s: [InkStroke] = []
        if r.limbs {
            let shadow = max(0.45, 1 - r.lift * 6)
            s.append(InkStroke.ellipse(r.cx, r.ground + 0.004, 0.17 * shadow, 0.022 * shadow, fill: line.opacity(0.14), width: 0.001, color: .clear, start: 0, span: 0.3))
        }
        s += behind
        var c: [InkStroke] = []
        let center = pt(r.cx, r.cy)
        if r.limbs {
            for (i, step) in [r.stepL, r.stepR].enumerated() {
                let f = pt(r.cx + 0.085 * CGFloat(sin((i == 0 ? -Double.pi / 2 : .pi / 2) + r.phi)) + step.x, r.ground - 0.02 + step.y)
                c += nub(f, 0.052, 0.034, t: t, seed: UInt64(300 + i), start: 0.05)
            }
        }
        // Body: felt fill, soft shading, texture, then the inked fur edge.
        // The far arm (turned away) goes behind the body.
        if r.limbs {
            for (i, target) in [r.armL, r.armR].enumerated() where r.facing(i == 0 ? -.pi / 2 : .pi / 2) < -0.35 {
                let sh = i == 0 ? r.shoulderL : r.shoulderR
                var tgt = target ?? pt(sh.x, sh.y + 0.03)
                let dx = tgt.x - sh.x, dy = tgt.y - sh.y, d = hypot(dx, dy)
                if d > 0.11 { tgt = pt(sh.x + dx / d * 0.11, sh.y + dy / d * 0.11) }
                c += nub(tgt, 0.044, 0.038, t: t, seed: UInt64(400 + i), start: 0.05).map { $0.faded(0.92) }
            }
        }
        // Fluffy silhouette: body-colored tufts poke past the fill so the edge is soft fur,
        // not a line; then shading and texture; then fine curled ink hairs on the rim.
        let n = r.furCount
        c += clumps(center, r.rx, r.ry, count: n * 5 / 8, length: 0.046, t: t + 0.7, seed: 61, fill: bodyDark, ink: false, inset: 0.9, start: 0, wind: r.wind)
        c.append(oval(center, r.rx * 0.95, r.ry * 0.95, fill: body, width: 0.001, color: .clear, start: 0, n: 18, misregister: 0))
        c += clumps(center, r.rx, r.ry, count: n * 3 / 4, length: 0.036, t: t, seed: 62, fill: body, lit: Color(hex: "#7DAD47"), ink: true, inset: 0.86, start: 0.02, wind: r.wind)
        c.append(oval(pt(r.cx + r.rx * 0.12, r.cy + r.ry * 0.2), r.rx * 0.86, r.ry * 0.78, fill: bodyDark.opacity(0.32), width: 0.001, color: .clear, start: 0.1, n: 14, misregister: 0))
        c.append(oval(pt(r.cx - r.rx * 0.22, r.cy - r.ry * 0.42), r.rx * 0.5, r.ry * 0.3, fill: belly.opacity(0.38), width: 0.001, color: .clear, start: 0.15, n: 12, misregister: 0))
        var rng = InkRNG(911)
        for i in 0..<(n * 3 / 2) {
            let a = rng.unit() * 2 * .pi, d = 0.15 + 0.8 * sqrt(rng.unit())
            let p0 = pt(r.cx + r.rx * CGFloat(d * cos(a)), r.cy + r.ry * CGFloat(d * sin(a)))
            let len = 0.012 + 0.012 * CGFloat(rng.unit())
            let ang = a * 0.35 + .pi / 2 + (rng.unit() - 0.5) * 0.9
            let lit = sin(a) < -0.2 && cos(a) < 0.4      // upper-left catches the light
            let shade: Color = lit ? belly : bodyDark
            c.append(InkStroke(points: [p0, polar(p0, len * 0.5, ang + 0.25), polar(p0, len, ang)], width: 0.006, color: shade, start: 0.2, span: 0.2, opacity: lit ? 0.6 : 0.28, ghost: false))
        }
        c += fur(center, r.rx, r.ry, count: n, length: 0.02, t: t, seed: 17, color: line.opacity(0.6), width: 0.006, inset: 1.02, start: 0.05, lean: 0.75, gravity: 0.3, curl: 0.8)
        // Inner clumps: a few tufts inside the body give the pelt its layered depth.
        var irng = InkRNG(733)
        for k in 0..<(n / 3) {
            let a = irng.unit() * 2 * .pi, d = 0.35 + 0.45 * irng.unit()
            let p0 = pt(r.cx + r.rx * CGFloat(d * cos(a)), r.cy + r.ry * CGFloat(d * sin(a)))
            if abs(p0.y - r.eyeL.y) < 0.07 && abs(p0.x - r.cx) < r.rx * 0.7 { continue }   // keep the face clean
            if abs(p0.y - r.mouthY) < 0.05 && abs(p0.x - r.cx) < 0.07 { continue }
            let sw = CGFloat(sin(t * 1.7 + Double(k))) * 0.003
            let tip = pt(p0.x + 0.006 + sw, p0.y + 0.026)
            c.append(curve([pt(p0.x - 0.008, p0.y), pt(p0.x - 0.002, p0.y + 0.014), tip], width: 0.006, color: bodyDark, start: 0.22, span: 0.15, opacity: 0.55))
            c.append(curve([pt(p0.x + 0.008, p0.y - 0.002), pt(p0.x + 0.006, p0.y + 0.012), tip], width: 0.005, color: bodyDark, start: 0.23, span: 0.15, opacity: 0.4))
        }
        c += fur(center, r.rx, r.ry, count: n / 2, length: 0.02, t: t + 1.3, seed: 29, color: bodyDark, width: 0.01, inset: 0.86, start: 0.2, opacity: 0.5, lean: 0.6)
        // Cowlick: three longer tufts on top that lag behind the body.
        for k in 0..<3 {
            let base = pt(r.cx - 0.012 + CGFloat(k) * 0.014 + r.rx * 0.25 * CGFloat(sin(r.phi)), r.top + 0.012)
            let a = -Double.pi / 2 + Double(k - 1) * 0.35 + r.cowlick
            let tip = polar(base, 0.05 + (k == 1 ? 0.012 : 0), a)
            let mid = polar(base, 0.028, a - r.cowlick * 0.5 - Double(k - 1) * 0.1)
            c.append(InkStroke(points: [base, mid, tip], width: 0.016, color: body, start: 0.3, span: 0.2, ghost: false))
            c.append(InkStroke(points: [base, mid, tip], width: 0.008, color: line, start: 0.32, span: 0.2, ghost: false))
        }
        if r.headband {
            let y0 = r.top + r.ry * 0.36
            let pts = [pt(r.cx - r.rx * 0.98, y0 + 0.022), pt(r.cx - r.rx * 0.5, y0 - 0.004), pt(r.cx, y0 - 0.012), pt(r.cx + r.rx * 0.5, y0 - 0.004), pt(r.cx + r.rx * 0.98, y0 + 0.022)]
            c.append(InkStroke(points: pts.map { pt($0.x, $0.y - 0.016) }, width: 0.022, color: accent, start: 0.35, span: 0.2, ghost: false))
            c.append(InkStroke(points: pts, width: 0.018, color: .white, start: 0.38, span: 0.2, ghost: false))
            c.append(InkStroke(points: pts.map { pt($0.x, $0.y + 0.016) }, width: 0.022, color: accent, start: 0.4, span: 0.2, ghost: false))
            let ks: CGFloat = r.yaw < -0.15 ? -1 : 1
            let knot = pt(r.cx - ks * r.rx * 0.96, y0 + 0.02)
            for k in 0..<2 {
                let flap = CGFloat(sin(t * 16 + Double(k) * 1.4)) * 0.018
                c.append(curve([knot, pt(knot.x - ks * 0.04, knot.y + 0.01 + flap), pt(knot.x - ks * (0.075 + 0.02 * abs(r.wind)), knot.y + 0.004 + CGFloat(k) * 0.02 - flap)], width: 0.016, color: accent, start: 0.5, span: 0.15))
            }
        }
        // The face wraps around the ball: features slide toward the side he faces, foreshorten,
        // and hide past the silhouette (one eye in profile, none from behind).
        func squeeze(_ strokes: [InkStroke], _ x0: CGFloat, _ k: CGFloat, dx: CGFloat = 0) -> [InkStroke] {
            strokes.map { s in var s = s; s.points = s.points.map { pt(x0 + dx + ($0.x - x0) * k, $0.y) }; return s }
        }
        for (side, e) in [(-1.0, r.eyeL), (1.0, r.eyeR)] {
            let f = r.facing(side * 0.38) / CGFloat(cos(0.38))
            if f > 0.2 { c += squeeze(eye(e, r, left: side < 0), e.x, min(1, f)) }
        }
        for (side, e) in [(-1.0, r.eyeL), (1.0, r.eyeR)] {
            let f = r.facing(side * 0.78)
            guard f > 0.22 else { continue }
            let k = min(1, f / CGFloat(cos(0.78)))
            let bx = r.around(side * 0.78, 0.86), by = e.y + 0.075
            for j in 0..<3 {
                c.append(oval(pt(bx + CGFloat(j - 1) * 0.013 * k, by + CGFloat(j % 2) * 0.006), 0.026 * k, 0.017, fill: blush.opacity(0.4), width: 0.001, color: .clear, start: 0.5, n: 8, misregister: 0))
            }
        }
        let mf = r.facing(0)
        if mf > 0.25 { c += squeeze(mouth(r), r.cx, mf, dx: r.around(0, 0.95) - r.cx) }
        c += held
        if r.limbs {
            for (i, target) in [r.armL, r.armR].enumerated() {
                let depth = r.facing(i == 0 ? -.pi / 2 : .pi / 2)
                if depth < -0.35 { continue }      // drawn behind the body (see below)
                let sh = i == 0 ? r.shoulderL : r.shoulderR
                let rest = pt(sh.x + (i == 0 ? -0.03 : 0.03), sh.y + 0.03)
                var tgt = target ?? rest
                let dx = tgt.x - sh.x, dy = tgt.y - sh.y
                let d = hypot(dx, dy)
                let reach: CGFloat = 0.11
                if d > reach { tgt = pt(sh.x + dx / d * reach, sh.y + dy / d * reach) }
                c += nub(tgt, 0.044, 0.038, t: t, seed: UInt64(400 + i), start: 0.5)
            }
        }
        let pivot = pt(r.cx, r.ground)
        s += r.tilt == 0 ? c : c.map { $0.rotated(r.tilt, around: pivot) }
        s += world
        return s
    }

    // MARK: poses

    static func strokes(_ pose: MascotPose, t: Double) -> [InkStroke] {
        var r = MascotRig()
        var behind: [InkStroke] = [], held: [InkStroke] = [], world: [InkStroke] = []
        r.squash = CGFloat(sin(t * 2.0)) * 0.022
        r.cowlick = 0.12 * sin(t * 2.0 - 1.0)

        switch pose {
        case .shield, .coin, .cheer:
            extraPose(pose, &r, t: t, held: &held, world: &world)

        case .wave:
            (r.yaw, r.glance) = turn(-0.18, t: t)
            // A reluctant wave. Every few seconds the scowl slips into a smirk.
            let b = hop(t * 2.2, 0.008)
            r.lift = b.lift; r.squash += b.squash
            let soft = frac(t / 4.6) > 0.72
            r.brow = soft ? 0.25 : 0.8
            r.lid = soft ? 0.18 : 0.32
            r.mouth = soft ? .smirk : .pout
            r.look = pt(0.1, 0.05)
            r.tilt = -0.04
            let wv = sin(t * 6.5)
            r.armR = polar(r.shoulderR, 0.11, -1.0 + 0.4 * wv)
            r.cowlick = 0.18 * sin(t * 6.5 - 1.3)
            let hand = r.armR!
            for k in 0..<2 {
                let rad: CGFloat = 0.065 + CGFloat(k) * 0.02
                held.append(curve([polar(hand, rad, -1.6), polar(hand, rad + 0.005, -1.1), polar(hand, rad, -0.6)], width: 0.01, start: 0.7, span: 0.15, opacity: 0.3 + 0.5 * abs(wv)))
            }

        case .map:
            (r.yaw, r.glance) = turn(0.5, t: t, glanceEvery: 4.2)
            r.cx = 0.42
            r.lid = 0.5; r.slant = 0.3; r.brow = 0.9
            r.look = pt(0.85, 0.55 + 0.15 * CGFloat(sin(t * 0.8)))
            r.tilt = 0.05
            r.mouth = frac(t / 3.6) > 0.8 ? .o : .flat
            let wob = 0.05 * sin(t * 1.4)
            let o = pt(0.76, 0.62)
            var m: [InkStroke] = [
                blob([pt(0.6, 0.53), pt(0.7, 0.5), pt(0.8, 0.54), pt(0.92, 0.5), pt(0.93, 0.74), pt(0.81, 0.78), pt(0.71, 0.74), pt(0.61, 0.77)], fill: InkPalette.paper, width: 0.016, start: 0.35, span: 0.3, smooth: false),
                blob([pt(0.62, 0.55), pt(0.7, 0.52), pt(0.71, 0.62), pt(0.63, 0.65)], fill: belly.opacity(0.6), width: 0.001, color: .clear, start: 0.55, misregister: 0.6),
                curve([pt(0.7, 0.5), pt(0.71, 0.74)], width: 0.01, start: 0.5, opacity: 0.5),
                curve([pt(0.8, 0.54), pt(0.81, 0.78)], width: 0.01, start: 0.52, opacity: 0.5),
            ]
            let route = [pt(0.65, 0.71), pt(0.69, 0.66), pt(0.73, 0.68), pt(0.77, 0.63), pt(0.81, 0.65), pt(0.85, 0.6), pt(0.88, 0.58)]
            let shown = 2 + Int(frac(t / 2.6) * 6)
            for i in 0..<min(shown, route.count - 1) {
                let p = route[i]
                m.append(curve([p, pt(p.x + 0.013, p.y - 0.005)], width: 0.01, color: accent, start: 0.6, span: 0.1))
            }
            let x = route[route.count - 1]
            m.append(curve([pt(x.x - 0.015, x.y - 0.015), pt(x.x + 0.015, x.y + 0.015)], width: 0.014, color: accent, start: 0.65, span: 0.1))
            m.append(curve([pt(x.x + 0.015, x.y - 0.015), pt(x.x - 0.015, x.y + 0.015)], width: 0.014, color: accent, start: 0.68, span: 0.1))
            // The map unfolds when the pose appears (panels open from the paw outward).
            let u = CGFloat(1 - pow(1 - min(1, t / 1.1), 3))
            let unfold: (InkStroke) -> InkStroke = { s in
                var s = s
                s.points = s.points.map { pt(0.6 + ($0.x - 0.6) * (0.12 + 0.88 * u), $0.y + (1 - u) * ($0.y - 0.64) * -0.2) }
                return s
            }
            held += m.map { unfold($0).rotated(wob, around: o) }
            r.armR = pt(0.62, 0.64)
            if u < 1 { r.mouth = .o; r.look = pt(0.9, 0.3) }

        case .juggle:
            (r.yaw, r.glance) = turn(-0.25, t: t, glanceEvery: 5)
            let b = hop(t * 4.2, 0.016)
            r.lift = b.lift; r.squash += b.squash
            r.brow = 0.55; r.lid = 0.15
            r.mouth = frac(t / 3) < 0.5 ? .o : .flat
            r.tilt = 0.03 * sin(t * 2.8)
            let colors = [InkPalette.butter, InkPalette.sky, InkPalette.blush]
            var topCard = pt(0.5, 1)
            for i in 0..<3 {
                let a = t * 2.8 + Double(i) * 2.094
                let p = pt(0.5 + 0.2 * CGFloat(cos(a)), 0.27 + 0.1 * CGFloat(sin(a)))
                if p.y < topCard.y { topCard = p }
                var card: [InkStroke] = [InkStroke.box(p.x - 0.04, p.y - 0.052, 0.08, 0.104, r: 0.22, fill: colors[i], width: 0.014, color: line, start: 0.55, span: 0.2)]
                card.append(oval(p, 0.014, 0.014, fill: Mascot.body, width: 0.008, start: 0.65, n: 8))
                world += card.map { $0.rotated(a * 0.5, around: p) }
            }
            r.look = pt(max(-1, min(1, (topCard.x - 0.5) * 4)), -1)
            // Paws toss upward in turn, following the cards.
            r.armL = pt(r.cx - 0.2, r.cy - 0.16 + 0.05 * CGFloat(sin(t * 5.6)))
            r.armR = pt(r.cx + 0.2, r.cy - 0.16 - 0.05 * CGFloat(sin(t * 5.6)))
            r.cowlick = 0.25 * sin(t * 5.6)

        case .run:
            (r.yaw, r.glance) = turn(1, t: t)   // side profile, fur streaming back
            r.wind = 1
            let w = t * 10
            let h = abs(sin(w))
            r.lift = CGFloat(h) * 0.035
            r.squash += CGFloat(pow(1 - h, 6) * 0.13 - h * 0.06)
            r.tilt = 0.14
            r.headband = true
            r.brow = 1; r.lid = 0.32; r.slant = 0.7
            r.look = pt(0.9, 0)
            r.mouth = .flat
            r.stepL = pt(CGFloat(sin(w)) * 0.06, -CGFloat(max(0, cos(w))) * 0.035)
            r.stepR = pt(-CGFloat(sin(w)) * 0.06, -CGFloat(max(0, -cos(w))) * 0.035)
            // Arms pump up and down at the sides, opposite to the feet (never across the face).
            r.armL = pt(r.shoulderL.x - 0.035 - CGFloat(cos(w)) * 0.012, r.shoulderL.y + 0.01 + CGFloat(sin(w)) * 0.045)
            r.armR = pt(r.shoulderR.x + 0.035 + CGFloat(cos(w)) * 0.012, r.shoulderR.y + 0.01 - CGFloat(sin(w)) * 0.045)
            r.cowlick = -0.6 + 0.15 * sin(w * 2)
            for k in 0..<3 {
                let ph = frac(t * 1.6 + Double(k) / 3)
                let c = pt(0.3 - CGFloat(ph) * 0.2, 0.84 - CGFloat(ph) * 0.05)
                let rr = 0.02 + CGFloat(ph) * 0.04
                world.append(InkStroke.ellipse(c.x, c.y, rr, rr * 0.75, closed: false, width: 0.01, color: Color(hex: "#B49A78"), start: 0, span: 0, n: 9, opacity: (1 - ph) * 0.8))
            }
            for k in 0..<3 {
                let x = CGFloat(frac(Double(k) / 3 - t * 1.2))
                world.append(InkStroke.line([(x, 0.885), (x + 0.12, 0.885)], width: 0.01, color: line, start: 0, span: 0, smooth: false, opacity: 0.35))
            }
            let sp = frac(t * 1.3)
            let d = pt(0.33 - CGFloat(sp) * 0.12, 0.4 - CGFloat(sin(sp * .pi)) * 0.06)
            world.append(blob([pt(d.x, d.y - 0.02), pt(d.x + 0.012, d.y + 0.004), pt(d.x, d.y + 0.014), pt(d.x - 0.012, d.y + 0.004)], fill: InkPalette.sky, width: 0.008, start: 0.7, span: 0.1, opacity: 1 - sp))

        case .phone:
            (r.yaw, r.glance) = turn(0.55, t: t, glanceEvery: 3.6)
            r.cx = 0.4
            let cyc = frac(t / 2.4)
            let ringing = cyc < 0.5
            let pb = pt(0.81, 0.86)
            let shake = ringing ? 0.12 * sin(t * 70) : 0
            var ph: [InkStroke] = [
                InkStroke.box(0.75, 0.62, 0.12, 0.24, r: 0.24, fill: line, width: 0.014, color: line, start: 0.4),
                InkStroke.box(0.762, 0.642, 0.096, 0.19, r: 0.18, fill: InkPalette.sky, width: 0.007, color: line, start: 0.5),
                InkStroke.box(0.77, 0.67, 0.08, 0.035, r: 0.4, fill: .white, width: 0.006, color: line, start: 0.6),
            ]
            ph.append(dot(pt(0.783, 0.687), 0.009, fill: accent, start: 0.65))
            behind += ph.map { $0.rotated(shake, around: pb) }
            if ringing {
                for k in 0..<2 {
                    let rad: CGFloat = 0.1 + CGFloat(k) * 0.034
                    let o = pt(0.81, 0.72)
                    let op = 0.5 + 0.5 * sin(t * 20 + Double(k))
                    world.append(curve([polar(o, rad, -0.5), polar(o, rad + 0.005, 0), polar(o, rad, 0.5)], width: 0.012, color: accent, start: 0.6, span: 0.1, opacity: op))
                    world.append(curve([polar(o, rad, .pi - 0.5), polar(o, rad + 0.005, .pi), polar(o, rad, .pi + 0.5)], width: 0.012, color: accent, start: 0.6, span: 0.1, opacity: op))
                }
                r.brow = 1; r.lid = 0.35; r.slant = 0.8
                r.mouth = .grit
                r.tilt = -0.07
                r.look = pt(1, 0.5)
                r.armL = pt(r.shoulderL.x - 0.02, r.cy - 0.1)
                r.armR = pt(r.shoulderR.x + 0.03, r.cy - 0.09)
                r.squash += CGFloat(sin(t * 50)) * 0.012
                r.cowlick = 0.3 * sin(t * 40)
                // Anger mark near the head.
                let m = pt(r.cx + r.rx * 0.9, r.top + 0.03)
                for k in 0..<4 {
                    let a = Double(k) * .pi / 2 + .pi / 4
                    world.append(curve([polar(m, 0.012, a - 0.5), polar(m, 0.026, a), polar(m, 0.012, a + 0.5)], width: 0.01, color: accent, start: 0.6, span: 0.1))
                }
            } else {
                r.look = pt(1, 0.6)
                r.tilt = 0.06
                r.lid = 0.45; r.slant = 0.6
                r.mouth = .pout
                r.armR = polar(r.shoulderR, 0.1, 0.2)
            }

        case .walk:
            (r.yaw, r.glance) = turn(1, t: t, glanceEvery: 5.2, glanceTo: 0.3)   // profile; looks back over the shoulder
            r.wind = 0.35
            r.cx = 0.42
            let w = t * 6.2
            let h = abs(sin(w))
            r.lift = CGFloat(h) * 0.016
            r.squash += CGFloat(pow(1 - h, 6) * 0.1 - h * 0.03)
            r.stepL = pt(CGFloat(sin(w)) * 0.04, -CGFloat(max(0, cos(w))) * 0.022)
            r.stepR = pt(-CGFloat(sin(w)) * 0.04, -CGFloat(max(0, -cos(w))) * 0.022)
            r.armL = pt(r.shoulderL.x - 0.03 - CGFloat(sin(w)) * 0.03, r.shoulderL.y + 0.035)
            r.armR = pt(r.shoulderR.x + 0.03 + CGFloat(sin(w)) * 0.03, r.shoulderR.y + 0.035)
            r.tilt = 0.06
            r.look = pt(1, -0.1)
            r.brow = 0.6; r.lid = 0.25
            r.mouth = .flat
            r.cowlick = -0.4 + 0.15 * sin(w * 2)
            for k in 0..<5 {
                let x = CGFloat(frac(Double(k) / 5 - t * 0.45))
                let edge = min(1, min(x, 1 - x) * 6)
                world.append(InkStroke.line([(x * 0.95, 0.885), (x * 0.95 + 0.07, 0.886)], width: 0.01, start: 0, span: 0, smooth: false, opacity: 0.45 * Double(edge)))
            }
            let bob = CGFloat(sin(t * 3)) * 0.015
            let pc = pt(0.84, 0.3 + bob)
            world.append(blob([pt(pc.x, pc.y + 0.11), pt(pc.x - 0.05, pc.y + 0.03), pt(pc.x - 0.05, pc.y - 0.02), pt(pc.x, pc.y - 0.06), pt(pc.x + 0.05, pc.y - 0.02), pt(pc.x + 0.05, pc.y + 0.03)], fill: accent, width: 0.016, start: 0.55, span: 0.25))
            world.append(oval(pt(pc.x, pc.y - 0.005), 0.02, 0.02, fill: InkPalette.paper, width: 0.01, start: 0.7, n: 8))
            let pulse = frac(t / 1.6)
            for k in 0..<2 {
                let f = frac(pulse + Double(k) / 2)
                let rr = 0.075 + CGFloat(f) * 0.05
                world.append(InkStroke.ellipse(pc.x, pc.y - 0.005, rr, rr, closed: false, width: 0.008, color: accent, start: 0.7, span: 0.1, n: 12, opacity: (1 - f) * 0.5))
            }

        case .zen:
            // Pouting in a sand garden while leaves drift past.
            r.lid = 0.42; r.slant = 0.45; r.brow = 0.75
            r.mouth = .pout
            r.look = pt(-0.2 + 0.4 * CGFloat(sin(t * 0.5)), 0.25)
            r.squash = CGFloat(sin(t * 1.4)) * 0.03
            for k in 0..<3 {
                let rx = 0.27 + CGFloat(k) * 0.08, ry = 0.045 + CGFloat(k) * 0.016
                behind.append(InkStroke.ellipse(0.5, 0.875, rx, ry, closed: false, width: 0.008, color: line, start: 0, span: 0.4, n: 16, opacity: 0.28))
            }
            behind.append(oval(pt(0.16, 0.86), 0.05, 0.026, fill: Color(hex: "#9C9A92"), width: 0.01, start: 0.2, n: 10))
            behind.append(oval(pt(0.86, 0.9), 0.035, 0.018, fill: Color(hex: "#7F7D76"), width: 0.009, start: 0.25, n: 9))
            for i in 0..<4 {
                let ph = frac(t * 0.11 + Double(i) / 4)
                let x = 1.08 - CGFloat(ph) * 1.2
                let y = 0.12 + CGFloat(i) * 0.16 + CGFloat(sin(t * 1.3 + Double(i) * 2)) * 0.03
                let fade = min(1, min(ph, 1 - ph) * 6)
                world += leaf(pt(x, y), size: 0.034 + CGFloat(i % 2) * 0.01, angle: sin(t * 1.7 + Double(i)) * 0.7 + 0.3, start: 0.6, opacity: fade)
            }

        case .roar:
            let burst = frac(t / 2.6)
            let roaring = burst < 0.6
            r.brow = 1; r.lid = 0.28; r.slant = 0.9
            r.mouth = roaring ? .roar : .grit
            r.wide = roaring
            r.look = pt(0, 0.1)
            r.armL = pt(r.shoulderL.x - 0.06, r.cy - 0.04)
            r.armR = pt(r.shoulderR.x + 0.06, r.cy - 0.04)
            if roaring {
                r.cx += CGFloat(sin(t * 55)) * 0.004
                r.squash = -0.06 + CGFloat(sin(t * 30)) * 0.01
                r.cowlick = 0.35 * sin(t * 45)
                for k in 0..<3 {
                    let a = -0.5 + Double(k) * 0.5
                    for side in [-1.0, 1.0] {
                        let o = pt(0.5, r.mouthY)
                        let ang = side > 0 ? a : .pi - a
                        world.append(curve([polar(o, 0.29, ang), polar(o, 0.35 + CGFloat(burst) * 0.04, ang)], width: 0.012, color: line, start: 0.6, span: 0.1, opacity: 0.6))
                    }
                }
                for i in 0..<3 {
                    let ph = frac(t * 0.9 + Double(i) / 3)
                    world += leaf(pt(0.5 + (i % 2 == 0 ? 1 : -1) * (0.3 + CGFloat(ph) * 0.3), 0.3 + CGFloat(i) * 0.15), size: 0.03, angle: t * 6 + Double(i), opacity: 1 - ph)
                }
            }
        }

        if r.glance > 0 {
            // Glancing back: eyes come to the viewer.
            r.look = pt(r.look.x + (-r.yaw * 0.7 - r.look.x) * r.glance, r.look.y * (1 - r.glance))
        }
        if !r.closed && blinking(t) && pose != .roar { r.lid = 1 }
        return draw(r, t: t, behind: behind, held: held, world: world)
    }

    /// The extra poses (privacy, budget, done) share the rig.
    static func extraPose(_ pose: MascotPose, _ r: inout MascotRig, t: Double, held: inout [InkStroke], world: inout [InkStroke]) {
        switch pose {
        case .shield:
            (r.yaw, r.glance) = turn(-0.3, t: t, glanceEvery: 4)
            // Hugs a shield with a lock: suspicious side-eye, a slow protective sway.
            r.tilt = 0.03 * sin(t * 1.3)
            r.brow = 0.9; r.lid = 0.3; r.slant = 0.6
            r.look = pt(-0.8 + 1.6 * CGFloat(frac(t / 5) > 0.5 ? 1 : 0), 0.1)
            r.mouth = .flat
            let c = pt(r.cx, r.cy + r.ry * 0.78 + CGFloat(sin(t * 2.1)) * 0.004)
            let w: CGFloat = 0.115, h: CGFloat = 0.14
            let shield = [pt(c.x - w, c.y - h * 0.55), pt(c.x, c.y - h * 0.7), pt(c.x + w, c.y - h * 0.55), pt(c.x + w * 0.9, c.y + h * 0.1), pt(c.x, c.y + h * 0.62), pt(c.x - w * 0.9, c.y + h * 0.1)]
            held.append(blob(shield, fill: InkPalette.sky, width: 0.018, start: 0.5, span: 0.25))
            held.append(blob(shield.map { pt(c.x + ($0.x - c.x) * 0.72, c.y + ($0.y - c.y) * 0.72) }, fill: Color(hex: "#DCEBF5"), width: 0.008, color: line.opacity(0.4), start: 0.55, span: 0.2, misregister: 0.3))
            // Lock: shackle + body + keyhole.
            held.append(curve([pt(c.x - 0.03, c.y - 0.005), pt(c.x - 0.03, c.y - 0.05), pt(c.x, c.y - 0.07), pt(c.x + 0.03, c.y - 0.05), pt(c.x + 0.03, c.y - 0.005)], width: 0.014, start: 0.62, span: 0.15))
            held.append(InkStroke.box(c.x - 0.045, c.y - 0.01, 0.09, 0.07, r: 0.2, fill: InkPalette.butter, width: 0.014, color: line, start: 0.66, span: 0.15))
            held.append(dot(pt(c.x, c.y + 0.018), 0.009, fill: line, start: 0.72))
            held.append(curve([pt(c.x, c.y + 0.02), pt(c.x, c.y + 0.04)], width: 0.008, start: 0.73, span: 0.1))
            // Glint sweeping across the shield.
            let g = frac(t / 2.8)
            if g < 0.3 {
                let gx = c.x - w + CGFloat(g / 0.3) * w * 2
                held.append(curve([pt(gx - 0.02, c.y + 0.05), pt(gx + 0.02, c.y - 0.07)], width: 0.012, color: .white, start: 0.8, span: 0.05, opacity: 0.7))
            }
            r.armL = pt(c.x - w * 0.95, c.y - 0.01)
            r.armR = pt(c.x + w * 0.95, c.y - 0.01)

        case .coin:
            (r.yaw, r.glance) = turn(0.35, t: t, glanceEvery: 4.5)
            // Holds up a big coin, squinting at it like it owes him money. It flips now and then.
            let b = hop(t * 1.6, 0.006)
            r.lift = b.lift; r.squash += b.squash
            r.brow = 1; r.lid = 0.4; r.slant = 0.7
            r.look = pt(0.6, -0.9)
            r.mouth = frac(t / 4) > 0.75 ? .smirk : .flat
            let c = pt(r.cx + 0.14, r.top - 0.03 + CGFloat(sin(t * 2.2)) * 0.008)
            let flipPhase = frac(t / 3.2)
            let sx = flipPhase < 0.18 ? CGFloat(abs(cos(flipPhase / 0.18 * .pi * 2))) : 1
            let R: CGFloat = 0.075
            var coin: [InkStroke] = [
                oval(c, R * max(0.08, sx), R, fill: Color(hex: "#F2C94C"), width: 0.016, start: 0.5, n: 14, misregister: 0.5),
                oval(c, R * 0.72 * max(0.08, sx), R * 0.72, fill: nil, width: 0.008, color: Color(hex: "#B8860B"), start: 0.58, n: 12),
            ]
            if sx > 0.5 {
                coin.append(curve([pt(c.x - 0.012 * sx, c.y - 0.03), pt(c.x - 0.012 * sx, c.y + 0.03)], width: 0.01, color: Color(hex: "#8A6508"), start: 0.62, span: 0.1))
                coin.append(curve([pt(c.x + 0.02 * sx, c.y - 0.02), pt(c.x - 0.022 * sx, c.y - 0.012), pt(c.x - 0.02 * sx, c.y + 0.004), pt(c.x + 0.022 * sx, c.y + 0.008), pt(c.x + 0.02 * sx, c.y + 0.022), pt(c.x - 0.024 * sx, c.y + 0.024)], width: 0.01, color: Color(hex: "#8A6508"), start: 0.64, span: 0.1))
            }
            held += coin
            // Sparkle.
            let sp = frac(t / 1.6)
            let so = pt(c.x + R * 0.8, c.y - R * 0.8)
            let ss = CGFloat(sin(sp * .pi)) * 0.025
            world.append(curve([pt(so.x - ss, so.y), pt(so.x + ss, so.y)], width: 0.008, color: Color(hex: "#E0A800"), start: 0, span: 0))
            world.append(curve([pt(so.x, so.y - ss), pt(so.x, so.y + ss)], width: 0.008, color: Color(hex: "#E0A800"), start: 0, span: 0))
            r.armR = pt(c.x - 0.02, c.y + R * 0.9)
            r.armL = pt(r.shoulderL.x - 0.03, r.shoulderL.y + 0.04)

        case .cheer:
            r.yaw = 4 * CGFloat(1 - pow(1 - min(1, t / 1.3), 3))   // a full happy spin, then faces you
            // Done: jumps with both paws up, eyes squeezed happy, confetti raining. Still a bit
            // reluctant: the smile is a smirk.
            let b = hop(t * 4.4, 0.05)
            r.lift = b.lift; r.squash += b.squash
            r.closed = frac(t / 2.2) < 0.6
            r.happy = true
            r.brow = 0.15
            r.mouth = r.closed ? .o : .smirk
            r.armL = pt(r.cx - 0.2, r.cy - 0.17 - CGFloat(sin(t * 8.8)) * 0.02)
            r.armR = pt(r.cx + 0.2, r.cy - 0.17 + CGFloat(sin(t * 8.8)) * 0.02)
            r.cowlick = 0.4 * sin(t * 8.8)
            let colors = [InkPalette.tomato, InkPalette.butter, InkPalette.sky, InkPalette.lilac, Mascot.belly, InkPalette.blush]
            var rng = InkRNG(5150)
            for i in 0..<18 {
                let x0 = 0.08 + CGFloat(rng.unit()) * 0.84
                let speed = 0.25 + rng.unit() * 0.2
                let ph = frac(t * speed + rng.unit())
                let y = 0.02 + CGFloat(ph) * 0.8
                let x = x0 + CGFloat(sin(t * 2 + Double(i))) * 0.03
                let a = t * (2 + rng.unit() * 3) + Double(i)
                let p = pt(x, y)
                let piece = InkStroke.box(p.x - 0.012, p.y - 0.006, 0.024, 0.012, r: 0.2, fill: colors[i % colors.count], width: 0.005, color: line.opacity(0.6), start: 0, span: 0, opacity: 1 - ph * 0.6)
                world.append(piece.rotated(a, around: p))
            }

        default: break
        }
    }


    /// Head only (for avatars): the furball with his face, filling the frame.
    static func head(t: Double, mood: MascotRig.Mouth = .pout) -> [InkStroke] {
        var r = MascotRig()
        r.limbs = false
        r.furCount = 40
        r.mouth = mood
        r.squash = CGFloat(sin(t * 2)) * 0.02
        r.cowlick = 0.12 * sin(t * 2 - 1)
        if blinking(t) { r.lid = 1 }
        let strokes = draw(r, t: t)
        let k: CGFloat = 1.85
        let c = pt(r.cx, r.cy)
        return strokes.map { s in
            var s = s
            s.points = s.points.map { pt(0.5 + ($0.x - c.x) * k, 0.54 + ($0.y - c.y) * k) }
            s.width *= k * 0.8
            return s
        }
    }
}

extension Mascot {
    /// Zo at work (same framing as `head`, so avatars keep their size): headphones, a focused
    /// look down at a laptop whose screen flickers while the keys tap.
    static func workingHead(t: Double) -> [InkStroke] {
        var r = MascotRig()
        r.limbs = false
        r.furCount = 40
        r.mouth = .flat
        r.lid = 0.45; r.slant = 0.3; r.brow = 0.5
        r.look = pt(0, 0.7)
        r.squash = CGFloat(sin(t * 6)) * 0.008
        r.cowlick = 0.18 * sin(t * 3)
        if blinking(t) { r.lid = 1 }
        let k: CGFloat = 1.85
        let c = pt(r.cx, r.cy)
        var out = draw(r, t: t).map { s -> InkStroke in
            var s = s
            s.points = s.points.map { pt(0.5 + ($0.x - c.x) * k, 0.5 + ($0.y - c.y) * k) }
            s.width *= k * 0.8
            return s
        }
        let nod = CGFloat(sin(t * 4.4)) * 0.01
        // Headphones: band over the top, two cups.
        out.append(curve([pt(0.13, 0.5), pt(0.18, 0.2 + nod), pt(0.5, 0.06 + nod), pt(0.82, 0.2 + nod), pt(0.87, 0.5)], width: 0.05, color: line, start: 0, span: 0.3))
        out.append(InkStroke.box(0.05, 0.4 + nod, 0.14, 0.24, r: 0.4, fill: InkPalette.tomato, width: 0.03, color: line, start: 0.1, span: 0.2))
        out.append(InkStroke.box(0.81, 0.4 + nod, 0.14, 0.24, r: 0.4, fill: InkPalette.tomato, width: 0.03, color: line, start: 0.1, span: 0.2))
        // Laptop at the bottom edge, screen facing Zo (we see its back), keys tapping.
        let flick = 0.6 + 0.4 * abs(sin(t * 9))
        out.append(InkStroke.shape([(0.2, 0.98), (0.8, 0.98), (0.74, 0.76), (0.26, 0.76)], fill: Color(hex: "#C9CED6"), width: 0.03, color: line, start: 0.2, span: 0.2, smooth: false))
        out.append(dot(pt(0.5, 0.87), 0.03, fill: .white, start: 0.3, opacity: flick))
        for i in 0..<2 where sin(t * 12 + Double(i) * 2) > 0.3 {
            let x: CGFloat = i == 0 ? 0.16 : 0.84
            out.append(curve([pt(x, 0.74), pt(x + (i == 0 ? -0.04 : 0.04), 0.68)], width: 0.02, color: line, start: 0.4, span: 0.1, opacity: 0.7))
        }
        return out
    }
}

struct MascotView: View {
    let pose: MascotPose
    var drawOn: Double = 0.9
    var freezeAt: Double? = nil

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown = false

    var body: some View {
        let settled = shown || reduceMotion || freezeAt != nil
        InkDrawing(seed: 77, drawOn: drawOn, fps: 12, breathe: 0, jitter: 0.6, freezeAt: freezeAt) { Mascot.strokes(pose, t: $0) }
            .id(pose)
            // Springs in (and again on every pose change) while the lines draw themselves on.
            .scaleEffect(settled ? 1 : 0.7, anchor: .bottom)
            .rotationEffect(.degrees(settled ? 0 : -7), anchor: .bottom)
            .offset(y: settled ? 0 : 16)
            .onAppear { withAnimation(.spring(duration: 0.62, bounce: 0.5)) { shown = true } }
            .onChange(of: pose) {
                var tx = Transaction(); tx.disablesAnimations = true
                withTransaction(tx) { shown = false }
                Task { @MainActor in
                    try? await Task.sleep(for: .milliseconds(20))
                    withAnimation(.spring(duration: 0.62, bounce: 0.5)) { shown = true }
                }
            }
    }
}

/// Zoen's face as an avatar: the furball head, boiling and blinking.
struct MascotHead: View {
    var size: CGFloat = 30
    var mood: MascotRig.Mouth = .pout
    /// Working pose: headphones on, eyes down at a little laptop, keys clacking.
    var working = false
    var body: some View {
        InkDrawing(seed: 78, drawOn: 0, fps: 10, breathe: 0, jitter: 0.5) { working ? Mascot.workingHead(t: $0) : Mascot.head(t: $0, mood: mood) }
            .frame(width: size, height: size)
    }
}

/// `-RodaMascotGallery YES`: every pose on one page, for review and screenshots.
/// `-RodaMascotPose run` shows one pose, big (close-ups).
struct MascotGallery: View {
    var body: some View {
        Group {
            if let one = UserDefaults.standard.string(forKey: "RodaMascotPose").flatMap(MascotPose.init(rawValue:)) {
                VStack {
                    Spacer()
                    MascotView(pose: one).frame(width: 440, height: 440)
                    Text(one.rawValue).font(.system(.title3, design: .monospaced)).foregroundStyle(.secondary)
                    Spacer()
                }
                .frame(maxWidth: .infinity)
            } else {
                ScrollView {
                    LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 0) {
                        ForEach(MascotPose.allCases) { p in
                            VStack(spacing: 0) {
                                MascotView(pose: p).frame(width: 180, height: 180)
                                Text(verbatim: p.rawValue).font(.caption2.monospaced()).foregroundStyle(.secondary)
                            }
                        }
                        VStack(spacing: 6) {
                            HStack(spacing: 10) { MascotHead(size: 28); MascotHead(size: 44); MascotHead(size: 64) }
                            Text(verbatim: "head").font(.caption2.monospaced()).foregroundStyle(.secondary)
                        }
                        .frame(height: 190)
                    }
                    .padding(.top, 40)
                }
            }
        }
        .background(InkPalette.paper.ignoresSafeArea())
    }
}
