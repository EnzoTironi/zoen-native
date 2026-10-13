package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Share
import androidx.compose.material.icons.rounded.Image
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CancellationException
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.ItemDetail
import xyz.tironi.zoen.core.PageDto
import xyz.tironi.zoen.core.formatMoney
import xyz.tironi.zoen.data.FileAccess
import xyz.tironi.zoen.media.DocumentPreview
import xyz.tironi.zoen.media.MediaFiles
import xyz.tironi.zoen.media.MediaPreview
import xyz.tironi.zoen.miniapps.SnapshotCard
import xyz.tironi.zoen.miniapps.WidgetSnapshot
import xyz.tironi.zoen.pages.styledPageText
import xyz.tironi.zoen.pages.pageTextStyle

private data class HistoricalContent(val item: ItemDetail, val page: PageDto?, val file: FileVersionContent?, val canRestore: Boolean)
private data class HistoricalRead(val content: HistoricalContent? = null, val failure: String? = null)

@Composable
fun VersionPreviewScreen(model: ZoenViewModel, id: String, number: UInt, back: () -> Unit, snackbar: SnackbarHostState?) {
    val context = LocalContext.current
    val fileTooLarge = stringResource(R.string.file_open_too_large)
    val state by model.state.collectAsState()
    val read by produceState(HistoricalRead(), id, number, state.revision) {
        value = try { model.repository.query { core ->
            val item = core.itemAt(id, number)
            val canRestore = when (item.kindId) {
                "plan" -> core.planPage(id).let { it.ready && it.legacyEditAdmitted }
                "page" -> core.page(id).canEdit
                else -> true
            }
            HistoricalRead(HistoricalContent(item, if (item.kindId == "page") core.pageAt(id, number) else null,
                if (item.file != null) readFileVersion(core, id, number, fileTooLarge) else null, canRestore))
        } } catch (error: Exception) {
            if (error is CancellationException) throw error
            HistoricalRead(failure = error.message ?: context.getString(R.string.something_wrong))
        }
    }
    val content = read.content
    var confirm by remember { mutableStateOf(false) }
    var observedPlanHash by remember(id, number) { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val saved = stringResource(R.string.saved)
    val pendingSync = stringResource(R.string.page_pending_sync)
    val draftSaved = stringResource(R.string.page_draft_saved)
    Scaffold(snackbarHost = { snackbar?.let { SnackbarHost(it, Modifier.testTag("app-snackbar")) } }, topBar = { ScreenBar(stringResource(R.string.page_preview_version, number.toInt()), back, actions = {
        if (content != null) IconButton(enabled = content.file == null || content.file.bytes != null, onClick = { model.launch {
            val historical = checkNotNull(content)
            val file = historical.file
            if (file != null) FileAccess.shareFile(context, file.info.name, file.info.mime, checkNotNull(file.bytes))
            else FileAccess.shareText(context, historical.page?.blocks?.joinToString("\n") { it.text } ?: itemText(historical.item, model.repository.locale))
        } }, modifier = Modifier.testTag("preview-share")) { Icon(Icons.Rounded.Share, stringResource(R.string.share)) }
    }) }, bottomBar = {
        Surface(tonalElevation = 2.dp) { Row(Modifier.fillMaxWidth().navigationBarsPadding().padding(16.dp), horizontalArrangement = Arrangement.End) {
            if (content?.page != null && !model.pageSaves.canRestore(id)) Text(stringResource(R.string.page_recovery_before_restore), Modifier.weight(1f), style = MaterialTheme.typography.bodySmall)
            Button(enabled = content?.canRestore == true && !busy && (content.page == null || model.pageSaves.canRestore(id)),
                modifier = Modifier.testTag("preview-restore"), onClick = {
                    model.launch {
                        observedPlanHash = model.repository.query { core ->
                            if (core.item(id).kindId == "plan") core.planPage(id).contentHash else null
                        }
                        confirm = true
                    }
                }) { Text(stringResource(R.string.page_restore_version)) }
        } }
    }) { padding ->
        val historical = content
        if (read.failure != null) EmptyState(stringResource(R.string.page_not_ready), read.failure.orEmpty(), Modifier.padding(padding))
        else if (historical == null) Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        else LazyColumn(Modifier.fillMaxSize().padding(padding).testTag("version-preview"), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (historical.page == null) item { Text(historical.item.title, style = MaterialTheme.typography.headlineLarge) }
            historical.page?.let { page ->
                items(page.blocks, key = { it.id }) { block ->
                    Row(verticalAlignment = Alignment.Top) {
                        when (block.kind) {
                            "task" -> Checkbox(block.checked, null)
                            "bullet" -> Text("• ")
                            "numbered" -> Text("${block.number}. ")
                            "quote" -> Text("│ ")
                        }
                        if (block.kind == "divider") HorizontalDivider()
                        else if (block.kind == "image") Row(verticalAlignment = Alignment.CenterVertically) { Icon(Icons.Rounded.Image, null); Text(block.alt, Modifier.padding(start = 8.dp)) }
                        else SelectionContainer { Text(styledPageText(block, MaterialTheme.colorScheme.primary, MaterialTheme.colorScheme.surfaceContainerHighest), style = pageTextStyle(block), modifier = Modifier.padding(start = (block.indent.coerceAtMost(8u).toInt() * 12).dp)) }
                    }
                }
            }
            historical.item.plan?.let { plan ->
                item { Text(plan.summary); Text(formatMoney(plan.totalCents, model.repository.locale), style = MaterialTheme.typography.titleLarge) }
                plan.sections.forEachIndexed { sectionIndex, section ->
                    item { SectionLabel(section.title) }
                    itemsIndexed(section.lines, key = { index, _ -> "$sectionIndex:$index" }) { index, line ->
                        Row(Modifier.testTag("version-preview-line:$sectionIndex:$index"), verticalAlignment = Alignment.CenterVertically) {
                            Checkbox(line.done, onCheckedChange = {}, enabled = false, modifier = Modifier.testTag("preview-plan-line:${line.id}"))
                            SelectionContainer(Modifier.weight(1f)) { Text(line.text) }; Text(formatMoney(line.costCents, model.repository.locale))
                        }
                    }
                }
            }
            if (historical.item.app != null) WidgetSnapshot.from(historical.item)?.let { snapshot -> item { SnapshotCard(snapshot, Modifier.fillMaxWidth(), live = false) } }
            historical.file?.let { version ->
                val file = version.info
                item { Text(file.name); Text(file.mime, style = MaterialTheme.typography.labelMedium) }
                version.bytes?.let { bytes ->
                    item {
                        val local by produceState<java.io.File?>(null, id, number) { value = MediaFiles.local(context, id, number, file.name, bytes) }
                        local?.let {
                            when {
                                file.mime.startsWith("image/") || file.mime == "application/pdf" -> DocumentPreview(it, file.mime, bytes, Modifier.fillMaxWidth())
                                file.mime.startsWith("video/") || file.mime.startsWith("audio/") -> MediaPreview(it, file.mime, Modifier.fillMaxWidth())
                                file.mime.startsWith("text/") || file.mime == "application/json" -> SelectionContainer { Text(bytes.toString(Charsets.UTF_8).take(20_000)) }
                                else -> Text(android.text.format.Formatter.formatShortFileSize(context, bytes.size.toLong()))
                            }
                        }
                    }
                } ?: item { Text(stringResource(R.string.file_not_ready)) }
            }
        }
    }
    if (confirm) AlertDialog(onDismissRequest = { confirm = false }, title = { Text(stringResource(R.string.restore)) }, text = { Text(stringResource(R.string.restore_question)) }, confirmButton = {
        TextButton(enabled = content?.canRestore == true && !busy, modifier = Modifier.testTag("preview-confirm-restore"), onClick = {
            val owner = model.state.value.me?.id ?: return@TextButton
            confirm = false; busy = true
            model.launch {
                try {
                    val fresh = model.pageSaves.restoreVersion(model.repository, id, owner, number, observedPlanHash)
                    model.notify(fresh?.saveError ?: when {
                        fresh?.pendingSync == true -> pendingSync
                        fresh?.unsaved == true -> draftSaved
                        else -> saved
                    })
                    back()
                } finally { busy = false }
            }
        }) { Text(stringResource(R.string.restore)) }
    }, dismissButton = { TextButton(onClick = { confirm = false }) { Text(stringResource(R.string.cancel)) } })
}
