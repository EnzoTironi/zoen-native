package xyz.tironi.zoen.ui.ink

import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import kotlin.math.*

internal class InkCanvasRenderer {
    private val path = Path()
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND }

    private fun draw(canvas: Canvas, points: List<InkPoint>, color: InkColor, opacity: Double, closed: Boolean = true, strokeWidth: Double? = null) {
        if (points.isEmpty() || color.alpha * opacity <= 0) return
        path.rewind(); path.moveTo(points[0].x.toFloat(), points[0].y.toFloat())
        for (i in 1 until points.size) path.lineTo(points[i].x.toFloat(), points[i].y.toFloat())
        if (closed) path.close()
        paint.color = color.rgb or ((255 * (color.alpha * opacity).coerceIn(0.0, 1.0)).roundToInt() shl 24)
        paint.style = if (strokeWidth == null) Paint.Style.FILL else Paint.Style.STROKE
        paint.strokeWidth = (strokeWidth ?: 0.0).toFloat()
        canvas.drawPath(path, paint)
    }

    fun render(strokes: List<InkStroke>, canvas: Canvas, width: Int, height: Int, seed: ULong,
               frame: Int, progress: Double, jitter: Double) {
        val side = min(width, height).toDouble(); val ox = (width - side) / 2; val oy = (height - side) / 2
        strokes.forEachIndexed { i, s ->
            if (s.opacity <= 0.01) return@forEachIndexed
            val local = if (s.span <= 0) 1.0 else ((progress - s.start) / s.span).coerceIn(0.0, 1.0)
            if (local <= 0) return@forEachIndexed
            val rng = InkRng(seed + i.toULong() * 1_000_003uL + frame.toULong() * 7_919uL)
            val amp = side * 0.0055 * jitter * s.wobble
            val points = s.points.map { InkPoint(ox + it.x * side + rng.signed() * amp, oy + it.y * side + rng.signed() * amp) }
            val poly = if (s.smooth) InkGeometry.catmull(points, s.closed) else InkGeometry.linear(points, s.closed)
            s.fill?.let { fill ->
                val a = ((local - 0.45) / 0.4).coerceIn(0.0, 1.0)
                if (a > 0) {
                    val dx = (side * 0.011 + rng.signed() * side * 0.003) * s.misregister
                    val dy = (side * 0.009 + rng.signed() * side * 0.003) * s.misregister
                    draw(canvas, poly.map { InkPoint(it.x + dx, it.y + dy) }, fill, a * s.opacity)
                }
            }
            if (s.width <= 0) return@forEachIndexed
            val drawn = InkGeometry.trim(poly, local)
            if (drawn.size <= 1) return@forEachIndexed
            val w = max(0.9, s.width * side)
            draw(canvas, InkGeometry.ribbon(drawn, w, !s.closed || local < 1, rng), s.color, s.opacity)
            if (s.ghost && w > 1.4) draw(canvas, drawn.map { InkPoint(it.x + side * 0.005, it.y - side * 0.004) }, s.color,
                0.22 * s.opacity, closed = false, strokeWidth = max(0.5, w * 0.28))
        }
    }
}
