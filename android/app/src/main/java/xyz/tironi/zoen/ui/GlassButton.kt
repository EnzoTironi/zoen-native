package xyz.tironi.zoen.ui

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonColors
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.Color
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.unit.dp

/** iOS glass surface with Material interaction/accessibility and shared haptics. */
@Composable
fun GlassButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    prominent: Boolean = true,
    shape: Shape = CircleShape,
    colors: ButtonColors? = null,
    interactionSource: MutableInteractionSource? = null,
    contentPadding: PaddingValues = PaddingValues(horizontal = 24.dp, vertical = 10.dp),
    content: @Composable RowScope.() -> Unit,
) {
    val interactions = interactionSource ?: remember { MutableInteractionSource() }
    val pressed by interactions.collectIsPressedAsState()
    val haptics = rememberZoenHaptics()
    LaunchedEffect(pressed) { if (pressed && enabled) haptics.perform(ZoenFeedback.Tap) }

    val glassModifier = modifier.zoenGlass(shape, if (prominent) MaterialTheme.colorScheme.primary.copy(alpha = 0.82f) else null)
    if (prominent) {
        Button(
            onClick = onClick,
            modifier = glassModifier,
            enabled = enabled,
            shape = shape,
            colors = colors ?: ButtonDefaults.buttonColors(containerColor = Color.Transparent, disabledContainerColor = Color.Transparent),
            interactionSource = interactions,
            contentPadding = contentPadding,
            content = content,
        )
    } else {
        OutlinedButton(
            onClick = onClick,
            modifier = glassModifier,
            enabled = enabled,
            shape = shape,
            colors = colors ?: ButtonDefaults.outlinedButtonColors(containerColor = Color.Transparent, disabledContainerColor = Color.Transparent),
            interactionSource = interactions,
            contentPadding = contentPadding,
            content = content,
        )
    }
}

/** Compatibility wrapper for Material3's outlined button. */
@Composable
fun GlassOutlinedButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    shape: Shape = CircleShape,
    colors: ButtonColors? = null,
    interactionSource: MutableInteractionSource? = null,
    contentPadding: PaddingValues = PaddingValues(horizontal = 24.dp, vertical = 10.dp),
    content: @Composable RowScope.() -> Unit,
) = GlassButton(onClick, modifier, enabled, prominent = false, shape, colors, interactionSource, contentPadding, content)
