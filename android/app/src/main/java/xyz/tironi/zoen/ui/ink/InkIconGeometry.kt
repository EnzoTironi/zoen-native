package xyz.tironi.zoen.ui.ink

import kotlin.math.*

internal data class InkMark(val points: List<InkPoint>, val closed: Boolean, val smooth: Boolean, val weight: Double, val overshoot: Boolean)
internal data class InkBlot(val point: InkPoint, val radius: Double)
internal data class InkIconArt(val seed: ULong, val base: List<InkMark>, val accent: List<InkMark>, val blots: List<InkBlot>, val accentBlots: List<InkBlot>, val wash: List<InkMark>, val pivot: InkPoint, val beats: List<IconMotion>)
internal data class IconMotion(val accentX: Double = 0.0, val accentY: Double = 0.0, val accentRot: Double = 0.0, val accentScale: Double = 1.0, val accentOpacity: Double = 1.0, val rot: Double = 0.0, val scale: Double = 1.0) {
    fun towards(other: IconMotion, t: Double): IconMotion {
        fun mix(a: Double, b: Double) = a + (b - a) * t
        return IconMotion(mix(accentX, other.accentX), mix(accentY, other.accentY), mix(accentRot, other.accentRot), mix(accentScale, other.accentScale), mix(accentOpacity, other.accentOpacity), mix(rot, other.rot), mix(scale, other.scale))
    }
}

internal enum class IconLayer { Base, Accent, Wash }

/** Original brush geometry on the 24 point grid. Android renders the resulting filled paths. */
internal object InkIconGeometry {
    fun paths(art: InkIconArt, layer: IconLayer, ink: Double = 1.75, progress: Double = 1.0, frame: Int = 0): List<List<InkPoint>> {
        val rng = InkRng(art.seed + 6uL * 31uL + frame.toULong() * 7_919uL + when (layer) { IconLayer.Base -> 0uL; IconLayer.Accent -> 101uL; IconLayer.Wash -> 202uL })
        val out = mutableListOf<List<InkPoint>>()
        if (layer == IconLayer.Wash) {
            for (mark in art.wash) {
                val points = mark.points.map { InkPoint(it.x + .8 + rng.signed() * .28 * 1.4, it.y + .7 + rng.signed() * .28 * 1.4) }
                out += positive(if (mark.smooth) InkGeometry.catmull(points, true, 5) else points)
            }
            return out
        }
        val marks = if (layer == IconLayer.Base) art.base else art.accent
        val blots = if (layer == IconLayer.Base) art.blots else art.accentBlots
        val n = max(1, marks.size + if (blots.isEmpty()) 0 else 1)
        fun local(index: Int) = if (progress >= 1) 1.0 else ((progress - index.toDouble() / n * .55) / .5).coerceIn(0.0, 1.0)
        for ((index, mark) in marks.withIndex()) {
            val drawn = local(index)
            if (drawn <= 0) continue
            var points = mark.points.map { InkPoint(it.x + rng.signed() * .28, it.y + rng.signed() * .28) }
            if (mark.closed && mark.overshoot && points.size > 2) {
                val extra = points.take(2).map { InkPoint(it.x + rng.signed() * .28 * 1.5, it.y + rng.signed() * .28 * 1.5) }
                points = points + extra[0] + InkPoint((extra[0].x + extra[1].x) / 2, (extra[0].y + extra[1].y) / 2)
            }
            val closed = mark.closed && !mark.overshoot
            val poly = InkGeometry.trim(if (mark.smooth) InkGeometry.catmull(points, closed, 6) else densify(if (closed) points + points[0] else points), drawn)
            if (poly.size > 1) out += brush(poly, ink * mark.weight, closed && drawn >= 1, rng)
        }
        for (blot in blots) {
            val drawn = local(n - 1)
            if (drawn <= 0) continue
            val points = (0 until 9).map { i ->
                val a = i / 9.0 * 2 * PI
                val r = blot.radius * min(1.0, drawn * 1.6) * (.86 + .24 * rng.unit())
                InkPoint(blot.point.x + r * cos(a), blot.point.y + r * sin(a))
            }
            out += InkGeometry.catmull(points, true, 4)
        }
        return out
    }

    private fun densify(points: List<InkPoint>): List<InkPoint> {
        val out = mutableListOf<InkPoint>()
        for (i in 0 until points.lastIndex) {
            val a = points[i]; val b = points[i + 1]
            val n = max(1, (hypot(b.x - a.x, b.y - a.y) / 1.2).toInt())
            for (j in 0 until n) out += InkPoint(a.x + (b.x - a.x) * j / n, a.y + (b.y - a.y) * j / n)
        }
        return out + points.last()
    }

    private fun positive(points: List<InkPoint>): List<InkPoint> {
        val area = points.indices.sumOf { i -> val a = points[i]; val b = points[(i + 1) % points.size]; a.x * b.y - b.x * a.y }
        return if (area < 0) points.asReversed() else points
    }

    private fun brush(points: List<InkPoint>, width: Double, closed: Boolean, rng: InkRng): List<List<InkPoint>> {
        val k = 3 + rng.unit() * 4; val phase = rng.unit() * 6.28
        val left = mutableListOf<InkPoint>(); val right = mutableListOf<InkPoint>(); val halfWidths = mutableListOf<Double>()
        for (i in points.indices) {
            val a = points[max(0, i - if (closed && i == 0) 0 else 1)]; val b = points[min(points.lastIndex, i + 1)]
            val len = max(.0001, hypot(b.x - a.x, b.y - a.y))
            val dx = (b.x - a.x) / len; val dy = (b.y - a.y) / len
            val u = i.toDouble() / points.lastIndex
            val pressure = .85 + .3 * (.5 + .5 * sin(u * k + phase))
            val lift = if (closed) 1.0 else .78 + .22 * sin(PI * u).pow(.35)
            val h = width * .5 * pressure * lift
            halfWidths += h
            left += InkPoint(points[i].x - dy * h, points[i].y + dx * h)
            right += InkPoint(points[i].x + dy * h, points[i].y - dx * h)
        }
        val out = mutableListOf(positive(left + right.asReversed()))
        if (!closed) for ((center, h) in listOf(points.first() to halfWidths.first(), points.last() to halfWidths.last())) {
            val rx = h * (.95 + .3 * rng.unit()); val ry = h * (.85 + .25 * rng.unit())
            val jx = rng.signed() * h * .2; val jy = rng.signed() * h * .2
            out += (0 until 12).map { j -> val a = j / 12.0 * 2 * PI; InkPoint(center.x + jx + rx * cos(a), center.y + jy + ry * sin(a)) }
        }
        return out
    }

    fun motion(art: InkIconArt, milliseconds: Double): IconMotion {
        val durations = intArrayOf(140, 160, 160, 200)
        var start = IconMotion(); var remaining = milliseconds
        for (i in durations.indices) {
            val end = art.beats[i]
            if (remaining < durations[i]) {
                val t = (remaining / durations[i]).coerceIn(0.0, 1.0)
                return start.towards(end, t * t * (3 - 2 * t))
            }
            remaining -= durations[i]; start = end
        }
        return IconMotion()
    }
}
