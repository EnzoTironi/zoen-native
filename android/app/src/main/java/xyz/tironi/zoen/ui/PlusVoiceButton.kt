package xyz.tironi.zoen.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.*

@Composable
internal fun PlusVoiceButton(actions: () -> Unit, start: () -> Unit, release: () -> Unit, lock: () -> Unit, cancel: () -> Unit, compact: Boolean = false, fanActions: List<FanAction> = emptyList()) {
    val haptics = rememberZoenHaptics()
    val density = LocalDensity.current.density
    var holding by remember { mutableStateOf(false) }
    var touching by remember { mutableStateOf(false) }
    var open by remember { mutableStateOf(false) }
    var hovered by remember { mutableStateOf<Int?>(null) }
    val currentStart by rememberUpdatedState(start)
    val currentRelease by rememberUpdatedState(release)
    val currentLock by rememberUpdatedState(lock)
    val currentCancel by rememberUpdatedState(cancel)
    val currentItems by rememberUpdatedState(fanActions)
    val dark = MaterialTheme.colorScheme.background.luminance() < .5f
    val tint = if (holding) MaterialTheme.colorScheme.error else if (dark) MaterialTheme.colorScheme.primary else Color(0xEB1D2418)
    val expansion by animateFloatAsState(if (open) 1f else 0f, spring(dampingRatio = .72f, stiffness = 420f), label = "fan spring")
    val morph by animateFloatAsState(if (holding) 1f else 0f, spring(dampingRatio = .75f, stiffness = 500f), label = "plus microphone morph")
    val press by animateFloatAsState(if (holding) 1.18f else if (touching) .9f else 1f, spring(dampingRatio = .8f, stiffness = 800f), label = "press spring")
    Box(Modifier.size(56.dp), contentAlignment = Alignment.Center) {
        if (open && !touching) AppleActionFan(currentItems.map { item -> item.copy(choose = { open = false; item.choose() }) }, onDismiss = { open = false })
        if (expansion > .001f && touching) {
            currentItems.forEachIndexed { index, item ->
                val angle = PI / 2 + index * PI / 2 / (currentItems.size - 1).coerceAtLeast(1)
                NativeIconButton(onClick = { open = false; item.choose() }, modifier = Modifier.offset((cos(angle) * fanRadius(currentItems.size) * expansion).dp, (-sin(angle) * fanRadius(currentItems.size) * expansion).dp).size(56.dp).graphicsLayer { alpha = expansion; scaleX = if (hovered == index) 1.15f else 1f; scaleY = scaleX }) {
                    Box(Modifier.size(48.dp).zoenGlass(CircleShape), contentAlignment = Alignment.Center) { Icon(item.icon, item.title, Modifier.size(22.dp)) }
                }
            }
            hovered?.let { index -> currentItems.getOrNull(index)?.let { Text(it.title, Modifier.offset(y = (-fanRadius(currentItems.size) - 48).dp), style = MaterialTheme.typography.labelMedium) } }
        }
        // This owner never leaves the hierarchy during the morph: its up event sends the take.
        Box(Modifier.size(56.dp).graphicsLayer { scaleX = press; scaleY = press }.zoenGlass(CircleShape, tint).testTag("home-plus").semantics {
            contentDescription = "Create and more"
            onClick { open = !open; true }
            customActions = listOf(CustomAccessibilityAction("Record audio for Zoen") { currentStart(); lock(); true })
        }.pointerInput(density) {
            coroutineScope {
            val timerScope = this
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                val wasOpen = open
                touching = true; open = true
                val gesture = FanGestureState(currentItems.size, fanRadius(currentItems.size))
                val voiceDrag = VoiceHoldDragState()
                    val arm = timerScope.launch { delay(700); if (gesture.arm()) { holding = true; open = false; currentStart() } }
                    try {
                        while (true) {
                            val event = awaitPointerEvent()
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            // A consumed/cancelled stream is not a normal finger-up/send.
                            // finally cancels a recording still owned by this gesture.
                            if (change.isConsumed) break
                            if (!change.pressed) {
                                arm.cancel()
                                if (voiceDrag.settled) { change.consume(); break }
                                when (val result = gesture.release()) {
                                    FanRelease.Voice -> { holding = false; currentRelease() }
                                    FanRelease.Tap -> { open = !wasOpen; if (currentItems.isEmpty()) actions() }
                                    FanRelease.Dismiss -> open = false
                                    is FanRelease.Choose -> { open = false; currentItems.getOrNull(result.index)?.choose() }
                                }
                                change.consume(); break
                            }
                            if (gesture.ownsVoice) {
                                when (voiceDrag.move((change.position.x - down.position.x) / density, (change.position.y - down.position.y) / density)) {
                                    VoiceHoldDragResult.Cancel -> { holding = false; currentCancel() }
                                    VoiceHoldDragResult.Lock -> { holding = false; currentLock() }
                                    null -> Unit
                                }
                                change.consume()
                                continue
                            }
                            gesture.move((change.position.x / density) - 28, (change.position.y / density) - 28)
                            hovered = gesture.hovered
                            if (gesture.dragging) arm.cancel()
                            change.consume()
                        }
                    } finally { arm.cancel(); touching = false; hovered = null; if (holding) { holding = false; currentCancel() } }
            }
            }
        }, contentAlignment = Alignment.Center) {
            val ink = if (dark && !holding) Color(0xFF10200A) else Color.White
            Canvas(Modifier.size(25.dp)) {
                val c = center; val r = size.width * .32f
                // Interpolated strokes converge into the microphone stem and capsule.
                drawLine(ink, Offset(c.x-r*(1-morph), c.y), Offset(c.x+r*(1-morph), c.y), 2.dp.toPx() * (1-morph), StrokeCap.Round)
                drawLine(ink, Offset(c.x, c.y-r), Offset(c.x, c.y+r), 2.dp.toPx(), StrokeCap.Round)
                if (morph > .001f) {
                    drawRoundRect(ink.copy(alpha = morph), Offset(c.x-r*.4f, c.y-r), androidx.compose.ui.geometry.Size(r*.8f, r*1.3f), androidx.compose.ui.geometry.CornerRadius(r*.4f), style = Stroke(2.dp.toPx()))
                    drawArc(ink.copy(alpha = morph), 0f, 180f, false, Offset(c.x-r*.8f, c.y-r*.3f), androidx.compose.ui.geometry.Size(r*1.6f, r*1.4f), style = Stroke(2.dp.toPx(), cap = StrokeCap.Round))
                }
            }
        }
    }
}
