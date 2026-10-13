package xyz.tironi.zoen.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import xyz.tironi.zoen.ui.LocalZoenPalette
import xyz.tironi.zoen.ui.ZoenPalette
import xyz.tironi.zoen.ui.zoenIncreasedContrast

private fun ZoenPalette.scheme(): ColorScheme {
    val night = bg.luminance() < 0.5f
    val inverse = if (night) ZoenPalette.light else ZoenPalette.dark
    return if (night) {
        darkColorScheme(
            primary = fillPrimary,
            onPrimary = textOn,
            primaryContainer = thinBg,
            onPrimaryContainer = textPrimary,
            secondary = textSecondary,
            onSecondary = textOn,
            secondaryContainer = cellBg,
            onSecondaryContainer = textPrimary,
            tertiary = systemOrange,
            onTertiary = textOn,
            background = bg,
            onBackground = textPrimary,
            surface = bg,
            onSurface = textPrimary,
            surfaceVariant = cellBg,
            onSurfaceVariant = textSecondary,
            outline = strokePrimary,
            outlineVariant = strokeSecondary,
            error = systemRed,
            onError = textOn,
            inverseSurface = inverse.bg,
            inverseOnSurface = inverse.textPrimary,
            inversePrimary = inverse.fillPrimary,
        )
    } else {
        lightColorScheme(
            primary = fillPrimary,
            onPrimary = textOn,
            primaryContainer = thinBg,
            onPrimaryContainer = textPrimary,
            secondary = textSecondary,
            onSecondary = textOn,
            secondaryContainer = cellBg,
            onSecondaryContainer = textPrimary,
            tertiary = systemOrange,
            onTertiary = textOn,
            background = bg,
            onBackground = textPrimary,
            surface = bg,
            onSurface = textPrimary,
            surfaceVariant = cellBg,
            onSurfaceVariant = textSecondary,
            outline = strokePrimary,
            outlineVariant = strokeSecondary,
            error = systemRed,
            onError = textOn,
            inverseSurface = inverse.bg,
            inverseOnSurface = inverse.textPrimary,
            inversePrimary = inverse.fillPrimary,
        )
    }
}

private val typography = Typography(
    displaySmall = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Normal, fontSize = 32.sp, lineHeight = 32.sp),
    headlineLarge = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Normal, fontSize = 32.sp, lineHeight = 32.sp),
    headlineMedium = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Medium, fontSize = 24.sp, lineHeight = 31.2f.sp),
    titleLarge = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Medium, fontSize = 20.sp, lineHeight = 26.4f.sp),
    bodyLarge = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Normal, fontSize = 15.sp, lineHeight = 19.sp),
)

@Composable
fun ZoenTheme(content: @Composable () -> Unit) {
    val palette = (if (isSystemInDarkTheme()) ZoenPalette.dark else ZoenPalette.light)
        .forContrast(zoenIncreasedContrast())
    CompositionLocalProvider(LocalZoenPalette provides palette) {
        MaterialTheme(colorScheme = palette.scheme(), typography = typography, content = content)
    }
}

private fun ColorScheme.palette(): ZoenPalette =
    if (background.luminance() > 0.5f) ZoenPalette.light else ZoenPalette.dark

val ColorScheme.ownMessage: Color
    get() = palette().bubbleUser

val ColorScheme.onOwnMessage: Color
    get() = palette().textPrimary

val ColorScheme.otherMessage: Color
    get() = palette().bubbleAgent
