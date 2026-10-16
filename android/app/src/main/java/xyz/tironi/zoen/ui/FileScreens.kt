package xyz.tironi.zoen.ui

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import android.graphics.BitmapFactory
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.data.FileAccess
import java.util.UUID
import xyz.tironi.zoen.media.*
import java.io.File
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

internal data class FileVersionContent(val info: FileDto, val bytes: ByteArray?)

internal fun readFileVersion(core: RodaEngine, id: String, version: UInt, tooLarge: String): FileVersionContent {
    val info = core.fileInfoAt(id, version)
    require(info.bytes <= FileAccess.MAX_BYTES.toULong()) { tooLarge }
    return FileVersionContent(info, core.fileBytesBounded(id, version, FileAccess.MAX_BYTES.toULong()))
}

private data class FileRead(val id: String = "", val version: UInt = 0u, val content: FileVersionContent? = null, val failure: String? = null)
private data class FileLocal(val id: String, val version: UInt, val file: File)

private fun imageEditAdmitted(bytes: ByteArray): Boolean {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    return bounds.outWidth > 0 && bounds.outHeight > 0 && bounds.outWidth.toLong() * bounds.outHeight <= 16_000_000L
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun FileScreen(model: ZoenViewModel, state: AppState, item: ItemDetail, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val tooLarge = stringResource(R.string.file_open_too_large)
    val imageTooLarge = stringResource(R.string.file_image_edit_too_large)
    val somethingWrong = stringResource(R.string.something_wrong)
    val read by produceState(FileRead(), item.id, item.version, state.revision) {
        value = try { FileRead(item.id, item.version, model.repository.query { readFileVersion(it, item.id, item.version, tooLarge) }) }
        catch (error: Exception) {
            if (error is CancellationException) throw error
            FileRead(item.id, item.version, failure = error.message ?: somethingWrong)
        }
    }
    val currentRead = read.takeIf { it.id == item.id && it.version == item.version }
    val file = currentRead?.content?.info ?: checkNotNull(item.file)
    val bytes = currentRead?.content?.bytes
    val loadedLocal by produceState<FileLocal?>(null, item.id, item.version, bytes) {
        value = bytes?.let { FileLocal(item.id, item.version, MediaFiles.local(context, item.id, item.version, file.name, it)) }
    }
    val local = loadedLocal?.takeIf { bytes != null && it.id == item.id && it.version == item.version }?.file
    val imageCanEdit by produceState<Boolean?>(null, item.id, item.version, bytes, file.mime) {
        value = if (file.mime.startsWith("image/") && bytes != null) withContext(Dispatchers.IO) { imageEditAdmitted(bytes) } else null
    }
    var editing by rememberSaveable(item.id) { mutableStateOf(false) }
    var markup by rememberSaveable(item.id) { mutableStateOf(false) }
    var trimming by rememberSaveable(item.id) { mutableStateOf(false) }
    var text by rememberSaveable(item.id) { mutableStateOf<String?>(null) }
    val saved = stringResource(R.string.saved)
    val markedUp = stringResource(R.string.media_marked_up)
    val trimmed = stringResource(R.string.media_trimmed)
    val versionTemplate = stringResource(R.string.media_new_version)
    val editableText = bytes != null && bytes!!.size <= 256 * 1024 && (file.mime.startsWith("text/") || file.mime == "application/json")
    val drawing = file.mime.startsWith("image/") || file.mime == "application/pdf"
    val media = file.mime.startsWith("audio/") || file.mime.startsWith("video/")
    LazyColumn(modifier.fillMaxSize(), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item {
            Text(file.name, style = MaterialTheme.typography.headlineMedium)
            Text(item.spaceTitle + " · v${item.version} · " + android.text.format.Formatter.formatShortFileSize(context, file.bytes.toLong()), color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodySmall)
            Text(file.mime, color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.labelSmall)
        }
        if (currentRead?.failure != null) item {
            Text(currentRead.failure!!, color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag("file-read-error"))
        }
        else if (currentRead?.content == null) item {
            CircularProgressIndicator(Modifier.testTag("file-loading"))
        }
        else if (bytes == null) item {
            LinearProgressIndicator(progress = { file.chunksHere.toFloat() / maxOf(1u, file.chunks).toFloat() }, modifier = Modifier.fillMaxWidth())
            Text(stringResource(R.string.file_not_ready), Modifier.testTag("file-pending-pieces"))
            Text("${file.chunksHere}/${file.chunks}", style = MaterialTheme.typography.labelSmall)
        }
        if (drawing && local != null && bytes != null) item { DocumentPreview(local!!, file.mime, bytes!!, Modifier.fillMaxWidth()) }
        if (media && local != null) item { MediaPreview(local!!, file.mime, Modifier.fillMaxWidth()) }
        if (bytes != null && (file.mime.startsWith("text/") || file.mime == "application/json")) item {
            if (editing) OutlinedTextField(text ?: bytes!!.toString(Charsets.UTF_8), { text = it }, Modifier.fillMaxWidth(), minLines = 8)
            else androidx.compose.foundation.text.selection.SelectionContainer { Text(bytes!!.toString(Charsets.UTF_8).take(20_000), fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace) }
        }
        if (bytes != null) item {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                GlassButton(onClick = { model.launch { FileAccess.shareFile(context, file.name, file.mime, bytes!!) } }, modifier = Modifier.testTag("file-export")) { Text(stringResource(R.string.export)) }
                GlassOutlinedButton(onClick = { model.launch { MediaFiles.open(context, file.name, file.mime, bytes!!) } }, modifier = Modifier.testTag("file-open-with")) { Text(stringResource(R.string.media_open_with)) }
                if (drawing && local != null) GlassOutlinedButton(enabled = file.mime == "application/pdf" || imageCanEdit == true,
                    onClick = { markup = true }, modifier = Modifier.testTag("file-markup")) { Icon(Icons.Rounded.Draw, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.media_markup)) }
                if (media && local != null) GlassOutlinedButton(onClick = { trimming = true }, modifier = Modifier.testTag("file-trim")) { Icon(Icons.Rounded.ContentCut, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.media_trim)) }
                if (editableText) GlassOutlinedButton(onClick = {
                    if (!editing) { text = bytes!!.toString(Charsets.UTF_8); editing = true }
                    else model.launch { model.repository.change { it.fileNewVersion(item.id, text.orEmpty().toByteArray(), null, saved) }; editing = false }
                }) { Text(stringResource(if (editing) R.string.save else R.string.edit)) }
            }
            if (drawing && file.mime.startsWith("image/") && imageCanEdit == false) Text(imageTooLarge, style = MaterialTheme.typography.bodySmall)
        }
        item { Text(file.sha256, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
    if (markup && local != null && bytes != null) MarkupEditor(item.id, item.version, local!!, file.mime, bytes!!, onDismiss = { markup = false }) { session, strokes ->
        if (!session.busy) {
            session.busy = true; session.error = null
            model.launch {
                try {
                    val edited = if (file.mime == "application/pdf") DocumentMarkup.pdf(context, local!!, strokes) else {
                        require(withContext(Dispatchers.IO) { imageEditAdmitted(bytes!!) }) { imageTooLarge }
                        DocumentMarkup.image(bytes!!, file.mime, strokes)
                    }
                    val outputMime = when (file.mime.lowercase()) { "application/pdf" -> "application/pdf"; "image/jpeg", "image/jpg" -> "image/jpeg"; "image/webp" -> "image/webp"; else -> "image/png" }
                    val extension = when (outputMime) { "application/pdf" -> "pdf"; "image/jpeg" -> "jpg"; "image/webp" -> "webp"; else -> "png" }
                    val outputName = file.name.substringBeforeLast('.', file.name) + "." + extension
                    val previewFile = MediaFiles.local(context, item.id, item.version + 1u, outputName, edited)
                    val thumbnail = MediaFiles.thumbnail(previewFile, outputMime)
                    val next = model.repository.change { it.fileNewVersionTyped(item.id, edited, thumbnail, markedUp, outputName, outputMime) }
                    markup = false
                    model.notify(String.format(versionTemplate, next.version.toInt()))
                } catch (error: Exception) {
                    if (error is CancellationException) throw error
                    session.error = error.message
                } finally { session.busy = false }
            }
        }
    }
    if (trimming && local != null) MediaTrimEditor(item.id, item.version, local!!, file.mime, onDismiss = { trimming = false }) { session, range, exportOnly ->
        if (!session.busy) {
            session.busy = true; session.error = null
            model.launch {
                val video = file.mime.startsWith("video/")
                val outputMime = if (video) "video/mp4" else "audio/mp4"
                val outputName = file.name.substringBeforeLast('.', file.name) + if (video) ".mp4" else ".m4a"
                val output = File(File(context.cacheDir, "exports").apply { mkdirs() }, "${UUID.randomUUID()}-$outputName")
                try {
                    if (video) MediaExport.trimVideo(context, local!!, range, output) else MediaExport.trimAudio(local!!, range, output)
                    val edited = withContext(Dispatchers.IO) { output.readBytes() }
                    if (exportOnly) FileAccess.shareFile(context, outputName, outputMime, edited)
                    else {
                        val thumbnail = MediaFiles.thumbnail(output, outputMime)
                        val next = model.repository.change { it.fileNewVersionTyped(item.id, edited, thumbnail, trimmed, outputName, outputMime) }
                        trimming = false
                        model.notify(String.format(versionTemplate, next.version.toInt()))
                    }
                } catch (error: Exception) {
                    if (error is CancellationException) throw error
                    session.error = error.message
                } finally { withContext(Dispatchers.IO) { output.delete() }; session.busy = false }
            }
        }
    }
}
