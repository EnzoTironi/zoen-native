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
import androidx.compose.ui.graphics.graphicsLayer
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
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * Zoen's one material: a marble of glass. GlassButton and GlassCard both draw with this,
 * so the look lives in one place (the same lens as apple/Shared/DesignSystem/ZoenGlass.metal,
 * our own port of the optics measured in Wabi's app, ui-refs/wabi-glass/app/NOTES.md:
 * hemisphere rail, Schlick Fresnel, 27deg/65deg key light, frost as the only whitening,
 * red/blue dispersion on the rail).
 *
 * [slab] true is Wabi's flat button preset (thin rail, no dispersion, frost .2) for small
 * controls; false is the full marble for hero buttons and cards. Compose can't sample what
 * is behind a view, so the body is a tinted film and the lens is painted, not refracted.
 * API 33+ uses AGSL (one RuntimeShader per surface, built once in the draw cache, so
 * scrolling lists only update the press uniform); older devices get gradients.
 */
@Composable
fun Modifier.zoenGlass(
    tint: Color,
    prominent: Boolean,
    dark: Boolean,
    cornerRadius: Dp? = null,
    press: () -> Float = { 0f },
    strength: Float = 1f,
    slab: Boolean = false,
    film: Color? = null,
): Modifier {
    // One shader per surface for its whole life: recomposition and resizes only touch uniforms.
    val glass = remember { if (Build.VERSION.SDK_INT >= 33) runCatching { MarbleShader() }.getOrNull() else null }
    return drawWithCache {
        val r = cornerRadius?.toPx()?.coerceAtMost(size.minDimension / 2) ?: (size.minDimension / 2)
        val corner = CornerRadius(r, r)
        val frost = if (slab) 0.2f else 0.6f
        // An explicit caller colour (a card's container) is used exactly as asked; otherwise a
        // prominent body is opaque (no backdrop to refract, white labels keep 4.5:1) and a clear one is a light film.
        val body = film?.takeIf { it.alpha > 0f }
            ?: if (prominent) tint.copy(alpha = 1f) else Color.White.copy(alpha = if (dark) 0.07f else 0.6f)
        val crown = Brush.radialGradient(listOf(Color.White.copy((if (dark) .08f else .3f) * strength), Color.Transparent),
            center = Offset(size.width * 0.35f, size.height * 0.2f), radius = size.minDimension * 0.8f)
        val volume = Brush.verticalGradient(0f to Color.Transparent, .6f to Color.Transparent, 1f to Color.Black.copy(.18f * strength))
        val rim = Brush.sweepGradient(listOf(Color.White.copy(.7f), Color.White.copy(.15f), Color.White.copy(.7f)))
        onDrawBehind {
            drawRoundRect(body, cornerRadius = corner)
            if (glass != null && Build.VERSION.SDK_INT >= 33) {
                glass.set(size.width, size.height, r, density, prominent, strength, slab, frost, press())
                drawRect(glass.brush)
            } else {
                drawRoundRect(volume, cornerRadius = corner)
                if (!prominent && film == null) drawRoundRect(crown, cornerRadius = corner)
                drawRoundRect(rim, cornerRadius = corner, style = Stroke(1.dp.toPx()))
            }
        }
    }
}

@RequiresApi(33)
private class MarbleShader {
    val shader = RuntimeShader(ZOEN_GLASS)
    val brush = ShaderBrush(shader)
    fun set(w: Float, h: Float, r: Float, unit: Float, prominent: Boolean, strength: Float, slab: Boolean, frost: Float, raw: Float) {
        val p = raw.coerceIn(0f, 1f)
        shader.setFloatUniform("size", w, h)
        shader.setFloatUniform("radius", r)
        shader.setFloatUniform("unit", unit)
        shader.setFloatUniform("prominent", if (prominent) 1f else 0f)
        shader.setFloatUniform("strength", strength)
        shader.setFloatUniform("slab", if (slab) 1f else 0f)
        shader.setFloatUniform("press", p)
        shader.setFloatUniform("frost", frost * (1f - 0.5f * p))   // a press clears the lens
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
    val focused by interactions.collectIsFocusedAsState()
    val scheme = MaterialTheme.colorScheme
    val dark = scheme.surface.luminance() < 0.5f
    // A coloured card keeps its colour as the glass tint; plain cards are clear glass.
    val tint = colors?.containerColor?.takeOrElse { Color.Transparent } ?: Color.Transparent
    val prominent = tint.alpha > 0.5f && tint != scheme.surface && tint != scheme.surfaceContainerHighest
    val ink = colors?.contentColor?.takeOrElse { scheme.onSurface } ?: scheme.onSurface
    Column(
        modifier
            .graphicsLayer { alpha = if (onClick != null && !enabled) 0.45f else 1f }
            .shadow(if (pressed) 2.dp else 8.dp, shape, clip = false,
                ambientColor = Color.Black.copy(alpha = 0.12f), spotColor = Color.Black.copy(alpha = 0.16f))
            .clip(shape)
            .zoenGlass(tint = if (prominent) tint else scheme.surface, prominent = prominent, dark = dark,
                cornerRadius = 22.dp, press = { if (pressed) 1f else 0f }, strength = 0.8f,
                film = colors?.containerColor?.takeOrElse { Color.Transparent })
            .then(if (focused) Modifier.border(2.dp, scheme.primary, shape) else Modifier)
            .then(if (onClick != null) Modifier.clickable(interactions, indication = null, enabled = enabled, role = Role.Button, onClick = onClick) else Modifier),
    ) {
        CompositionLocalProvider(LocalContentColor provides ink) { content() }
    }
}

private const val ZOEN_GLASS = """
uniform float2 size;
uniform float radius;
uniform float press;
uniform float prominent;
uniform float strength;
uniform float frost;
uniform float slab;
uniform float unit;                    // px per dp, so the rail is the same size on every screen

float roundedDistance(float2 p, float2 size, float r) {
    float2 h = size * 0.5;
    float2 q = abs(p - h) - (h - r);
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}
half4 main(float2 position) {
    float2 c = size * 0.5;
    float r = min(radius, min(c.x, c.y));
    float depth = -roundedDistance(position, size, r);           // px in from the edge
    float coverage = clamp(depth + 0.5, 0.0, 1.0);
    float2 e = float2(0.75 * unit, 0.0);
    float2 outward = float2(roundedDistance(position + e.xy, size, r) - roundedDistance(position - e.xy, size, r),
                            roundedDistance(position + e.yx, size, r) - roundedDistance(position - e.yx, size, r));
    outward = outward / max(length(outward), 0.0001);
    // The lens profile: a hemisphere across the rail. A marble's rail is the whole radius;
    // the flat preset's is a thin bevel, so small buttons read as a slab with a rounded lip.
    float rail = max(mix(min(r, 28.0 * unit), min(r, 28.0 * unit) * 0.35, slab), 3.0 * unit);
    float edge = 1.0 - clamp(depth / rail, 0.0, 1.0);             // 0 on the crown, 1 at the rim
    float3 n = normalize(float3(outward * edge, sqrt(max(1.0 - edge * edge, 0.0))));
    float3 V = float3(0.0, 0.0, 1.0);
    // Key light 27deg around from straight up-left, 65deg high
    float az = -2.0420352;                                       // -90deg - 27deg
    float el = 1.1344640;                                        // 65deg
    float3 L = normalize(float3(cos(az) * cos(el), sin(az) * cos(el), sin(el)));
    float3 H = normalize(L + V);

    // Schlick Fresnel: almost clear facing you, mirror-bright at grazing (the rim)
    float cosT = clamp(n.z, 0.0, 1.0);
    float fresnel = 0.035 + 0.965 * pow(1.0 - cosT, 5.0);
    // Specular, roughness .116 (marble) / .14 (flat), highlights 1.41 / clearcoat .4; press slides it
    float rough = mix(0.116, 0.14, slab);
    float3 Hp = normalize(H + float3(0.0, 0.10, 0.0) * press);
    float spec = pow(max(dot(n, Hp), 0.0), 0.5 / (rough * rough)) * mix(1.41, 0.4, slab) * 0.7
               * smoothstep(mix(0.08, 0.42, prominent), mix(0.45, 0.75, prominent), edge);   // only the curved rail glints: the crown stays clear behind the label
    // Frost lifts the body toward white (the only whitening: no separate tint film)
    float frostWhite = frost * mix(0.16, 0.03, prominent);       // a coloured body stays deep enough for white text
    // Dispersion fringe: red out, blue in, only on the rail (strength = dispersion x edge^2)
    float disp = mix(0.81, 0.0, slab) * edge * edge;
    float3 fringe = float3(1.0, 0.0, -1.0) * disp * 0.10;
    // Lambert shading of the rail gives the body its volume; the far side falls into shadow
    float shade = (1.0 - clamp(dot(n, L) * 1.2, 0.0, 1.0)) * edge * 0.30 * strength;

    float3 rgb = float3(fresnel * 0.9 * strength + spec * strength + frostWhite) + fringe;
    float lum = clamp(max(max(rgb.r, rgb.g), rgb.b), 0.0, 1.0);
    float alpha = clamp(lum + shade * (1.0 - lum), 0.0, 1.0);
    rgb = clamp(rgb, 0.0, 1.0) * (1.0 - shade);                   // premultiplied
    return half4(half3(rgb), half(alpha)) * half(coverage);
}
"""
