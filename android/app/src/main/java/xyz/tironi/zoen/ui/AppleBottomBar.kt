package xyz.tironi.zoen.ui

import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Search
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.text.font.FontWeight
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

/** Three real destinations and a detached unified Search/start-chat action.
 * OS navigation insets remain intact: no screenshot crop participates in layout. */
@Composable
internal fun AppleBottomBar(tab: Tab, unread: Int, pending: Int, me: Persona?, select: (Tab) -> Unit, newMessage: () -> Unit, pendingIds: Set<String> = emptySet(), transportActive: Boolean = false, offline: Boolean = false) {
    // The native footer clears the bottom edge by 21pt. Android's gesture inset
    // already supplies that clearance; adding another 21dp floated the bar too high.
    Row(Modifier.fillMaxWidth().navigationBarsPadding().padding(start = 20.dp, end = 21.dp).testTag("bottom-bar"),
        horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.Top) {
        Row(Modifier.weight(1f).height(61.dp).zoenGlass(RoundedCornerShape(31.dp)).padding(4.dp)) {
            Tab.main.forEach { target ->
                val on = tab == target
                var taps by remember { mutableIntStateOf(0) }
                val interactions = androidx.compose.runtime.remember { androidx.compose.foundation.interaction.MutableInteractionSource() }
                val pressed by interactions.collectIsPressedAsState()
                val pressScale by androidx.compose.animation.core.animateFloatAsState(
                    if (target == Tab.Profile && pressed && rememberMotionEnabled()) .96f else 1f,
                    androidx.compose.animation.core.tween(150), label = "avatar press")
                Box(Modifier.weight(1f).fillMaxHeight()
                    .graphicsLayer { scaleX = pressScale; scaleY = pressScale }
                    .background(if (on) MaterialTheme.colorScheme.primary.copy(alpha = .14f) else Color.Transparent, CircleShape)
                    .clickable(interactionSource = interactions, indication = null, onClickLabel = tabName(target)) { taps++; select(target) }
                    .testTag("tab:${target.name}").semantics { selected = on }, contentAlignment = Alignment.Center) {
                    val color = if (on) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface
                    CompositionLocalProvider(LocalContentColor provides color) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            TabIcon(target, if (target == Tab.Chats) unread else 0,
                                on, me, press = taps,
                                active = target == Tab.Chats && transportActive)
                            Text(tabName(target), fontSize = 11.sp, lineHeight = 13.sp,
                                fontWeight = if (on) FontWeight.SemiBold else FontWeight.Medium, maxLines = 1)
                        }
                    }
                    if (target == Tab.Chats && offline) {
                        Box(Modifier.align(Alignment.BottomEnd).padding(6.dp).size(6.dp)
                            .background(MaterialTheme.colorScheme.onSurfaceVariant, CircleShape)
                            .semantics { contentDescription = "Offline" })
                    }
                }
            }
        }
        var composeTaps by remember { mutableIntStateOf(0) }
        NativeIconButton({ composeTaps++; newMessage() }, Modifier.size(62.dp).zoenGlass(CircleShape).testTag("newChat")) {
            MotionVectorIcon(Icons.Rounded.Search, "Search")
        }
    }
}
