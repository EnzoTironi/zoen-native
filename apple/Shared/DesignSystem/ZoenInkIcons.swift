import SwiftUI

// MARK: - Zoen ink icons (v2): real objects, drawn with a pen
//
// The v1 set read as generic outline icons. v2 draws objects from Zoen's pen-and-paper world
// (a paper plane, a tent, a market stall with a striped awning) with a brush-pen ribbon whose
// weight swells and thins, slightly blobby end caps, ink-blot dots and a marker wash.
// Three exploration directions live here for the comparison sheet; `IconDirection.shipping`
// is the one the app uses.
//
// Each icon is split into `base` and `accent` layers: the accent is the part that plays the
// icon's signature micro-animation (the plane flies off, the bell's scribbles jiggle, the
// awning flutters, the tent's flag flaps, the magnifier's sparkle twinkles). Static at rest.

enum IconDirection: String, CaseIterable, Identifiable {
    /// 1 · Doodle objects: everyday things drawn as quick doodles, ink blots, light wash.
    case doodle
    /// 2 · Mascot DNA: the same glyphs carrying the green furball's tufts and tiny eyes.
    case mascot
    /// 3 · Ink and marker: bold brush strokes over an always-on, misregistered marker wash.
    case marker
    var id: String { rawValue }
    static let shipping: IconDirection = .doodle
    var title: String {
        switch self { case .doodle: "1 · Doodle objects"; case .mascot: "2 · Mascot DNA"; case .marker: "3 · Ink & marker" }
    }
}

/// One ink mark on the 24-unit grid.
struct InkMark {
    var points: [CGPoint]
    var closed = false
    var smooth = true
    /// Stroke weight multiplier (1 = the icon's base ink).
    var weight: CGFloat = 1
    /// Closed marks are drawn as a loop that overshoots its start, like a quick pen circle.
    var overshoot = true
}

struct IconArt {
    var base: [InkMark] = []
    var accent: [InkMark] = []
    /// Filled ink blots (dots, splats): centre and radius.
    var blots: [(CGPoint, CGFloat)] = []
    var accentBlots: [(CGPoint, CGFloat)] = []
    /// Marker wash regions (closed).
    var wash: [InkMark] = []
    /// Where accent rotations pivot.
    var pivot = CGPoint(x: 12, y: 12)
}

// MARK: Drawing helpers

private func P(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: x, y: y) }
private func L(_ pts: (CGFloat, CGFloat)..., w: CGFloat = 1) -> InkMark { InkMark(points: pts.map { P($0.0, $0.1) }, smooth: false, weight: w) }
private func C(_ pts: (CGFloat, CGFloat)..., w: CGFloat = 1) -> InkMark { InkMark(points: pts.map { P($0.0, $0.1) }, weight: w) }
private func Z(_ pts: (CGFloat, CGFloat)..., smooth: Bool = true, w: CGFloat = 1, overshoot: Bool = true) -> InkMark {
    InkMark(points: pts.map { P($0.0, $0.1) }, closed: true, smooth: smooth, weight: w, overshoot: overshoot)
}
private func O(_ cx: CGFloat, _ cy: CGFloat, _ rx: CGFloat, _ ry: CGFloat? = nil, n: Int = 10, w: CGFloat = 1, a0: Double = -2.2) -> InkMark {
    let r2 = ry ?? rx
    return InkMark(points: (0..<n).map { i in let a = a0 + Double(i) / Double(n) * 2 * .pi; return P(cx + rx * CGFloat(cos(a)), cy + r2 * CGFloat(sin(a))) }, closed: true, weight: w)
}
/// A four-point twinkle.
private func star(_ cx: CGFloat, _ cy: CGFloat, _ r: CGFloat, w: CGFloat = 0.8) -> [InkMark] {
    [L((cx, cy - r), (cx, cy + r), w: w), L((cx - r, cy), (cx + r, cy), w: w)]
}
/// Furball tuft: three little hairs fanning up from a point.
private func tuft(_ x: CGFloat, _ y: CGFloat, _ s: CGFloat = 1) -> [InkMark] {
    [C((x - 0.4 * s, y), (x - 1.4 * s, y - 1.8 * s), w: 0.7), C((x, y), (x + 0.1 * s, y - 2.3 * s), w: 0.7), C((x + 0.4 * s, y), (x + 1.5 * s, y - 1.7 * s), w: 0.7)]
}

extension ZoenGlyph {
    func art(_ d: IconDirection) -> IconArt? {
        switch d {
        case .doodle: doodleArt
        case .mascot: mascotArt
        case .marker: markerArt
        }
    }

    // MARK: Direction 1 · Doodle objects (the full set)
    var doodleArt: IconArt? {
        switch self {
        case .chats:
            // Two overlapping doodle bubbles; the front one holds three ink-blot dots.
            let back = C((10.6, 13.6), (6.6, 13.4), (3.4, 10.8), (3.2, 7.2), (5.8, 4.2), (10.2, 3.1), (14.4, 4.0), (16.8, 6.6), (17.0, 8.0))
            let backTail = L((5.6, 12.4), (3.4, 15.8), (8.0, 13.4), w: 0.9)
            let front = O(14.4, 14.6, 7.0, 5.4, n: 11, a0: -2.6)
            let tail = L((18.8, 18.6), (21.2, 21.6), (16.2, 19.8), w: 0.9)
            return IconArt(base: [back, backTail, front, tail], accentBlots: [(P(11.6, 14.6), 1.05), (P(14.4, 14.6), 1.05), (P(17.2, 14.6), 1.05)], wash: [front])
        case .spaces:
            // A little tent with a flag on top (the flag flaps).
            let tent = L((3.0, 20.0), (12.0, 4.6), (21.0, 20.0))
            let door = Z((12.0, 9.8), (9.0, 20.0), (12.6, 18.2), (15.0, 20.0), smooth: false, w: 0.85, overshoot: false)
            let ground = C((1.8, 20.3), (8, 20.0), (14, 20.5), (22.2, 20.1), w: 0.9)
            let pole = L((12.0, 4.6), (12.0, 1.6), w: 0.8)
            let flag = Z((12.2, 1.5), (16.2, 2.4), (12.2, 3.6), smooth: false, w: 0.75, overshoot: false)
            return IconArt(base: [tent, ground, door, pole], accent: [flag], wash: [Z((3.0, 20.0), (12.0, 4.6), (21.0, 20.0), smooth: false)], pivot: P(12, 3))
        case .store:
            // Market stall: scalloped awning with marker stripes (it flutters), posts, counter.
            let top = L((3.0, 4.4), (21.0, 4.4))
            let scallops = C((2.2, 9.0), (4.2, 10.6), (6.6, 9.0), (9.0, 10.6), (11.6, 9.0), (14.2, 10.6), (16.8, 9.0), (19.2, 10.6), (21.8, 9.0), w: 0.95)
            let sides = [L((3.0, 4.4), (2.2, 9.0), w: 0.95), L((21.0, 4.4), (21.8, 9.0), w: 0.95)]
            let stripes = [L((7.4, 4.8), (6.6, 9.0), w: 0.7), L((12.0, 4.8), (11.6, 9.0), w: 0.7), L((16.6, 4.8), (16.8, 9.0), w: 0.7)]
            let posts = [L((4.4, 10.4), (4.4, 20.4)), L((19.6, 10.4), (19.6, 20.4))]
            let counter = C((2.6, 15.0), (12, 14.6), (21.4, 15.0))
            let floor = L((2.8, 20.4), (21.2, 20.4), w: 0.9)
            return IconArt(base: posts + [counter, floor], accent: [top, scallops] + sides + stripes,
                           wash: [Z((3.4, 4.8), (7.4, 4.8), (6.6, 9.0), (4.2, 10.0), (2.6, 9.0), smooth: false), Z((12.0, 4.8), (16.6, 4.8), (16.8, 9.0), (14.2, 10.0), (11.6, 9.0), smooth: false)],
                           pivot: P(12, 4.4))
        case .search:
            // Magnifier with a glint and a tiny sparkle (it twinkles).
            let lens = O(10.0, 10.0, 6.4, n: 11, a0: -2.4)
            let handle = L((14.8, 14.8), (20.6, 20.6), w: 1.45)
            let glint = C((6.8, 9.4), (7.4, 7.4), (9.2, 6.6), w: 0.6)
            return IconArt(base: [lens, handle, glint], accent: star(19.4, 4.4, 2.4, w: 0.75), wash: [lens], pivot: P(19.4, 4.4))
        case .plus:
            // A pencil star-plus with an ink splat.
            let v = L((12.1, 4.2), (11.9, 19.8), w: 1.15), h = L((4.2, 12.1), (19.8, 11.9), w: 1.15)
            let rays = [L((6.8, 6.8), (8.0, 8.0), w: 0.6), L((17.2, 17.2), (16.0, 16.0), w: 0.6), L((6.8, 17.2), (8.0, 16.0), w: 0.6)]
            return IconArt(base: [v, h] + rays, blots: [(P(18.0, 6.0), 1.5), (P(20.4, 3.9), 0.55), (P(16.2, 3.6), 0.38), (P(20.6, 8.0), 0.32)], pivot: P(12, 12))
        case .send:
            // A folded paper plane with two motion dashes (it flies off).
            let wing = Z((2.6, 10.6), (21.2, 3.0), (13.8, 20.8), (10.6, 13.4), smooth: false)
            let fold = L((10.6, 13.4), (21.2, 3.0), w: 0.8)
            let flap = L((10.6, 13.4), (10.9, 17.8), (13.0, 15.6), w: 0.8)
            let dashes = [L((2.4, 17.2), (5.8, 15.6), w: 0.7), L((4.6, 21.0), (7.4, 19.6), w: 0.7)]
            return IconArt(base: dashes, accent: [wing, fold, flap], wash: [Z((2.6, 10.6), (21.2, 3.0), (10.6, 13.4), smooth: false)], pivot: P(12, 12))
        case .back:
            // A hand-drawn arrow whose tail makes a little loop.
            let shaft = C((20.6, 16.4), (18.2, 13.4), (15.2, 13.2), (14.6, 15.6), (16.4, 16.6), (17.2, 14.0), (15.0, 11.8), (10.0, 11.8), (5.2, 12.0))
            let head = L((9.8, 7.4), (4.8, 12.0), (9.8, 16.4), w: 1.1)
            return IconArt(base: [shaft, head])
        case .phone:
            // An old handset with a curly cord.
            let handset = Z((3.0, 8.6), (3.4, 5.6), (6.4, 4.0), (12, 3.4), (17.6, 4.0), (20.6, 5.6), (21.0, 8.6), (17.6, 9.4), (16.2, 7.2), (12, 6.6), (7.8, 7.2), (6.4, 9.4), overshoot: false)
            let cord = C((12.0, 7.2), (12.6, 10.6), (10.4, 12.2), (12.8, 13.0), (10.6, 14.6), (13.0, 15.4), (10.8, 17.0), (13.2, 17.8), (12.2, 20.0), (15.6, 21.0), w: 0.7)
            return IconArt(base: [handset], accent: [cord], wash: [handset], pivot: P(12, 7))
        case .video:
            // A little film camera: two reels, body and lens.
            let body = InkMark(points: [P(3, 10.6), P(15, 10.6), P(15, 19.4), P(3, 19.4)], closed: true, smooth: false)
            let lens = Z((15.0, 12.6), (21.0, 10.4), (21.0, 19.0), (15.0, 17.2), smooth: false, w: 0.9, overshoot: false)
            let reels = [O(6.2, 6.6, 3.0, n: 9, w: 0.9), O(12.0, 6.8, 2.6, n: 9, w: 0.9)]
            return IconArt(base: [body, lens], accent: reels, blots: [(P(6.2, 6.6), 0.7), (P(12.0, 6.8), 0.6)], wash: [body], pivot: P(9, 7))
        case .bell:
            // A bell with motion scribbles (it jiggles).
            let dome = C((5.6, 17.0), (6.2, 10.6), (8.6, 6.6), (12.0, 5.4), (15.4, 6.6), (17.8, 10.6), (18.4, 17.0))
            let lip = C((3.8, 17.2), (12, 16.6), (20.2, 17.2))
            let clapper = O(12.0, 19.6, 1.4, n: 7, w: 0.85)
            let knob = L((12.0, 5.4), (12.0, 3.0), w: 0.9)
            let scribbles = [C((2.4, 7.4), (1.6, 9.6), (2.2, 11.6), w: 0.6), C((21.6, 7.4), (22.4, 9.6), (21.8, 11.6), w: 0.6)]
            return IconArt(base: [dome, lip, knob, clapper], accent: scribbles,
                           wash: [Z((5.6, 17.0), (6.2, 10.6), (8.6, 6.6), (12.0, 5.4), (15.4, 6.6), (17.8, 10.6), (18.4, 17.0))], pivot: P(12, 3))
        case .camera:
            let body = Z((2.6, 9.6), (3.6, 7.4), (7.8, 7.4), (9.4, 4.8), (14.6, 4.8), (16.2, 7.4), (20.4, 7.4), (21.4, 9.6), (21.4, 18.0), (20.0, 19.6), (4.0, 19.6), (2.6, 18.0), smooth: false, overshoot: false)
            let lens = O(12.0, 13.4, 3.9, n: 10, w: 0.95)
            return IconArt(base: [body, lens], accent: [C((10.2, 12.0), (11.0, 11.0), w: 0.55)], blots: [(P(18.2, 10.2), 0.75)], wash: [body])
        case .mic:
            // A mic with scribbled sound waves (they pulse).
            let cap = Z((9.0, 6.0), (9.4, 3.6), (12, 2.6), (14.6, 3.6), (15.0, 6.0), (15.0, 11.6), (14.6, 14.0), (12, 15.0), (9.4, 14.0), (9.0, 11.6), overshoot: false)
            let stand = L((12.0, 15.0), (12.0, 20.4), w: 0.95)
            let foot = C((8.6, 20.8), (12, 20.4), (15.4, 20.8), w: 0.95)
            let grille = [L((10.4, 7.0), (13.6, 7.0), w: 0.5), L((10.4, 9.4), (13.6, 9.4), w: 0.5)]
            let waves = [C((17.4, 6.2), (18.6, 8.8), (17.4, 11.4), w: 0.7), C((19.8, 4.6), (21.8, 8.8), (19.8, 13.0), w: 0.7),
                         C((6.6, 6.2), (5.4, 8.8), (6.6, 11.4), w: 0.7), C((4.2, 4.6), (2.2, 8.8), (4.2, 13.0), w: 0.7)]
            return IconArt(base: [cap, stand, foot] + grille, accent: waves, wash: [cap], pivot: P(12, 8.8))
        case .info:
            let ring = O(12, 12, 9.4, n: 12, a0: -2.0)
            return IconArt(base: [ring, C((10.8, 11.0), (12.2, 10.8), (12.0, 17.2), w: 1.05)], accentBlots: [(P(12.1, 7.4), 1.15)], wash: [ring])
        case .pin:
            // A thumbtack, tilted.
            let head = O(12, 5.6, 5.0, 2.4, n: 10)
            let body = [L((9.6, 7.4), (10.4, 12.2), w: 0.9), L((14.4, 7.4), (13.6, 12.2), w: 0.9)]
            let collar = O(12, 12.9, 5.4, 1.5, n: 10, w: 0.95)
            let needle = L((12.0, 14.6), (12.0, 21.6), w: 0.8)
            var art = IconArt(base: [head] + body + [collar, needle], wash: [Z((7.2, 5.6), (12, 3.2), (16.8, 5.6), (14.4, 7.6), (13.6, 12.4), (10.4, 12.4), (9.6, 7.6))])
            art.rotate(0.5, around: P(12, 12))
            return art
        case .settings:
            var cog: [CGPoint] = []
            for k in 0..<7 {
                let a = Double(k) / 7 * 2 * .pi
                for (da, r) in [(-0.34, 6.8), (-0.16, 9.4), (0.16, 9.4), (0.34, 6.8)] as [(Double, CGFloat)] {
                    cog.append(P(12 + r * CGFloat(cos(a + da)), 12 + r * CGFloat(sin(a + da))))
                }
            }
            let gear = InkMark(points: cog, closed: true, smooth: false, overshoot: false)
            return IconArt(base: [gear], accent: [O(12, 12, 3.0, n: 8, w: 0.95)], wash: [gear])
        case .share:
            let box = L((8.4, 9.0), (5.0, 9.0), (5.0, 20.6), (19.0, 20.6), (19.0, 9.0), (15.6, 9.0))
            let arrow = [C((12.0, 15.2), (12.4, 9.0), (12.0, 3.2)), L((8.2, 6.8), (12.0, 3.0), (15.8, 6.8), w: 1.05)]
            return IconArt(base: [box], accent: arrow, wash: [Z((5, 9), (19, 9), (19, 20.6), (5, 20.6), smooth: false)], pivot: P(12, 12))
        case .close:
            return IconArt(base: [C((5.4, 5.6), (12.2, 11.8), (18.8, 18.6), w: 1.05), C((18.6, 5.4), (11.8, 12.2), (5.6, 18.8), w: 1.05)])
        case .check:
            return IconArt(base: [C((4.0, 12.4), (7.4, 15.4), (9.6, 18.0), (13.6, 11.6), (20.4, 5.0), w: 1.1)])
        case .more:
            return IconArt(accentBlots: [(P(5.4, 12.2), 1.6), (P(12.0, 11.8), 1.6), (P(18.6, 12.1), 1.6)])
        case .sparkle:
            let s = Z((12, 2.8), (13.5, 9.8), (20.6, 11.8), (13.5, 13.6), (12, 21), (10.5, 13.6), (3.4, 11.8), (10.5, 9.8), smooth: false, overshoot: false)
            return IconArt(base: [s], accent: star(19.4, 4.2, 2.0, w: 0.7), wash: [s], pivot: P(19.4, 4.2))
        case .agents:
            // A little furball friend (Zoen's agents).
            let ball = O(11.0, 13.0, 7.0, 6.6, n: 11)
            return IconArt(base: [ball] + tuft(11.0, 6.4, 1.1), accent: star(19.6, 4.4, 2.0, w: 0.7),
                           blots: [(P(8.8, 12.4), 0.85), (P(13.2, 12.4), 0.85)], wash: [ball], pivot: P(19.6, 4.4))
        case .folder:
            let f = Z((3, 6.6), (3.6, 5.2), (9.0, 5.2), (11.0, 7.8), (20.4, 7.8), (21, 9.2), (21, 18.6), (20, 19.6), (4, 19.6), (3, 18.6), smooth: false, overshoot: false)
            return IconArt(base: [f], accent: [C((3.4, 10.6), (12, 10.2), (20.6, 10.6), w: 0.85)], wash: [f])
        case .you:
            let ring = O(12, 12, 9.4, n: 12, a0: -2.0)
            return IconArt(base: [ring, O(12, 9.8, 3.2, n: 8, w: 0.95), C((6.4, 18.6), (8.6, 15.6), (12, 14.8), (15.4, 15.6), (17.6, 18.6), w: 0.95)], wash: [ring])
        case .chevron:
            return IconArt(base: [L((9.2, 5.0), (16.0, 12.0), (9.2, 19.0), w: 1.1)])
        case .crown:
            let c = Z((3.6, 17.0), (4.4, 7.6), (9.0, 12.2), (12.0, 5.0), (15.0, 12.2), (19.6, 7.6), (20.4, 17.0), smooth: false, overshoot: false)
            return IconArt(base: [c, C((4.0, 19.8), (12, 19.3), (20.0, 19.8), w: 0.9)], accentBlots: [(P(4.4, 6.4), 1.0), (P(12.0, 3.8), 1.0), (P(19.6, 6.4), 1.0)], wash: [c])
        case .sun:
            var rays: [InkMark] = []
            for k in 0..<8 { let a = Double(k) / 8 * 2 * .pi; let c = CGFloat(cos(a)), sn = CGFloat(sin(a)); rays.append(L((12 + 6.8 * c, 12 + 6.8 * sn), (12 + 9.6 * c, 12 + 9.6 * sn), w: 0.85)) }
            return IconArt(base: [O(12, 12, 4.4, n: 9)], accent: rays, wash: [O(12, 12, 4.4, n: 9)], pivot: P(12, 12))
        case .heart:
            let h = Z((12, 20.0), (5.0, 13.6), (3.6, 8.6), (6.2, 5.0), (9.6, 5.2), (12, 8.0), (14.4, 5.2), (17.8, 5.0), (20.4, 8.6), (19.0, 13.6))
            return IconArt(base: [h], wash: [h])
        case .game:
            let pad = Z((4.0, 9.0), (8.0, 7.4), (16.0, 7.4), (20.0, 9.0), (21.4, 15.6), (19.6, 18.6), (16.8, 17.6), (14.6, 15.0), (9.4, 15.0), (7.2, 17.6), (4.4, 18.6), (2.6, 15.6), overshoot: false)
            return IconArt(base: [pad, L((7.8, 9.8), (7.8, 13.8), w: 0.85), L((5.8, 11.8), (9.8, 11.8), w: 0.85)], accentBlots: [(P(15.8, 10.6), 0.85), (P(17.8, 12.8), 0.85)], wash: [pad])
        case .list:
            return IconArt(base: [L((9.2, 6.4), (20.2, 6.4)), L((9.2, 12.0), (20.2, 12.0)), L((9.2, 17.6), (16.6, 17.6))],
                           accentBlots: [(P(4.8, 6.4), 1.15), (P(4.8, 12.0), 1.15), (P(4.8, 17.6), 1.15)])
        case .note:
            return IconArt(base: [L((15.2, 4.0), (15.2, 16.6)), C((15.2, 4.0), (18.8, 5.6), (19.8, 9.4), w: 0.9)], blots: [(P(12.0, 17.2), 3.0)], pivot: P(15, 10))
        case .download:
            return IconArt(base: [L((4.6, 15.4), (4.6, 20.2), (19.4, 20.2), (19.4, 15.4))], accent: [C((12.0, 3.4), (12.2, 9.0), (12.0, 14.0)), L((7.6, 10.0), (12.0, 14.4), (16.4, 10.0), w: 1.05)], pivot: P(12, 9))
        case .lock:
            let body = InkMark(points: [P(5.0, 10.6), P(19.0, 10.6), P(19.0, 20.4), P(5.0, 20.4)], closed: true, smooth: false)
            let shackle = C((8.0, 10.4), (8.0, 6.2), (9.6, 3.8), (12.0, 3.4), (14.4, 3.8), (16.0, 6.2), (16.0, 10.4), w: 0.95)
            return IconArt(base: [body], accent: [shackle], accentBlots: [(P(12.0, 15.2), 1.2)], wash: [body], pivot: P(12, 10))
        case .trash:
            let can = Z((5.6, 7.4), (18.4, 7.4), (17.0, 20.4), (7.0, 20.4), smooth: false, overshoot: false)
            let lid = C((3.4, 6.8), (12, 6.2), (20.6, 6.8))
            let handle = L((9.4, 6.4), (10.0, 3.6), (14.0, 3.6), (14.6, 6.4), w: 0.9)
            return IconArt(base: [can, lid], accent: [handle, L((10.0, 10.6), (10.4, 17.4), w: 0.6), L((14.0, 10.6), (13.6, 17.4), w: 0.6)], wash: [can], pivot: P(12, 6))
        case .play:
            let t = Z((7.0, 4.6), (19.6, 12.0), (7.0, 19.4), smooth: false, overshoot: false)
            return IconArt(base: [t], wash: [t])
        case .pause:
            return IconArt(base: [L((8.4, 5.0), (8.6, 19.0), w: 1.25), L((15.6, 5.0), (15.4, 19.0), w: 1.25)])
        }
    }

    // MARK: Direction 2 · Mascot DNA (exploration: 6 key icons)
    var mascotArt: IconArt? {
        switch self {
        case .chats:
            let back = C((10.6, 13.6), (6.6, 13.4), (3.4, 10.8), (3.2, 7.2), (5.8, 4.2), (10.2, 3.1), (14.4, 4.0), (16.8, 6.6), (17.0, 8.0))
            let front = O(14.4, 14.8, 7.0, 5.4, n: 11, a0: -2.6)
            let tail = L((18.8, 18.8), (21.2, 21.6), (16.2, 20.0), w: 0.9)
            return IconArt(base: [back, front, tail] + tuft(14.6, 9.4, 0.9), accentBlots: [(P(12.6, 14.6), 0.9), (P(16.4, 14.6), 0.9)], wash: [front])
        case .spaces:
            // Three little furballs.
            let l = O(5.8, 15.4, 3.6, n: 9, w: 0.9), r = O(18.2, 15.4, 3.6, n: 9, w: 0.9), m = O(12, 13.2, 4.6, n: 10)
            return IconArt(base: [l, r, m] + tuft(12, 8.6) + tuft(5.8, 11.8, 0.7) + tuft(18.2, 11.8, 0.7),
                           accentBlots: [(P(10.4, 12.8), 0.75), (P(13.6, 12.8), 0.75), (P(4.8, 15.2), 0.55), (P(6.8, 15.2), 0.55), (P(17.2, 15.2), 0.55), (P(19.2, 15.2), 0.55)],
                           wash: [m])
        case .store:
            // Storefront with a furball peeking over the counter.
            let awning = Z((4.2, 4.2), (19.8, 4.2), (21.0, 8.8), (17.5, 10.0), (14.5, 8.8), (12.0, 10.0), (9.5, 8.8), (6.5, 10.0), (3.0, 8.8), smooth: false, overshoot: false)
            let box = L((4.5, 10.4), (4.5, 20.4), (19.5, 20.4), (19.5, 10.4))
            let peek = C((8.6, 16.0), (9.4, 13.0), (12.0, 12.0), (14.6, 13.0), (15.4, 16.0), w: 0.9)
            let counter = L((4.5, 16.0), (19.5, 16.0), w: 0.9)
            return IconArt(base: [awning, box, counter, peek] + tuft(12, 12.0, 0.7), accentBlots: [(P(10.8, 14.6), 0.6), (P(13.2, 14.6), 0.6)], wash: [awning])
        case .search:
            // The lens is an eye.
            let lens = O(10.0, 10.0, 6.4, n: 11, a0: -2.4)
            let handle = L((14.8, 14.8), (20.6, 20.6), w: 1.45)
            return IconArt(base: [lens, handle] + tuft(10, 3.6, 0.8), accentBlots: [(P(11.2, 10.6), 2.0)], wash: [lens])
        case .plus:
            let v = L((12, 5.0), (12, 19.4), w: 1.15), h = L((4.6, 12), (19.4, 12), w: 1.15)
            return IconArt(base: [v, h] + tuft(12, 4.8, 0.7))
        case .send:
            let wing = Z((2.6, 10.6), (21.2, 3.0), (13.8, 20.8), (10.6, 13.4), smooth: false)
            let fold = L((10.6, 13.4), (21.2, 3.0), w: 0.8)
            return IconArt(base: [], accent: [wing, fold] + tuft(13.8, 20.8, -0.6), accentBlots: [(P(15.6, 7.4), 0.6), (P(17.2, 9.2), 0.6)],
                           wash: [Z((2.6, 10.6), (21.2, 3.0), (10.6, 13.4), smooth: false)])
        default: return nil
        }
    }

    // MARK: Direction 3 · Ink & marker (exploration: 6 key icons)
    var markerArt: IconArt? {
        switch self {
        case .chats:
            let b = Z((4.0, 11.0), (4.6, 5.6), (12.0, 3.8), (19.4, 5.6), (20.0, 11.0), (17.0, 16.6), (11.0, 17.2), (6.0, 20.4), (7.0, 16.0), w: 1.4)
            return IconArt(base: [b], wash: [O(12.6, 11.4, 7.6, 6.0, n: 10)])
        case .spaces:
            return IconArt(base: [O(8.6, 8.4, 3.4, n: 9, w: 1.35), C((2.8, 20.0), (4.2, 14.4), (8.6, 13.4), (13.0, 14.4), (14.4, 20.0), w: 1.35), O(16.8, 9.6, 2.6, n: 8, w: 1.2), C((15.6, 14.0), (19.6, 14.6), (21.2, 19.4), w: 1.2)],
                           wash: [O(15.0, 14.6, 6.4, 5.6, n: 10)])
        case .store:
            // Folded paper bag with a handle.
            let bag = L((4.4, 8.6), (5.2, 20.8), (18.8, 20.8), (19.6, 8.6), (4.4, 8.6), w: 1.4)
            let handle = C((9.0, 8.6), (9.4, 4.2), (12.0, 3.2), (14.6, 4.2), (15.0, 8.6), w: 1.2)
            return IconArt(base: [bag, handle], wash: [Z((6.4, 10.8), (20.4, 10.4), (19.6, 21.6), (7.0, 21.8), smooth: false)])
        case .search:
            return IconArt(base: [O(10, 10, 6.4, n: 11, w: 1.4), L((14.8, 14.8), (20.8, 20.8), w: 1.8)], wash: [O(11.4, 11.2, 5.6, n: 10)])
        case .plus:
            return IconArt(base: [L((12, 4.4), (12.2, 19.6), w: 1.6), L((4.4, 12.2), (19.6, 11.8), w: 1.6)], wash: [O(13.4, 13.2, 6.0, n: 10)])
        case .send:
            return IconArt(base: [L((12, 20), (12, 4.6), w: 1.6), L((5.4, 11.0), (12, 4.4), (18.6, 11.0), w: 1.5)], wash: [O(13.6, 13.6, 7.2, n: 10)])
        default: return nil
        }
    }
}

extension IconArt {
    mutating func rotate(_ a: Double, around o: CGPoint) {
        let c = CGFloat(cos(a)), s = CGFloat(sin(a))
        func r(_ p: CGPoint) -> CGPoint { let dx = p.x - o.x, dy = p.y - o.y; return P(o.x + dx * c - dy * s, o.y + dx * s + dy * c) }
        func rm(_ m: InkMark) -> InkMark { var m = m; m.points = m.points.map(r); return m }
        base = base.map(rm); accent = accent.map(rm); wash = wash.map(rm)
        blots = blots.map { (r($0.0), $0.1) }; accentBlots = accentBlots.map { (r($0.0), $0.1) }
        pivot = r(pivot)
    }
}

// MARK: Rendering

/// Which part of an icon a shape draws.
enum IconLayer { case base, accent, wash }

/// An ink icon layer as a filled Shape: brush ribbons with pressure, blobby end caps, ink
/// blots. `progress` (animatable) draws it on; `frame` re-seeds the wobble (boil).
struct InkIconShape: Shape {
    let glyph: ZoenGlyph
    let direction: IconDirection
    let layer: IconLayer
    /// Base ink weight on the 24 grid.
    var ink: CGFloat = 1.75
    var frame = 0
    var progress: CGFloat = 1
    var animatableData: CGFloat { get { progress } set { progress = newValue } }

    func path(in rect: CGRect) -> Path {
        guard let art = glyph.art(direction) else { return Path() }
        let k = min(rect.width, rect.height) / 24
        let o = CGPoint(x: rect.midX - 12 * k, y: rect.midY - 12 * k)
        var rng = InkRNG(glyph.seed &+ UInt64(direction.rawValue.count) &* 31 &+ UInt64(frame) &* 7919 &+ (layer == .accent ? 101 : layer == .wash ? 202 : 0))
        var path = Path()
        let marker = direction == .marker
        // Wobble: visible but small. Thin icons wobble less so 20pt stays crisp.
        let amp: CGFloat = marker ? 0.32 : 0.28
        switch layer {
        case .wash:
            let slip = marker ? CGPoint(x: 1.3, y: 1.1) : CGPoint(x: 0.8, y: 0.7)
            for m in art.wash {
                var pts = m.points.map { P(o.x + ($0.x + slip.x + rng.signed() * amp * 1.4) * k, o.y + ($0.y + slip.y + rng.signed() * amp * 1.4) * k) }
                pts = m.smooth ? Ink.catmull(pts, closed: true, steps: 5) : pts
                if Self.area(pts) < 0 { pts.reverse() }
                path.addLines(pts); path.closeSubpath()
            }
        case .base, .accent:
            let marks = layer == .base ? art.base : art.accent
            let blots = layer == .base ? art.blots : art.accentBlots
            let n = max(1, marks.count + (blots.isEmpty ? 0 : 1))
            for (i, m) in marks.enumerated() {
                let local = Self.local(progress, index: i, of: n)
                guard local > 0 else { continue }
                var pts = m.points.map { P(o.x + ($0.x + rng.signed() * amp) * k, o.y + ($0.y + rng.signed() * amp) * k) }
                if m.closed && m.overshoot && pts.count > 2 {
                    // A quick pen loop: run past the start a little, slightly off the line.
                    let extra = [pts[0], pts[1]].map { P($0.x + rng.signed() * amp * k * 1.5, $0.y + rng.signed() * amp * k * 1.5) }
                    pts += [extra[0], P((extra[0].x + extra[1].x) / 2, (extra[0].y + extra[1].y) / 2)]
                }
                let closedLoop = m.closed && !m.overshoot
                var poly = m.smooth ? Ink.catmull(pts, closed: closedLoop, steps: 6) : Self.densify(closedLoop ? pts + [pts[0]] : pts, step: 1.2 * k)
                poly = Ink.trim(poly, Double(local))
                guard poly.count > 1 else { continue }
                let w = ink * k * m.weight * (marker ? 1.18 : 1)
                path.addPath(Self.brush(poly, width: w, closed: closedLoop && local >= 1, marker: marker, rng: &rng))
            }
            if !blots.isEmpty {
                let local = Self.local(progress, index: n - 1, of: n)
                for (c, r) in blots where local > 0 {
                    let rr = r * k * min(1, local * 1.6)
                    var pts: [CGPoint] = []
                    for j in 0..<9 {
                        let a = Double(j) / 9 * 2 * .pi
                        let q = rr * (0.86 + 0.24 * CGFloat(rng.unit()))
                        pts.append(P(o.x + c.x * k + q * CGFloat(cos(a)), o.y + c.y * k + q * CGFloat(sin(a))))
                    }
                    path.addLines(Ink.catmull(pts, closed: true, steps: 4)); path.closeSubpath()
                }
            }
        }
        return path
    }

    /// Shoelace sum (positive = increasing-angle winding in y-down space).
    static func area(_ p: [CGPoint]) -> CGFloat {
        guard p.count > 2 else { return 0 }
        var a: CGFloat = 0
        for i in 0..<p.count { let u = p[i], v = p[(i + 1) % p.count]; a += u.x * v.y - v.x * u.y }
        return a
    }

    private static func local(_ p: CGFloat, index i: Int, of n: Int) -> CGFloat {
        guard p < 1 else { return 1 }
        let start = CGFloat(i) / CGFloat(n) * 0.55
        return min(1, max(0, (p - start) / 0.5))
    }

    private static func densify(_ p: [CGPoint], step: CGFloat) -> [CGPoint] {
        guard p.count > 1 else { return p }
        var out: [CGPoint] = []
        for i in 0..<(p.count - 1) {
            let a = p[i], b = p[i + 1]
            let n = max(1, Int(hypot(b.x - a.x, b.y - a.y) / max(0.5, step)))
            for s in 0..<n { let t = CGFloat(s) / CGFloat(n); out.append(P(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)) }
        }
        out.append(p[p.count - 1])
        return out
    }

    /// A brush-pen ribbon: width swells and thins along the stroke (pressure), slightly
    /// thinner where the pen lifts, then a blobby, irregular cap at each open end.
    private static func brush(_ p: [CGPoint], width: CGFloat, closed: Bool, marker: Bool, rng: inout InkRNG) -> Path {
        let n = p.count
        let k1 = 3 + rng.unit() * 4, phase = rng.unit() * 6.28
        let swing = marker ? 0.42 : 0.3
        var left: [CGPoint] = [], right: [CGPoint] = []
        var hs: [CGFloat] = []
        for i in 0..<n {
            let a = p[max(0, i - (closed && i == 0 ? 0 : 1))], b = p[min(n - 1, i + 1)]
            var dx = b.x - a.x, dy = b.y - a.y
            let len = max(0.0001, hypot(dx, dy)); dx /= len; dy /= len
            let u = n > 1 ? Double(i) / Double(n - 1) : 0
            let pressure = (1 - swing / 2) + swing * (0.5 + 0.5 * sin(u * k1 + phase))
            let lift = closed ? 1 : 0.78 + 0.22 * pow(sin(Double.pi * u), 0.35)
            let h = width * 0.5 * CGFloat(pressure * lift)
            hs.append(h)
            left.append(P(p[i].x - dy * h, p[i].y + dx * h))
            right.append(P(p[i].x + dy * h, p[i].y - dx * h))
        }
        // Every subpath wound the same way (positive area), so the caps and blots add to the
        // ribbon under the non-zero rule instead of punching holes where they overlap.
        var ring = left + right.reversed()
        if Self.area(ring) < 0 { ring.reverse() }
        var path = Path()
        path.addLines(ring)
        path.closeSubpath()
        if !closed, let f = p.first, let l = p.last {
            for (c, h) in [(f, hs.first ?? width / 2), (l, hs.last ?? width / 2)] {
                let rx = h * (0.95 + 0.3 * CGFloat(rng.unit())), ry = h * (0.85 + 0.25 * CGFloat(rng.unit()))
                let jx = rng.signed() * h * 0.2, jy = rng.signed() * h * 0.2
                path.addLines((0..<12).map { j in let a = Double(j) / 12 * 2 * .pi; return P(c.x + jx + rx * CGFloat(cos(a)), c.y + jy + ry * CGFloat(sin(a))) })
                path.closeSubpath()
            }
        }
        return path
    }
}

/// The selected state: a highlighter swipe behind the icon (two marker passes, slightly
/// rotated), swiped on left to right.
struct HighlighterBlob: Shape {
    var seed: UInt64
    var progress: CGFloat = 1
    var animatableData: CGFloat { get { progress } set { progress = newValue } }
    func path(in rect: CGRect) -> Path {
        var rng = InkRNG(seed &+ 77)
        var path = Path()
        let w = rect.width * progress
        for pass in 0..<2 {
            let inset = CGFloat(pass) * rect.height * 0.12
            let r = CGRect(x: rect.minX + (pass == 0 ? 0 : rect.width * 0.08), y: rect.minY + inset + rng.signed() * rect.height * 0.04,
                           width: max(0, w - (pass == 0 ? 0 : rect.width * 0.1)), height: rect.height - inset * 1.6)
            guard r.width > 1 else { continue }
            var pts: [CGPoint] = []
            for j in 0..<10 {
                let a = Double(j) / 10 * 2 * .pi
                let q = 1 + 0.06 * rng.signed()
                pts.append(P(r.midX + r.width / 2 * CGFloat(cos(a)) * q, r.midY + r.height / 2 * CGFloat(sin(a)) * q))
            }
            path.addLines(Ink.catmull(pts, closed: true, steps: 4)); path.closeSubpath()
        }
        return path.applying(CGAffineTransform(translationX: rect.midX, y: rect.midY).rotated(by: -0.08).translatedBy(x: -rect.midX, y: -rect.midY))
    }
}

/// Per-glyph signature motion, played on interaction (selection or tap).
struct IconMotion {
    var accentX: CGFloat = 0, accentY: CGFloat = 0
    var accentRot: Double = 0, accentScale: CGFloat = 1, accentOpacity: Double = 1
    var rot: Double = 0, scale: CGFloat = 1
}

extension ZoenGlyph {
    /// Keyframes as (accentX, accentY, accentRot, accentScale, accentOpacity, rot, scale) per beat.
    var motionBeats: [IconMotion] {
        switch self {
        case .send:  // flies off up-right, then lands back
            return [IconMotion(accentX: 4, accentY: -4, accentRot: -6), IconMotion(accentX: 18, accentY: -18, accentRot: -10, accentScale: 0.7, accentOpacity: 0),
                    IconMotion(accentX: -6, accentY: 6, accentOpacity: 0), IconMotion()]
        case .bell:
            return [IconMotion(accentScale: 1.15, rot: 14), IconMotion(accentScale: 1.2, rot: -12), IconMotion(accentScale: 1.1, rot: 6), IconMotion()]
        case .store:  // awning flutters
            return [IconMotion(accentY: -0.8, accentRot: 3), IconMotion(accentY: 0.4, accentRot: -3), IconMotion(accentY: -0.4, accentRot: 1.5), IconMotion()]
        case .spaces:  // flag flaps
            return [IconMotion(accentRot: 16, accentScale: 1.1), IconMotion(accentRot: -10), IconMotion(accentRot: 8, accentScale: 1.05), IconMotion()]
        case .search, .sparkle, .agents:  // sparkle twinkles
            return [IconMotion(accentRot: 45, accentScale: 1.6), IconMotion(accentRot: 90, accentScale: 0.6), IconMotion(accentRot: 90, accentScale: 1.2), IconMotion(accentRot: 90)]
        case .chats:  // dots hop
            return [IconMotion(accentY: -2, accentScale: 1.1), IconMotion(accentY: 0.6), IconMotion(accentY: -0.8), IconMotion()]
        case .plus:
            return [IconMotion(rot: 28, scale: 1.12), IconMotion(rot: -10), IconMotion(rot: 5), IconMotion()]
        case .mic:
            return [IconMotion(accentScale: 1.25, accentOpacity: 0.5), IconMotion(accentScale: 0.9), IconMotion(accentScale: 1.15, accentOpacity: 0.7), IconMotion()]
        case .phone:
            return [IconMotion(rot: -10), IconMotion(rot: 10), IconMotion(rot: -6), IconMotion()]
        case .video:
            return [IconMotion(accentRot: 120), IconMotion(accentRot: 240), IconMotion(accentRot: 360), IconMotion(accentRot: 360)]
        case .share:
            return [IconMotion(accentY: -3), IconMotion(accentY: 0.5), IconMotion(accentY: -1), IconMotion()]
        case .settings:
            return [IconMotion(rot: 20), IconMotion(rot: 46), IconMotion(rot: 53), IconMotion(rot: 360.0 / 7)]
        default:
            return [IconMotion(scale: 1.14), IconMotion(scale: 0.96), IconMotion(scale: 1.03), IconMotion()]
        }
    }
}

/// The ink icon view: base + accent ink in the foreground style, optional marker wash and
/// highlighter (selected). Draws on and plays its signature motion when selected or tapped.
struct InkIcon: View {
    @Environment(\.ambientPaused) private var ambientPaused
    let glyph: ZoenGlyph
    var direction: IconDirection = .shipping
    var selected = false
    var size: CGFloat = 22
    /// Changes play the signature motion (taps).
    var trigger: Int = 0
    var highlighter: Color? = nil

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.zoenIconBoil) private var boil
    @Environment(\.colorScheme) private var scheme
    @State private var draw: CGFloat = 1
    @State private var plays = 0

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 10, paused: ambientPaused || !boil || reduceMotion)) { tl in
            let frame = boil && !reduceMotion ? Int(tl.date.timeIntervalSinceReferenceDate * 10) % 4 + 1 : 0
            let art = glyph.art(direction)
            let pivot = art.map { UnitPoint(x: $0.pivot.x / 24, y: $0.pivot.y / 24) } ?? .center
            ZStack {
                if selected {
                    HighlighterBlob(seed: glyph.seed, progress: draw)
                        // Light: a butter highlighter. Dark: yellow at any opacity over near-black
                        // turns olive, so the marker is the brand green there.
                        .fill(highlighter ?? (scheme == .dark ? Palette.action.opacity(0.3) : InkPalette.butter.opacity(0.45)))
                        .frame(width: size * 1.25, height: size * 0.82)
                        .offset(x: size * 0.04, y: size * 0.1)
                }
                if direction == .marker || selected {
                    InkIconShape(glyph: glyph, direction: direction, layer: .wash, frame: frame)
                        .fill(direction == .marker ? AnyShapeStyle(markerWash) : AnyShapeStyle(.foreground.opacity(0.22)))
                        .opacity(direction == .marker ? 1 : Double(draw))
                }
                InkIconShape(glyph: glyph, direction: direction, layer: .base, ink: ink, frame: frame, progress: draw)
                    .fill(.foreground)
                    .frame(width: size, height: size)
                    .keyframeAnimator(initialValue: IconMotion(), trigger: plays) { v, m in
                        v.rotationEffect(.degrees(m.rot), anchor: .center).scaleEffect(m.scale)
                    } keyframes: { _ in
                KeyframeTrack(\.accentX) {
                    CubicKeyframe(b[0].accentX, duration: 0.14); CubicKeyframe(b[1].accentX, duration: 0.16)
                    CubicKeyframe(b[2].accentX, duration: 0.16); CubicKeyframe(b[3].accentX, duration: 0.2)
                }
                KeyframeTrack(\.accentY) {
                    CubicKeyframe(b[0].accentY, duration: 0.14); CubicKeyframe(b[1].accentY, duration: 0.16)
                    CubicKeyframe(b[2].accentY, duration: 0.16); CubicKeyframe(b[3].accentY, duration: 0.2)
                }
                KeyframeTrack(\.accentRot) {
                    CubicKeyframe(b[0].accentRot, duration: 0.14); CubicKeyframe(b[1].accentRot, duration: 0.16)
                    CubicKeyframe(b[2].accentRot, duration: 0.16); CubicKeyframe(b[3].accentRot, duration: 0.2)
                }
                KeyframeTrack(\.accentScale) {
                    CubicKeyframe(b[0].accentScale, duration: 0.14); CubicKeyframe(b[1].accentScale, duration: 0.16)
                    CubicKeyframe(b[2].accentScale, duration: 0.16); CubicKeyframe(b[3].accentScale, duration: 0.2)
                }
                KeyframeTrack(\.accentOpacity) {
                    CubicKeyframe(b[0].accentOpacity, duration: 0.14); CubicKeyframe(b[1].accentOpacity, duration: 0.16)
                    CubicKeyframe(b[2].accentOpacity, duration: 0.16); CubicKeyframe(b[3].accentOpacity, duration: 0.2)
                }
                KeyframeTrack(\.rot) {
                    CubicKeyframe(b[0].rot, duration: 0.14); CubicKeyframe(b[1].rot, duration: 0.16)
                    CubicKeyframe(b[2].rot, duration: 0.16); CubicKeyframe(b[3].rot, duration: 0.2)
                }
                KeyframeTrack(\.scale) {
                    CubicKeyframe(b[0].scale, duration: 0.14); CubicKeyframe(b[1].scale, duration: 0.16)
                    CubicKeyframe(b[2].scale, duration: 0.16); CubicKeyframe(b[3].scale, duration: 0.2)
                }
            }
                InkIconShape(glyph: glyph, direction: direction, layer: .accent, ink: ink, frame: frame, progress: draw)
                    .fill(.foreground)
                    .frame(width: size, height: size)
                    .keyframeAnimator(initialValue: IconMotion(), trigger: plays) { v, m in
                        v.scaleEffect(m.accentScale, anchor: pivot)
                            .rotationEffect(.degrees(m.accentRot), anchor: pivot)
                            .offset(x: m.accentX * size / 24, y: m.accentY * size / 24)
                            .opacity(m.accentOpacity)
                            .rotationEffect(.degrees(m.rot), anchor: .center).scaleEffect(m.scale)
                    } keyframes: { _ in
                KeyframeTrack(\.accentX) {
                    CubicKeyframe(b[0].accentX, duration: 0.14); CubicKeyframe(b[1].accentX, duration: 0.16)
                    CubicKeyframe(b[2].accentX, duration: 0.16); CubicKeyframe(b[3].accentX, duration: 0.2)
                }
                KeyframeTrack(\.accentY) {
                    CubicKeyframe(b[0].accentY, duration: 0.14); CubicKeyframe(b[1].accentY, duration: 0.16)
                    CubicKeyframe(b[2].accentY, duration: 0.16); CubicKeyframe(b[3].accentY, duration: 0.2)
                }
                KeyframeTrack(\.accentRot) {
                    CubicKeyframe(b[0].accentRot, duration: 0.14); CubicKeyframe(b[1].accentRot, duration: 0.16)
                    CubicKeyframe(b[2].accentRot, duration: 0.16); CubicKeyframe(b[3].accentRot, duration: 0.2)
                }
                KeyframeTrack(\.accentScale) {
                    CubicKeyframe(b[0].accentScale, duration: 0.14); CubicKeyframe(b[1].accentScale, duration: 0.16)
                    CubicKeyframe(b[2].accentScale, duration: 0.16); CubicKeyframe(b[3].accentScale, duration: 0.2)
                }
                KeyframeTrack(\.accentOpacity) {
                    CubicKeyframe(b[0].accentOpacity, duration: 0.14); CubicKeyframe(b[1].accentOpacity, duration: 0.16)
                    CubicKeyframe(b[2].accentOpacity, duration: 0.16); CubicKeyframe(b[3].accentOpacity, duration: 0.2)
                }
                KeyframeTrack(\.rot) {
                    CubicKeyframe(b[0].rot, duration: 0.14); CubicKeyframe(b[1].rot, duration: 0.16)
                    CubicKeyframe(b[2].rot, duration: 0.16); CubicKeyframe(b[3].rot, duration: 0.2)
                }
                KeyframeTrack(\.scale) {
                    CubicKeyframe(b[0].scale, duration: 0.14); CubicKeyframe(b[1].scale, duration: 0.16)
                    CubicKeyframe(b[2].scale, duration: 0.16); CubicKeyframe(b[3].scale, duration: 0.2)
                }
            }
            }
            .frame(width: size, height: size)
        }
        .onChange(of: selected) { _, on in
            guard on, !reduceMotion else { return }
            draw = 0
            withAnimation(.easeOut(duration: 0.42)) { draw = 1 }
            plays += 1
        }
        .onChange(of: trigger) { _, _ in if !reduceMotion { plays += 1 } }
        .accessibilityHidden(true)
    }

    /// Thicker at small sizes so 20pt stays legible; a touch heavier when selected.
    private var ink: CGFloat { (size <= 20 ? 1.95 : size >= 30 ? 1.6 : 1.75) * (selected ? 1.08 : 1) }

    private var markerWash: Color { scheme == .dark ? Color(hex: "#C9A43A").opacity(0.55) : InkPalette.butter.opacity(0.85) }

    private var b: [IconMotion] { let m = glyph.motionBeats; return m.count >= 4 ? Array(m.prefix(4)) : m + Array(repeating: IconMotion(), count: 4 - m.count) }

}

/// `-RodaIconExplore YES`: the three directions for the six key icons, at 20/24/32pt, the
/// selected state, and a mini bottom bar per direction.
struct IconExploreSheet: View {
    static let keys: [ZoenGlyph] = [.chats, .spaces, .store, .search, .plus, .send]
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(IconDirection.allCases) { d in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: d.title).font(.caption.weight(.bold)).foregroundStyle(Palette.textSecondary)
                        ForEach([20, 24, 32] as [CGFloat], id: \.self) { s in
                            row(label: "\(Int(s))") { g in InkIcon(glyph: g, direction: d, size: s) }
                        }
                        row(label: "sel") { g in InkIcon(glyph: g, direction: d, selected: true, size: 24).foregroundStyle(Palette.action) }
                        miniBar(d)
                    }
                    .padding(10)
                    .background(Palette.surface.opacity(0.6), in: .rect(cornerRadius: 14))
                }
            }
            .padding(.horizontal, 12)
            .padding(.top, 54)
            .foregroundStyle(Palette.textPrimary)
        }
        .background(Palette.background.ignoresSafeArea())
    }

    private func row(label: String, @ViewBuilder _ cell: @escaping (ZoenGlyph) -> some View) -> some View {
        HStack(spacing: 0) {
            Text(verbatim: label).font(.system(size: 9).monospaced()).foregroundStyle(Palette.textTertiary).frame(width: 26, alignment: .leading)
            ForEach(Self.keys) { g in cell(g).frame(maxWidth: .infinity).frame(height: 36) }
        }
    }

    private func miniBar(_ d: IconDirection) -> some View {
        HStack(spacing: 8) {
            InkIcon(glyph: .search, direction: d, size: 22).frame(width: 44, height: 44).glassEffect(.regular, in: .circle)
            HStack(spacing: 2) {
                HStack(spacing: 6) {
                    InkIcon(glyph: .chats, direction: d, selected: true, size: 22)
                    Text("Chats").font(.system(size: 12, weight: .bold)).fixedSize()
                }
                .foregroundStyle(Palette.action).padding(.horizontal, 12).frame(height: 40)
                InkIcon(glyph: .spaces, direction: d, size: 22).frame(maxWidth: .infinity).foregroundStyle(Palette.textSecondary)
                InkIcon(glyph: .store, direction: d, size: 22).frame(maxWidth: .infinity).foregroundStyle(Palette.textSecondary)
            }
            .padding(2).frame(height: 44).glassEffect(.regular, in: .capsule)
            InkIcon(glyph: .plus, direction: d, size: 22).foregroundStyle(.white).frame(width: 44, height: 44).background(Palette.action, in: .circle)
        }
        .padding(.top, 6)
    }
}
