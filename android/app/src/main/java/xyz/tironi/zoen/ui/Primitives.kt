package xyz.tironi.zoen.ui

import android.app.UiModeManager
import android.os.Build
import android.provider.Settings
import android.view.HapticFeedbackConstants
import android.view.View
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.EaseOut
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.ArrowUpward
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Mic
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.BlurredEdgeTreatment
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.scale
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.min
import kotlin.math.roundToInt
import kotlinx.coroutines.launch

private const val MotionPrefs = "zoen_motion"
private const val ReduceTransparencyKey = "reduce_transparency"

private val SegmentStops = listOf(
    0.0 to 0.0,
    0.07 to 0.056,
    0.14 to 0.182,
    0.21 to 0.332,
    0.29 to 0.48,
    0.36 to 0.611,
    0.43 to 0.72,
    0.5 to 0.807,
    0.57 to 0.872,
    0.64 to 0.92,
    0.71 to 0.954,
    0.79 to 0.976,
    0.86 to 0.99,
    0.93 to 0.999,
    1.0 to 1.0,
)

private fun segmentEase(time: Float): Float {
    val x = time.coerceIn(0f, 1f).toDouble()
    var previous = SegmentStops.first()
    for (stop in SegmentStops.drop(1)) {
        if (x <= stop.first) {
            val span = stop.first - previous.first
            val u = if (span == 0.0) 0.0 else (x - previous.first) / span
            return (previous.second + (stop.second - previous.second) * u).toFloat()
        }
        previous = stop
    }
    return 1f
}

enum class ZoenAppearance(val label: String) {
    Dark("Escuro"),
    Light("Claro"),
    System("Sistema"),
    Automatic("Automático");

    fun isDark(systemDark: Boolean): Boolean = when (this) {
        Dark -> true
        Light -> false
        System, Automatic -> systemDark
    }
}

enum class ZoenHaptic {
    Selection,
    Send,
    Record,
    Lock,
    Success,
    Warning,
    Deny,
    Error,
}

private const val HapticMaxPulseMs = 20

fun View.prepareZoenHaptic(kind: ZoenHaptic) {
    // Android feedback has no prepare step. The touch-down call stays so every platform shares the same moment.
}

fun View.playZoenHaptic(kind: ZoenHaptic) {
    performHapticFeedback(
        when (kind) {
            ZoenHaptic.Selection, ZoenHaptic.Deny -> HapticFeedbackConstants.CLOCK_TICK
            ZoenHaptic.Send -> HapticFeedbackConstants.KEYBOARD_TAP
            ZoenHaptic.Record -> HapticFeedbackConstants.CONTEXT_CLICK
            ZoenHaptic.Lock, ZoenHaptic.Success ->
                if (Build.VERSION.SDK_INT >= 30) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.CONTEXT_CLICK
            ZoenHaptic.Warning, ZoenHaptic.Error ->
                if (Build.VERSION.SDK_INT >= 30) HapticFeedbackConstants.REJECT else HapticFeedbackConstants.LONG_PRESS
        },
    )
}

@Composable
fun zoenIncreasedContrast(): Boolean {
    if (Build.VERSION.SDK_INT < 34) return false
    val context = LocalContext.current
    return (context.getSystemService(UiModeManager::class.java)?.contrast ?: 0f) > 0f
}

object ZoenTokens {
    object Motion {
        const val threadSwitchMs = 150
        const val fingerReleaseMs = 240
        const val statusLineMs = 150
        const val receiptMs = 150
        const val queueIntervalMs = 40
        const val queueMaxPerFrame = 8
        const val caretMs = 420
        const val caretHz = 8
        const val peekMs = 220
        const val threadPanelMs = 220
        const val reactionGrowMs = 150
    }

    object Layout {
        const val composerBottomInset = 42
        const val threadPanelWidth = 390
        const val threadPanelRadius = 24
        const val threadPanelGap = 16
        const val threadPanelFrom = 1024
        const val threadFace = 16
        const val threadFaceHit = 44
        const val broadcastChipMs = 4000
        const val threadRootMaxLines = 3
        const val threadSwipeOpen = 64
        const val threadEdgeClose = 24
        const val summaryMinReplies = 8
        const val reactionLongPressMs = 350
        const val reactionTrayMax = 8
        const val reactionAboveFinger = 8
        const val editWindowMin = 15
        const val deleteForAllHours = 48
        const val pinMax = 3
        const val liveLocationMin = 15
    }

    object Signature {
        const val composerBottomInset = 42
        const val bubbleAnchorGap = 12
        const val peekVisible = 8
        const val voiceArmMs = 80
        const val voiceLockTravel = 48
        const val voiceCancelTravel = 72
        const val receiptDot = 8
        const val receiptOffset = 4
        const val statusAboveComposer = 8
        const val menuAboveAttach = 8
        const val callButtonHeight = 48
        const val callButtonMinWidth = 120
        const val callButtonMinCenterDistance = 80
        const val terminalMaxWidth390 = 270
        const val unreadDot = 8
        const val presenceDot = 8
        const val presenceBorder = 2
        const val groupIndent = 16
        const val signOutGap = 24
        const val closeBelowSafeArea = 14
        const val safeTopIslandApprox = 59
        const val searchDebounceMs = 80
        const val draftDebounceMs = 150
        const val localSkeletonAfterMs = 50
        const val clipboardClearMs = 60000
        const val caretHz = 8
        const val relinkChipS = 30
        const val notificationCollapseS = 5
        const val chipHitTarget = 44
        const val flingBlurResumeMs = 80
    }

    object Feeling {
        const val coldStartIphoneMs = 400
        const val coldStartAndroid35Ms = 700
        const val coldStartAndroid28Ms = 1200
        const val coldStartWebMs = 1000
        const val openThreadMs = 100
        const val sendToBubbleMs = 16
        const val scrollP99Ms = 8.3
        const val callStageMs = 150
        const val callAudioMedianMs = 1500
        const val callAudioCeilingMs = 3000
        const val miniAppSnapshotMs = 16
        const val miniAppHtmlMacMs = 500
        const val androidBlurFallbackAlpha = 0.92
        const val hapticMaxPulseMs = HapticMaxPulseMs
    }
}

private enum class ZoenGlassRole {
    Nav,
    Search,
    Composer,
}

data class ZoenPalette(
    val bg: Color,
    val textPrimary: Color,
    val textSecondary: Color,
    val textOn: Color,
    val fillPrimary: Color,
    val thinBg: Color,
    val regularBg: Color,
    val composerBg: Color,
    val strokePrimary: Color,
    val strokeSecondary: Color,
    val strokeComposer: Color,
    val rowValue: Color,
    val skyGlow: Color,
    val textTertiary: Color,
    val bubbleAgent: Color,
    val bubbleUser: Color,
    val bubbleUserOnBackdrop: Color,
    val bubbleUserOnBackdropText: Color,
    val bubbleAgentOnBackdrop: Color,
    val bubbleAgentOnBackdropText: Color,
    val cellBg: Color,
    val systemRed: Color,
    val systemGreen: Color,
    val systemOrange: Color,
) {
    fun forContrast(increased: Boolean): ZoenPalette =
        if (!increased) {
            this
        } else {
            copy(
                textSecondary = textPrimary.copy(alpha = 0.7f),
                strokePrimary = textPrimary.copy(alpha = 0.3f),
            )
        }
    companion object {
        private fun argb(hex: Long, alpha: Float = 1f): Color {
            val rgb = hex and 0xFFFFFF
            return Color(
                red = ((rgb shr 16) and 0xFF) / 255f,
                green = ((rgb shr 8) and 0xFF) / 255f,
                blue = (rgb and 0xFF) / 255f,
                alpha = alpha,
            )
        }

        val light = ZoenPalette(
            bg = argb(0xFFFFFF),
            textPrimary = argb(0x1D1D1F),
            textSecondary = argb(0x1D1D1F, 0.5f),
            textOn = argb(0xF9F9FA),
            fillPrimary = argb(0x1D1D1F),
            thinBg = argb(0xFFFFFF, 0.55f),
            regularBg = argb(0xFFFFFF, 0.668f),
            composerBg = argb(0xFFFFFF, 0.55f),
            strokePrimary = argb(0x1D1D1F, 0.14f),
            strokeSecondary = argb(0x1D1D1F, 0.07f),
            strokeComposer = argb(0xF5F5F7, 0.5f),
            rowValue = argb(0x1D1D1F, 0.5f),
            skyGlow = argb(0x1D1D1F, 0.06f),
            textTertiary = argb(0x1D1D1F, 0.3f),
            bubbleAgent = argb(0xFFFFFF, 0.98f),
            bubbleUser = argb(0xFFFFFF, 0.3f),
            bubbleUserOnBackdrop = argb(0x1D1D1F),
            bubbleUserOnBackdropText = argb(0xF9F9FA),
            bubbleAgentOnBackdrop = argb(0xF5F5F7),
            bubbleAgentOnBackdropText = argb(0x1D1D1F),
            cellBg = argb(0xFFFFFF, 0.6f),
            systemRed = argb(0xDA2F35),
            systemGreen = argb(0x00A665),
            systemOrange = argb(0xFF4D00),
        )
        val dark = ZoenPalette(
            bg = argb(0x000000),
            textPrimary = argb(0xF5F5F7),
            textSecondary = argb(0xF5F5F7, 0.5f),
            textOn = argb(0x1D1D1F),
            fillPrimary = argb(0xF5F5F7),
            thinBg = argb(0x1D1D1F, 0.615f),
            regularBg = argb(0x1D1D1F, 0.618f),
            composerBg = argb(0x1D1D1F, 0.618f),
            strokePrimary = argb(0xF5F5F7, 0.14f),
            strokeSecondary = argb(0xF5F5F7, 0.07f),
            strokeComposer = argb(0x1D1D1F, 0.5f),
            rowValue = argb(0xFFFFFF, 0.5f),
            skyGlow = argb(0xF5F5F7, 0.08f),
            textTertiary = argb(0xF5F5F7, 0.3f),
            bubbleAgent = argb(0x1D1D1F, 0.7f),
            bubbleUser = argb(0x1D1D1F, 0.3f),
            bubbleUserOnBackdrop = argb(0xF5F5F7),
            bubbleUserOnBackdropText = argb(0x1D1D1F),
            bubbleAgentOnBackdrop = argb(0x1D1D1F),
            bubbleAgentOnBackdropText = argb(0xF5F5F7),
            cellBg = argb(0xFFFFFF, 0.05f),
            systemRed = argb(0xF1555B),
            systemGreen = argb(0x00A665),
            systemOrange = argb(0xFF4D00),
        )

        fun resolve(
            appearance: ZoenAppearance,
            systemDark: Boolean,
            increasedContrast: Boolean = false,
        ): ZoenPalette = (if (appearance.isDark(systemDark)) dark else light).forContrast(increasedContrast)
    }
}

val LocalZoenPalette = compositionLocalOf { ZoenPalette.dark }
private val LocalZoenRoot = compositionLocalOf { IntSize.Zero }

private class SuperellipseShape(private val radius: Dp) : Shape {
    override fun createOutline(size: Size, layoutDirection: LayoutDirection, density: Density): Outline {
        val radiusPx = with(density) { radius.toPx() }
        return Outline.Generic(superellipsePath(size, radiusPx))
    }
}

private fun superellipsePath(size: Size, radius: Float): Path {
    val s = min(radius * 1.53f, min(size.width, size.height) / 2f)
    val pull = s * 0.45f
    return Path().apply {
        moveTo(s, 0f)
        lineTo(size.width - s, 0f)
        cubicTo(size.width - pull, 0f, size.width, pull, size.width, s)
        lineTo(size.width, size.height - s)
        cubicTo(size.width, size.height - pull, size.width - pull, size.height, size.width - s, size.height)
        lineTo(s, size.height)
        cubicTo(pull, size.height, 0f, size.height - pull, 0f, size.height - s)
        lineTo(0f, s)
        cubicTo(0f, pull, pull, 0f, s, 0f)
        close()
    }
}

private fun pillPath(size: Size): Path {
    val radius = min(size.width, size.height) / 2f
    return Path().apply {
        addRoundRect(RoundRect(Rect(0f, 0f, size.width, size.height), CornerRadius(radius, radius)))
    }
}

private fun Modifier.zoenRing(color: Color, radius: Dp, stroke: Dp, pill: Boolean): Modifier = drawBehind {
    val strokePx = stroke.toPx()
    if (size.width <= strokePx * 2f || size.height <= strokePx * 2f) return@drawBehind
    val outer = if (pill) pillPath(size) else superellipsePath(size, radius.toPx())
    val innerSize = Size(size.width - strokePx * 2f, size.height - strokePx * 2f)
    val innerRadius = if (pill) min(innerSize.width, innerSize.height) / 2f else (radius.toPx() - strokePx).coerceAtLeast(0f)
    val inner = if (pill) pillPath(innerSize) else superellipsePath(innerSize, innerRadius)
    val ring = Path().apply {
        addPath(outer)
        addPath(inner, Offset(strokePx, strokePx))
        fillType = PathFillType.EvenOdd
    }
    drawPath(ring, color)
}

@Composable
private fun zoenReduceMotion(): Boolean {
    val resolver = LocalContext.current.contentResolver
    val scale = Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f)
    return scale == 0f
}

@Composable
private fun rememberReduceTransparency(): Pair<Boolean, (Boolean) -> Unit> {
    val context = LocalContext.current
    val prefs = remember { context.getSharedPreferences(MotionPrefs, android.content.Context.MODE_PRIVATE) }
    var value by remember { mutableStateOf(prefs.getBoolean(ReduceTransparencyKey, false)) }
    val write: (Boolean) -> Unit = { next ->
        value = next
        prefs.edit().putBoolean(ReduceTransparencyKey, next).apply()
    }
    return value to write
}

@Composable
private fun ZoenPress(reduceMotion: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier, role: Role? = null, content: @Composable () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(
        targetValue = if (pressed && !reduceMotion) 0.97f else 1f,
        animationSpec = tween(durationMillis = if (reduceMotion) 1 else 120, easing = EaseOut),
        label = "press",
    )
    Box(
        modifier
            .scale(scale)
            .sizeIn(minWidth = 44.dp, minHeight = 44.dp)
            .clickable(interactionSource = interaction, indication = null, role = role, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { content() }
}

@Composable
fun ZoenSky(modifier: Modifier = Modifier) {
    val palette = LocalZoenPalette.current
    Box(modifier.background(palette.bg)) {
        Box(
            Modifier
                .align(Alignment.TopCenter)
                .fillMaxWidth()
                .fillMaxHeight(0.333f)
                .background(Brush.radialGradient(listOf(palette.skyGlow, Color.Transparent))),
        )
    }
}

@Composable
private fun ZoenGlassSurface(
    role: ZoenGlassRole,
    shape: Shape,
    reduceTransparency: Boolean,
    modifier: Modifier = Modifier,
    pill: Boolean = false,
    ring: Boolean = false,
    backdrop: (@Composable () -> Unit)? = null,
    content: @Composable () -> Unit,
) {
    val palette = LocalZoenPalette.current
    val canBlur = !reduceTransparency && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
    val blur = when (role) {
        ZoenGlassRole.Nav, ZoenGlassRole.Composer -> 50.dp
        ZoenGlassRole.Search -> 8.dp
    }
    val tokenFill = when (role) {
        ZoenGlassRole.Nav, ZoenGlassRole.Search -> palette.thinBg
        ZoenGlassRole.Composer -> palette.composerBg
    }
    val fill = if (!reduceTransparency && !canBlur) tokenFill.copy(alpha = 0.92f) else tokenFill
    val root = LocalZoenRoot.current
    val density = LocalDensity.current
    var origin by remember { mutableStateOf(Offset.Zero) }
    val stroke = ring || reduceTransparency || !canBlur
    Box(modifier.onGloballyPositioned { origin = it.positionInRoot() }) {
        Box(Modifier.matchParentSize().clip(shape)) {
            if (canBlur && backdrop != null && root != IntSize.Zero) {
                Box(Modifier.matchParentSize().clipToBounds()) {
                    Box(
                        Modifier
                            .requiredSize(
                                with(density) { root.width.toDp() },
                                with(density) { root.height.toDp() },
                            )
                            .offset { IntOffset(-origin.x.roundToInt(), -origin.y.roundToInt()) }
                            .blur(blur, edgeTreatment = BlurredEdgeTreatment.Unbounded),
                    ) { backdrop() }
                }
            } else if (reduceTransparency || !canBlur) {
                Box(Modifier.matchParentSize().background(palette.bg))
            }
            Box(Modifier.matchParentSize().background(fill))
        }
        Box(Modifier.clip(shape)) { content() }
        if (stroke) {
            Box(
                Modifier.matchParentSize().zoenRing(
                    color = if (ring) palette.strokeComposer else palette.strokePrimary,
                    radius = 24.dp,
                    stroke = 1.dp,
                    pill = pill,
                ),
            )
        }
    }
}

@Composable
fun ZoenPill(
    items: List<String>,
    index: Int,
    onIndex: (Int) -> Unit,
    reduceMotion: Boolean,
    reduceTransparency: Boolean,
    modifier: Modifier = Modifier,
) {
    val palette = LocalZoenPalette.current
    var from by remember { mutableFloatStateOf(index.toFloat()) }
    var target by remember { mutableFloatStateOf(index.toFloat()) }
    var fraction by remember { mutableFloatStateOf(1f) }
    LaunchedEffect(index, reduceMotion) {
        val shown = from + (target - from) * segmentEase(fraction)
        from = shown
        target = index.toFloat()
        if (reduceMotion || shown == target) {
            fraction = 1f
            return@LaunchedEffect
        }
        val anim = Animatable(0f)
        fraction = 0f
        anim.animateTo(1f, tween(233, easing = LinearEasing)) { fraction = value }
    }
    val shown = from + (target - from) * segmentEase(fraction)
    ZoenGlassSurface(
        role = ZoenGlassRole.Nav,
        shape = RoundedCornerShape(percent = 50),
        reduceTransparency = reduceTransparency,
        modifier = modifier.height(48.dp),
        pill = true,
        backdrop = { ZoenSky(Modifier.fillMaxSize()) },
    ) {
        BoxWithConstraints(Modifier.fillMaxSize().padding(2.dp)) {
            if (items.isNotEmpty()) {
                val cell = maxWidth / items.size
                Box(
                    Modifier
                        .width(cell)
                        .fillMaxHeight()
                        .offset(x = cell * shown)
                        .background(palette.fillPrimary, RoundedCornerShape(22.dp)),
                )
            }
            Row(Modifier.fillMaxSize()) {
                items.forEachIndexed { item, title ->
                    val selected = item == index
                    ZoenPress(reduceMotion = reduceMotion, onClick = { onIndex(item) }, modifier = Modifier.weight(1f).fillMaxSize(), role = Role.Tab) {
                        Text(
                            title,
                            color = if (selected) palette.textOn else palette.textSecondary,
                            fontFamily = FontFamily.Default,
                            fontWeight = FontWeight.Medium,
                            fontSize = 15.sp,
                            modifier = Modifier.semantics { this.selected = selected },
                        )
                    }
                }
            }
        }
    }
}

@Composable
fun ZoenComposer(
    text: String,
    onText: (String) -> Unit,
    reduceMotion: Boolean,
    reduceTransparency: Boolean,
    modifier: Modifier = Modifier,
    onSend: () -> Unit = {},
) {
    val palette = LocalZoenPalette.current
    val empty = text.isBlank()
    ZoenGlassSurface(
        role = ZoenGlassRole.Composer,
        shape = SuperellipseShape(24.dp),
        reduceTransparency = reduceTransparency,
        modifier = modifier.heightIn(min = 48.dp),
        ring = true,
        backdrop = { ZoenSky(Modifier.fillMaxSize()) },
    ) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Bottom) {
            ComposerIcon(Icons.Rounded.Add, "Anexar", reduceMotion, palette.textSecondary) {}
            BasicTextField(
                value = text,
                onValueChange = onText,
                textStyle = TextStyle(
                    color = palette.textPrimary,
                    fontFamily = FontFamily.Default,
                    fontSize = 15.sp,
                    lineHeight = 19.sp,
                ),
                cursorBrush = SolidColor(palette.textPrimary),
                modifier = Modifier.weight(1f).heightIn(min = 48.dp, max = 219.dp).padding(horizontal = 8.dp, vertical = 14.5.dp),
                decorationBox = { inner ->
                    Box {
                        if (text.isEmpty()) {
                            Text("Mensagem", color = palette.textSecondary, fontSize = 15.sp, fontFamily = FontFamily.Default)
                        }
                        inner()
                    }
                },
            )
            if (empty) {
                ComposerIcon(Icons.Rounded.Mic, "Ditado", reduceMotion, palette.textSecondary) {}
            } else {
                ZoenPress(reduceMotion = reduceMotion, onClick = onSend, modifier = Modifier.size(44.dp)) {
                    Box(Modifier.size(32.dp).background(palette.fillPrimary, RoundedCornerShape(percent = 50)), contentAlignment = Alignment.Center) {
                        Icon(Icons.Rounded.ArrowUpward, "Enviar", tint = palette.textOn, modifier = Modifier.size(18.dp))
                    }
                }
            }
        }
    }
}

@Composable
private fun ComposerIcon(
    image: ImageVector,
    label: String,
    reduceMotion: Boolean,
    tint: Color,
    onClick: () -> Unit,
) {
    ZoenPress(reduceMotion = reduceMotion, onClick = onClick, modifier = Modifier.size(44.dp)) {
        Icon(image, label, tint = tint, modifier = Modifier.size(32.dp))
    }
}

@Composable
fun ZoenCard(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val palette = LocalZoenPalette.current
    val light = palette.bg.red > 0.5f && palette.bg.green > 0.5f && palette.bg.blue > 0.5f
    Column(
        modifier
            .clip(SuperellipseShape(24.dp))
            .background(palette.regularBg)
            .then(
                if (light) {
                    Modifier.zoenRing(Color.White.copy(alpha = 0.9f), 24.dp, 1.dp, pill = false)
                } else {
                    Modifier
                },
            )
            .padding(16.dp),
    ) { content() }
}

@Composable
fun ZoenRow(
    title: String,
    reduceMotion: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    value: String? = null,
    selected: Boolean? = null,
    showsDivider: Boolean = false,
) {
    val palette = LocalZoenPalette.current
    Column(modifier.fillMaxWidth()) {
        if (showsDivider) {
            Box(Modifier.padding(horizontal = 20.dp).fillMaxWidth().height(1.dp).background(palette.strokeSecondary))
        }
        ZoenPress(
            reduceMotion = reduceMotion,
            onClick = onClick,
            modifier = Modifier.fillMaxWidth().heightIn(min = if (selected == null) 56.dp else 59.8.dp),
            role = if (selected == null) Role.Button else Role.RadioButton,
        ) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(title, color = palette.textPrimary, fontFamily = FontFamily.Default, fontWeight = FontWeight.Medium, fontSize = 15.sp, modifier = Modifier.weight(1f))
                if (value != null) {
                    Text(value, color = palette.rowValue, fontFamily = FontFamily.Default, fontSize = 15.sp)
                }
                if (selected != null) {
                    Box(
                        Modifier.size(18.dp).semantics { this.selected = selected },
                        contentAlignment = Alignment.Center,
                    ) {
                        Box(Modifier.fillMaxSize().background(Color.Transparent, RoundedCornerShape(percent = 50)).zoenRing(palette.strokePrimary, 9.dp, 1.5.dp, pill = true))
                        if (selected) {
                            Box(Modifier.size(10.dp).background(palette.fillPrimary, RoundedCornerShape(percent = 50)))
                        }
                    }
                }
            }
        }
    }
}

@Composable
fun ZoenDialog(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val palette = LocalZoenPalette.current
    Column(
        modifier
            .clip(SuperellipseShape(24.dp))
            .background(palette.regularBg)
            .zoenRing(palette.strokePrimary, 24.dp, 1.dp, pill = false),
    ) {
        content()
    }
}

@Composable
fun ZoenHostRing(modifier: Modifier = Modifier) {
    val palette = LocalZoenPalette.current
    Box(modifier.zoenRing(palette.strokePrimary, 24.dp, 1.dp, pill = false))
}

@Composable
fun ZoenPrimitivesGallery() {
    val systemDark = isSystemInDarkTheme()
    var appearance by remember { mutableStateOf(ZoenAppearance.System) }
    val palette = ZoenPalette.resolve(appearance, systemDark, zoenIncreasedContrast())
    val reduceMotion = zoenReduceMotion()
    val (reduceTransparency, setReduceTransparency) = rememberReduceTransparency()
    var segment by remember { mutableIntStateOf(0) }
    var draft by remember { mutableStateOf("") }
    var sheet by remember { mutableStateOf(false) }
    var root by remember { mutableStateOf(IntSize.Zero) }
    val opener = remember { FocusRequester() }
    val close = remember { FocusRequester() }
    val scale = remember { Animatable(1f) }
    val blur = remember { Animatable(0f) }
    var sheetWasOpen by remember { mutableStateOf(false) }
    LaunchedEffect(sheet) {
        if (sheet) {
            sheetWasOpen = true
            if (reduceMotion) {
                scale.snapTo(1f)
                blur.snapTo(0f)
            } else {
                scale.snapTo(0.96f)
                blur.snapTo(4f)
                launch {
                    blur.animateTo(0f, tween(220, easing = CubicBezierEasing(0f, 0f, 0.2f, 1f)))
                }
                scale.animateTo(1f, tween(220, easing = CubicBezierEasing(0f, 0f, 0.2f, 1f)))
            }
            close.requestFocus()
        } else if (sheetWasOpen) {
            opener.requestFocus()
        }
    }
    CompositionLocalProvider(LocalZoenPalette provides palette, LocalZoenRoot provides root) {
        Box(Modifier.fillMaxSize().onGloballyPositioned { root = it.size }) {
            ZoenSky(Modifier.fillMaxSize())
            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Text("Primitivas", color = palette.textPrimary, fontFamily = FontFamily.Default, fontWeight = FontWeight.Medium, fontSize = 20.sp)
                ZoenPill(listOf("Um", "Dois", "Três"), segment, { segment = it }, reduceMotion, reduceTransparency, Modifier.fillMaxWidth())
                ZoenComposer(draft, { draft = it }, reduceMotion, reduceTransparency, Modifier.fillMaxWidth())
                ZoenCard(Modifier.fillMaxWidth()) {
                    Text("Notas", color = palette.textSecondary, fontSize = 14.sp, fontFamily = FontFamily.Default)
                    Text("12", color = palette.textPrimary, fontSize = 24.sp, fontWeight = FontWeight.Medium, fontFamily = FontFamily.Default)
                }
                ZoenPress(reduceMotion, { sheet = true }, Modifier.focusRequester(opener).fillMaxWidth().height(48.dp)) {
                    Box(Modifier.fillMaxSize().background(palette.fillPrimary, RoundedCornerShape(percent = 50)), contentAlignment = Alignment.Center) {
                        Text("Ajustes", color = palette.textOn, fontFamily = FontFamily.Default, fontWeight = FontWeight.Medium, fontSize = 15.sp)
                    }
                }
            }
            if (sheet) {
                Box(
                    Modifier
                        .fillMaxSize()
                        .scale(scale.value)
                        .then(if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && blur.value > 0f) Modifier.blur(blur.value.dp) else Modifier),
                ) {
                    ZoenSky(Modifier.fillMaxSize())
                    Column(Modifier.fillMaxSize().padding(start = 16.dp, end = 16.dp, top = 76.dp)) {
                        Column(
                            Modifier
                                .fillMaxWidth()
                                .clip(SuperellipseShape(24.dp))
                                .background(palette.thinBg),
                        ) {
                            ZoenAppearance.entries.forEachIndexed { offset, item ->
                                ZoenRow(
                                    title = item.label,
                                    reduceMotion = reduceMotion,
                                    onClick = { appearance = item },
                                    selected = appearance == item,
                                    showsDivider = offset > 0,
                                )
                            }
                            ZoenRow(
                                title = "Transparência reduzida",
                                reduceMotion = reduceMotion,
                                onClick = { setReduceTransparency(!reduceTransparency) },
                                selected = reduceTransparency,
                                showsDivider = true,
                            )
                        }
                    }
                    ZoenPress(
                        reduceMotion,
                        { sheet = false },
                        Modifier.align(Alignment.TopEnd).padding(top = 14.dp, end = 16.dp).focusRequester(close).size(48.dp),
                    ) {
                        Box(Modifier.fillMaxSize().background(palette.thinBg, RoundedCornerShape(percent = 50)), contentAlignment = Alignment.Center) {
                            Icon(Icons.Rounded.Close, "Fechar", tint = palette.textPrimary)
                        }
                    }
                }
            }
        }
    }
}

@Preview(name = "Primitivas", widthDp = 390, heightDp = 844)
@Composable
private fun ZoenPrimitivesGalleryPreview() {
    ZoenPrimitivesGallery()
}
