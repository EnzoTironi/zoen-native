package xyz.tironi.zoen.ui

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import xyz.tironi.zoen.R
import xyz.tironi.zoen.media.detectVoiceHoldGestures

@Composable
fun PlusVoiceButton(actions: () -> Unit, start: () -> Unit, release: () -> Unit, lock: () -> Unit, cancel: () -> Unit, compact: Boolean = false) {
    val haptics = LocalHapticFeedback.current
    val density = LocalDensity.current
    val cancelDistance = with(density) { 110.dp.toPx() }
    val lockDistance = with(density) { 80.dp.toPx() }
    val recordLabel = stringResource(R.string.media_hold_zoen)
    val currentStart by rememberUpdatedState(start)
    val currentRelease by rememberUpdatedState(release)
    val currentLock by rememberUpdatedState(lock)
    val currentCancel by rememberUpdatedState(cancel)
    val modifier = Modifier.testTag("home-plus").semantics {
        customActions = listOf(CustomAccessibilityAction(recordLabel) { currentStart(); currentLock(); true })
    }.pointerInput(cancelDistance, lockDistance) {
        detectVoiceHoldGestures(cancelDistance, lockDistance,
            onStart = { haptics.performHapticFeedback(HapticFeedbackType.LongPress); currentStart() },
            onRelease = { currentRelease() },
            onLock = { currentLock(); haptics.performHapticFeedback(HapticFeedbackType.LongPress) },
            onCancel = { currentCancel(); haptics.performHapticFeedback(HapticFeedbackType.Reject) })
    }
    if (compact) FloatingActionButton(onClick = actions, modifier = modifier) { Icon(Icons.Rounded.Add, stringResource(R.string.ask_zoen)) }
    else ExtendedFloatingActionButton(onClick = actions, icon = { Icon(Icons.Rounded.Add, null) }, text = { Text(stringResource(R.string.ask_zoen)) }, modifier = modifier)
}
