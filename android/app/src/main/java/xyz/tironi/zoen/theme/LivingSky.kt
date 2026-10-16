package xyz.tironi.zoen.theme

import android.provider.Settings
import androidx.compose.foundation.Canvas
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.*
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.delay
import java.time.ZonedDateTime
import kotlin.math.sin

/** Permission-free civil-time palette. Smoothstep gives continuous values and slopes. */
internal object SkyClock {
    val hours = listOf(0.0, 6.0, 9.0, 12.0, 17.0, 20.0, 24.0)
    private val colors = listOf(Color(0xFF333B75), Color(0xFFD67A87), Color(0xFF61ABDE), Color(0xFF459EE6), Color(0xFFF09C4D), Color(0xFF8257A1), Color(0xFF333B75))
    fun sample(hour: Double): Pair<Color, Float> {
        val h = if (hour.isFinite()) (hour % 24 + 24) % 24 else 12.0
        val i = (0..5).first { h < hours[it + 1] }
        val t = ((h - hours[i]) / (hours[i + 1] - hours[i])).toFloat()
        val s = t * t * (3 - 2 * t)
        val stars = listOf(1f, 0f, 0f, 0f, 0f, .4f, 1f)
        return lerp(colors[i], colors[i + 1], s) to (stars[i] + (stars[i + 1] - stars[i]) * s)
    }
}

/** Set previewHour from a debug preview, or debug shared preference sky_hour (string). */
@Composable
fun LivingSky(modifier: Modifier = Modifier, previewHour: Double? = null) {
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var now by remember { mutableStateOf(ZonedDateTime.now()) }
    var reduceMotion by remember { mutableStateOf(false) }
    LaunchedEffect(lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.RESUMED) {
            while (true) {
                reduceMotion = Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
                now = ZonedDateTime.now()
                delay(if (reduceMotion) 60_000 else 1_000)
            }
        }
    }
    val override = if (xyz.tironi.zoen.BuildConfig.DEBUG) previewHour ?: context.getSharedPreferences("zoen", 0).getString("sky_hour", null)?.toDoubleOrNull() else null
    val hour = override ?: (now.hour + now.minute / 60.0 + now.second / 3600.0)
    val (tint, stars) = SkyClock.sample(hour)
    val base = MaterialTheme.colorScheme.background
    val dark = base.luminance() < .5f
    Canvas(modifier) {
        drawRect(Brush.verticalGradient(listOf(lerp(base, tint, .08f), base)))
        val time = if (reduceMotion || override != null) 0.0 else now.toEpochSecond().toDouble()
        repeat(5) { i ->
            val x = size.width * i / 4 + sin(time / 90 + i).toFloat() * 9
            val y = size.height * (.10f + i % 3 * .12f)
            val cloud = Path().apply { moveTo(x - 65, y); cubicTo(x - 30, y - 24, x + 20, y + 16, x + 70, y + 3) }
            drawPath(cloud, Color.White.copy(alpha = if (dark) .025f else .22f), style = Stroke(13f, cap = StrokeCap.Round))
        }
        repeat(36) { i ->
            val p = Offset(size.width * ((i * 73 + 19) % 997) / 997, size.height * ((i * 137 + 31) % 631) / 1000)
            val ink = (if (dark) Color.White else Color(0xFF525E80)).copy(alpha = stars * if (dark) .08f else .025f)
            drawLine(ink, p - Offset(2f, 0f), p + Offset(2f, .5f), .8f)
            drawLine(ink, p - Offset(0f, 2f), p + Offset(.5f, 2f), .8f)
        }
    }
}
