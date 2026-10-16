package xyz.tironi.zoen.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import xyz.tironi.zoen.ui.ink.ZoenGlyph

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

@Composable
internal fun AppleBottomBar(tab: Tab, unread: Int, select: (Tab) -> Unit, search: () -> Unit, plus: @Composable () -> Unit) {
    Row(Modifier.fillMaxWidth().navigationBarsPadding().padding(start = 16.dp, end = 16.dp, bottom = 2.dp).testTag("bottom-bar"),
        horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
        IconButton(search, Modifier.size(56.dp).zoenGlass(CircleShape).testTag("bar-search")) {
            Icon(Icons.Rounded.Search, "Search", Modifier.size(23.dp))
        }
        Row(Modifier.weight(1f).height(62.dp).zoenGlass(RoundedCornerShape(31.dp)).padding(4.dp), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
            Tab.main.forEach { target ->
                val on = tab == target
                Row(Modifier.weight(if (on) 1.65f else 1f).fillMaxHeight()
                    .background(if (on) MaterialTheme.colorScheme.primary.copy(alpha = .14f) else androidx.compose.ui.graphics.Color.Transparent, CircleShape)
                    .clickable { select(target) }.testTag("tab:${target.name}").semantics { selected = on }
                    .padding(horizontal = if (on) 14.dp else 0.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally), verticalAlignment = Alignment.CenterVertically) {
                    val color = if (on) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
                    BadgedBox(badge = { if (target == Tab.Chats && unread > 0) Badge { Text(unread.coerceAtMost(99).toString(), fontSize = 10.sp) } }) {
                        ZoenIcon(if (target == Tab.Chats) ZoenGlyph.Chats else ZoenGlyph.Store, modifier = Modifier.size(23.dp), selected = on, tint = color)
                    }
                    if (on) Text(if (target == Tab.Chats) "Chats" else "Store", color = color, fontSize = 13.sp, fontWeight = FontWeight.Bold, maxLines = 1)
                }
            }
        }
        plus()
    }
}
