package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import kotlin.math.cos
import kotlin.math.sin

internal data class FanAction(val icon: ImageVector, val title: String, val choose: () -> Unit)
internal fun fanRadius(count: Int): Float = maxOf((count - 1).coerceAtLeast(0) * 56f / (Math.PI.toFloat() / 2), 64f)

/** iPhone's 90…180 degree arc, 48 visual / 56 hit diameter and 8 point arc gap. */
@Composable
internal fun AppleActionFan(actions: List<FanAction>, onDismiss: () -> Unit) {
    Popup(alignment = Alignment.BottomEnd, onDismissRequest = onDismiss, properties = PopupProperties(focusable = true)) {
        Box(Modifier.width(230.dp).height(230.dp).navigationBarsPadding().padding(end = 16.dp, bottom = 5.dp).testTag("quickActions"), contentAlignment = Alignment.BottomEnd) {
            val radius = fanRadius(actions.size)
            actions.forEachIndexed { index, action ->
                val angle = Math.PI / 2 + index * Math.PI / 2 / (actions.size - 1).coerceAtLeast(1)
                NativeIconButton(onClick = { onDismiss(); action.choose() }, modifier = Modifier.offset(x = (cos(angle) * radius).dp, y = (-sin(angle) * radius).dp).size(56.dp)) {
                    Box(Modifier.size(48.dp).zoenGlass(CircleShape), contentAlignment = Alignment.Center) { Icon(action.icon, action.title, Modifier.size(22.dp)) }
                }
            }
            NativeIconButton(onDismiss, Modifier.size(56.dp).zoenGlass(CircleShape, MaterialTheme.colorScheme.primary)) { MotionVectorIcon(Icons.Rounded.Close, "Close menu") }
        }
    }
}
