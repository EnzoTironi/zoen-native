package xyz.tironi.zoen.ui

import android.graphics.drawable.AnimatedVectorDrawable
import android.widget.ImageView
import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import xyz.tironi.zoen.R

/** Checkmark-circle vector draws only after a confirmed core operation; no motion on first render. */
@Composable
fun LivingSpaceFeedback(label: String, revision: Int, positive: Boolean = true) {
    val motion = rememberMotionEnabled()
    val tint = MaterialTheme.colorScheme.primary.toArgb()
    if (revision > 0) Row(Modifier.testTag("living-space-confirmation"), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        key(revision, motion, positive) { AndroidView(modifier = Modifier.size(24.dp), factory = { context -> ImageView(context).apply {
            tag = "living-space-feedback"
            importantForAccessibility = android.view.View.IMPORTANT_FOR_ACCESSIBILITY_NO
            setImageResource(if (positive) { if (motion) R.drawable.living_space_success else R.drawable.ic_living_space_success } else { if (motion) R.drawable.living_space_declined else R.drawable.ic_living_space_declined })
            imageTintList = android.content.res.ColorStateList.valueOf(tint)
            if (motion) (drawable as? AnimatedVectorDrawable)?.start()
        } }) }
        Text(label, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.primary)
    }
}
