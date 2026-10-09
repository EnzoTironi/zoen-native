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
import java.util.UUID
import org.json.JSONArray
import org.json.JSONObject
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.data.FileAccess
import xyz.tironi.zoen.media.*
import java.io.File
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
fun FilesScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit, back: () -> Unit) {
    var create by remember { mutableStateOf(false) }
    var title by rememberSaveable { mutableStateOf("") }
    val context = LocalContext.current
    val home = state.zoenChat?.id ?: state.spaces.firstOrNull()?.id
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null && home != null) model.launch { val item = FileAccess.import(context, model.repository, home, uri); navigate(Item(item.id)) }
    }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.files), back, actions = {
        IconButton(onClick = { create = true }, enabled = home != null) { Icon(Icons.Rounded.NoteAdd, stringResource(R.string.new_page)) }
        IconButton(onClick = { picker.launch(arrayOf("*/*")) }, enabled = home != null) { Icon(Icons.Rounded.FileUpload, stringResource(R.string.import_file)) }
    }) }) { padding ->
        LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            if (state.items.isEmpty()) item { EmptyState(stringResource(R.string.no_files), stringResource(R.string.no_files_detail)) }
            items(state.items, key = { it.id }) { item ->
                Card(onClick = { navigate(Item(item.id)) }, colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
                    ListItem(headlineContent = { Text(item.title, fontWeight = FontWeight.SemiBold) }, supportingContent = { Text(item.spaceTitle + " · " + item.kindLabel) }, leadingContent = { Icon(appIcon(item.app?.appId ?: item.kindId), null, tint = MaterialTheme.colorScheme.primary) }, trailingContent = { Text("v${item.version}", style = MaterialTheme.typography.labelSmall) }, colors = ListItemDefaults.colors(containerColor = Color.Transparent))
                }
            }
        }
    }
    if (create) AlertDialog(onDismissRequest = { create = false }, title = { Text(stringResource(R.string.new_page)) }, text = { OutlinedTextField(title, { title = it }, label = { Text(stringResource(R.string.page_title)) }) }, confirmButton = {
        TextButton(enabled = title.isNotBlank(), onClick = { create = false; model.launch { val item = model.repository.change { it.pageCreate(checkNotNull(home), title) }; navigate(Item(item.id)) } }) { Text(stringResource(R.string.create)) }
    }, dismissButton = { TextButton(onClick = { create = false }) { Text(stringResource(R.string.cancel)) } })
}

private fun encodeBlocks(blocks: List<PageBlockDto>): String = JSONArray().apply {
    blocks.forEach { block -> put(JSONObject().put("id", block.id).put("text", block.text).put("kind", block.kind).put("level", block.level.toInt()).put("checked", block.checked)) }
}.toString()

@Composable
fun PageEditor(model: ZoenViewModel, state: AppState, item: ItemDetail, modifier: Modifier = Modifier) {
    val page by produceState<PageDto?>(null, item.id, state.revision) { value = model.repository.query { it.page(item.id) } }
    val draftKey = "pageDraft:${item.id}"
    var draft by rememberSaveable(item.id) { mutableStateOf(model.repository.preferences.getString(draftKey, null)) }
    var base by remember(item.id) { mutableStateOf<PageDto?>(null) }
    var blockMenu by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val saved = stringResource(R.string.saved)
    LaunchedEffect(page?.itemId) { if (base == null) base = page }
    val current = page
    if (current == null) { Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }; return }
    if (!current.ready) { EmptyState(stringResource(R.string.page_not_ready), "", modifier); return }
    val blocks = remember(current, draft) {
        val originals = (base ?: current).blocks.associateBy { it.id }
        if (draft == null) current.blocks
        else {
            val list = JSONArray(draft)
            (0 until list.length()).map { index ->
                val json = list.getJSONObject(index)
                val original = originals[json.getString("id")] ?: PageBlockDto(json.getString("id"), "paragraph", 0u, 0u, 0u, false, "", "", "", "", emptyList())
                val text = json.getString("text")
                original.copy(text = text, kind = json.getString("kind"), level = json.optInt("level").toUInt(), checked = json.optBoolean("checked"), spans = if (text == original.text) original.spans else emptyList())
            }
        }
    }
    fun update(next: List<PageBlockDto>) {
        if (base == null) base = current
        draft = encodeBlocks(next)
        model.repository.preferences.edit().putString(draftKey, draft).apply()
    }
    Column(modifier.fillMaxSize().imePadding()) {
        LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { Text(item.title, style = MaterialTheme.typography.displaySmall) }
            items(blocks, key = { it.id }) { block ->
                Row(verticalAlignment = Alignment.Top) {
                    if (block.kind == "task") Checkbox(block.checked, onCheckedChange = { update(blocks.map { if (it.id == block.id) it.copy(checked = !it.checked) else it }) })
                    else if (block.kind == "bullet") Text("•", Modifier.padding(top = 16.dp, end = 8.dp))
                    if (block.kind == "divider") HorizontalDivider(Modifier.weight(1f).padding(vertical = 24.dp))
                    else OutlinedTextField(block.text.replace('\u2028', '\n'), { text -> update(blocks.map { if (it.id == block.id) it.copy(text = text.replace('\n', '\u2028')) else it }) }, Modifier.weight(1f),
                        textStyle = when (block.kind) { "heading" -> MaterialTheme.typography.headlineSmall; "code" -> MaterialTheme.typography.bodyMedium.copy(fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace); else -> MaterialTheme.typography.bodyLarge },
                        placeholder = { Text(stringResource(R.string.block_text)) },
                    )
                    Box {
                        IconButton(onClick = { blockMenu = block.id }) { Icon(Icons.Rounded.MoreVert, stringResource(R.string.format)) }
                        DropdownMenu(blockMenu == block.id, onDismissRequest = { blockMenu = null }) {
                            listOf("paragraph" to R.string.paragraph, "heading" to R.string.heading, "task" to R.string.task).forEach { (kind, label) ->
                                DropdownMenuItem(text = { Text(stringResource(label)) }, onClick = { blockMenu = null; update(blocks.map { if (it.id == block.id) it.copy(kind = kind, level = if (kind == "heading") 2u else 0u) else it }) })
                            }
                            DropdownMenuItem(text = { Text(stringResource(R.string.delete)) }, onClick = { blockMenu = null; update(blocks.filterNot { it.id == block.id }) })
                        }
                    }
                }
            }
            item { TextButton(onClick = { update(blocks + PageBlockDto(UUID.randomUUID().toString(), "paragraph", 0u, 0u, 0u, false, "", "", "", "", emptyList())) }) { Icon(Icons.Rounded.Add, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.add_block)) } }
        }
        Surface(tonalElevation = 2.dp) {
            Row(Modifier.fillMaxWidth().navigationBarsPadding().padding(16.dp), horizontalArrangement = Arrangement.End) {
                Button(enabled = draft != null && !busy, onClick = {
                    busy = true
                    val savingDraft = draft
                    model.launch {
                        try {
                            model.repository.change { core ->
                                val remote = core.page(item.id)
                                val originals = (base ?: current).blocks.associateBy { it.id }
                                val localIds = blocks.map { it.id }
                                val remoteAdditions = remote.blocks.map { it.id }.filter { it !in originals && it !in localIds }
                                core.pageApply(item.id, localIds + remoteAdditions, blocks.filter { originals[it.id] != it })
                                core.pageCommit(item.id, saved)
                            }
                            if (draft == savingDraft) { draft = null; base = null; model.repository.preferences.edit().remove(draftKey).apply() }
                            model.notify(saved)
                        } finally { busy = false }
                    }
                }) { Text(stringResource(R.string.save)) }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun FileScreen(model: ZoenViewModel, state: AppState, item: ItemDetail, modifier: Modifier = Modifier) {
    val file = checkNotNull(item.file)
    val bytes by produceState<ByteArray?>(null, item.id, item.version, file.ready) { value = model.repository.query { it.fileBytes(item.id, null) } }
    val context = LocalContext.current
    val local by produceState<File?>(null, item.id, item.version, bytes) { bytes?.let { value = MediaFiles.local(context, item.id, item.version, file.name, it) } }
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
        if (!file.ready) item {
            LinearProgressIndicator(progress = { file.chunksHere.toFloat() / maxOf(1u, file.chunks).toFloat() }, modifier = Modifier.fillMaxWidth())
            Text(stringResource(R.string.file_not_ready))
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
                Button(onClick = { model.launch { FileAccess.shareFile(context, file.name, file.mime, bytes!!) } }) { Text(stringResource(R.string.export)) }
                OutlinedButton(onClick = { model.launch { MediaFiles.open(context, file.name, file.mime, bytes!!) } }) { Text(stringResource(R.string.media_open_with)) }
                if (drawing && local != null) OutlinedButton(onClick = { markup = true }, modifier = Modifier.testTag("file-markup")) { Icon(Icons.Rounded.Draw, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.media_markup)) }
                if (media && local != null) OutlinedButton(onClick = { trimming = true }, modifier = Modifier.testTag("file-trim")) { Icon(Icons.Rounded.ContentCut, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.media_trim)) }
                if (editableText) OutlinedButton(onClick = {
                    if (!editing) { text = bytes!!.toString(Charsets.UTF_8); editing = true }
                    else model.launch { model.repository.change { it.fileNewVersion(item.id, text.orEmpty().toByteArray(), null, saved) }; editing = false }
                }) { Text(stringResource(if (editing) R.string.save else R.string.edit)) }
            }
        }
        item { Text(file.sha256, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
    if (markup && local != null && bytes != null) MarkupEditor(item.id, item.version, local!!, file.mime, bytes!!, onDismiss = { markup = false }) { session, strokes ->
        if (!session.busy) {
            session.busy = true; session.error = null
            model.launch {
                try {
                    val edited = if (file.mime == "application/pdf") DocumentMarkup.pdf(context, local!!, strokes) else DocumentMarkup.image(bytes!!, file.mime, strokes)
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
