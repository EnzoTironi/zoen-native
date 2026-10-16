package xyz.tironi.zoen.ui

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
internal fun AppleBottomBar(tab: Tab, unread: Int, pending: Int, me: Persona?, select: (Tab) -> Unit, newMessage: () -> Unit) {
    Row(Modifier.fillMaxWidth().navigationBarsPadding().padding(start = 20.dp, end = 20.dp, bottom = 2.dp).testTag("bottom-bar"),
        horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        Row(Modifier.weight(1f).height(61.dp).zoenGlass(RoundedCornerShape(31.dp)).padding(4.dp)) {
            Tab.main.forEach { target ->
                val on = tab == target
                Box(Modifier.weight(1f).fillMaxHeight()
                    .background(if (on) MaterialTheme.colorScheme.primary.copy(alpha = .14f) else Color.Transparent, CircleShape)
                    .clickable(onClickLabel = tabName(target)) { select(target) }
                    .testTag("tab:${target.name}").semantics { selected = on }, contentAlignment = Alignment.Center) {
                    val color = if (on) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
                    CompositionLocalProvider(LocalContentColor provides color) {
                        TabIcon(target, if (target == Tab.Chats) unread else if (target == Tab.Notifications) pending else 0, on, me)
                    }
                }
            }
        }
        IconButton(newMessage, Modifier.size(63.dp).zoenGlass(CircleShape).testTag("newChat")) {
            Icon(Icons.Rounded.Edit, "New message", Modifier.size(26.dp))
        }
    }
}
