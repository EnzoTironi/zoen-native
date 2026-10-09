package xyz.tironi.zoen.ui

import android.provider.Settings
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import kotlin.math.*
import kotlinx.coroutines.delay

@Composable
fun ZoenMascot(modifier: Modifier = Modifier, animated: Boolean = false, pose: Int = 0) {
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val reduced = Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    var frame by remember { mutableLongStateOf(0) }
    LaunchedEffect(animated, reduced, lifecycle) {
        if (animated && !reduced) lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            while (true) { delay(83); frame++ }
        }
    }
    Canvas(modifier.semantics { contentDescription = "Zoen" }) {
        val unit = min(size.width / 160f, size.height / 170f)
        val origin = Offset((size.width - 160 * unit) / 2, (size.height - 170 * unit) / 2)
        val phase = if (animated && !reduced) frame / 12f else 0f
        val bounce = sin(phase * 2.5f) * 1.5f
        scale(unit, unit, Offset.Zero) {
            val ox = origin.x / unit
            val oy = origin.y / unit
            fun p(x: Float, y: Float) = Offset(ox + x, oy + y + bounce)
            val ink = Color(0xFF253A1C)
            val fur = Color(0xFF8BB961)
            drawOval(Color(0x1A3D542A), p(29f, 145f), Size(105f, 11f))
            drawLine(ink, p(55f, 125f), p(49f, 144f), 6f, StrokeCap.Round)
            drawLine(ink, p(107f, 125f), p(114f, 144f), 6f, StrokeCap.Round)
            drawLine(ink, p(38f, 98f), p(17f, if (pose == 0) 62f else 106f), 6f, StrokeCap.Round)
            drawLine(ink, p(124f, 98f), p(141f, if (pose == 6) 61f else 108f), 6f, StrokeCap.Round)
            val body = Path().apply {
                for (i in 0..90) {
                    val angle = (i * 2 * PI / 90).toFloat()
                    val jitter = sin(i * 2.8f + floor(phase * 12) * .13f) * 1.2f
                    val r = 53f + sin(i * 1.9f) * 2.8f + jitter
                    val point = p(80 + cos(angle) * r, 85 + sin(angle) * r * .97f)
                    if (i == 0) moveTo(point.x, point.y) else lineTo(point.x, point.y)
                }
                close()
            }
            drawPath(body, fur)
            drawPath(body, ink, style = Stroke(2.2f, cap = StrokeCap.Round))
            drawOval(Color(0x2267A435), p(36f, 41f), Size(85f, 83f), style = Stroke(3f))
            drawLine(ink, p(74f, 35f), p(66f, 20f), 2.5f, StrokeCap.Round)
            drawLine(ink, p(79f, 35f), p(82f, 17f), 2.5f, StrokeCap.Round)
            val blink = animated && !reduced && frame % 51L in 47..49
            for (x in listOf(57f, 98f)) {
                if (blink) drawLine(ink, p(x - 11, 77f), p(x + 11, 78f), 3f, StrokeCap.Round)
                else {
                    drawOval(Color(0xFFFFFEF5), p(x - 13, 59f), Size(26f, 33f))
                    drawOval(ink, p(x - 5, 66f), Size(15f, 23f))
                    drawCircle(Color.White, 3.3f, p(x, 70f))
                    drawOval(ink, p(x - 13, 59f), Size(26f, 33f), style = Stroke(1.8f))
                }
            }
            drawLine(ink, p(43f, 60f), p(67f, 56f), 3f, StrokeCap.Round)
            drawLine(ink, p(87f, 56f), p(112f, 60f), 3f, StrokeCap.Round)
            drawOval(Color(0xFFEFAAA2), p(35f, 91f), Size(20f, 10f))
            drawOval(Color(0xFFEFAAA2), p(107f, 91f), Size(20f, 10f))
            val mouth = Path().apply { moveTo(p(69f, 104f).x, p(69f, 104f).y); quadraticTo(p(80f, if (pose == 6) 118f else 112f).x, p(80f, if (pose == 6) 118f else 112f).y, p(91f, 104f).x, p(91f, 104f).y) }
            drawPath(mouth, ink, style = Stroke(2.4f, cap = StrokeCap.Round))
            if (pose == 1) {
                drawRoundRect(Color(0xFFFFEED0), p(16f, 110f), Size(126f, 26f), cornerRadius = androidx.compose.ui.geometry.CornerRadius(3f))
                drawLine(Color(0xFFB2A787), p(51f, 110f), p(51f, 136f), 1.5f)
                drawLine(Color(0xFFB2A787), p(99f, 110f), p(99f, 136f), 1.5f)
            }
            if (pose == 3) {
                drawRoundRect(Color(0xFFF9EDD7), p(67f, 114f), Size(28f, 26f), cornerRadius = androidx.compose.ui.geometry.CornerRadius(5f))
                drawArc(ink, 180f, 180f, false, p(72f, 106f), Size(18f, 19f), style = Stroke(2.5f))
            }
            if (pose == 6) for (i in 0..9) {
                val x = 12f + i * 14f
                drawLine(if (i % 2 == 0) Color(0xFFE4AC5C) else fur, p(x, 14f + (i % 3) * 8), p(x + 2, 22f + (i % 3) * 8), 3f, StrokeCap.Round)
            }
        }
    }
}
