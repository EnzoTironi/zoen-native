package xyz.tironi.zoen.ui

import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Edit
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import xyz.tironi.zoen.core.Persona

internal object AppleChromeMetrics {
    const val Circle = 56
    const val BarHeight = 62
    const val Gap = 10
    const val Inset = 16
    const val Bottom = 2
    const val ComposerHeight = 48
    const val Attachment = 44
    const val PlusHoldMillis = 700L
    const val ComposerHoldMillis = 230L
}

/** Four real destinations and a separate New message action, matching the native TabView.
 * OS navigation insets remain intact: no screenshot crop participates in layout. */
@Composable
internal fun AppleBottomBar(tab: Tab, unread: Int, pending: Int, me: Persona?, select: (Tab) -> Unit, newMessage: () -> Unit, pendingIds: Set<String> = emptySet(), transportActive: Boolean = false) {
    // Native f30d5149: 402×812 app content after the status-only crop. The
    // detached action is (319,729,62,62), leaving 21pt of app content below it.
    // Android's real OS navigation inset is additional, not a crop of this gap.
    Row(Modifier.fillMaxWidth().navigationBarsPadding().padding(start = 20.dp, end = 21.dp, bottom = 21.dp).testTag("bottom-bar"),
        horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.Top) {
        Row(Modifier.weight(1f).height(61.dp).zoenGlass(RoundedCornerShape(31.dp)).padding(4.dp)) {
            Tab.main.forEach { target ->
                val on = tab == target
                val interactions = androidx.compose.runtime.remember { androidx.compose.foundation.interaction.MutableInteractionSource() }
                val pressed by interactions.collectIsPressedAsState()
                val pressScale by androidx.compose.animation.core.animateFloatAsState(
                    if (target == Tab.Profile && pressed && rememberMotionEnabled()) .96f else 1f,
                    androidx.compose.animation.core.tween(150), label = "avatar press")
                Box(Modifier.weight(1f).fillMaxHeight()
                    .graphicsLayer { scaleX = pressScale; scaleY = pressScale }
                    .background(if (on) MaterialTheme.colorScheme.primary.copy(alpha = .14f) else Color.Transparent, CircleShape)
                    .clickable(interactionSource = interactions, indication = null, onClickLabel = tabName(target)) { select(target) }
                    .testTag("tab:${target.name}").semantics { selected = on }, contentAlignment = Alignment.Center) {
                    val color = if (on) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
                    CompositionLocalProvider(LocalContentColor provides color) {
                        NativeIconMotion(selection = on,
                            pendingIds = if (target == Tab.Notifications) pendingIds else emptySet(),
                            active = target == Tab.Chats && transportActive) {
                            TabIcon(target, if (target == Tab.Chats) unread else if (target == Tab.Notifications) pending else 0, on, me)
                        }
                    }
                }
            }
        }
        IconButton(newMessage, Modifier.size(62.dp).zoenGlass(CircleShape).testTag("newChat")) {
            Icon(Icons.Rounded.Edit, "New message", Modifier.size(26.dp))
        }
    }
}
