package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Mic
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.dp
import xyz.tironi.zoen.ui.ink.ZoenGlyph
import xyz.tironi.zoen.R
import xyz.tironi.zoen.media.detectVoiceHoldGestures

@Composable
fun PlusVoiceButton(actions: () -> Unit, start: () -> Unit, release: () -> Unit, lock: () -> Unit, cancel: () -> Unit, compact: Boolean = false) {
    val haptics = rememberZoenHaptics()
    val density = LocalDensity.current
    val cancelDistance = with(density) { 110.dp.toPx() }
    val lockDistance = with(density) { 80.dp.toPx() }
    val armSlop = with(density) { 14.dp.toPx() }
    var holding by remember { mutableStateOf(false) }
    val currentStart by rememberUpdatedState(start)
    val currentRelease by rememberUpdatedState(release)
    val currentLock by rememberUpdatedState(lock)
    val currentCancel by rememberUpdatedState(cancel)
    val dark = MaterialTheme.colorScheme.background.luminance() < .5f
    val tint = if (holding) MaterialTheme.colorScheme.error else if (dark) MaterialTheme.colorScheme.primary else Color(0xEB1D2418)
    val recordLabel = stringResource(R.string.media_hold_zoen)
    IconButton(onClick = { haptics.perform(ZoenFeedback.Open); actions() },
        modifier = Modifier.size(56.dp).zoenGlass(CircleShape, tint).testTag("home-plus").semantics {
            customActions = listOf(CustomAccessibilityAction(recordLabel) { currentStart(); currentLock(); true })
        }.pointerInput(cancelDistance, lockDistance, armSlop) {
            detectVoiceHoldGestures(cancelDistance, lockDistance,
                onStart = { holding = true; currentStart() },
                onRelease = { holding = false; currentRelease() },
                onLock = { holding = false; currentLock() },
                onCancel = { holding = false; currentCancel() },
                holdMillis = AppleChromeMetrics.PlusHoldMillis, armSlop = armSlop)
        }) {
        if (holding) Icon(Icons.Rounded.Mic, recordLabel, tint = Color.White, modifier = Modifier.size(21.dp))
        else ZoenIcon(ZoenGlyph.Plus, stringResource(R.string.ask_zoen), tint = if (dark) Color(0xFF10200A) else Color.White)
    }
}
