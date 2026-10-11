package xyz.tironi.zoen.ui.ink

import kotlin.math.*
import xyz.tironi.zoen.ui.MascotMood
import xyz.tironi.zoen.ui.MascotPose

internal enum class MascotMouth { Pout, Smirk, O, Flat, Roar, Grit }

internal class MascotRig {
    var cx = 0.5; var ground = 0.86; var lift = 0.0; var squash = 0.0; var tilt = 0.0
    var look = InkPoint.Zero; var lid = 0.2; var slant = 0.5; var brow = 0.8
    var wide = false; var closed = false; var happy = false; var mouth = MascotMouth.Pout
    var armL: InkPoint? = null; var armR: InkPoint? = null
    var stepL = InkPoint.Zero; var stepR = InkPoint.Zero; var cowlick = 0.0
    var headband = false; var furCount = 64; var limbs = true; var yaw = 0.0; var wind = 0.0; var glance = 0.0
    val phi get() = yaw * PI / 2
    val rx get() = 0.205 * (1 + squash)
    val ry get() = 0.19 * (1 - squash)
    val cy get() = ground - 0.03 - ry - lift
    val top get() = cy - ry
    fun around(theta: Double, k: Double) = cx + rx * k * sin(theta + phi)
    fun facing(theta: Double) = cos(theta + phi)
    val shoulderL get() = InkPoint(around(-PI / 2, 0.9), cy + ry * 0.1)
    val shoulderR get() = InkPoint(around(PI / 2, 0.9), cy + ry * 0.1)
    val eyeL get() = InkPoint(around(-0.38, 0.98), cy - ry * 0.1)
    val eyeR get() = InkPoint(around(0.38, 0.98), cy - ry * 0.1)
    val mouthY get() = cy + ry * 0.26
}

// A direct port of the original rig, in unit coordinates. No sampled animation frames.
internal object MascotArt {
    val body = InkColor(0x6a9a36); val bodyDark = InkColor(0x4c7524); val belly = InkColor(0xa9cc6e)
    val fur = InkColor(0x3e6120); val line = InkColor(0x2b4019); val accent = InkColor(0xe0533e)
    val blush = InkColor(0xf49a9a); val iris = InkColor(0x1b2213); val mouthIn = InkColor(0x5a1f22)
    val tongue = InkColor(0xe8707a); val leaf = InkColor(0x5e9b3a)
    fun frac(x: Double) = x - floor(x)
    fun pt(x: Double, y: Double) = InkPoint(x, y)
    fun polar(o: InkPoint, r: Double, a: Double) = pt(o.x + r * cos(a), o.y + r * sin(a))

    fun curve(p: List<InkPoint>, width: Double = 0.016, color: InkColor = line, start: Double = 0.5,
              span: Double = 0.25, opacity: Double = 1.0) = InkStroke(p, width = width, color = color, start = start, span = span, opacity = opacity)
    fun blob(p: List<InkPoint>, fill: InkColor, width: Double = 0.016, color: InkColor = line,
             start: Double = 0.5, span: Double = 0.25, smooth: Boolean = true, misregister: Double = 1.0,
             opacity: Double = 1.0) = InkStroke(p, true, width, color, fill, start, span, smooth = smooth, opacity = opacity, misregister = misregister)
    fun dot(c: InkPoint, r: Double, fill: InkColor, start: Double = 0.55, opacity: Double = 1.0) =
        InkStroke.ellipse(c.x, c.y, r, r, fill = fill, width = 0.001, color = InkColor.Clear, start = start, span = 0.15, n = 8, opacity = opacity).copy(misregister = 0.2)
    fun oval(c: InkPoint, rx: Double, ry: Double, fill: InkColor?, width: Double = 0.014, color: InkColor = line,
             start: Double = 0.5, n: Int = 10, misregister: Double = 0.3, opacity: Double = 1.0) =
        InkStroke.ellipse(c.x, c.y, rx, ry, fill, width = width, color = color, start = start, span = 0.2, n = n, opacity = opacity).copy(misregister = misregister)

    fun fur(c: InkPoint, rx: Double, ry: Double, count: Int, length: Double, t: Double, seed: ULong,
            color: InkColor = fur, width: Double = 0.011, inset: Double = 0.93, start: Double = 0.1,
            opacity: Double = 1.0, lean: Double = 0.0, gravity: Double = 0.35, curl: Double = 0.5): List<InkStroke> {
        val rng = InkRng(seed)
        return List(count) { k ->
            val a = k.toDouble() / count * 2 * PI + rng.unit() * 0.09
            val nx = cos(a); val ny = sin(a); val side = if (rng.unit() < 0.5) -1 else 1
            var dx = nx - ny * lean * side; var dy = ny + nx * lean * side + gravity
            val dl = max(0.001, hypot(dx, dy)); dx /= dl; dy /= dl
            val shimmer = 1 + 0.14 * sin(t * 2.6 + k * 1.7)
            val l = length * (0.55 + 0.9 * rng.unit()) * shimmer
            val base = pt(c.x + rx * inset * nx, c.y + ry * inset * ny)
            val tip = pt(base.x + dx * l, base.y + dy * l)
            val bend = (rng.unit() - 0.5 + 0.35 * side) * l * curl
            val mid = pt((base.x + tip.x) / 2 - dy * bend, (base.y + tip.y) / 2 + dx * bend)
            InkStroke(listOf(base, mid, tip), width = width * (0.7 + 0.6 * rng.unit()), color = color,
                start = start + k.toDouble() / count * 0.3, span = 0.2, opacity = opacity, ghost = false)
        }
    }

    fun clumps(c: InkPoint, rx: Double, ry: Double, count: Int, length: Double, t: Double, seed: ULong,
               fill: InkColor, lit: InkColor? = null, ink: Boolean, inset: Double = 0.88, start: Double = 0.03,
               wind: Double = 0.0): List<InkStroke> {
        val rng = InkRng(seed); val out = arrayListOf<InkStroke>(); val half = PI / count * 1.45
        fun edge(a: Double, s: Double) = pt(c.x + rx * s * cos(a), c.y + ry * s * sin(a))
        repeat(count) { k ->
            val a = k.toDouble() / count * 2 * PI + rng.unit() * 0.14
            val nx = cos(a); val ny = sin(a)
            val trailing = if (wind == 0.0) 0.0 else max(0.0, -nx * if (wind > 0) 1 else -1) * abs(wind)
            val l = length * (0.65 + 0.7 * rng.unit()) * (if (ny < -0.5) 1.15 else 1.0) * (1 + 0.6 * trailing)
            val lean = (rng.unit() - 0.5) * 0.9
            var dx = nx - ny * lean - wind * 0.8; var dy = ny + nx * lean + 0.32
            val dl = max(0.001, hypot(dx, dy)); dx /= dl; dy /= dl
            val sway = sin(t * 1.9 + k * 0.9) * 0.006
            val root = edge(a, inset); val tip = pt(root.x + dx * l - dy * sway, root.y + dy * l + dx * sway)
            val bl = edge(a - half, inset); val br = edge(a + half, inset)
            val ml = pt((bl.x + tip.x) / 2 + nx * l * 0.14, (bl.y + tip.y) / 2 + ny * l * 0.14)
            val mr = pt((br.x + tip.x) / 2 + nx * l * 0.1, (br.y + tip.y) / 2 + ny * l * 0.1)
            out.add(blob(listOf(bl, ml, tip, mr, br, root), if (lit != null && ny < -0.15 && nx < 0.45) lit else fill,
                width = 0.001, color = InkColor.Clear, start = start + k.toDouble() / count * 0.25, span = 0.15, misregister = 0.0))
            if (ink) {
                val shadow = max(0.0, ny * 0.6 + nx * 0.4)
                val flank = if (rng.unit() < 0.5) ml else mr
                val from = pt((flank.x * 2 + root.x) / 3, (flank.y * 2 + root.y) / 3)
                out.add(InkStroke(listOf(from, flank, tip), width = 0.0055 + 0.004 * shadow,
                    color = line.opacity(0.55 + 0.35 * shadow), start = start + 0.1 + k.toDouble() / count * 0.25, span = 0.15, ghost = false))
            }
        }
        return out
    }

    fun nub(c: InkPoint, rx: Double, ry: Double, t: Double, seed: ULong, start: Double = 0.45): List<InkStroke> {
        val s = arrayListOf(oval(c, rx, ry, body, width = 0.001, color = InkColor.Clear, start = start, misregister = 0.0))
        s.addAll(fur(c, rx, ry, 16, 0.016, t, seed, color = body, width = 0.018, inset = 0.8, start = start, lean = 0.5))
        s.add(oval(pt(c.x - rx * 0.2, c.y - ry * 0.25), rx * 0.45, ry * 0.35, belly.opacity(0.4), width = 0.001, color = InkColor.Clear, start = start, n = 8, misregister = 0.0))
        s.addAll(fur(c, rx, ry, 20, 0.012, t, seed + 5uL, color = line.opacity(0.75), width = 0.006, inset = 0.98, start = start, lean = 0.8, gravity = 0.2))
        return s
    }

    fun leaf(c: InkPoint, size: Double, angle: Double, start: Double = 0.6, opacity: Double = 1.0): List<InkStroke> {
        val p = listOf(pt(-1.0, 0.0), pt(-0.4, -0.32), pt(0.45, -0.22), pt(1.0, 0.0), pt(0.45, 0.2), pt(-0.4, 0.3)).map { pt(c.x + it.x * size, c.y + it.y * size) }
        val rib = curve(listOf(pt(c.x - size * 0.9, c.y), pt(c.x + size * 0.85, c.y - size * 0.02)), width = 0.007, start = start + 0.1, span = 0.1, opacity = 0.6 * opacity)
        return listOf(blob(p, leaf, width = 0.009, start = start, span = 0.15, misregister = 0.4, opacity = opacity), rib).map { it.rotated(angle, c) }
    }

    private fun eye(p: InkPoint, r: MascotRig, left: Boolean): List<InkStroke> {
        if (r.closed) {
            val d = if (r.happy) -0.022 else 0.02
            val edge = if (r.happy) 0.008 else 0.0
            return listOf(curve(listOf(pt(p.x - 0.04, p.y + edge), pt(p.x, p.y + d), pt(p.x + 0.04, p.y + edge)), width = 0.017, start = 0.4))
        }
        val w = if (r.wide) 0.062 else 0.056; val h = if (r.wide) 0.066 else 0.06
        val out = arrayListOf(oval(p, w, h, InkColor(0xf4f6ec), width = 0.008, color = line.opacity(0.8), start = 0.35, n = 12, misregister = 0.2))
        val ir = if (r.wide) 0.04 else 0.047
        val ix = p.x + r.look.x * (w - ir) * 0.9 + if (left) 0.003 else -0.003
        val iy = p.y + r.look.y * (h - ir) * 0.8 + 0.003
        out.add(oval(pt(ix, iy), ir, ir * 1.05, InkColor(0x56634a), width = 0.005, start = 0.4, misregister = 0.1))
        out.add(oval(pt(ix, iy + 0.002), ir * 0.8, ir * 0.84, iris, width = 0.001, color = InkColor.Clear, start = 0.42, misregister = 0.0))
        out.add(oval(pt(ix - ir * 0.15, iy + ir * 0.45), ir * 0.5, ir * 0.22, InkColor(0x6e7f5c).opacity(0.55), width = 0.001, color = InkColor.Clear, start = 0.43, n = 8, misregister = 0.0))
        out.add(dot(pt(ix + ir * 0.32, iy - ir * 0.38), ir * 0.3, InkColor.White))
        out.add(dot(pt(ix - ir * 0.05, iy - ir * 0.52), ir * 0.11, InkColor.White, opacity = 0.9))
        out.add(dot(pt(ix - ir * 0.42, iy + ir * 0.36), ir * 0.12, InkColor.White, opacity = 0.75))
        val lid = if (r.lid >= 0.99) 1.0 else r.lid * 0.45
        if (lid > 0.02) {
            val s = if (left) 1 else -1
            fun ly(x: Double) = p.y - h * 1.1 + 2.2 * h * min(1.0, lid) + r.slant * (if (lid >= 1) 0.0 else 0.6) * (x - p.x) * s * 0.55
            val points = (0..10).map { k ->
                val a = PI + k.toDouble() / 10 * PI; val x = p.x + w * 1.15 * cos(a)
                pt(x, min(p.y + h * 1.15 * sin(a), ly(x)))
            } + listOf(pt(p.x + w * 1.15, ly(p.x + w * 1.15)), pt(p.x - w * 1.15, ly(p.x - w * 1.15)))
            out.add(blob(points, body, width = 0.001, color = InkColor.Clear, start = 0.45, span = 0.15, smooth = false, misregister = 0.0))
            out.add(curve(listOf(pt(p.x - w * 1.02, ly(p.x - w * 1.02)), pt(p.x, ly(p.x) + 0.003), pt(p.x + w * 1.02, ly(p.x + w * 1.02))),
                width = if (lid >= 1) 0.012 else 0.006, color = if (lid >= 1) line else bodyDark, start = 0.48, span = 0.15))
        }
        if (r.brow > 0.02) {
            val s = if (left) 1 else -1; val by = p.y - h - 0.008 + if (r.wide) -0.016 else 0.0
            repeat(5) { k ->
                val x0 = p.x - w * 1.05 + k * w * 0.44; val y0 = by + r.brow * 0.026 * (x0 - p.x) / w * s
                out.add(curve(listOf(pt(x0, y0 + 0.004), pt(x0 + w * 0.2, y0 - 0.004), pt(x0 + w * 0.46, y0 + r.brow * 0.012 * s + 0.004)),
                    width = 0.017, color = if (k % 2 == 0) bodyDark else fur, start = 0.5, span = 0.15, opacity = 0.95))
            }
        }
        return out
    }

    private fun mouth(r: MascotRig): List<InkStroke> {
        val x = r.cx; val y = r.mouthY
        return when (r.mouth) {
            MascotMouth.Pout -> listOf(curve(listOf(pt(x - 0.036, y + 0.014), pt(x - 0.014, y - 0.005), pt(x + 0.014, y - 0.005), pt(x + 0.036, y + 0.014)), start = 0.55))
            MascotMouth.Smirk -> listOf(curve(listOf(pt(x - 0.026, y), pt(x + 0.004, y + 0.012), pt(x + 0.03, y - 0.006)), width = 0.015, start = 0.55))
            MascotMouth.O -> listOf(oval(pt(x, y + 0.004), 0.014, 0.017, mouthIn, width = 0.011, start = 0.55, n = 9, misregister = 0.1))
            MascotMouth.Flat -> listOf(curve(listOf(pt(x - 0.026, y + 0.004), pt(x + 0.026, y + 0.002)), width = 0.015, start = 0.55))
            MascotMouth.Grit -> listOf(
                blob(listOf(pt(x - 0.036, y - 0.006), pt(x + 0.036, y - 0.006), pt(x + 0.032, y + 0.014), pt(x - 0.032, y + 0.014)), InkColor.White, width = 0.012, start = 0.55, misregister = 0.1),
                curve(listOf(pt(x - 0.034, y + 0.004), pt(x + 0.034, y + 0.004)), width = 0.008, start = 0.6, span = 0.1),
                curve(listOf(pt(x - 0.008, y - 0.006), pt(x - 0.008, y + 0.014)), width = 0.007, start = 0.6, span = 0.1),
                curve(listOf(pt(x + 0.012, y - 0.006), pt(x + 0.012, y + 0.014)), width = 0.007, start = 0.6, span = 0.1))
            MascotMouth.Roar -> {
                val w = 0.07; val top = y - 0.035; val bot = y + 0.06
                val s = arrayListOf(blob(listOf(pt(x - w, top + 0.004), pt(x - w * 0.4, top - 0.006), pt(x + w * 0.4, top - 0.006), pt(x + w, top + 0.004), pt(x + w * 0.8, bot - 0.02), pt(x, bot), pt(x - w * 0.8, bot - 0.02)), mouthIn, start = 0.55, misregister = 0.15))
                s.add(blob(listOf(pt(x - w * 0.6, bot - 0.022), pt(x, bot - 0.04), pt(x + w * 0.6, bot - 0.022), pt(x, bot - 0.004)), tongue, width = 0.001, color = InkColor.Clear, start = 0.6, misregister = 0.0))
                for ((fx, dir) in listOf(x - w * 0.62 to 1, x + w * 0.62 to 1, x - w * 0.5 to -1, x + w * 0.5 to -1)) {
                    val base = if (dir > 0) top + 0.002 else bot - 0.012
                    s.add(blob(listOf(pt(fx - 0.011, base), pt(fx + 0.011, base), pt(fx, base + dir * 0.026)), InkColor.White, width = 0.008, start = 0.65, span = 0.1, smooth = false, misregister = 0.0))
                }
                s.add(blob(listOf(pt(x - w * 0.42, top), pt(x + w * 0.42, top), pt(x + w * 0.38, top + 0.012), pt(x - w * 0.38, top + 0.012)), InkColor.White, width = 0.006, start = 0.62, span = 0.1, smooth = false, misregister = 0.0))
                s
            }
        }
    }

    fun hop(w: Double, height: Double): Pair<Double, Double> {
        val h = abs(sin(w)); val k = min(1.5, height / 0.03)
        return h * height to ((1 - h).pow(6) * 0.12 - h * 0.05) * k
    }
    fun turn(target: Double, t: Double, glanceEvery: Double? = null, glanceTo: Double = 0.12): Pair<Double, Double> {
        var y = target * (1 - (1 - min(1.0, t / 0.9)).pow(3)); var g = 0.0
        if (glanceEvery != null) {
            val p = frac(t / glanceEvery)
            if (t > 1.2 && p > 0.74) { g = sin((p - 0.74) / 0.26 * PI); y += (glanceTo - y) * g }
        }
        return y to g
    }
    fun blinking(t: Double): Boolean {
        val p = frac(t / 3.9)
        return p > 0.955 || (frac(t / 7.8) > 0.5 && p > 0.9 && p < 0.925)
    }

    fun draw(r: MascotRig, t: Double, behind: List<InkStroke> = emptyList(), held: List<InkStroke> = emptyList(), world: List<InkStroke> = emptyList()): List<InkStroke> {
        val s = arrayListOf<InkStroke>()
        if (r.limbs) {
            val shadow = max(0.45, 1 - r.lift * 6)
            s.add(InkStroke.ellipse(r.cx, r.ground + 0.004, 0.17 * shadow, 0.022 * shadow, line.opacity(0.14), width = 0.001, color = InkColor.Clear, span = 0.3))
        }
        s.addAll(behind)
        val c = arrayListOf<InkStroke>(); val center = pt(r.cx, r.cy)
        if (r.limbs) {
            listOf(r.stepL, r.stepR).forEachIndexed { i, step ->
                val f = pt(r.cx + 0.085 * sin((if (i == 0) -PI / 2 else PI / 2) + r.phi) + step.x, r.ground - 0.02 + step.y)
                c.addAll(nub(f, 0.052, 0.034, t, (300 + i).toULong(), start = 0.05))
            }
            listOf(r.armL, r.armR).forEachIndexed { i, target ->
                if (r.facing(if (i == 0) -PI / 2 else PI / 2) < -0.35) {
                    val sh = if (i == 0) r.shoulderL else r.shoulderR
                    var tgt = target ?: pt(sh.x, sh.y + 0.03)
                    val dx = tgt.x - sh.x; val dy = tgt.y - sh.y; val d = hypot(dx, dy)
                    if (d > 0.11) tgt = pt(sh.x + dx / d * 0.11, sh.y + dy / d * 0.11)
                    c.addAll(nub(tgt, 0.044, 0.038, t, (400 + i).toULong(), start = 0.05).map { it.faded(0.92) })
                }
            }
        }
        val n = r.furCount
        c.addAll(clumps(center, r.rx, r.ry, n * 5 / 8, 0.046, t + 0.7, 61uL, bodyDark, ink = false, inset = 0.9, start = 0.0, wind = r.wind))
        c.add(oval(center, r.rx * 0.95, r.ry * 0.95, body, width = 0.001, color = InkColor.Clear, start = 0.0, n = 18, misregister = 0.0))
        c.addAll(clumps(center, r.rx, r.ry, n * 3 / 4, 0.036, t, 62uL, body, lit = InkColor(0x7dad47), ink = true, inset = 0.86, start = 0.02, wind = r.wind))
        c.add(oval(pt(r.cx + r.rx * 0.12, r.cy + r.ry * 0.2), r.rx * 0.86, r.ry * 0.78, bodyDark.opacity(0.32), width = 0.001, color = InkColor.Clear, start = 0.1, n = 14, misregister = 0.0))
        c.add(oval(pt(r.cx - r.rx * 0.22, r.cy - r.ry * 0.42), r.rx * 0.5, r.ry * 0.3, belly.opacity(0.38), width = 0.001, color = InkColor.Clear, start = 0.15, n = 12, misregister = 0.0))
        val rng = InkRng(911uL)
        repeat(n * 3 / 2) {
            val a = rng.unit() * 2 * PI; val d = 0.15 + 0.8 * sqrt(rng.unit())
            val p0 = pt(r.cx + r.rx * d * cos(a), r.cy + r.ry * d * sin(a))
            val len = 0.012 + 0.012 * rng.unit(); val ang = a * 0.35 + PI / 2 + (rng.unit() - 0.5) * 0.9
            val lit = sin(a) < -0.2 && cos(a) < 0.4
            c.add(InkStroke(listOf(p0, polar(p0, len * 0.5, ang + 0.25), polar(p0, len, ang)), width = 0.006, color = if (lit) belly else bodyDark, start = 0.2, span = 0.2, opacity = if (lit) 0.6 else 0.28, ghost = false))
        }
        c.addAll(fur(center, r.rx, r.ry, n, 0.02, t, 17uL, color = line.opacity(0.6), width = 0.006, inset = 1.02, start = 0.05, lean = 0.75, gravity = 0.3, curl = 0.8))
        val irng = InkRng(733uL)
        repeat(n / 3) { k ->
            val a = irng.unit() * 2 * PI; val d = 0.35 + 0.45 * irng.unit()
            val p0 = pt(r.cx + r.rx * d * cos(a), r.cy + r.ry * d * sin(a))
            if (!(abs(p0.y - r.eyeL.y) < 0.07 && abs(p0.x - r.cx) < r.rx * 0.7) && !(abs(p0.y - r.mouthY) < 0.05 && abs(p0.x - r.cx) < 0.07)) {
                val sw = sin(t * 1.7 + k) * 0.003; val tip = pt(p0.x + 0.006 + sw, p0.y + 0.026)
                c.add(curve(listOf(pt(p0.x - 0.008, p0.y), pt(p0.x - 0.002, p0.y + 0.014), tip), width = 0.006, color = bodyDark, start = 0.22, span = 0.15, opacity = 0.55))
                c.add(curve(listOf(pt(p0.x + 0.008, p0.y - 0.002), pt(p0.x + 0.006, p0.y + 0.012), tip), width = 0.005, color = bodyDark, start = 0.23, span = 0.15, opacity = 0.4))
            }
        }
        c.addAll(fur(center, r.rx, r.ry, n / 2, 0.02, t + 1.3, 29uL, color = bodyDark, width = 0.01, inset = 0.86, start = 0.2, opacity = 0.5, lean = 0.6))
        repeat(3) { k ->
            val base = pt(r.cx - 0.012 + k * 0.014 + r.rx * 0.25 * sin(r.phi), r.top + 0.012)
            val a = -PI / 2 + (k - 1) * 0.35 + r.cowlick
            val tip = polar(base, 0.05 + if (k == 1) 0.012 else 0.0, a)
            val mid = polar(base, 0.028, a - r.cowlick * 0.5 - (k - 1) * 0.1)
            c.add(InkStroke(listOf(base, mid, tip), width = 0.016, color = body, start = 0.3, span = 0.2, ghost = false))
            c.add(InkStroke(listOf(base, mid, tip), width = 0.008, color = line, start = 0.32, span = 0.2, ghost = false))
        }
        if (r.headband) {
            val y0 = r.top + r.ry * 0.36
            val p = listOf(pt(r.cx - r.rx * 0.98, y0 + 0.022), pt(r.cx - r.rx * 0.5, y0 - 0.004), pt(r.cx, y0 - 0.012), pt(r.cx + r.rx * 0.5, y0 - 0.004), pt(r.cx + r.rx * 0.98, y0 + 0.022))
            c.add(InkStroke(p.map { pt(it.x, it.y - 0.016) }, width = 0.022, color = accent, start = 0.35, span = 0.2, ghost = false))
            c.add(InkStroke(p, width = 0.018, color = InkColor.White, start = 0.38, span = 0.2, ghost = false))
            c.add(InkStroke(p.map { pt(it.x, it.y + 0.016) }, width = 0.022, color = accent, start = 0.4, span = 0.2, ghost = false))
            val ks = if (r.yaw < -0.15) -1 else 1; val knot = pt(r.cx - ks * r.rx * 0.96, y0 + 0.02)
            repeat(2) { k ->
                val flap = sin(t * 16 + k * 1.4) * 0.018
                c.add(curve(listOf(knot, pt(knot.x - ks * 0.04, knot.y + 0.01 + flap), pt(knot.x - ks * (0.075 + 0.02 * abs(r.wind)), knot.y + 0.004 + k * 0.02 - flap)), width = 0.016, color = accent, start = 0.5, span = 0.15))
            }
        }
        fun squeeze(strokes: List<InkStroke>, x0: Double, k: Double, dx: Double = 0.0) = strokes.map { s0 -> s0.copy(points = s0.points.map { pt(x0 + dx + (it.x - x0) * k, it.y) }) }
        for ((side, e) in listOf(-1.0 to r.eyeL, 1.0 to r.eyeR)) {
            val f = r.facing(side * 0.38) / cos(0.38)
            if (f > 0.2) c.addAll(squeeze(eye(e, r, side < 0), e.x, min(1.0, f)))
        }
        for ((side, e) in listOf(-1.0 to r.eyeL, 1.0 to r.eyeR)) {
            val f = r.facing(side * 0.78)
            if (f > 0.22) {
                val k = min(1.0, f / cos(0.78)); val bx = r.around(side * 0.78, 0.86); val by = e.y + 0.075
                repeat(3) { j -> c.add(oval(pt(bx + (j - 1) * 0.013 * k, by + (j % 2) * 0.006), 0.026 * k, 0.017, blush.opacity(0.4), width = 0.001, color = InkColor.Clear, n = 8, misregister = 0.0)) }
            }
        }
        val mf = r.facing(0.0)
        if (mf > 0.25) c.addAll(squeeze(mouth(r), r.cx, mf, dx = r.around(0.0, 0.95) - r.cx))
        c.addAll(held)
        if (r.limbs) listOf(r.armL, r.armR).forEachIndexed { i, target ->
            if (r.facing(if (i == 0) -PI / 2 else PI / 2) >= -0.35) {
                val sh = if (i == 0) r.shoulderL else r.shoulderR
                var tgt = target ?: pt(sh.x + if (i == 0) -0.03 else 0.03, sh.y + 0.03)
                val dx = tgt.x - sh.x; val dy = tgt.y - sh.y; val d = hypot(dx, dy)
                if (d > 0.11) tgt = pt(sh.x + dx / d * 0.11, sh.y + dy / d * 0.11)
                c.addAll(nub(tgt, 0.044, 0.038, t, (400 + i).toULong(), start = 0.5))
            }
        }
        s.addAll(if (r.tilt == 0.0) c else c.map { it.rotated(r.tilt, pt(r.cx, r.ground)) })
        s.addAll(world)
        return s
    }

    fun strokes(pose: MascotPose, t: Double): List<InkStroke> = MascotPoses.strokes(pose, t)
    fun head(t: Double, mood: MascotMood): List<InkStroke> {
        if (mood == MascotMood.Working) return workingHead(t)
        val r = MascotRig().apply {
            limbs = false; furCount = 40; mouth = if (mood == MascotMood.Smirk) MascotMouth.Smirk else MascotMouth.Pout
            squash = sin(t * 2) * 0.02; cowlick = 0.12 * sin(t * 2 - 1)
            if (blinking(t)) lid = 1.0
        }
        return frameHead(draw(r, t), pt(r.cx, r.cy), 0.54)
    }
    private fun frameHead(strokes: List<InkStroke>, center: InkPoint, y: Double): List<InkStroke> = strokes.map { s ->
        s.copy(points = s.points.map { pt(0.5 + (it.x - center.x) * 1.85, y + (it.y - center.y) * 1.85) }, width = s.width * 1.85 * 0.8)
    }
    private fun workingHead(t: Double): List<InkStroke> {
        val r = MascotRig().apply {
            limbs = false; furCount = 40; mouth = MascotMouth.Flat; lid = 0.45; slant = 0.3; brow = 0.5
            look = pt(0.0, 0.7); squash = sin(t * 6) * 0.008; cowlick = 0.18 * sin(t * 3)
            if (blinking(t)) lid = 1.0
        }
        val out = frameHead(draw(r, t), pt(r.cx, r.cy), 0.5).toMutableList()
        val nod = sin(t * 4.4) * 0.01
        out.add(curve(listOf(pt(0.13, 0.5), pt(0.18, 0.2 + nod), pt(0.5, 0.06 + nod), pt(0.82, 0.2 + nod), pt(0.87, 0.5)), width = 0.05, start = 0.0, span = 0.3))
        out.add(InkStroke.box(0.05, 0.4 + nod, 0.14, 0.24, r = 0.4, fill = InkPalette.tomato, width = 0.03, color = line, start = 0.1, span = 0.2))
        out.add(InkStroke.box(0.81, 0.4 + nod, 0.14, 0.24, r = 0.4, fill = InkPalette.tomato, width = 0.03, color = line, start = 0.1, span = 0.2))
        out.add(blob(listOf(pt(0.2, 0.98), pt(0.8, 0.98), pt(0.74, 0.76), pt(0.26, 0.76)), InkColor(0xc9ced6), width = 0.03, start = 0.2, span = 0.2, smooth = false))
        out.add(dot(pt(0.5, 0.87), 0.03, InkColor.White, start = 0.3, opacity = 0.6 + 0.4 * abs(sin(t * 9))))
        repeat(2) { i -> if (sin(t * 12 + i * 2) > 0.3) {
            val x = if (i == 0) 0.16 else 0.84
            out.add(curve(listOf(pt(x, 0.74), pt(x + if (i == 0) -0.04 else 0.04, 0.68)), width = 0.02, start = 0.4, span = 0.1, opacity = 0.7))
        } }
        return out
    }
}
