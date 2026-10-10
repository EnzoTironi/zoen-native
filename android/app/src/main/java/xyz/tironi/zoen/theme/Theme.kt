package xyz.tironi.zoen.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp

private val light = lightColorScheme(
    primary = Color(0xFF3D7A28), onPrimary = Color.White,
    primaryContainer = Color(0xFFDDEFD2), onPrimaryContainer = Color(0xFF20381A),
    secondary = Color(0xFF65795E), onSecondary = Color.White,
    secondaryContainer = Color(0xFFE8EDDF), onSecondaryContainer = Color(0xFF20381A),
    tertiary = Color(0xFF98612D), onTertiary = Color.White,
    tertiaryContainer = Color(0xFFFFE8CE), onTertiaryContainer = Color(0xFF3C230B),
    background = Color(0xFFF4F6FB), onBackground = Color(0xFF0E1320),
    surface = Color.White, onSurface = Color(0xFF0E1320),
    surfaceContainer = Color(0xFFECEFF6), surfaceContainerLow = Color(0xFFF4F6FB),
    surfaceContainerHigh = Color(0xFFE6EAF2), surfaceContainerHighest = Color(0xFFE1E5EE),
    surfaceVariant = Color(0xFFECEFF6), onSurfaceVariant = Color(0xFF5B6478),
    outline = Color(0xFF8C95A8), outlineVariant = Color(0xFFE1E5EE),
    inverseSurface = Color(0xFF182032), inverseOnSurface = Color(0xFFEEF2FA), inversePrimary = Color(0xFF86C25A),
)
private val dark = darkColorScheme(
    primary = Color(0xFFA4D581), onPrimary = Color(0xFF1E370F),
    primaryContainer = Color(0xFF324A25), onPrimaryContainer = Color(0xFFDDEFD2),
    secondary = Color(0xFFBACCAE), onSecondary = Color(0xFF21351D),
    secondaryContainer = Color(0xFF374632), onSecondaryContainer = Color(0xFFE8EDDF),
    tertiary = Color(0xFFF1BF88), onTertiary = Color(0xFF4A280F),
    tertiaryContainer = Color(0xFF573919), onTertiaryContainer = Color(0xFFFFE8CE),
    background = Color(0xFF090D15), onBackground = Color(0xFFEEF2FA),
    surface = Color(0xFF121826), onSurface = Color(0xFFEEF2FA),
    surfaceContainer = Color(0xFF1A2233), surfaceContainerLow = Color(0xFF121826),
    surfaceContainerHigh = Color(0xFF182032), surfaceContainerHighest = Color(0xFF253047),
    surfaceVariant = Color(0xFF1A2233), onSurfaceVariant = Color(0xFF93A0B8),
    outline = Color(0xFF5F6B82), outlineVariant = Color(0xFF253047),
    inverseSurface = Color(0xFFEEF2FA), inverseOnSurface = Color(0xFF121826), inversePrimary = Color(0xFF3D7A28),
)
private val typography = Typography(
    displaySmall = TextStyle(fontFamily = FontFamily.Serif, fontWeight = FontWeight.Medium, fontSize = 36.sp, lineHeight = 42.sp),
    headlineLarge = TextStyle(fontWeight = FontWeight.Bold, fontSize = 32.sp, lineHeight = 38.sp, letterSpacing = (-.7).sp),
    headlineMedium = TextStyle(fontWeight = FontWeight.Bold, fontSize = 28.sp, lineHeight = 34.sp, letterSpacing = (-.5).sp),
    titleLarge = TextStyle(fontWeight = FontWeight.SemiBold, fontSize = 22.sp, lineHeight = 28.sp),
    bodyLarge = TextStyle(fontSize = 16.sp, lineHeight = 24.sp),
)

@Composable
fun ZoenTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = if (isSystemInDarkTheme()) dark else light, typography = typography, content = content)
}

val ColorScheme.ownMessage: Color
    get() = if (background.luminance() > .5f) Color(0xFF111113) else Color(0xFFF2F2F0)

val ColorScheme.onOwnMessage: Color
    get() = if (background.luminance() > .5f) Color.White else Color(0xFF111113)

val ColorScheme.otherMessage: Color
    get() = if (background.luminance() > .5f) Color(0xFFF2F2F2) else Color(0xFF26272B)
