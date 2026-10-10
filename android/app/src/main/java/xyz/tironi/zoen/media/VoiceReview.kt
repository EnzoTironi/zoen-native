package xyz.tironi.zoen.media

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Send
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInParent
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import java.util.Locale
import kotlin.math.abs
import kotlin.math.max
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun VoiceReview(session: VoiceSession, model: ZoenViewModel, space: String, reply: String?, thread: Boolean, onSent: () -> Unit) {
    val state by session.state.collectAsStateWithLifecycle()
    val clip = state.clip ?: return
    val editor = state.editor ?: return
    val playback by VoicePlayback.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val modelRequested = stringResource(R.string.media_model_requested)
    val appState by model.state.collectAsStateWithLifecycle()
    var selectionAnchor by remember(clip.id) { mutableDoubleStateOf(0.0) }
    var selection by remember(clip.id) { mutableStateOf<TimeRange?>(null) }
    var playhead by remember(clip.id) { mutableDoubleStateOf(0.0) }
    val previewId = "preview:${clip.id}"
    val current = playback.id == previewId
    val head = if (current) editor.originalTime(playback.position) else playhead
    LaunchedEffect(clip.id) { session.requestTranscript(model.repository.locale) }
    Dialog(onDismissRequest = { if (!state.busy) session.cancel() }, properties = DialogProperties(usePlatformDefaultWidth = false, dismissOnBackPress = !state.busy, dismissOnClickOutside = false)) {
        Surface(Modifier.widthIn(max = 700.dp).fillMaxWidth().fillMaxHeight().safeDrawingPadding(), shape = RoundedCornerShape(24.dp)) {
            Column(Modifier.fillMaxSize()) {
                Row(Modifier.fillMaxWidth().padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(stringResource(if (state.editing) R.string.media_edit_recording else R.string.media_review_recording), Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
                    if (state.editing) {
                        IconButton(onClick = { selection = null; session.change { it.undo() } }, enabled = editor.canUndo && !state.busy) { Icon(Icons.Rounded.Undo, stringResource(R.string.media_undo)) }
                        IconButton(onClick = { selection = null; session.change { it.redo() } }, enabled = editor.canRedo && !state.busy) { Icon(Icons.Rounded.Redo, stringResource(R.string.media_redo)) }
                    }
                    IconButton(onClick = { session.cancel() }, enabled = !state.busy) { Icon(Icons.Rounded.Close, stringResource(R.string.close)) }
                }
                Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    if (state.editing) Text(pluralStringResource(R.plurals.media_cut_summary, editor.removedRanges.size, editor.removedRanges.size, String.format(Locale.getDefault(), "%.1f", editor.duration - editor.keptDuration)), Modifier.testTag("voice-cut-summary"), style = MaterialTheme.typography.titleMedium)
                    VoiceWaveform(clip.detail, head / editor.duration, editor.duration, editor.removedRanges, selection,
                        Modifier.fillMaxWidth().height(if (state.editing) 90.dp else 64.dp).testTag("voice-waveform")
                            .pointerInput(editor, state.editing) {
                                detectDragGestures(onDragStart = { start ->
                                    if (state.editing) { val time = (start.x / size.width * editor.duration).coerceIn(0.0, editor.duration); selectionAnchor = time; selection = TimeRange(time, time) }
                                }, onDrag = { change, _ ->
                                    change.consume()
                                    val time = (change.position.x / size.width * editor.duration).coerceIn(0.0, editor.duration)
                                    if (state.editing) selection?.let { selection = TimeRange(minOf(selectionAnchor, time), maxOf(selectionAnchor, time)) }
                                    else { playhead = time; if (current) VoicePlayback.seek(editor.editedTime(time)) }
                                })
                            }.pointerInput(editor, state.editing, state.revision) {
                                detectTapGestures { point ->
                                    val time = (point.x / size.width * editor.duration).coerceIn(0.0, editor.duration)
                                    if (state.editing && editor.isRemoved(time)) { selection = null; session.change { it.restoreRegion(time) } }
                                    else { playhead = time; if (current) VoicePlayback.seek(editor.editedTime(time)) }
                                }
                            })
                    Text(voiceTime(editor.editedTime(head)) + " / " + voiceDuration(editor.keptDuration), style = MaterialTheme.typography.labelLarge)
                    if (state.editing) {
                        Text(stringResource(R.string.media_restore_hint), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        selection?.takeIf { it.duration >= .05 }?.let { range ->
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(stringResource(R.string.media_selection, voiceTime(range.start), voiceTime(range.end)), Modifier.weight(1f), style = MaterialTheme.typography.bodySmall)
                                Button(onClick = { session.change { it.cut(range) }; selection = null }, enabled = !state.busy, modifier = Modifier.testTag("voice-cut-selection")) { Icon(Icons.Rounded.ContentCut, null); Text(stringResource(R.string.media_cut)) }
                            }
                        }
                        Text(stringResource(R.string.media_trim), style = MaterialTheme.typography.titleSmall)
                        RangeSlider(value = editor.edit.trimStart.toFloat()..editor.edit.trimEnd.toFloat(),
                            onValueChange = { range -> session.change { it.trim(range.start.toDouble(), range.endInclusive.toDouble(), live = true) } },
                            valueRange = 0f..editor.duration.toFloat(), onValueChangeFinished = { session.change { it.endLive() } }, enabled = !state.busy,
                            modifier = Modifier.testTag("voice-trim"))
                        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                            Text(stringResource(R.string.media_trim_start) + " " + voiceTime(editor.edit.trimStart), style = MaterialTheme.typography.labelSmall)
                            Text(stringResource(R.string.media_trim_end) + " " + voiceTime(editor.edit.trimEnd), style = MaterialTheme.typography.labelSmall)
                        }
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            if (editor.fillers.isNotEmpty()) AssistChip(onClick = { session.change { it.removeWords(it.fillers.toSet()) } }, enabled = !state.busy, label = { Text(stringResource(R.string.media_remove_fillers, editor.fillers.size)) }, modifier = Modifier.testTag("voice-remove-fillers"))
                            val pauses = editor.longPauses.indices.filterNot { it in editor.edit.shortenedPauses }
                            if (pauses.isNotEmpty()) AssistChip(onClick = { session.change { it.shortenPauses(pauses.toSet()) } }, enabled = !state.busy, label = { Text(stringResource(R.string.media_shorten_pauses, pauses.size)) }, modifier = Modifier.testTag("voice-shorten-pauses"))
                        }
                    }
                    if (state.transcribing) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) { CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp); Text(stringResource(R.string.media_transcribing)) }
                    else if (editor.transcript.text.isEmpty()) {
                        Text(stringResource(R.string.media_no_transcript), color = MaterialTheme.colorScheme.onSurfaceVariant)
                        if (VoiceTranscriber.supported(context)) {
                            TextButton(onClick = { VoiceTranscriber.downloadModel(context, model.repository.locale); model.notify(modelRequested) }) { Text(stringResource(R.string.media_download_speech)) }
                            TextButton(onClick = { session.requestTranscript(model.repository.locale) }) { Text(stringResource(R.string.media_try_transcript)) }
                        }
                    } else if (state.editing && editor.words.isNotEmpty()) VoiceWords(editor, session, !state.busy)
                    else {
                        androidx.compose.foundation.text.selection.SelectionContainer { Text(editor.keptTranscript.ifEmpty { editor.transcript.text }) }
                        if (state.editing && editor.words.isEmpty()) Text(stringResource(R.string.media_no_word_timing), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    if (state.editing) Text(stringResource(R.string.media_original_deleted), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    state.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                    Spacer(Modifier.height(12.dp))
                }
                HorizontalDivider()
                FlowRow(Modifier.fillMaxWidth().padding(16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedIconButton(onClick = { session.cancel() }, enabled = !state.busy) { Icon(Icons.Rounded.DeleteOutline, stringResource(R.string.media_delete_recording)) }
                    OutlinedButton(onClick = { session.preview() }, enabled = !state.busy && editor.keptDuration >= .3) {
                        Icon(if (current && playback.playing) Icons.Rounded.Pause else Icons.Rounded.PlayArrow, stringResource(if (current && playback.playing) R.string.media_pause else R.string.media_play_preview))
                        Text(voiceDuration(editor.keptDuration))
                    }
                    if (!state.editing) OutlinedButton(onClick = { session.edit() }, enabled = !state.busy) { Text(stringResource(R.string.edit)) }
                    Button(onClick = { session.sendNow(model.repository, space, reply, thread, onSent) }, enabled = !state.busy && !state.transcribing && editor.keptDuration >= .3 && !appState.keyMissing, modifier = Modifier.testTag("voice-send")) {
                        if (state.busy) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp, color = MaterialTheme.colorScheme.onPrimary)
                        else Icon(Icons.AutoMirrored.Rounded.Send, null)
                        Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.media_send_voice))
                    }
                }
            }
        }
    }
}

@Composable
fun VoiceWaveform(levels: List<Float>, progress: Double = 0.0, duration: Double = 1.0, removed: List<TimeRange> = emptyList(), selection: TimeRange? = null, modifier: Modifier = Modifier, tint: Color? = null) {
    val primary = tint ?: MaterialTheme.colorScheme.primary
    val remaining = (tint ?: MaterialTheme.colorScheme.onSurfaceVariant).copy(alpha = .55f)
    val cut = MaterialTheme.colorScheme.error
    val selected = MaterialTheme.colorScheme.tertiaryContainer
    val label = stringResource(R.string.media_waveform)
    Canvas(modifier.semantics { contentDescription = label }) {
        selection?.let { drawRect(selected, Offset((it.start / duration * size.width).toFloat(), 0f), androidx.compose.ui.geometry.Size((it.duration / duration * size.width).toFloat(), size.height)) }
        val step = size.width / max(1, levels.size)
        levels.forEachIndexed { index, level ->
            val time = (index + .5) / max(1, levels.size) * duration
            val deleted = removed.any { time in it }
            val height = max(3f, level * size.height * .85f)
            val x = step * (index + .5f)
            drawLine(if (deleted) remaining.copy(alpha = .15f) else if (index.toDouble() / max(1, levels.size) <= progress) primary else remaining,
                Offset(x, (size.height - height) / 2), Offset(x, (size.height + height) / 2), max(1.5f, step * .6f), androidx.compose.ui.graphics.StrokeCap.Round)
        }
        removed.forEach { drawLine(cut, Offset((it.start / duration * size.width).toFloat(), size.height * .55f), Offset((it.end / duration * size.width).toFloat(), size.height * .45f), 2.dp.toPx()) }
        if (progress > 0) drawLine(primary, Offset((progress * size.width).toFloat(), 0f), Offset((progress * size.width).toFloat(), size.height), 2.dp.toPx())
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun VoiceWords(editor: VoiceEditor, session: VoiceSession, enabled: Boolean) {
    val frames = remember(editor) { mutableStateMapOf<Int, Rect>() }
    var touched by remember { mutableStateOf(setOf<Int>()) }
    var removing by remember { mutableStateOf(true) }
    val keptLabel = stringResource(R.string.media_kept_word)
    val removedLabel = stringResource(R.string.media_removed_word)
    FlowRow(Modifier.fillMaxWidth().pointerInput(editor, enabled) {
        if (enabled) detectDragGesturesAfterLongPress(onDragStart = { point ->
            touched = frames.entries.firstOrNull { point in it.value }?.key?.let { setOf(it) }.orEmpty()
            removing = touched.firstOrNull()?.let { !editor.wordRemoved(it) } ?: true
        }, onDrag = { change, _ -> change.consume(); frames.entries.firstOrNull { change.position in it.value }?.key?.let { touched += it } },
            onDragEnd = { if (touched.isNotEmpty()) session.change { if (removing) it.removeWords(touched) else it.restoreWords(touched) }; touched = emptySet() }, onDragCancel = { touched = emptySet() })
    }, horizontalArrangement = Arrangement.spacedBy(4.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        editor.words.forEachIndexed { index, word ->
            val removed = if (index in touched) removing else editor.wordRemoved(index)
            FilterChip(selected = !removed, onClick = { session.change { if (it.wordRemoved(index)) it.restoreWords(setOf(index)) else it.removeWords(setOf(index)) } }, enabled = enabled,
                label = { Text(word.text, textDecoration = if (removed) TextDecoration.LineThrough else null) },
                modifier = Modifier.testTag("voice-word-$index").onGloballyPositioned { frames[index] = it.boundsInParent() }.semantics { contentDescription = word.text + ". " + if (removed) removedLabel else keptLabel })
        }
    }
}
