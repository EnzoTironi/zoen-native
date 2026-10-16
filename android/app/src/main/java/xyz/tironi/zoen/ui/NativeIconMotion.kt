package xyz.tironi.zoen.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf

internal fun hasNewPendingRequest(previous: Set<String>, current: Set<String>): Boolean =
    (current - previous).isNotEmpty()

internal data class NativeSymbolSignals(
    val selection: Any? = null,
    val pendingIds: Set<String> = emptySet(),
    val active: Boolean = false,
)
internal val LocalNativeSymbolSignals = staticCompositionLocalOf { NativeSymbolSignals() }

/** Propagate real state to the glyph's AnimatedVectorDrawable. Never animate its
 * container: text, badges and touch targets must stay still with native symbols. */
@Composable
internal fun NativeIconMotion(
    selection: Any? = null,
    pendingIds: Set<String> = emptySet(),
    active: Boolean = false,
    content: @Composable () -> Unit,
) {
    val parent = LocalNativeSymbolSignals.current
    CompositionLocalProvider(LocalNativeSymbolSignals provides NativeSymbolSignals(
        parent.selection to selection, parent.pendingIds + pendingIds, parent.active || active
    ), content = content)
}
