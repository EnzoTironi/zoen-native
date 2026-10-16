package xyz.tironi.zoen.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** SwiftUI's unboxed body actions; the platform still expands the pointer hit target. */
@Composable
fun LivingLink(label: String, onClick: () -> Unit, modifier: Modifier = Modifier, icon: ImageVector? = null, enabled: Boolean = true, verticalPadding: Dp = 0.dp) {
    val color = if (enabled) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface.copy(alpha = 0.38f)
    Row(modifier.clickable(enabled = enabled, role = Role.Button, onClick = onClick).padding(vertical = verticalPadding), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        if (icon != null) Icon(icon, null, Modifier.size(17.dp), tint = color)
        Text(label, style = MaterialTheme.typography.bodyLarge.copy(fontSize = 17.sp, lineHeight = 20.333f.sp), color = color)
    }
}
