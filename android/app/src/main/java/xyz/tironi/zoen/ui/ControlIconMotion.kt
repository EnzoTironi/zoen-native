package xyz.tironi.zoen.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.currentStateAsState

internal const val ICON_REPLACEMENT_MS = 200
internal const val ICON_PRESS_MS = 150
internal fun iconPressScale(pressed: Boolean, running: Boolean): Float =
    if (pressed && running) .96f else 1f

@Composable
internal fun rememberIconMotionRunning(): Boolean {
    val enabled = rememberMotionEnabled()
    val lifecycle by LocalLifecycleOwner.current.lifecycle.currentStateAsState()
    return enabled && lifecycle.isAtLeast(Lifecycle.State.RESUMED)
}

/** State-keyed reveal/replacement on the artwork only. Does not duplicate or retain
 * outgoing controls (especially the recording gesture owner). Timing/contours are
 * provisional until native footage; this is not an SF magic-effect equivalence. */
@Composable
internal fun Modifier.iconReveal(state: Any?): Modifier {
    val running = rememberIconMotionRunning()
    val reveal = remember { Animatable(if (running) 0f else 1f) }
    var previous by remember { mutableStateOf(state) }
    var appeared by remember { mutableStateOf(false) }
    LaunchedEffect(state, running) {
        val changed = !appeared || state != previous
        previous = state
        appeared = true
        if (running && changed) {
            reveal.snapTo(0f)
            reveal.animateTo(1f, tween(ICON_REPLACEMENT_MS))
        } else reveal.snapTo(1f)
    }
    return drawWithContent { clipRect(right = size.width * reveal.value) { this@drawWithContent.drawContent() } }
}

@Composable
internal fun Modifier.iconPress(interactions: MutableInteractionSource): Modifier {
    val pressed by interactions.collectIsPressedAsState()
    val running = rememberIconMotionRunning()
    val scale = remember { Animatable(1f) }
    LaunchedEffect(pressed, running) {
        val target = iconPressScale(pressed, running)
        if (running) scale.animateTo(target, tween(ICON_PRESS_MS)) else scale.snapTo(1f)
    }
    return graphicsLayer { scaleX = scale.value; scaleY = scale.value }
}

@Composable
internal fun MotionVectorIcon(icon: ImageVector, description: String?, modifier: Modifier = Modifier,
                              state: Any? = icon, tint: Color = LocalContentColor.current,
                              interactions: MutableInteractionSource? = null) {
    val native = nativeSymbolForMaterial(icon.name)
    val pressed = interactions?.collectIsPressedAsState()?.value ?: false
    var taps by remember { mutableIntStateOf(0) }
    var wasPressed by remember { mutableStateOf(pressed) }
    LaunchedEffect(pressed) {
        if (wasPressed && !pressed) taps++
        wasPressed = pressed
    }
    if (native != null) {
        CompositionLocalProvider(LocalContentColor provides tint) {
            NativeIconMotion(selection = state) { NativeAnimatedSymbol(native, description, modifier, press = taps) }
        }
    } else {
        Icon(icon, description, modifier, tint = tint)
    }
}

internal fun nativeSymbolForMaterial(name: String): String? = when (name.substringAfterLast('.')) {
    "Call" -> "phone"
    "VideoCall" -> "video"
    "MoreVert" -> "more"
    "Close", "Cancel" -> "close"
    "Add" -> "plus"
    "ArrowBack" -> "back"
    "ChevronRight", "KeyboardArrowRight" -> "chevron"
    "Search" -> "search"
    "Lock", "Security", "VerifiedUser" -> "lock"
    "Info", "ErrorOutline" -> "info"
    "Person", "PersonOutline", "AccountCircle" -> "you"
    "Description", "Article" -> "note"
    "Folder", "FolderOpen" -> "folder"
    "Delete", "DeleteOutline" -> "trash"
    "Check", "Done", "TaskAlt", "CheckCircle", "CheckCircleOutline" -> "check"
    "Notifications", "NotificationsNone", "NotificationsActive" -> "bell"
    "IosShare", "Share" -> "share"
    "Favorite" -> "heart"
    "AutoAwesome" -> "sparkle"
    "Forum", "Chat", "ChatBubbleOutline", "Sms" -> "chats"
    "Layers" -> "spaces"
    "CameraAlt", "PhotoCamera" -> "camera"
    "Edit", "Draw" -> "compose"
    "SportsEsports" -> "game"
    "PushPin" -> "pin"
    "PlayArrow" -> "play"
    "Pause" -> "pause"
    "Storefront", "Marketplace" -> "marketplace"
    "Settings" -> "settings"
    "Translate", "Language" -> "language"
    "Contrast" -> "contrast"
    "Devices" -> "devices"
    "Storage" -> "storage"
    "CloudQueue", "CloudDone", "CloudSync", "CloudOff" -> "cloud"
    "AccessibilityNew", "Animation", "DirectionsWalk" -> "accessibility"
    "EventRepeat", "Timer", "Schedule", "Restore", "History" -> "clock"
    "Explore", "Public" -> "compass"
    "Flag" -> "flag"
    "CardGiftcard" -> "gift"
    "CreditCard" -> "wallet"
    "Key" -> "key"
    "AlternateEmail" -> "at"
    else -> null
}

@Composable
internal fun MotionIconButton(icon: ImageVector, description: String?, onClick: () -> Unit,
                              modifier: Modifier = Modifier, enabled: Boolean = true) {
    val interactions = remember { MutableInteractionSource() }
    NativeIconButton(onClick, modifier, enabled, interactionSource = interactions) {
        MotionVectorIcon(icon, description, interactions = interactions)
    }
}
