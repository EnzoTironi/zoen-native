package xyz.tironi.zoen.ui

import android.os.Build
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.unit.dp
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.HazeTint
import dev.chrisbanes.haze.hazeEffect
import dev.chrisbanes.haze.hazeSource
import dev.chrisbanes.haze.rememberHazeState

/** Pure policy: API 28–30 never create a blur/render-effect capture. */
internal fun supportsGlassBackdrop(sdk: Int): Boolean = sdk >= 31

internal data class GlassAppearance(
    val tintAlpha: Float,
    val edgeTopAlpha: Float,
    val edgeBottomAlpha: Float,
    val shadowAlpha: Float,
)

internal fun glassAppearance(dark: Boolean, backdrop: Boolean) = GlassAppearance(
    tintAlpha = if (backdrop) { if (dark) 0.62f else 0.34f } else { if (dark) 0.92f else 0.88f },
    edgeTopAlpha = if (dark) 0.28f else 0.70f,
    edgeBottomAlpha = if (dark) 0.12f else 0.20f,
    shadowAlpha = if (dark) 0.55f else 0.12f,
)

private val LocalGlassBackdrop = staticCompositionLocalOf<HazeState?> { null }

/**
 * One shared capture state per screen. Place source content and floating glass as SIBLINGS.
 * Never put zoenGlassSource on an ancestor of glass: that captures the effect recursively.
 * Haze samples only the registered source, not the glass's own text/icons.
 */
@Composable
fun ZoenGlassBackdrop(content: @Composable () -> Unit) {
    val state = rememberHazeState(blurEnabled = supportsGlassBackdrop(Build.VERSION.SDK_INT))
    CompositionLocalProvider(LocalGlassBackdrop provides state, content = content)
}

/** Register the scrolling underlay, not the whole root containing the floating overlays. */
@Composable
fun Modifier.zoenGlassSource(): Modifier {
    val state = LocalGlassBackdrop.current
    return if (state != null && supportsGlassBackdrop(Build.VERSION.SDK_INT)) hazeSource(state) else this
}

/**
 * Backdrop diffusion, slate/white tint and iOS BarEdge's light hairline and depth.
 * API 28–30 (and previews without a provider) use a translucent material, with NO blur.
 * This does not claim Apple's proprietary refractive lens distortion: Haze 1.x provides
 * real backdrop blur and subtle grain, not geometric refraction.
 */
@Composable
fun Modifier.zoenGlass(shape: Shape, tint: Color? = null): Modifier {
    val state = LocalGlassBackdrop.current
    val dark = MaterialTheme.colorScheme.background.luminance() < 0.5f
    val backdrop = supportsGlassBackdrop(Build.VERSION.SDK_INT) && state != null
    val appearance = glassAppearance(dark, backdrop)
    val background = MaterialTheme.colorScheme.background
    val base = if (dark) Color(0xFF1C2433) else Color.White
    val materialTint = tint ?: base.copy(alpha = appearance.tintAlpha)
    val surface = shadow(
        elevation = if (dark) 14.dp else 10.dp,
        shape = shape,
        clip = false,
        ambientColor = Color.Black.copy(alpha = appearance.shadowAlpha),
        spotColor = Color.Black.copy(alpha = appearance.shadowAlpha),
    ).clip(shape)
    val diffuse = if (backdrop) {
        surface.hazeEffect(state = checkNotNull(state)) {
            backgroundColor = background
            blurRadius = 24.dp
            noiseFactor = 0.025f
            tints = listOf(HazeTint(materialTint))
        }
    } else surface.background(materialTint)
    return diffuse.border(
        0.75.dp,
        Brush.verticalGradient(listOf(
            Color.White.copy(alpha = appearance.edgeTopAlpha),
            Color.White.copy(alpha = appearance.edgeBottomAlpha),
        )),
        shape,
    )
}
