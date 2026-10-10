package xyz.tironi.zoen.widgets

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import xyz.tironi.zoen.miniapps.GlobePoint
import xyz.tironi.zoen.miniapps.OrthographicGlobe
import xyz.tironi.zoen.ui.drawPixelDonkey
import xyz.tironi.zoen.ui.ink.DoodleArt
import xyz.tironi.zoen.ui.ink.InkCanvasRenderer
import kotlin.math.hypot

internal object WidgetArt {
    private val cached = mutableMapOf<String, Bitmap>()
    @Synchronized fun bitmap(context: Context, art: String?): Bitmap? {
        val doodle = DoodleArt.entries.find { it.name.equals(art, true) }
        if (doodle == null && art !in setOf("pet", "pet.asleep", "pet.gone", "globe")) return null
        return cached.getOrPut(checkNotNull(art)) {
            val bitmap = Bitmap.createBitmap(192, 162, Bitmap.Config.ARGB_8888)
            val canvas = Canvas(bitmap)
            val paint = Paint(Paint.ANTI_ALIAS_FLAG)
            if (doodle != null) {
                InkCanvasRenderer().render(doodle.strokes(4.0), canvas, bitmap.width, bitmap.height,
                    doodle.seed, 0, 2.0, 1.0)
            } else if (art == "globe") {
                val center = GlobePoint(25.0, 10.0)
                val circle = Path().apply { addCircle(96f, 81f, 74f, Path.Direction.CW) }
                paint.color = 0xFF234756.toInt(); canvas.drawPath(circle, paint); canvas.clipPath(circle)
                val contours = context.assets.open("world/land110.json").bufferedReader().use { OrthographicGlobe.land(it.readText()) }
                paint.color = 0xFF99B697.toInt()
                contours.forEach { contour ->
                    val projected = contour.map { OrthographicGlobe.project(it, center) }
                    if (projected.any { it.z >= 0 }) {
                        val path = Path()
                        projected.forEachIndexed { index, point ->
                            val divisor = if (point.z < 0) hypot(point.x, point.y).coerceAtLeast(1e-9) else 1.0
                            val x = (96 + point.x / divisor * 74).toFloat(); val y = (81 - point.y / divisor * 74).toFloat()
                            if (index == 0) path.moveTo(x, y) else path.lineTo(x, y)
                        }
                        path.close(); canvas.drawPath(path, paint)
                    }
                }
            } else {
                drawPixelDonkey(art == "pet.asleep") { x, y, w, h, color ->
                    paint.color = color.toInt(); paint.alpha = if (art == "pet.gone") 89 else (color ushr 24).toInt()
                    canvas.drawRect(6 + x * 6f, 15 + y * 6f, 6 + (x + w) * 6f + .3f, 15 + (y + h) * 6f + .3f, paint)
                }
            }
            bitmap
        }
    }
}
