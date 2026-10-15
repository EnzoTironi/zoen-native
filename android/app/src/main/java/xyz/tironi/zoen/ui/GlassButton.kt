package xyz.tironi.zoen.ui

import android.graphics.RuntimeShader
import android.os.Build
import androidx.annotation.RequiresApi
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
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
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.ShaderBrush
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

/**
 * Zoen's one button material: Wabi's glass, ported from wabi.ai's production
 * `.glass-btn` (design-reference-rea-20261014/wabi-static/wabi-page-css.css), the same
 * five layers as apple/Shared/DesignSystem/WabiGlass.metal. API 33+ draws it with an AGSL
 * RuntimeShader; older devices get the same layers from gradients.
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
    val view = LocalView.current
    val reduce = remember(view) { reduceMotion(view) }
    val haptics = rememberZoenHaptics()
    LaunchedEffect(pressed) { if (pressed && enabled) haptics.perform(ZoenFeedback.Tap) }

    val held = pressed && enabled
    val press by animateFloatAsState(if (held) 1f else 0f,
        if (reduce) tween(150) else spring(dampingRatio = 0.6f, stiffness = Spring.StiffnessMediumLow), label = "glass-press")
    val tint = colors?.containerColor?.takeIf { prominent || it != Color.Transparent } ?: MaterialTheme.colorScheme.primary
    val ink = when {
        colors != null && prominent -> colors.contentColor
        prominent -> Color.White
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
            .shadow(if (held) 2.dp else 6.dp, CircleShape, clip = false,
                ambientColor = (if (prominent) tint else Color.Black).copy(alpha = 0.18f),
                spotColor = (if (prominent) tint else Color.Black).copy(alpha = 0.22f))
            .drawBehind { drawGlass(prominent, tint, dark, rim = -75f - 50f * press, shine = 0.25f * press) }
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

private fun Color.luminance(): Float = 0.2126f * red + 0.7152f * green + 0.0722f * blue

private const val WABI_GLASS = """
uniform float2 size;
uniform float rimAngle;
uniform float shine;
uniform float prominent;

float capsuleDistance(float2 p) {
    float2 c = size * 0.5;
    float r = min(c.x, c.y);
    float2 q = abs(p - c) - (c - r);
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}
float alongCSS(float2 p, float deg) {
    float a = radians(deg);
    float2 dir = float2(sin(a), -cos(a));
    float halfLength = 0.5 * (abs(size.x * dir.x) + abs(size.y * dir.y));
    return dot(p - size * 0.5, dir) / (2.0 * halfLength) + 0.5;
}
half4 main(float2 p) {
    float depth = -capsuleDistance(p);
    float coverage = clamp(depth + 0.5, 0.0, 1.0);
    float y = p.y / size.y;
    float t = alongCSS(p, -75.0);
    float light = mix(0.05, 0.20, 1.0 - abs(t * 2.0 - 1.0)) * mix(1.0, 0.55, prominent);
    float dark = (1.0 - smoothstep(0.0, 0.22, y)) * 0.07;
    light += smoothstep(0.70, 1.0, y) * 0.5 * smoothstep(0.0, 3.0, depth);
    light += (1.0 - smoothstep(0.0, 4.0, depth)) * 0.2;
    float2 d = p - size * 0.5;
    float turn = fract((degrees(atan(d.y, d.x)) + 90.0 - rimAngle) / 360.0 + 1.0);
    float conic = max(max(1.0 - smoothstep(0.0, 0.05, turn), smoothstep(0.95, 1.0, turn)),
                      smoothstep(0.40, 0.50, turn) * (1.0 - smoothstep(0.50, 0.60, turn)));
    float ring = 1.0 - smoothstep(0.0, 1.25, depth);
    dark += ring * conic * 0.5;
    light += ring * mix(0.5, 0.0, y);
    float u = (alongCSS(p, -45.0) + shine) * 0.5;
    light += smoothstep(0.20, 0.40, u) * (1.0 - smoothstep(0.50, 0.55, u)) * mix(0.5, 0.16, prominent);
    light = clamp(light, 0.0, 1.0);
    dark = clamp(dark, 0.0, 1.0);
    float alpha = light + dark - light * dark;
    float white = light * (1.0 - dark);
    return half4(half3(half(white)), half(alpha)) * half(coverage);
}
"""

@RequiresApi(33)
private class GlassShader {
    val shader = RuntimeShader(WABI_GLASS)
    val brush = ShaderBrush(shader)
}

private val glassShader: Any? by lazy { if (Build.VERSION.SDK_INT >= 33) runCatching { GlassShader() }.getOrNull() else null }

private fun DrawScope.drawGlass(prominent: Boolean, tint: Color, dark: Boolean, rim: Float, shine: Float) {
    val radius = CornerRadius(size.height / 2, size.height / 2)
    // Layer 0, the body: Compose has no backdrop blur, so the lens is a tinted film.
    drawRoundRect(if (prominent) tint.copy(alpha = 0.92f) else Color.White.copy(alpha = if (dark) 0.08f else 0.55f), cornerRadius = radius)
    val glass = glassShader
    if (Build.VERSION.SDK_INT >= 33 && glass is GlassShader) {
        glass.shader.setFloatUniform("size", size.width, size.height)
        glass.shader.setFloatUniform("rimAngle", rim)
        glass.shader.setFloatUniform("shine", shine)
        glass.shader.setFloatUniform("prominent", if (prominent) 1f else 0f)
        drawRect(glass.brush)
        return
    }
    // Fallback: the same layers as gradients.
    val w = size.width; val h = size.height
    drawRoundRect(Brush.linearGradient(listOf(Color.White.copy(.05f), Color.White.copy(if (prominent) .11f else .2f), Color.White.copy(.05f)),
        start = Offset(0f, h), end = Offset(w, 0f)), cornerRadius = radius)
    drawRoundRect(Brush.verticalGradient(0f to Color.Black.copy(.07f), .22f to Color.Transparent, .7f to Color.Transparent, 1f to Color.White.copy(.45f)), cornerRadius = radius)
    drawRoundRect(Brush.sweepGradient(listOf(Color.Black.copy(.45f), Color.White.copy(.5f), Color.White.copy(.2f), Color.Black.copy(.45f), Color.White.copy(.2f), Color.White.copy(.5f), Color.Black.copy(.45f))),
        cornerRadius = radius, style = Stroke(1.dp.toPx()))
}
