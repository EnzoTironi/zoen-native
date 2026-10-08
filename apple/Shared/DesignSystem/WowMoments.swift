import SwiftUI

// MARK: - Shared helpers

enum WowGate {
    /// First-time (or rare) gates so repeats stay subtle.
    static func once(_ key: String) -> Bool {
        let k = "RodaWow.\(key)"
        if UserDefaults.standard.bool(forKey: k) { return false }
        UserDefaults.standard.set(true, forKey: k)
        return true
    }

    static func seen(_ key: String) -> Bool {
        UserDefaults.standard.bool(forKey: "RodaWow.\(key)")
    }

    static func mark(_ key: String) {
        UserDefaults.standard.set(true, forKey: "RodaWow.\(key)")
    }
}

// MARK: - 1. Store ink stamp

/// Hand-drawn “Instalado / Installed” stamp: presses in with squash, ink spread + splatter,
/// then a short line-boil settle. Interruptible; Reduce Motion → opacity fade only.
struct InkStampMark: View {
    var portuguese = false
    var animate = true
    /// Another word and ink (the approvals stack's "SEMPRE" stamp).
    var word: String? = nil
    var inkColor: Color = InkPalette.tomato
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var press: CGFloat = 0      // 0…1 squash-in
    @State private var spread: CGFloat = 0     // ink bloom
    @State private var settle: CGFloat = 0     // boil amp
    @State private var start = Date()

    private var label: String { word ?? (portuguese ? "INSTALADO" : "INSTALLED") }

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 60, paused: reduceMotion || !animate)) { tl in
            let t = reduceMotion ? 1.2 : max(0, tl.date.timeIntervalSince(start))
            let p = reduceMotion ? 1.0 : min(1, t / 0.18)
            let s = reduceMotion ? 1.0 : max(0, min(1, (t - 0.12) / 0.28))
            let boil = reduceMotion ? 0 : (t > 0.35 ? sin((t - 0.35) * 14) * exp(-(t - 0.35) * 3) : 0)
            Canvas { ctx, size in
                draw(ctx: ctx, size: size, press: p, spread: s, boil: boil)
            }
            .scaleEffect(x: 1 + (1 - p) * 0.35, y: 0.55 + p * 0.45) // squash → settle
            .rotationEffect(.degrees(-14 + Double(boil) * 1.2))
            .opacity(Double(min(1, p * 1.4)))
        }
        .onAppear {
            start = .now
            if !animate || reduceMotion { press = 1; spread = 1 }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    private func draw(ctx: GraphicsContext, size: CGSize, press: CGFloat, spread: CGFloat, boil: Double) {
        var rng = InkRNG(91 &+ UInt64(press * 10))
        let cx = size.width * 0.5, cy = size.height * 0.5
        let r = min(size.width, size.height) * 0.42
        // Stamp oval (double ring)
        var ring = Path(ellipseIn: CGRect(x: cx - r, y: cy - r * 0.72, width: r * 2, height: r * 1.44))
        let ink = inkColor.opacity(0.55 + 0.25 * spread)
        ctx.stroke(ring, with: .color(ink), style: StrokeStyle(lineWidth: 3.2 + CGFloat(boil), lineCap: .round))
        // Inner wobble ring
        var inner = Path()
        for i in 0..<18 {
            let a = Double(i) / 18 * .pi * 2
            let wob = 1 + rng.signed() * 0.04 + boil * 0.02
            let p = CGPoint(x: cx + cos(a) * r * 0.82 * wob, y: cy + sin(a) * r * 0.58 * wob)
            if i == 0 { inner.move(to: p) } else { inner.addLine(to: p) }
        }
        inner.closeSubpath()
        ctx.stroke(inner, with: .color(ink.opacity(0.85)), style: StrokeStyle(lineWidth: 1.6, lineCap: .round))

        // Label as rough ink caps (drawn as short strokes, not a system font block)
        let chars = Array(label)
        let totalW = CGFloat(chars.count) * 7.2
        var x0 = cx - totalW / 2
        for ch in chars {
            let h = 9 + rng.signed() * 1.2
            var stroke = Path()
            stroke.move(to: CGPoint(x: x0, y: cy + h / 2))
            stroke.addQuadCurve(to: CGPoint(x: x0 + 5.5, y: cy - h / 2),
                                control: CGPoint(x: x0 + 2.2 + rng.signed(), y: cy + rng.signed() * 2))
            ctx.stroke(stroke, with: .color(ink), style: StrokeStyle(lineWidth: 1.8, lineCap: .round))
            // Glyph body as tiny filled blot for readability of the mark
            x0 += 7.2
        }

        // Splatter
        if spread > 0.2 {
            for _ in 0..<Int(6 + spread * 10) {
                let a = rng.unit() * .pi * 2
                let d = r * (0.9 + CGFloat(rng.unit()) * 0.55) * spread
                let dot = CGRect(x: cx + cos(a) * d - 1.2, y: cy + sin(a) * d * 0.7 - 1.2,
                                 width: 1.5 + CGFloat(rng.unit()) * 2.5,
                                 height: 1.2 + CGFloat(rng.unit()) * 2)
                ctx.fill(Path(ellipseIn: dot), with: .color(ink.opacity(0.35 + rng.unit() * 0.35)))
            }
        }
    }

    /// Readable stamp word over the ink rings (hand-feel via tracking + rotation already on parent).
    var stampedLabel: some View {
        Text(label)
            .font(.system(size: 11, weight: .heavy, design: .rounded))
            .tracking(1.4)
            .foregroundStyle(InkPalette.tomato.opacity(0.85))
            .rotationEffect(.degrees(-14))
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }
}

struct InkStampOverlay: View {
    var portuguese: Bool
    var body: some View {
        ZStack {
            InkStampMark(portuguese: portuguese)
            InkStampMark(portuguese: portuguese).opacity(0) // keep layout; label on top
            Text(portuguese ? "INSTALADO" : "INSTALLED")
                .font(.system(size: 12, weight: .heavy, design: .rounded))
                .tracking(1.6)
                .foregroundStyle(InkPalette.tomato.opacity(0.9))
                .rotationEffect(.degrees(-14))
        }
        .frame(width: 118, height: 78)
    }
}

// MARK: - 2. Space doodle reveal (stroke draw + wash bloom)

/// Plays the group/space doodle drawing in, then blooms a watercolor wash.
struct SpaceArtReveal: View {
    let doodle: Doodle
    var accent: Color = InkPalette.sky
    var size: CGFloat = 96
    /// AvatarV1 drawing for the new Space; when set it inks in instead of the live doodle.
    var art: HandDrawnAvatarAsset.Asset? = nil
    var onFinished: (() -> Void)? = nil
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var wash: CGFloat = 0
    @State private var started = Date()

    var body: some View {
        ZStack {
            PaperBackground(seed: doodle.seed, wash: nil)
            // Wash blooms after strokes start
            Circle()
                .fill((art.map { Color(hex: $0.backdrop) } ?? accent).opacity((art == nil ? 0.22 : 0.5) * wash))
                .frame(width: size * (0.7 + 0.35 * wash), height: size * (0.7 + 0.35 * wash))
                .blur(radius: 8)
            if let art {
                HandDrawnAvatarView(asset: art, size: size)
                    .scaleEffect(reduceMotion ? 1 : 0.82 + 0.18 * wash)
                    .opacity(reduceMotion ? 1 : Double(min(1, wash * 1.6)))
            } else {
                DoodleView(doodle: doodle, drawOn: reduceMotion ? 0 : 0.85, freezeAt: reduceMotion ? 2 : nil)
                    .padding(size * 0.12)
            }
        }
        .frame(width: size, height: size)
        .clipShape(.circle)
        .overlay {
            InkDrawing(seed: doodle.seed &+ 41, drawOn: reduceMotion ? 0 : 0.7, fps: 20, breathe: 0, jitter: 0.5) { _ in
                [InkStroke.ellipse(0.5, 0.5, 0.475, 0.475, width: max(0.02, 1.6 / size), color: InkPalette.ink.opacity(0.75), n: 14)]
            }
        }
        .onAppear {
            if reduceMotion {
                wash = 1
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { onFinished?() }
            } else {
                withAnimation(.easeOut(duration: 0.55).delay(0.35)) { wash = 1 }
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.05) { onFinished?() }
            }
        }
    }
}

// MARK: - 3. First-message ink flourish

struct InkFlourish: View {
    var width: CGFloat = 120
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var start = Date()

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 60, paused: reduceMotion)) { tl in
            let t = reduceMotion ? 1 : max(0, tl.date.timeIntervalSince(start))
            let progress = reduceMotion ? 1 : min(1, t / 0.45)
            Canvas { ctx, size in
                var rng = InkRNG(44)
                let y = size.height * 0.55
                var pts: [CGPoint] = []
                let n = 12
                for i in 0...n {
                    let u = CGFloat(i) / CGFloat(n)
                    pts.append(CGPoint(x: size.width * u,
                                       y: y + sin(u * .pi * 2.2) * 2.2 + rng.signed() * 0.8))
                }
                let drawn = Int(Double(pts.count - 1) * progress) + 1
                guard drawn >= 2 else { return }
                var path = Path()
                path.move(to: pts[0])
                for p in pts.prefix(drawn).dropFirst() { path.addLine(to: p) }
                ctx.stroke(path, with: .color(InkPalette.ink.opacity(0.45)),
                           style: StrokeStyle(lineWidth: 1.6, lineCap: .round, lineJoin: .round))
                // End flick
                if progress > 0.85 {
                    var flick = Path()
                    let last = pts[min(pts.count - 1, drawn - 1)]
                    flick.move(to: last)
                    flick.addQuadCurve(to: CGPoint(x: last.x + 8, y: last.y - 6),
                                       control: CGPoint(x: last.x + 3, y: last.y - 8))
                    ctx.stroke(flick, with: .color(InkPalette.ink.opacity(0.4)),
                               style: StrokeStyle(lineWidth: 1.3, lineCap: .round))
                }
            }
        }
        .frame(width: width, height: 14)
        .onAppear { start = .now }
        .accessibilityHidden(true)
    }
}

