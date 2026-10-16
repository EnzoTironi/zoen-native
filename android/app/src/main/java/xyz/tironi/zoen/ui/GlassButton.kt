package xyz.tironi.zoen.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.ButtonColors
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

/**
 * Zoen's shared marble button. API 33+ uses the original rounded-lens optics in
 * ZoenGlass.kt, matching the Apple material's lighting; older devices use gradients.
 *
 * Drop-in for Material's Button / OutlinedButton: same onClick, modifier, enabled,
 * colors (container colour tints the lens) and interactionSource.
 */
@Composable
fun GlassButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    prominent: Boolean = true,
    @Suppress("UNUSED_PARAMETER") shape: Shape = CircleShape,
    colors: ButtonColors? = null,
    interactionSource: MutableInteractionSource? = null,
    contentPadding: PaddingValues = PaddingValues(horizontal = 24.dp, vertical = 10.dp),
    content: @Composable RowScope.() -> Unit,
) {
    val interactions = interactionSource ?: remember { MutableInteractionSource() }
    val pressed by interactions.collectIsPressedAsState()
    val focused by interactions.collectIsFocusedAsState()
    val reduce = !rememberMotionEnabled()
    val haptics = rememberZoenHaptics()
    LaunchedEffect(pressed) { if (pressed && enabled) haptics.perform(ZoenFeedback.Tap) }

    val held = pressed && enabled
    val press by animateFloatAsState(if (held) 1f else 0f,
        if (reduce) tween(150) else if (held) wabiSpring(0.34f, 0.92f) else wabiSpring(1.2f, 0.8f), label = "glass-press")
    val tint = colors?.containerColor?.takeIf { prominent || it != Color.Transparent } ?: MaterialTheme.colorScheme.primary
    val ink = when {
        colors != null && (prominent || colors.containerColor.alpha > 0f) -> colors.contentColor
        prominent -> MaterialTheme.colorScheme.onPrimary
        else -> MaterialTheme.colorScheme.onSurface
    }
    val dark = MaterialTheme.colorScheme.surface.luminance() < 0.5f

    Box(
        modifier
            .graphicsLayer {
                val s = if (reduce) 1f else 1f - 0.04f * press             // better-ui: 0.96
                scaleX = s; scaleY = s
                rotationX = if (reduce) 0f else 10f * press                 // Wabi: rotate3d(1,0,0,25deg)
                cameraDistance = 8 * density
                alpha = if (enabled) 1f else 0.45f
            }
            .marbleShadow(CircleShape, held, slab = !prominent)
            .zoenGlass(tint, prominent, dark, press = { press }, slab = !prominent,
                film = if (prominent) null else colors?.containerColor)
            .then(if (focused) Modifier.border(2.dp, MaterialTheme.colorScheme.primary, CircleShape) else Modifier)
            .clickable(interactions, indication = null, enabled = enabled, role = Role.Button, onClick = onClick)
            .defaultMinSize(minWidth = 64.dp, minHeight = 46.dp)
            .padding(contentPadding),
        contentAlignment = Alignment.Center,
    ) {
        CompositionLocalProvider(
            LocalContentColor provides ink,
            LocalTextStyle provides MaterialTheme.typography.labelLarge.copy(fontWeight = FontWeight.SemiBold),
        ) {
            Row(horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically, content = content)
        }
    }
}

/** The clear lens, for secondary actions (Material's OutlinedButton). */
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


/** Wabi's SwiftUI-style spring (response, damping ratio): drag .34/.92, release 1.2/.8. */
private fun wabiSpring(response: Float, damping: Float) =
    spring<Float>(dampingRatio = damping, stiffness = (2f * Math.PI.toFloat() / response).let { it * it })
