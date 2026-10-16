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
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.unit.DpOffset
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

/** Shared rounded-lens optics. API 33+ shades a local tinted field through a
 * refractive lens; it does not capture or refract other Compose views behind it.
 * Small controls use a lower-sag preset. Older devices retain the gradient fallback.
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
            if (glass != null && Build.VERSION.SDK_INT >= 33) {
                glass.set(size.width, size.height, r, density, prominent, strength, slab, frost, press(), body)
                drawRect(glass.brush)
            } else {
                drawRoundRect(body, cornerRadius = corner)
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
    fun set(w: Float, h: Float, r: Float, unit: Float, prominent: Boolean, strength: Float, slab: Boolean, frost: Float, raw: Float, body: Color) {
        val p = raw.coerceIn(0f, 1f)
        shader.setFloatUniform("body", body.red, body.green, body.blue, body.alpha)
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

internal fun Modifier.marbleShadow(shape: Shape, pressed: Boolean, slab: Boolean = false): Modifier =
    dropShadow(shape, Shadow(
        radius = (if (pressed) 6 else if (slab) 8 else 60).dp,
        spread = (if (slab || pressed) 0 else -8).dp,
        color = Color.Black.copy(alpha = if (pressed) .12f else .26f),
        offset = DpOffset((if (slab) 0 else 2).dp, (if (pressed) 3 else if (slab) 4 else 41).dp),
    ))

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
            .marbleShadow(shape, pressed)
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
uniform float4 body;
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
    float edge = 1.0 - clamp(depth / max(r, unit), 0.0, 1.0);
    float slope = edge * mix(.98, .55, slab);
    float3 n = float3(outward * slope, sqrt(max(1.0-slope*slope, .0001)));
    // Snell's law, air to glass (IOR 1.5), with quadratic radial displacement.
    float eta = 1.0 / 1.5;
    float k = 1.0 - eta*eta*(1.0-n.z*n.z);
    float3 ray = eta*float3(0,0,-1) + (eta*n.z-sqrt(max(k,0.0)))*n;
    float2 bend = ray.xy * r * edge*edge * mix(.65,.18,slab) * (1.0-.15*press);
    float2 uv = clamp((position+bend)/size,0.0,1.0);
    float2 dispersion = bend*.04*(1.0-slab)/size;
    // Refract a local illumination field, not a claimed backdrop capture.
    float3 field = float3(uv.y+dispersion.y, uv.y, uv.y-dispersion.y);
    float3 rgb = body.rgb * (1.06-field*.12);
    float fresnel = .035 + .965 * pow(1.0-n.z,5.0);
    float3 light = normalize(float3(-.55,-.65,.65));
    float3 halfway = normalize(light + float3(0,0,1));
    float spec = pow(max(dot(n,normalize(halfway+float3(0,.08*press,0))),0.0),72.0);
    float broad = pow(max(dot(n,halfway),0.0),9.0);
    float innerShadow = edge*edge * (.12 + .32*max(dot(outward,float2(.6,.8)),0.0));
    float caustic = exp(-pow((edge-.66)/.18,2.0)) * max(dot(outward,normalize(float2(.55,.8))),0.0);
    float rim = fresnel*.65 + exp(-depth*depth/(1.8*unit*unit))*.24;
    float3 fringe = float3(.04,0,.04) * edge*edge*(1.0-slab)
        * float3(max(-outward.x,0.0),0,max(outward.x,0.0));
    rgb = rgb*(1.0-innerShadow*strength) + (spec*.9+broad*.065+rim*.48+caustic*.16)*strength + fringe;
    return half4(clamp(rgb,0.0,1.0)*body.a,body.a)*coverage;
}
"""
