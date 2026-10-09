package xyz.tironi.zoen.media

import android.widget.VideoView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.viewmodel.compose.viewModel
import java.io.File
import kotlinx.coroutines.delay
import xyz.tironi.zoen.R

@Composable
fun MediaPreview(file: File, mime: String, modifier: Modifier = Modifier, range: TimeRange? = null) {
    val owner = LocalLifecycleOwner.current
    var view by remember(file.absolutePath) { mutableStateOf<VideoView?>(null) }
    var playing by remember(file.absolutePath) { mutableStateOf(false) }
    var ready by remember(file.absolutePath) { mutableStateOf(false) }
    var duration by remember(file.absolutePath) { mutableDoubleStateOf(0.0) }
    var position by remember(file.absolutePath) { mutableDoubleStateOf(0.0) }
    var failed by remember(file.absolutePath) { mutableStateOf(false) }
    val video = mime.startsWith("video/")
    val currentRange by rememberUpdatedState(range)
    DisposableEffect(owner, view) {
        val player = view
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_STOP) { player?.pause(); playing = false } }
        owner.lifecycle.addObserver(observer)
        onDispose { owner.lifecycle.removeObserver(observer); player?.stopPlayback() }
    }
    LaunchedEffect(file.absolutePath, playing, range) {
        while (playing) {
            position = (view?.currentPosition ?: 0) / 1000.0
            if (range != null && position >= range.end) { view?.pause(); playing = false; break }
            delay(40)
        }
    }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Box(Modifier.fillMaxWidth().height(if (video) 260.dp else 112.dp).background(if (video) Color.Black else MaterialTheme.colorScheme.surfaceContainer, RoundedCornerShape(16.dp)), contentAlignment = Alignment.Center) {
            if (!video) Icon(Icons.Rounded.GraphicEq, null, Modifier.size(64.dp), tint = MaterialTheme.colorScheme.primary)
            AndroidView(factory = { context ->
                VideoView(context).apply {
                    setOnPreparedListener { media -> duration = media.duration / 1000.0; ready = true; currentRange?.let { seekTo((it.start * 1000).toInt()) } }
                    setOnCompletionListener { playing = false; position = currentRange?.start ?: 0.0; seekTo((position * 1000).toInt()) }
                    setOnErrorListener { _, _, _ -> failed = true; ready = false; playing = false; true }
                    setVideoPath(file.absolutePath)
                    view = this
                }
            }, modifier = if (video) Modifier.fillMaxSize() else Modifier.size(1.dp))
            if (!ready && !failed) CircularProgressIndicator()
        }
        if (failed) Text(stringResource(R.string.media_video_unavailable), color = MaterialTheme.colorScheme.error)
        else {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                FilledIconButton(onClick = {
                    if (playing) { view?.pause(); playing = false }
                    else {
                        VoicePlayback.pause()
                        if (range != null && (position < range.start || position >= range.end)) { view?.seekTo((range.start * 1000).toInt()); position = range.start }
                        view?.start(); playing = true
                    }
                }, enabled = ready, modifier = Modifier.testTag("media-play")) { Icon(if (playing) Icons.Rounded.Pause else Icons.Rounded.PlayArrow, stringResource(if (playing) R.string.media_pause else R.string.media_play)) }
                Slider(value = position.toFloat().coerceIn(0f, maxOf(.001, duration).toFloat()), onValueChange = { position = it.toDouble(); view?.seekTo((it * 1000).toInt()) }, enabled = ready,
                    valueRange = 0f..maxOf(.001, duration).toFloat(), modifier = Modifier.weight(1f).testTag("media-seek"))
                Text(voiceTime(position) + " / " + voiceTime(duration), style = MaterialTheme.typography.labelSmall)
            }
        }
    }
}

class MediaTrimSession : ViewModel() {
    var start by mutableDoubleStateOf(0.0)
    var end by mutableDoubleStateOf(0.0)
    var busy by mutableStateOf(false)
    var error by mutableStateOf<String?>(null)
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun MediaTrimEditor(itemId: String, version: UInt, file: File, mime: String, onDismiss: () -> Unit, onExport: (MediaTrimSession, TimeRange, Boolean) -> Unit) {
    val session: MediaTrimSession = viewModel(key = "trim:$itemId:$version")
    val duration by produceState(0.0, file.absolutePath) {
        try { value = MediaExport.duration(file); if (session.end == 0.0) session.end = value }
        catch (error: Exception) { session.error = error.message }
    }
    Dialog(onDismissRequest = { if (!session.busy) onDismiss() }, properties = DialogProperties(usePlatformDefaultWidth = false, dismissOnClickOutside = false, dismissOnBackPress = !session.busy)) {
        Surface(Modifier.widthIn(max = 800.dp).fillMaxWidth().fillMaxHeight(.92f).safeDrawingPadding(), shape = RoundedCornerShape(24.dp)) {
            Column(Modifier.padding(20.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(stringResource(R.string.media_trim_clip), Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
                    IconButton(onClick = onDismiss, enabled = !session.busy) { Icon(Icons.Rounded.Close, stringResource(R.string.close)) }
                }
                if (duration > 0) {
                    MediaPreview(file, mime, range = TimeRange(session.start, session.end))
                    Text(stringResource(R.string.media_selection, voiceTime(session.start), voiceTime(session.end)), style = MaterialTheme.typography.titleMedium)
                    RangeSlider(value = session.start.toFloat()..session.end.toFloat(), onValueChange = { selected ->
                        session.start = selected.start.toDouble().coerceAtMost(duration - .1)
                        session.end = selected.endInclusive.toDouble().coerceIn(session.start + .1, duration)
                    }, valueRange = 0f..duration.toFloat(), enabled = !session.busy, modifier = Modifier.testTag("media-trim-range"))
                    Text(stringResource(R.string.media_keep_original), color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodySmall)
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = { onExport(session, TimeRange(session.start, session.end), true) }, enabled = !session.busy) { Icon(Icons.Rounded.IosShare, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.media_export_clip)) }
                        Button(onClick = { onExport(session, TimeRange(session.start, session.end), false) }, enabled = !session.busy, modifier = Modifier.testTag("media-trim-save")) { Text(stringResource(R.string.save)) }
                    }
                } else if (session.error == null) CircularProgressIndicator()
                session.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                if (session.busy) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) { CircularProgressIndicator(Modifier.size(24.dp)); Text(stringResource(R.string.media_exporting)) }
            }
        }
    }
}
