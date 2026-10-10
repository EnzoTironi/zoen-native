package xyz.tironi.zoen.ui

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.MaterialTheme
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import xyz.tironi.zoen.ui.ink.ZoenGlyph
import androidx.compose.foundation.interaction.MutableInteractionSource
import xyz.tironi.zoen.R
import xyz.tironi.zoen.media.detectVoiceHoldGestures

@Composable
fun PlusVoiceButton(actions: () -> Unit, start: () -> Unit, release: () -> Unit, lock: () -> Unit, cancel: () -> Unit, compact: Boolean = false) {
    val haptics = rememberZoenHaptics()
    val interactions = remember { MutableInteractionSource() }
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
            onStart = { currentStart() },
            onRelease = { currentRelease() },
            onLock = { currentLock() },
            onCancel = { currentCancel() })
    }
    if (compact) FloatingActionButton(onClick = { haptics.perform(ZoenFeedback.Open); actions() }, modifier = modifier.size(52.dp), shape = CircleShape, containerColor = MaterialTheme.colorScheme.primary, contentColor = MaterialTheme.colorScheme.onPrimary, interactionSource = interactions) { ZoenIcon(ZoenGlyph.Plus, stringResource(R.string.ask_zoen), interactions = interactions) }
    else ExtendedFloatingActionButton(onClick = { haptics.perform(ZoenFeedback.Open); actions() }, icon = { ZoenIcon(ZoenGlyph.Plus, interactions = interactions) }, text = { Text(stringResource(R.string.ask_zoen)) }, modifier = modifier, interactionSource = interactions)
}
