package xyz.tironi.zoen.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ChevronLeft
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.HazeStyle
import dev.chrisbanes.haze.HazeTint
import dev.chrisbanes.haze.hazeEffect
import dev.chrisbanes.haze.hazeSource
import dev.chrisbanes.haze.rememberHazeState

private val LocalLivingBackdrop = staticCompositionLocalOf<HazeState?> { null }

/** Actual recorded backdrop, not a blurred foreground or a translucent Material card. */
@Composable
fun LivingGlassScene(content: @Composable () -> Unit) {
    val backdrop = rememberHazeState()
    CompositionLocalProvider(LocalLivingBackdrop provides backdrop) {
        Box(Modifier.fillMaxSize()) {
            Box(Modifier.matchParentSize().background(MaterialTheme.colorScheme.background).hazeSource(backdrop))
            content()
        }
    }
}

/** Scoped to the opt-in experiment. Haze uses RenderEffect on Android 12+; older OSes use its scrim fallback. */
@Composable
fun LivingGlassCard(onClick: (() -> Unit)? = null, modifier: Modifier = Modifier,
                    shape: Shape = RoundedCornerShape(20.dp), elevation: Dp = 16.dp, content: @Composable ColumnScope.() -> Unit) {
    val dark = MaterialTheme.colorScheme.background.luminance() < 0.5f
    val tint = MaterialTheme.colorScheme.surface.copy(alpha = if (dark) 0.66f else 0.72f)
    val style = HazeStyle(backgroundColor = MaterialTheme.colorScheme.background, tint = HazeTint(tint), blurRadius = 20.dp, noiseFactor = 0.15f)
    val backdrop = LocalLivingBackdrop.current
    Column(modifier.shadow(elevation, shape, clip = false).clip(shape).then(if (backdrop != null) Modifier.hazeEffect(backdrop, style) else Modifier.background(tint))
        .border(1.dp, if (dark) Color.White.copy(alpha = 0.1f) else Color.Black.copy(alpha = 0.06f), shape)
        .then(if (onClick != null) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier), content = content)
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LivingScreenBar(title: String, back: (() -> Unit)? = null) {
    val backdrop = LocalLivingBackdrop.current
    val style = HazeStyle(backgroundColor = MaterialTheme.colorScheme.background, tint = HazeTint(MaterialTheme.colorScheme.surface.copy(alpha = 0.72f)), blurRadius = 20.dp, noiseFactor = 0.15f)
    CenterAlignedTopAppBar(title = { Text(title, style = MaterialTheme.typography.bodyLarge.copy(fontSize = 17.sp, fontWeight = FontWeight.SemiBold)) },
        modifier = if (backdrop != null) Modifier.hazeEffect(backdrop, style) else Modifier,
        expandedHeight = 54.dp,
        colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent, scrolledContainerColor = Color.Transparent),
        navigationIcon = { if (back != null) Box(Modifier.padding(start = 16.dp)) {
            LivingGlassCard(modifier = Modifier.size(44.dp), shape = RoundedCornerShape(50), elevation = 0.dp) {
                IconButton(onClick = back, modifier = Modifier.size(44.dp)) { Icon(Icons.Rounded.ChevronLeft, "Back") }
            }
        } })
}
