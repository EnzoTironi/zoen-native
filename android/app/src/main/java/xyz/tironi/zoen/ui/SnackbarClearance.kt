package xyz.tironi.zoen.ui

import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.findRootCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot

internal class SnackbarClearance {
    private val controls = mutableStateMapOf<Any, Float>()
    val bottomPx: Float get() = controls.values.maxOrNull() ?: 0f
    fun update(owner: Any, height: Float) { controls[owner] = height }
    fun remove(owner: Any) { controls.remove(owner) }
}

internal val LocalSnackbarClearance = staticCompositionLocalOf<SnackbarClearance?> { null }

@Composable
internal fun Modifier.keepSnackbarAbove(): Modifier {
    val clearance = LocalSnackbarClearance.current ?: return this
    val owner = remember { Any() }
    DisposableEffect(clearance, owner) { onDispose { clearance.remove(owner) } }
    return onGloballyPositioned { coordinates ->
        clearance.update(owner, (coordinates.findRootCoordinates().size.height - coordinates.positionInRoot().y).coerceAtLeast(0f))
    }
}
