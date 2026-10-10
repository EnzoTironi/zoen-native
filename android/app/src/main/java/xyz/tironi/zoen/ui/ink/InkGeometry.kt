package xyz.tironi.zoen.ui.ink

import kotlin.math.*

internal data class InkPoint(val x: Double, val y: Double) {
    companion object { val Zero = InkPoint(0.0, 0.0) }
}

internal data class InkColor(val rgb: Int, val alpha: Double = 1.0) {
    fun opacity(value: Double) = copy(alpha = alpha * value)
    companion object {
        val Clear = InkColor(0, 0.0)
        val White = InkColor(0xffffff)
    }
}

internal object InkPalette {
    val ink = InkColor(0x2b2530)
    val paper = InkColor(0xfffcf5)
    val tomato = InkColor(0xf07052)
    val butter = InkColor(0xf7ce5b)
    val mint = InkColor(0x5cc79e)
    val sky = InkColor(0x9ccbff)
    val lilac = InkColor(0xc3b2ff)
    val blush = InkColor(0xffb8aa)
}

// The same SplitMix64 sequence as apple/Shared/DesignSystem/HandDrawn.swift.
internal class InkRng(seed: ULong) {
    private var state = seed * 0x2545F4914F6CDD1DuL + 0x9E3779B97F4A7C15uL
    fun unit(): Double {
        state += 0x9E3779B97F4A7C15uL
        var z = state
        z = (z xor (z shr 30)) * 0xBF58476D1CE4E5B9uL
        z = (z xor (z shr 27)) * 0x94D049BB133111EBuL
        z = z xor (z shr 31)
        return (z shr 11).toDouble() / 9_007_199_254_740_992.0
    }
    fun signed() = unit() * 2 - 1
}

internal data class InkStroke(
    val points: List<InkPoint>, val closed: Boolean = false, val width: Double = 0.024,
    val color: InkColor = InkPalette.ink, val fill: InkColor? = null,
    val start: Double = 0.0, val span: Double = 0.55, val wobble: Double = 1.0,
    val smooth: Boolean = true, val opacity: Double = 1.0, val ghost: Boolean = true,
    val misregister: Double = 1.0,
) {
    fun rotated(angle: Double, around: InkPoint): InkStroke {
        val c = cos(angle); val s = sin(angle)
        return copy(points = points.map {
            val dx = it.x - around.x; val dy = it.y - around.y
            InkPoint(around.x + dx * c - dy * s, around.y + dx * s + dy * c)
        })
    }
    fun faded(value: Double) = copy(opacity = opacity * value)

    companion object {
        fun ellipse(cx: Double, cy: Double, rx: Double, ry: Double, fill: InkColor? = null,
                    closed: Boolean = true, width: Double = 0.024, color: InkColor = InkPalette.ink,
                    start: Double = 0.0, span: Double = 0.55, n: Int = 12, opacity: Double = 1.0): InkStroke {
            val count = if (closed) n else (n * 1.1).toInt() + 1
            return InkStroke(List(count) { i ->
                val a = -PI * 0.6 + i.toDouble() / n * 2 * PI
                InkPoint(cx + rx * cos(a), cy + ry * sin(a))
            }, closed, width, color, fill, start, span, opacity = opacity)
        }
        fun box(x: Double, y: Double, w: Double, h: Double, r: Double = 0.12,
                fill: InkColor? = null, closed: Boolean = true, width: Double = 0.024,
                color: InkColor = InkPalette.ink, start: Double = 0.0, span: Double = 0.55,
                opacity: Double = 1.0): InkStroke {
            val rr = min(w, h) * r
            val corners = listOf(Triple(x + w - rr, y + rr, -PI / 2), Triple(x + w - rr, y + h - rr, 0.0),
                Triple(x + rr, y + h - rr, PI / 2), Triple(x + rr, y + rr, PI))
            val points = corners.flatMap { (cx, cy, a0) -> (0..2).map { k ->
                val a = a0 + k.toDouble() / 2 * PI / 2
                InkPoint(cx + rr * cos(a), cy + rr * sin(a))
            } }.toMutableList()
            if (!closed) { points.add(points[0]); points.add(points[1]) }
            return InkStroke(points, closed, width, color, fill, start, span, opacity = opacity)
        }
    }
}

internal object InkGeometry {
    fun catmull(p: List<InkPoint>, closed: Boolean, steps: Int = 8): List<InkPoint> {
        val n = p.size
        if (n <= 2) return if (closed && n == 2) p + p[0] else p
        fun at(i: Int) = p[if (closed) ((i % n) + n) % n else i.coerceIn(0, n - 1)]
        val out = ArrayList<InkPoint>(n * steps + 1)
        for (i in 0 until if (closed) n else n - 1) {
            val a = at(i - 1); val b = at(i); val c = at(i + 1); val d = at(i + 2)
            repeat(steps) { s ->
                val t = s.toDouble() / steps
                out.add(InkPoint(spline(a.x, b.x, c.x, d.x, t), spline(a.y, b.y, c.y, d.y, t)))
            }
        }
        out.add(if (closed) p[0] else p[n - 1])
        return out
    }

    private fun spline(a: Double, b: Double, c: Double, d: Double, t: Double) =
        0.5 * (2 * b + (-a + c) * t + (2 * a - 5 * b + 4 * c - d) * t * t + (-a + 3 * b - 3 * c + d) * t * t * t)

    fun linear(p: List<InkPoint>, closed: Boolean): List<InkPoint> {
        if (!closed || p.isEmpty()) return p
        val ring = p + p[0]
        return (0 until ring.lastIndex).flatMap { i -> (0..3).map { s ->
            val t = s.toDouble() / 4
            InkPoint(ring[i].x + (ring[i + 1].x - ring[i].x) * t, ring[i].y + (ring[i + 1].y - ring[i].y) * t)
        } } + p[0]
    }

    fun trim(p: List<InkPoint>, f: Double): List<InkPoint> {
        if (f >= 1 || p.size <= 1) return p
        val target = (1 until p.size).sumOf { i -> hypot(p[i].x - p[i - 1].x, p[i].y - p[i - 1].y) } * f
        var acc = 0.0
        val out = arrayListOf(p[0])
        for (i in 1 until p.size) {
            val d = hypot(p[i].x - p[i - 1].x, p[i].y - p[i - 1].y)
            if (acc + d >= target) {
                val t = if (d > 0) (target - acc) / d else 0.0
                out.add(InkPoint(p[i - 1].x + (p[i].x - p[i - 1].x) * t, p[i - 1].y + (p[i].y - p[i - 1].y) * t))
                return out
            }
            acc += d; out.add(p[i])
        }
        return out
    }

    fun ribbon(p: List<InkPoint>, width: Double, tapered: Boolean, rng: InkRng): List<InkPoint> {
        val k = 5 + rng.unit() * 7; val phase = rng.unit() * 6.28
        val left = ArrayList<InkPoint>(p.size); val right = ArrayList<InkPoint>(p.size)
        p.indices.forEach { i ->
            val a = p[max(0, i - 1)]; val b = p[min(p.lastIndex, i + 1)]
            val length = max(0.0001, hypot(b.x - a.x, b.y - a.y))
            val dx = (b.x - a.x) / length; val dy = (b.y - a.y) / length
            val u = if (p.size > 1) i.toDouble() / p.lastIndex else 0.0
            val pressure = 0.82 + 0.3 * (0.5 + 0.5 * sin(u * k + phase))
            val taper = if (tapered) 0.3 + 0.7 * max(0.0, sin(PI * u)).pow(0.45) else 1.0
            val h = width * 0.5 * pressure * taper
            left.add(InkPoint(p[i].x - dy * h, p[i].y + dx * h))
            right.add(InkPoint(p[i].x + dy * h, p[i].y - dx * h))
        }
        return left + right.asReversed()
    }
}
