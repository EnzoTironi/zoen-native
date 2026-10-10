package xyz.tironi.zoen.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlinx.coroutines.delay
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ui.ink.*
import kotlin.math.*

// Geometry and timing from apple/Shared/DesignSystem/HandDrawn.swift UnboxingReveal.
@Composable
internal fun NativePetReveal(name: String, onDone: () -> Unit) {
    val motion = rememberMotionEnabled()
    val owner = LocalLifecycleOwner.current
    var active by remember(owner) { mutableStateOf(owner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) }
    var elapsed by rememberSaveable { mutableDoubleStateOf(0.0) }
    val finish by rememberUpdatedState(onDone)
    DisposableEffect(owner) {
        val observer = LifecycleEventObserver { _, _ -> active = owner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) }
        owner.lifecycle.addObserver(observer)
        onDispose { owner.lifecycle.removeObserver(observer) }
    }
    LaunchedEffect(active, motion) {
        if (!active) return@LaunchedEffect
        if (!motion) {
            elapsed = 3.2
            delay(2_200)
        } else {
            var previous = 0L
            while (elapsed < 4.4) {
                val now = withFrameNanos { it }
                if (previous != 0L) elapsed += (now - previous) / 1_000_000_000.0
                previous = now
            }
        }
        finish()
    }
    val renderer = remember { InkCanvasRenderer() }
    val t = if (motion) elapsed else 3.2
    val frame = if (motion) (t * 10).toInt() % 4 else 0
    val closeLabel = stringResource(R.string.pet_reveal_close)
    Dialog(onDismissRequest = onDone, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        BoxWithConstraints(Modifier.fillMaxSize().testTag("pet-unboxing")
            .clickable(role = Role.Button, onClickLabel = closeLabel, onClick = onDone), contentAlignment = Alignment.Center) {
            Canvas(Modifier.fillMaxSize()) {
                drawRect(Color(0xFFFFFCF5).copy(alpha = .97f))
                val rng = InkRng((9_001 + frame).toULong())
                repeat(160) {
                    val x = rng.unit().toFloat() * size.width
                    val y = rng.unit().toFloat() * size.height
                    val diameter = (.4 + rng.unit() * .8).toFloat() * density
                    drawCircle(Color(0xFF2B2530).copy(alpha = .07f), diameter / 2,
                        androidx.compose.ui.geometry.Offset(x + diameter / 2, y + diameter / 2))
                }
            }
            val side = minOf(maxWidth - 48.dp, 420.dp)
            Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp), horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(18.dp)) {
                Box(Modifier.size(side).graphicsLayer {
                    rotationZ = if (t > .9 && t < 1.55) (sin(t * 38) * .045 * sin((t - .9) / .65 * PI) * 180 / PI).toFloat() else 0f
                    transformOrigin = androidx.compose.ui.graphics.TransformOrigin(.5f, .82f)
                }) {
                    Canvas(Modifier.fillMaxSize()) { drawIntoCanvas {
                        renderer.render(revealBack(t), it.nativeCanvas, size.width.toInt(), size.height.toInt(), 41uL, frame, t / .9, 1.0)
                    } }
                    val p = 1 - exp(-5.5 * max(0.0, t - 1.75)) * cos(9 * max(0.0, t - 1.75))
                    Box(Modifier.fillMaxWidth().height(side * .58f).clipToBounds(), contentAlignment = Alignment.Center) {
                        PixelDonkey(Modifier.size(side * .4f).offset(y = side * (.21 + .12 - p * .33).toFloat())
                            .alpha(if (t > 1.75) 1f else 0f), live = t > 2.4)
                    }
                    Canvas(Modifier.fillMaxSize()) { drawIntoCanvas {
                        renderer.render(revealFront(t), it.nativeCanvas, size.width.toInt(), size.height.toInt(), 42uL, frame, t / .9, 1.0)
                    } }
                }
                Column(Modifier.alpha(((t - 2.3) / .4).coerceIn(0.0, 1.0).toFloat())
                    .offset(y = (max(0.0, 1 - (t - 2.3) / .4) * 12).dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(stringResource(R.string.pet_meet, name), Modifier.graphicsLayer { rotationZ = -3f },
                        style = MaterialTheme.typography.headlineLarge, color = Color(0xFF2B2530))
                    Text(stringResource(R.string.pet_reveal_detail), color = Color(0xFF2B2530).copy(alpha = .65f),
                        textAlign = androidx.compose.ui.text.style.TextAlign.Center)
                }
            }
        }
    }
}

private fun revealStroke(vararg p: Pair<Double, Double>, fill: InkColor? = null, width: Double = .024,
                         start: Double = 0.0, span: Double = .55, smooth: Boolean = false,
                         color: InkColor = InkPalette.ink, opacity: Double = 1.0) =
    InkStroke(p.map { InkPoint(it.first, it.second) }, closed = fill != null, fill = fill,
        width = width, start = start, span = span, smooth = smooth, color = color, opacity = opacity)

private fun revealBack(t: Double) = listOf(revealStroke(.24 to .5, .3 to .44, .7 to .44, .76 to .5,
    fill = InkColor(0x9A6A3A), width = .022, opacity = if (t > 1.5) 1.0 else 0.0))

private fun revealFront(t: Double): List<InkStroke> {
    val p = ((t - 1.5) / .45).coerceIn(0.0, 1.0)
    val angle = (1 - (1 - p).pow(3)) * 2.25
    val strokes = mutableListOf(
        revealStroke(.1 to .84, .5 to .83, .9 to .845, width = .012, smooth = true, opacity = .4),
        revealStroke(.24 to .5, .76 to .5, .76 to .82, .24 to .82, fill = InkColor(0xDDAE73), width = .028),
        revealStroke(.465 to .5, .535 to .5, .535 to .66, .465 to .66, fill = InkColor(0xF1DDAF), width = .014, start = .4, span = .3),
        revealStroke(.36 to .7, .33 to .66, .36 to .63, .39 to .66, fill = InkPalette.blush, width = .014,
            color = InkPalette.tomato, start = .6, span = .25, smooth = true),
        revealStroke(.6 to .73, .7 to .73, width = .014, start = .7, span = .2, smooth = true, opacity = .55),
        revealStroke(.6 to .77, .67 to .77, width = .014, start = .75, span = .2, smooth = true, opacity = .55),
        revealStroke(.24 to .5, .5 to .5, .5 to .47, .24 to .47, fill = InkColor(0xE8BE86), width = .022, start = .3)
            .rotated(-angle, InkPoint(.24, .5)),
        revealStroke(.76 to .5, .5 to .5, .5 to .47, .76 to .47, fill = InkColor(0xE8BE86), width = .022, start = .35)
            .rotated(angle, InkPoint(.76, .5)),
    )
    if (t > 2) repeat(9) { i ->
        val a = i / 9.0 * 2 * PI + .2
        val r = .25 + .07 * min(1.0, (t - 2) / .5)
        strokes.add(revealStroke(.5 + .25 * cos(a) to .3 + .25 * sin(a), .5 + r * cos(a) to .3 + r * sin(a),
            width = .02, span = 0.0, color = listOf(InkPalette.butter, InkPalette.tomato, InkPalette.sky)[i % 3],
            opacity = if (t > 3.4) max(0.0, 1 - (t - 3.4) / .6) else 1.0))
    }
    return strokes
}
