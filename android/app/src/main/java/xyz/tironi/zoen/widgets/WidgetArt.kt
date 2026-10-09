package xyz.tironi.zoen.widgets

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import xyz.tironi.zoen.miniapps.GlobePoint
import xyz.tironi.zoen.miniapps.OrthographicGlobe
import xyz.tironi.zoen.ui.drawPixelDonkey
import kotlin.math.hypot

internal object WidgetArt {
    private val cached = mutableMapOf<String, Bitmap>()
    @Synchronized fun bitmap(context: Context, art: String?): Bitmap? {
        if (art !in setOf("pet", "pet.asleep", "pet.gone", "globe")) return null
        return cached.getOrPut(checkNotNull(art)) {
            val bitmap = Bitmap.createBitmap(192, 162, Bitmap.Config.ARGB_8888)
            val canvas = Canvas(bitmap)
            val paint = Paint(Paint.ANTI_ALIAS_FLAG)
            if (art == "globe") {
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
                    paint.color = color.toInt(); paint.alpha = if (art == "pet.gone") 110 else (color ushr 24).toInt()
                    canvas.drawRect(x * 6f, y * 6f, (x + w) * 6f, (y + h) * 6f, paint)
                }
            }
            bitmap
        }
    }
}
