package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Share
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
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

private data class HistoricalContent(val item: ItemDetail, val page: PageDto?, val bytes: ByteArray?)

@Composable
fun VersionPreviewScreen(model: ZoenViewModel, id: String, number: UInt, back: () -> Unit, snackbar: SnackbarHostState?) {
    val context = LocalContext.current
    val content by produceState<HistoricalContent?>(null, id, number) {
        value = model.repository.query { core ->
            val item = core.itemAt(id, number)
            HistoricalContent(item, if (item.kindId == "page") core.pageAt(id, number) else null, if (item.file != null) core.fileBytes(id, number) else null)
        }
    }
    var confirm by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    val saved = stringResource(R.string.saved)
    Scaffold(snackbarHost = { snackbar?.let { SnackbarHost(it, Modifier.testTag("app-snackbar")) } }, topBar = { ScreenBar(stringResource(R.string.page_preview_version, number.toInt()), back, actions = {
        if (content != null) IconButton(onClick = { model.launch {
            val historical = checkNotNull(content)
            val file = historical.item.file
            if (file != null && historical.bytes != null) FileAccess.shareFile(context, file.name, file.mime, historical.bytes!!)
            else FileAccess.shareText(context, historical.page?.blocks?.joinToString("\n") { it.text } ?: itemText(historical.item, model.repository.locale))
        } }) { Icon(Icons.Rounded.Share, stringResource(R.string.share)) }
    }) }, bottomBar = {
        Surface(tonalElevation = 2.dp) { Row(Modifier.fillMaxWidth().navigationBarsPadding().padding(16.dp), horizontalArrangement = Arrangement.End) {
            Button(enabled = content != null && !busy, onClick = { confirm = true }) { Text(stringResource(R.string.page_restore_version)) }
        } }
    }) { padding ->
        val historical = content
        if (historical == null) Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        else LazyColumn(Modifier.fillMaxSize().padding(padding), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { Text(historical.item.title, style = MaterialTheme.typography.headlineLarge) }
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
                        else SelectionContainer { Text(if (block.kind == "image") androidx.compose.ui.text.AnnotatedString("${block.alt}\n${block.url}") else styledPageText(block, MaterialTheme.colorScheme.primary, MaterialTheme.colorScheme.surfaceContainerHighest), style = if (block.kind == "heading") MaterialTheme.typography.headlineSmall else MaterialTheme.typography.bodyLarge, modifier = Modifier.padding(start = (block.indent.coerceAtMost(8u).toInt() * 12).dp)) }
                    }
                }
            }
            historical.item.plan?.let { plan ->
                item { Text(plan.summary); Text(formatMoney(plan.totalCents, model.repository.locale), style = MaterialTheme.typography.titleLarge) }
                plan.sections.forEach { section ->
                    item { SectionLabel(section.title) }
                    items(section.lines, key = { it.id }) { line -> Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(line.done, null); Text(line.text, Modifier.weight(1f)); if (line.costCents > 0) Text(formatMoney(line.costCents, model.repository.locale)) } }
                }
            }
            if (historical.item.app != null) WidgetSnapshot.from(historical.item)?.let { snapshot -> item { SnapshotCard(snapshot, Modifier.fillMaxWidth(), live = false) } }
            historical.item.file?.let { file ->
                item { Text(file.name); Text(file.mime, style = MaterialTheme.typography.labelMedium) }
                historical.bytes?.let { bytes ->
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
        TextButton(onClick = { confirm = false; busy = true; model.launch {
            try {
                model.repository.change { it.restoreVersion(id, number) }
                model.repository.preferences.edit().remove(model.repository.localKey("pageDraft", id)).remove(model.repository.localKey("pageDraft", id) + ":base").commit()
                model.notify(saved); back()
            } finally { busy = false }
        } }) { Text(stringResource(R.string.restore)) }
    }, dismissButton = { TextButton(onClick = { confirm = false }) { Text(stringResource(R.string.cancel)) } })
}
