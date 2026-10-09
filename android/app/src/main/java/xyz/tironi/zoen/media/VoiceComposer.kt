package xyz.tironi.zoen.media

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import java.io.File
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel

/** Home can drive the same recorder with monotonically increasing hold/release signals. */
@Composable
fun VoiceComposer(
    model: ZoenViewModel,
    spaceId: String,
    modifier: Modifier = Modifier,
    reply: String? = null,
    thread: Boolean = false,
    startSignal: Int = 0,
    releaseSignal: Int = 0,
    showTrigger: Boolean = true,
    onActiveChanged: (Boolean) -> Unit = {},
    onSent: () -> Unit = {},
) {
    val context = LocalContext.current
    val session: VoiceSession = viewModel(key = "voice:$spaceId", factory = remember(context.applicationContext) { viewModelFactory { initializer { VoiceSession(context.applicationContext) } } })
    val appState by model.state.collectAsStateWithLifecycle()
    val microphonePermission = stringResource(R.string.media_microphone_permission)
    val recording by session.recorder.state.collectAsStateWithLifecycle()
    val review by session.state.collectAsStateWithLifecycle()
    val haptics = LocalHapticFeedback.current
    val owner = LocalLifecycleOwner.current
    val sent by rememberUpdatedState(onSent)
    val active by rememberUpdatedState(onActiveChanged)
    val currentReply by rememberUpdatedState(reply)
    val density = LocalDensity.current
    val cancelDistance = with(density) { 110.dp.toPx() }
    val lockDistance = with(density) { 80.dp.toPx() }
    var x by remember { mutableFloatStateOf(0f) }
    var y by remember { mutableFloatStateOf(0f) }
    fun start(locked: Boolean) {
        if (appState.keyMissing || review.clip != null) return
        VoicePlayback.pause()
        if (session.recorder.start()) {
            if (locked) session.recorder.lock()
            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
        } else recording.error?.let(model::notify)
    }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) start(true) else model.notify(microphonePermission)
    }
    val begin: (Boolean) -> Unit = { locked ->
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) start(locked)
        else permission.launch(Manifest.permission.RECORD_AUDIO)
    }
    LaunchedEffect(startSignal) { if (startSignal > 0 && recording.phase == RecordingPhase.IDLE && review.clip == null) begin(false) }
    LaunchedEffect(releaseSignal) {
        if (releaseSignal > 0 && session.recorder.state.value.phase == RecordingPhase.HOLDING) session.release(model.repository, spaceId, currentReply, thread) { sent() }
    }
    LaunchedEffect(recording.phase, review.clip) { active(recording.phase != RecordingPhase.IDLE || review.clip != null) }
    DisposableEffect(owner, session) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_STOP) {
                VoicePlayback.pause()
                if (session.recorder.state.value.phase != RecordingPhase.IDLE) session.cancel()
            }
        }
        owner.lifecycle.addObserver(observer)
        onDispose { owner.lifecycle.removeObserver(observer); active(false) }
    }
    Box(modifier.size(48.dp), contentAlignment = Alignment.Center) {
        if (showTrigger) FilledTonalIconButton(onClick = { begin(true) }, enabled = !appState.keyMissing && recording.phase == RecordingPhase.IDLE && review.clip == null,
            modifier = Modifier.size(48.dp).testTag("voice-record").pointerInput(session) {
                detectDragGesturesAfterLongPress(onDragStart = { x = 0f; y = 0f; begin(false) },
                    onDrag = { change, amount ->
                        change.consume(); x += amount.x; y += amount.y
                        if (session.recorder.state.value.phase == RecordingPhase.HOLDING) {
                            if (x < -cancelDistance) { session.cancel(); haptics.performHapticFeedback(HapticFeedbackType.Reject) }
                            else if (y < -lockDistance) { session.recorder.lock(); haptics.performHapticFeedback(HapticFeedbackType.LongPress) }
                        }
                    }, onDragEnd = {
                        if (session.recorder.state.value.phase == RecordingPhase.HOLDING) session.release(model.repository, spaceId, currentReply, thread) { sent() }
                    }, onDragCancel = { if (session.recorder.state.value.phase == RecordingPhase.HOLDING) session.cancel() })
            }) { Icon(Icons.Rounded.Mic, stringResource(R.string.media_hold_record)) }
        if (recording.phase != RecordingPhase.IDLE) {
            val width = (LocalConfiguration.current.screenWidthDp - 32).coerceIn(240, 380).dp
            Popup(alignment = Alignment.BottomEnd, offset = IntOffset(0, -with(density) { 60.dp.roundToPx() }), properties = PopupProperties(focusable = false)) {
                Surface(Modifier.width(width), shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.surfaceContainerHigh, tonalElevation = 6.dp, shadowElevation = 8.dp) {
                    Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                            IconButton(onClick = { session.cancel() }, enabled = recording.phase != RecordingPhase.FINISHING) { Icon(Icons.Rounded.DeleteOutline, stringResource(R.string.media_delete_recording), tint = MaterialTheme.colorScheme.error) }
                            Column(Modifier.weight(1f)) {
                                Text(stringResource(if (recording.phase == RecordingPhase.LOCKED) R.string.media_recording_locked else R.string.media_recording, voiceTime(recording.seconds)), style = MaterialTheme.typography.labelLarge)
                                VoiceWaveform(recording.levels, modifier = Modifier.fillMaxWidth().height(32.dp))
                            }
                            if (recording.phase == RecordingPhase.HOLDING) IconButton(onClick = { session.recorder.lock() }) { Icon(Icons.Rounded.Lock, stringResource(R.string.media_lock)) }
                            IconButton(onClick = { session.review() }, enabled = recording.phase != RecordingPhase.FINISHING) {
                                if (recording.phase == RecordingPhase.FINISHING) CircularProgressIndicator(Modifier.size(22.dp), strokeWidth = 2.dp)
                                else Icon(Icons.Rounded.StopCircle, stringResource(R.string.media_stop_review), tint = MaterialTheme.colorScheme.error)
                            }
                        }
                        if (recording.phase == RecordingPhase.HOLDING) Text(stringResource(R.string.media_slide_cancel), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        recording.error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
                    }
                }
            }
        }
    }
    if (review.clip != null) VoiceReview(session, model, spaceId, reply, thread) { sent() }
}

@Composable
fun VoiceBubble(model: ZoenViewModel, reference: VoiceNoteRef, modifier: Modifier = Modifier) {
    val playback by VoicePlayback.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val current = playback.id == reference.id
    var transcript by rememberSaveable(reference.id) { mutableStateOf(false) }
    var loading by remember(reference.id) { mutableStateOf(false) }
    val label = stringResource(R.string.media_audio_arriving)
    fun play() {
        if (loading) return
        if (current) VoicePlayback.toggle(context, reference.id, File(context.cacheDir, "unused"))
        else {
            loading = true
            model.launch {
                try {
                    val file = VoiceTransport.localFile(context, model.repository, reference)
                    if (file == null) model.notify(label) else VoicePlayback.toggle(context, reference.id, file)
                } finally { loading = false }
            }
        }
    }
    val owner = LocalLifecycleOwner.current
    DisposableEffect(owner, reference.id) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_STOP && VoicePlayback.state.value.id == reference.id) VoicePlayback.pause() }
        owner.lifecycle.addObserver(observer)
        onDispose { owner.lifecycle.removeObserver(observer); if (VoicePlayback.state.value.id == reference.id) VoicePlayback.stop() }
    }
    Column(modifier.widthIn(min = 210.dp, max = 360.dp).testTag("voice-message"), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilledIconButton(onClick = ::play) {
                if (loading) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp, color = MaterialTheme.colorScheme.onPrimary)
                else Icon(if (current && playback.playing) Icons.Rounded.Pause else Icons.Rounded.PlayArrow, stringResource(if (current && playback.playing) R.string.media_pause else R.string.media_play_voice))
            }
            Column(Modifier.weight(1f)) {
                VoiceWaveform(reference.levels, if (current) playback.position / maxOf(.001, playback.duration) else 0.0, modifier = Modifier.fillMaxWidth().height(28.dp))
                Slider(value = if (current) (playback.position / maxOf(.001, playback.duration)).toFloat().coerceIn(0f, 1f) else 0f,
                    onValueChange = { fraction -> if (current) VoicePlayback.seek(fraction * playback.duration) else play() }, modifier = Modifier.fillMaxWidth().height(24.dp))
            }
            Column(horizontalAlignment = Alignment.End) {
                Text(voiceTime(if (current) playback.position else reference.ms / 1000.0), style = MaterialTheme.typography.labelMedium)
                TextButton(onClick = { if (current) VoicePlayback.cycleRate() else play() }, contentPadding = PaddingValues(4.dp)) { Text(if (current) "${playback.rate}×" else "1×", modifier = Modifier.testTag("voice-speed")) }
            }
        }
        if (reference.transcript.isNotBlank()) {
            TextButton(onClick = { transcript = !transcript }, contentPadding = PaddingValues(0.dp)) { Text(stringResource(if (transcript) R.string.media_hide_transcript else R.string.media_show_transcript)) }
            if (transcript) androidx.compose.foundation.text.selection.SelectionContainer { Text(reference.transcript, style = MaterialTheme.typography.bodyMedium) }
        }
        if (current) playback.error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
    }
}
