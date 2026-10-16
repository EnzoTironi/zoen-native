package xyz.tironi.zoen.ui

import android.graphics.RuntimeShader
import android.os.Build
import androidx.annotation.RequiresApi
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.material3.CardColors
import androidx.compose.material3.CardElevation
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.ShaderBrush
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.takeOrElse
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * Zoen's one material: a marble of glass. GlassButton and GlassCard both draw with this,
 * so the look lives in one place (the same shader as apple/Shared/DesignSystem/ZoenGlass.metal).
 *
 * Wabi's conic edge and travelling shine plus the lens volume measured from Wabi's video
 * (ui-refs/wabi-glass/VIDEO-ANALYSIS.md): hemisphere shading, clear↔frosted specular,
 * 1.5% rim brighter top-left, bottom-right inner shadow, .05–.25 white film. Compose can't
 * sample what's behind a view, so the body is a tinted film; API 33+ paints the volume with
 * AGSL, older devices with gradients. One RuntimeShader per surface, built once in the draw
 * cache, so scrolling lists only update uniforms.
 *
 * [press] is 0…1 (animated by the caller); [cornerRadius] null means a capsule.
 */
fun Modifier.zoenGlass(
    tint: Color,
    prominent: Boolean,
    dark: Boolean,
    cornerRadius: Dp? = null,
    press: () -> Float = { 0f },
    strength: Float = 1f,
    frost: Float = 0.35f,
): Modifier = drawWithCache {
    val r = cornerRadius?.toPx()?.coerceAtMost(size.minDimension / 2) ?: (size.minDimension / 2)
    val corner = CornerRadius(r, r)
    val body = if (prominent) tint.copy(alpha = 0.92f) else Color.White.copy(alpha = if (dark) 0.07f else 0.6f)
    val glass = if (Build.VERSION.SDK_INT >= 33) runCatching { MarbleShader() }.getOrNull() else null
    glass?.setStatic(size.width, size.height, r, prominent, strength)
    glass?.frost = frost
    val fill = Brush.linearGradient(listOf(Color.White.copy(.05f), Color.White.copy(if (prominent) .11f else .2f), Color.White.copy(.05f)),
        start = Offset(0f, size.height), end = Offset(size.width, 0f))
    val inset = Brush.verticalGradient(0f to Color.Black.copy(.08f), .22f to Color.Transparent, .55f to Color.Transparent,
        .8f to Color.White.copy(.18f), 1f to Color.White.copy(.4f))
    val rim = Brush.sweepGradient(listOf(Color.Black.copy(.35f), Color.White.copy(.5f), Color.White.copy(.2f), Color.Black.copy(.35f),
        Color.White.copy(.2f), Color.White.copy(.5f), Color.Black.copy(.35f)))
    val spec = Brush.radialGradient(listOf(Color.White.copy(.55f * strength), Color.Transparent),
        center = Offset(size.width * 0.32f, size.height * 0.18f), radius = size.minDimension * 0.9f)
    onDrawBehind {
        drawRoundRect(body, cornerRadius = corner)
        val p = press()
        if (glass != null) {
            glass.setPress(p)
            drawRect(glass.brush)
        } else {
            drawRoundRect(fill, cornerRadius = corner)
            drawRoundRect(inset, cornerRadius = corner)
            drawRoundRect(spec, cornerRadius = corner)
            drawRoundRect(rim, cornerRadius = corner, style = Stroke(1.dp.toPx()))
        }
    }
}

@RequiresApi(33)
private class MarbleShader {
    val shader = RuntimeShader(ZOEN_GLASS)
    val brush = ShaderBrush(shader)
    fun setStatic(w: Float, h: Float, r: Float, prominent: Boolean, strength: Float) {
        shader.setFloatUniform("size", w, h)
        shader.setFloatUniform("radius", r)
        shader.setFloatUniform("prominent", if (prominent) 1f else 0f)
        shader.setFloatUniform("strength", strength)
    }
    var frost = 0.35f
    fun setPress(raw: Float) {
        val p = raw.coerceIn(0f, 1f)
        shader.setFloatUniform("press", p)
        shader.setFloatUniform("frost", frost * (1f - p))       // a press clears the lens
        shader.setFloatUniform("rimAngle", -75f - 50f * p)
        shader.setFloatUniform("shine", 0.25f * p)
    }
}

private val CardShape = RoundedCornerShape(22.dp)

/** Drop-in for Material's Card / OutlinedCard / ElevatedCard, on the marble. */
@Composable
fun GlassCard(
    modifier: Modifier = Modifier,
    shape: Shape = CardShape,
    colors: CardColors? = null,
    @Suppress("UNUSED_PARAMETER") elevation: CardElevation? = null,
    @Suppress("UNUSED_PARAMETER") border: BorderStroke? = null,
    content: @Composable ColumnScope.() -> Unit,
) = GlassCardBody(null, modifier, true, shape, colors, null, content)

@Composable
fun GlassCard(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    shape: Shape = CardShape,
    colors: CardColors? = null,
    @Suppress("UNUSED_PARAMETER") elevation: CardElevation? = null,
    @Suppress("UNUSED_PARAMETER") border: BorderStroke? = null,
    interactionSource: MutableInteractionSource? = null,
    content: @Composable ColumnScope.() -> Unit,
) = GlassCardBody(onClick, modifier, enabled, shape, colors, interactionSource, content)

@Composable
private fun GlassCardBody(
    onClick: (() -> Unit)?,
    modifier: Modifier,
    enabled: Boolean,
    shape: Shape,
    colors: CardColors?,
    interactionSource: MutableInteractionSource?,
    content: @Composable ColumnScope.() -> Unit,
) {
    val interactions = interactionSource ?: remember { MutableInteractionSource() }
    val pressed by interactions.collectIsPressedAsState()
    val scheme = MaterialTheme.colorScheme
    val dark = scheme.surface.luminance() < 0.5f
    // A coloured card keeps its colour as the glass tint; plain cards are clear glass.
    val tint = colors?.containerColor?.takeOrElse { Color.Transparent } ?: Color.Transparent
    val prominent = tint.alpha > 0.5f && tint != scheme.surface && tint != scheme.surfaceContainerHighest
    val ink = colors?.contentColor?.takeOrElse { scheme.onSurface } ?: scheme.onSurface
    Column(
        modifier
            .shadow(if (pressed) 2.dp else 8.dp, shape, clip = false,
                ambientColor = Color.Black.copy(alpha = 0.12f), spotColor = Color.Black.copy(alpha = 0.16f))
            .clip(shape)
            .zoenGlass(tint = if (prominent) tint else scheme.surface, prominent = prominent, dark = dark,
                cornerRadius = 22.dp, press = { if (pressed) 1f else 0f }, strength = 0.7f, frost = 0.8f)
            .then(if (onClick != null) Modifier.clickable(interactions, indication = null, enabled = enabled, role = Role.Button, onClick = onClick) else Modifier),
    ) {
        CompositionLocalProvider(LocalContentColor provides ink) { content() }
    }
}

private const val ZOEN_GLASS = """
uniform float2 size;
uniform float radius;
uniform float rimAngle;
uniform float shine;
uniform float press;
uniform float prominent;
uniform float strength;
uniform float frost;

float sq(float x) { return x * x; }
float roundedDistance(float2 p, float2 size, float r) {
    float2 h = size * 0.5;
    float2 q = abs(p - h) - (h - r);
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}
float alongCSS(float2 p, float2 size, float deg) {
    float a = radians(deg);
    float2 dir = float2(sin(a), -cos(a));
    float halfLength = 0.5 * (abs(size.x * dir.x) + abs(size.y * dir.y));
    return dot(p - size * 0.5, dir) / (2.0 * halfLength) + 0.5;
}
half4 main(float2 position) {
    float2 c = size * 0.5;
    float r = min(radius, min(c.x, c.y));
    float depth = -roundedDistance(position, size, r);           // px in from the edge
    float coverage = clamp(depth + 0.5, 0.0, 1.0);
    float2 e = float2(0.75, 0.0);
    float2 outward = float2(roundedDistance(position + e.xy, size, r) - roundedDistance(position - e.xy, size, r),
                            roundedDistance(position + e.yx, size, r) - roundedDistance(position - e.yx, size, r));
    outward = outward / max(length(outward), 0.0001);
    // Hemisphere profile across the shoulder (Wabi's lens: the outer band bends hardest).
    float bevel = max(min(r, 28.0), 4.0);
    float s = 1.0 - clamp(depth / bevel, 0.0, 1.0);               // 1 at the edge, 0 on the crown
    float3 n = normalize(float3(outward * s, sqrt(max(1.0 - s * s, 0.0))));
    float3 L = normalize(float3(-0.7, -0.7, 1.0));                // top-left (y grows down)
    float3 H = normalize(L + float3(0.0, 0.0, 1.0));
    float clear = 1.0 - frost;                                    // Wabi: clear lens ↔ frosted

    // Tint film: white .05 clear → .25 frosted
    float light = mix(0.05, 0.25, frost) * mix(1.0, 0.4, prominent);
    float dark = 0.0;
    // Specular: sharp (roughness .1) when clear, broad (.7) when frosted; slides on press
    float rough = mix(0.1, 0.7, frost);
    float3 Hp = normalize(H + float3(0.12, 0.12, 0.0) * press);
    float spec = pow(max(dot(n, Hp), 0.0), mix(400.0, 12.0, rough));
    light += spec * mix(0.9, 0.45, frost) * strength;
    // Lambert falloff gives the body its volume: shoulders facing away from the light darken
    float lambert = dot(n, L);
    dark += (1.0 - clamp(lambert * 1.25, 0.0, 1.0)) * 0.22 * s * strength;
    // Inner shadow, bottom-right, 15-20% of the radius deep, 10-20% black
    float br = clamp(dot(outward, normalize(float2(1.0, 1.0))), 0.0, 1.0);
    dark += exp(-depth / max(bevel * 0.18, 1.0)) * br * 0.18 * strength;
    // Rim: thin white, 1.5% of the radius (≥ 1px), brighter top-left
    float rimW = max(r * 0.015 * 2.0, 1.0);
    float rim = 1.0 - smoothstep(0.0, rimW, depth);
    float tl = clamp(dot(outward, normalize(float2(-1.0, -1.0))) * 0.5 + 0.5, 0.0, 1.0);
    light += rim * mix(0.25, 0.85, tl);
    // Wabi's conic edge turns on press
    float2 d = position - c;
    float turn = fract((atan(d.y, d.x) * 57.29578 + 90.0 - rimAngle) / 360.0 + 1.0);
    float conic = max(max(1.0 - smoothstep(0.0, 0.05, turn), smoothstep(0.95, 1.0, turn)),
                      smoothstep(0.40, 0.50, turn) * (1.0 - smoothstep(0.50, 0.60, turn)));
    dark += rim * conic * 0.25 * clear;
    // Wabi's travelling shine band
    float u = (alongCSS(position, size, -45.0) + shine) * 0.5;
    light += smoothstep(0.20, 0.40, u) * (1.0 - smoothstep(0.50, 0.55, u)) * mix(0.25, 0.10, prominent);

    light = clamp(light, 0.0, 1.0);
    dark = clamp(dark, 0.0, 1.0);
    float alpha = light + dark - light * dark;
    float white = light * (1.0 - dark);                           // premultiplied
    return half4(half3(half(white)), half(alpha)) * half(coverage);
}
"""
