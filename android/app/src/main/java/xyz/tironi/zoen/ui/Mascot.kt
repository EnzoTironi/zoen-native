package xyz.tironi.zoen.ui

import android.provider.Settings
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Brush
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

enum class MascotPose { Wave, Phone, Map, Run, Juggle, Walk, Cheer }

@Composable
fun ZoenMascot(modifier: Modifier = Modifier, animated: Boolean = false, pose: MascotPose = MascotPose.Wave, headOnly: Boolean = false) {
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
        val unit = if (headOnly) min(size.width / 124f, size.height / 124f) else min(size.width / 160f, size.height / 170f)
        val origin = Offset((size.width - 160 * unit) / 2, (size.height - 170 * unit) / 2)
        val phase = if (animated && !reduced) frame / 12f else 0f
        val bounce = sin(phase * 2.5f) * 1.5f
        scale(unit, unit, Offset.Zero) {
            val ox = origin.x / unit
            val oy = origin.y / unit
            fun p(x: Float, y: Float) = Offset(ox + x, oy + y + bounce)
            val ink = Color(0xFF2B4019)
            val fur = Color(0xFF6A9A36)
            if (!headOnly) {
                drawOval(Color(0x1A3D542A), p(29f, 145f), Size(105f, 11f))
                drawLine(ink, p(55f, 125f), p(49f, 144f), 6f, StrokeCap.Round)
                drawLine(ink, p(107f, 125f), p(114f, 144f), 6f, StrokeCap.Round)
                drawLine(ink, p(38f, 98f), p(23f, if (pose == MascotPose.Wave || pose == MascotPose.Cheer) 72f else 106f), 6f, StrokeCap.Round)
                drawLine(ink, p(124f, 98f), p(136f, if (pose == MascotPose.Cheer) 72f else 108f), 6f, StrokeCap.Round)
            }
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
            drawPath(body, Brush.radialGradient(listOf(Color(0xFFA9CC6E), fur, Color(0xFF4C7524)), center = p(63f, 67f), radius = 85f))
            drawPath(body, Color(0xFF3E6120), style = Stroke(1.6f, cap = StrokeCap.Round))
            for (i in 0 until 88) {
                val angle = i * 2.39996f
                val r = sqrt((i + .5f) / 88) * 47f
                val x = 80 + cos(angle) * r
                val y = 85 + sin(angle) * r
                val shimmer = sin(i * 1.9f + phase) * .7f
                drawLine(if (i % 3 == 0) Color(0x33A9CC6E) else Color(0x333E6120), p(x, y), p(x + 1.6f + shimmer, y - 2.3f), .9f, StrokeCap.Round)
            }
            drawLine(ink, p(74f, 35f), p(70f, 27f), 1.8f, StrokeCap.Round)
            drawLine(ink, p(79f, 35f), p(82f, 25f), 1.8f, StrokeCap.Round)
            val blink = animated && !reduced && frame % 51L in 47..49
            for (x in listOf(57f, 98f)) {
                if (blink || pose == MascotPose.Cheer) drawArc(ink, 200f, 140f, false, p(x - 10, 75f), Size(20f, 12f), style = Stroke(2.4f, cap = StrokeCap.Round))
                else {
                    drawOval(Color(0xFFA9CC6E), p(x - 12, 63f), Size(24f, 30f))
                    drawOval(Color(0xFF1B2213), p(x - 10, 66f), Size(20f, 25f))
                    drawCircle(Color.White.copy(alpha = .9f), 3f, p(x - 3, 71f))
                    drawCircle(Color.White.copy(alpha = .45f), 1.3f, p(x + 4, 84f))
                }
            }
            drawLine(ink, p(43f, 60f), p(67f, 56f), 3f, StrokeCap.Round)
            drawLine(ink, p(87f, 56f), p(112f, 60f), 3f, StrokeCap.Round)
            drawOval(Color(0xFFEFAAA2), p(35f, 91f), Size(20f, 10f))
            drawOval(Color(0xFFEFAAA2), p(107f, 91f), Size(20f, 10f))
            val mouth = Path().apply { moveTo(p(72f, 106f).x, p(72f, 106f).y); quadraticTo(p(80f, if (pose == MascotPose.Cheer) 116f else 102f).x, p(80f, if (pose == MascotPose.Cheer) 116f else 102f).y, p(88f, 106f).x, p(88f, 106f).y) }
            drawPath(mouth, ink, style = Stroke(2.4f, cap = StrokeCap.Round))
            if (headOnly) return@scale
            if (pose == MascotPose.Map) {
                drawRoundRect(Color(0xFFFFFCF5), p(109f, 89f), Size(40f, 34f), cornerRadius = androidx.compose.ui.geometry.CornerRadius(3f))
                drawLine(Color(0xFF6A9A36), p(122f, 89f), p(122f, 123f), 1.5f)
                drawLine(Color(0xFF6A9A36), p(135f, 89f), p(135f, 123f), 1.5f)
                drawCircle(Color(0xFFE0533E), 2.8f, p(141f, 99f))
            }
            if (pose == MascotPose.Run) drawLine(Color(0xFFE0533E), p(40f, 57f), p(119f, 57f), 4.5f, StrokeCap.Round)
            if (pose == MascotPose.Phone) {
                drawRoundRect(ink, p(121f, 84f), Size(21f, 37f), cornerRadius = androidx.compose.ui.geometry.CornerRadius(4f))
                drawRoundRect(Color(0xFF9CCBFF), p(124f, 89f), Size(15f, 24f), cornerRadius = androidx.compose.ui.geometry.CornerRadius(2f))
                drawCircle(Color(0xFFFFFCF5), 1.5f, p(131.5f, 117f))
            }
            if (pose == MascotPose.Juggle) for (i in 0..2) {
                drawRoundRect(listOf(Color(0xFFF7CE5B), Color(0xFF9CCBFF), Color(0xFFFFB8AA))[i], p(48f + i * 23, 9f + (i % 2) * 9), Size(11f, 11f), cornerRadius = androidx.compose.ui.geometry.CornerRadius(2f))
            }
            if (pose == MascotPose.Walk) {
                drawCircle(Color(0xFFE0533E), 7f, p(80f, 13f))
                drawLine(Color(0xFFE0533E), p(74f, 17f), p(80f, 25f), 3f, StrokeCap.Round)
                drawLine(Color(0xFFE0533E), p(86f, 17f), p(80f, 25f), 3f, StrokeCap.Round)
                drawCircle(Color(0xFFFFFCF5), 3f, p(80f, 13f))
            }
            if (pose == MascotPose.Cheer) for (i in 0..9) {
                val x = 12f + i * 14f
                drawLine(if (i % 2 == 0) Color(0xFFE4AC5C) else fur, p(x, 14f + (i % 3) * 8), p(x + 2, 22f + (i % 3) * 8), 3f, StrokeCap.Round)
            }
        }
    }
}
