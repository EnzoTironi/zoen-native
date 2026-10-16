package xyz.tironi.zoen.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.currentStateAsState
import kotlinx.coroutines.delay

internal fun hasNewPendingRequest(previous: Set<String>, current: Set<String>): Boolean =
    (current - previous).isNotEmpty()

/** Android approximation of native symbol effects; timing awaits measured native frames.
 * Keys are application state, never externally incremented animation counters. */
@Composable
internal fun NativeIconMotion(
    selection: Any? = null,
    pendingIds: Set<String> = emptySet(),
    active: Boolean = false,
    content: @Composable () -> Unit,
) {
    val enabled = rememberMotionEnabled()
    val lifecycle by LocalLifecycleOwner.current.lifecycle.currentStateAsState()
    val running = enabled && lifecycle.isAtLeast(Lifecycle.State.RESUMED)
    val translation = remember { Animatable(0f) }
    val rotation = remember { Animatable(0f) }
    val opacity = remember { Animatable(1f) }
    var previousSelection by remember { mutableStateOf(selection) }
    var previousPending by remember { mutableStateOf(pendingIds) }
    LaunchedEffect(selection, running) {
        val changed = selection != previousSelection
        previousSelection = selection
        translation.snapTo(0f)
        if (running && changed) {
            translation.animateTo(-4f, tween(100))
            translation.animateTo(0f, tween(167))
        }
    }
    LaunchedEffect(pendingIds, running) {
        val added = hasNewPendingRequest(previousPending, pendingIds)
        previousPending = pendingIds
        rotation.snapTo(0f)
        if (running && added) {
            for (angle in listOf(-12f, 12f, -8f, 8f, 0f)) rotation.animateTo(angle, tween(70))
        }
    }
    LaunchedEffect(active, running) {
        opacity.snapTo(1f)
        while (active && running) {
            opacity.animateTo(.45f, tween(400))
            opacity.animateTo(1f, tween(400))
            delay(100)
        }
    }
    Box(Modifier.graphicsLayer {
        translationY = translation.value * density
        rotationZ = rotation.value
        alpha = opacity.value
    }) { content() }
}
