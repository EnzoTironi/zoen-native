package xyz.tironi.zoen.ui

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonColors
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier

/** Supplies the real click event to native symbols without moving the hit target,
 * badge, avatar or surrounding text. Disabled buttons cannot produce a trigger. */
@Composable
internal fun NativeIconButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    colors: IconButtonColors = IconButtonDefaults.iconButtonColors(),
    interactionSource: MutableInteractionSource? = null,
    content: @Composable () -> Unit,
) {
    var clicks by remember { mutableIntStateOf(0) }
    val parent = LocalNativeSymbolSignals.current
    IconButton(onClick = { clicks++; onClick() }, modifier = modifier, enabled = enabled,
        colors = colors, interactionSource = interactionSource) {
        CompositionLocalProvider(LocalNativeSymbolSignals provides parent.copy(selection = parent.selection to clicks), content = content)
    }
}
