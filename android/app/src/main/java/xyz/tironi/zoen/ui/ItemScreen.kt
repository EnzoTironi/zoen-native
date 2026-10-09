package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.clickable
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import java.text.DateFormat
import java.util.Date
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.data.FileAccess
import xyz.tironi.zoen.miniapps.MiniAppDetailsButton

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun ItemScreen(model: ZoenViewModel, state: AppState, id: String, navigate: (NavKey) -> Unit, back: () -> Unit) {
    val item = state.items.firstOrNull { it.id == id }
    var versions by rememberSaveable { mutableStateOf(false) }
    var restore by remember { mutableStateOf<UInt?>(null) }
    var lineToEdit by remember { mutableStateOf<PlanLineDto?>(null) }
    var addSection by remember { mutableStateOf<Int?>(null) }
    var restored by remember { mutableIntStateOf(0) }
    val context = LocalContext.current
    val fileUnavailable = stringResource(R.string.file_not_ready)
    val saved = stringResource(R.string.saved)
    DisposableEffect(id) { model.viewingItem(id); onDispose { model.viewingItem(null) } }
    if (item == null) {
        Scaffold(topBar = { ScreenBar(stringResource(R.string.files), back) }) { padding -> EmptyState(stringResource(R.string.unavailable), stringResource(R.string.unavailable_detail), Modifier.padding(padding)) }
        return
    }
    Scaffold(topBar = {
        ScreenBar(item.title, back, actions = {
            if (item.app != null) MiniAppDetailsButton(model, state, item)
            IconButton(onClick = { model.launch { model.pageSaves.flush(id); versions = true } }) { Icon(Icons.Rounded.History, stringResource(R.string.versions)) }
            IconButton(onClick = {
                model.launch {
                    model.pageSaves.flush(id)
                    when {
                        item.file != null -> {
                            val bytes = model.repository.query { it.fileBytes(id, null) } ?: error(fileUnavailable)
                            FileAccess.shareFile(context, item.file!!.name, item.file!!.mime, bytes)
                        }
                        item.kindId == "page" -> FileAccess.shareFile(context, "${item.title}.md", "text/markdown", model.repository.query { it.pageMarkdown(id) }.toByteArray())
                        else -> FileAccess.shareText(context, itemText(item, model.repository.locale))
                    }
                }
            }) { Icon(Icons.Rounded.Share, stringResource(R.string.share)) }
        })
    }) { padding ->
        when {
            item.app != null -> MiniAppScreen(model, state, item, Modifier.padding(padding), onClose = back)
            item.kindId == "page" -> key(id, restored) { xyz.tironi.zoen.pages.RichPageEditor(model, state, item, Modifier.padding(padding)) }
            item.file != null -> FileScreen(model, state, item, Modifier.padding(padding))
            else -> LazyColumn(Modifier.padding(padding).testTag("plan-lines"), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                item {
                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Icon(appIcon(item.kindId), null, Modifier.size(32.dp), tint = MaterialTheme.colorScheme.primary)
                        Text(item.title, style = MaterialTheme.typography.displaySmall)
                        Text(item.plan?.summary ?: item.text.orEmpty(), color = MaterialTheme.colorScheme.onSurfaceVariant)
                        AssistChip(onClick = { navigate(Chat(item.spaceId)) }, label = { Text(item.spaceTitle) }, leadingIcon = { Icon(Icons.Rounded.Forum, null, Modifier.size(16.dp)) })
                    }
                }
                item.plan?.let { plan ->
                    item {
                        Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer)) {
                            FlowRow(Modifier.fillMaxWidth().padding(20.dp), horizontalArrangement = Arrangement.spacedBy(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                                Column { Text(stringResource(R.string.total), style = MaterialTheme.typography.labelMedium); Text(formatMoney(plan.totalCents, model.repository.locale), style = MaterialTheme.typography.headlineMedium) }
                                plan.budgetCents?.let { Column { Text(stringResource(R.string.budget), style = MaterialTheme.typography.labelMedium); Text(formatMoney(it, model.repository.locale), style = MaterialTheme.typography.titleLarge) } }
                            }
                        }
                    }
                    plan.sections.forEachIndexed { sectionIndex, section ->
                        item { SectionLabel(section.title) }
                        items(section.lines, key = { it.id }) { line ->
                            Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
                                Row(Modifier.fillMaxWidth().padding(8.dp), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                                    Checkbox(line.done, onCheckedChange = { model.edit { it.togglePlanLine(id, line.id) } }, modifier = Modifier.testTag("plan-line:${line.id}"))
                                    Column(Modifier.weight(1f)) { Text(line.text); if (line.costCents > 0) Text(formatMoney(line.costCents, model.repository.locale), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                                    IconButton(onClick = { lineToEdit = line }) { Icon(Icons.Rounded.Edit, stringResource(R.string.edit)) }
                                }
                                item.linkedRequests.filter { it.lineId == line.id }.forEach { request -> TextButton(onClick = { navigate(Request(request.id)) }, Modifier.padding(start = 48.dp)) { Text(stringResource(requestStatusLabel(request.status))) } }
                            }
                        }
                        item { TextButton(onClick = { addSection = sectionIndex }) { Icon(Icons.Rounded.Add, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.add_line)) } }
                    }
                }
                item {
                    HorizontalDivider()
                    Text(stringResource(R.string.origin), style = MaterialTheme.typography.labelMedium, modifier = Modifier.padding(top = 12.dp))
                    Text(item.origin, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(stringResource(R.string.version, item.version.toInt()), style = MaterialTheme.typography.labelSmall)
                }
            }
        }
    }
    if (versions) ModalBottomSheet(onDismissRequest = { versions = false }) {
        Text(stringResource(R.string.versions), Modifier.padding(horizontal = 24.dp, vertical = 12.dp), style = MaterialTheme.typography.headlineMedium)
        LazyColumn(contentPadding = PaddingValues(bottom = 24.dp)) {
            items(item.versions.reversed(), key = { it.number.toInt() }) { version ->
                ListItem(modifier = Modifier.clickable { versions = false; navigate(VersionPreview(id, version.number)) }, headlineContent = { Text(stringResource(R.string.version, version.number.toInt())) }, supportingContent = {
                    Text(version.note + "\n" + version.author.name + " · " + DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(version.atMs)))
                }, leadingContent = { Icon(if (version.isUndo) Icons.Rounded.Undo else Icons.Rounded.History, null) }, trailingContent = {
                    if (version.number != item.version) TextButton(onClick = { restore = version.number }) { Text(stringResource(R.string.restore)) }
                    else Icon(Icons.Rounded.Check, stringResource(R.string.saved))
                })
            }
        }
    }
    restore?.let { number -> AlertDialog(onDismissRequest = { restore = null }, title = { Text(stringResource(R.string.restore)) }, text = { Text(stringResource(R.string.restore_question)) }, confirmButton = {
        TextButton(onClick = {
            restore = null; versions = false
            model.launch {
                model.repository.change { it.restoreVersion(id, number) }
                model.repository.preferences.edit().remove(model.repository.localKey("pageDraft", id)).remove(model.repository.localKey("pageDraft", id) + ":base").apply()
                restored++
                model.notify(saved)
            }
        }) { Text(stringResource(R.string.restore)) }
    }, dismissButton = { TextButton(onClick = { restore = null }) { Text(stringResource(R.string.cancel)) } }) }
    if (lineToEdit != null || addSection != null) {
        val line = lineToEdit
        var text by remember(line?.id, addSection) { mutableStateOf(line?.text.orEmpty()) }
        var cost by remember(line?.id, addSection) { mutableStateOf((line?.costCents ?: 0).toBigDecimal().movePointLeft(2).toPlainString()) }
        val cents = try { cost.replace(',', '.').toBigDecimalOrNull()?.movePointRight(2)?.longValueExact()?.takeIf { it >= 0 } } catch (_: ArithmeticException) { null }
        AlertDialog(onDismissRequest = { lineToEdit = null; addSection = null }, title = { Text(stringResource(if (line != null) R.string.edit else R.string.add_line)) }, text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedTextField(text, { text = it }, label = { Text(stringResource(R.string.task)) })
                OutlinedTextField(cost, { if (it.length <= 15 && Regex("[0-9]*[.,]?[0-9]{0,2}").matches(it)) cost = it }, label = { Text(stringResource(R.string.cost)) },
                    prefix = { Text(if (model.repository.locale.startsWith("pt")) "R$ " else "$ ") }, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal))
                if (line != null) TextButton(onClick = { lineToEdit = null; model.edit { it.removePlanLine(id, line.id) } }) { Text(stringResource(R.string.delete), color = MaterialTheme.colorScheme.error) }
            }
        }, confirmButton = {
            TextButton(enabled = text.isNotBlank() && cents != null, onClick = {
                val section = addSection
                lineToEdit = null; addSection = null
                model.edit { if (line != null) it.editPlanLine(id, line.id, text, cents!!) else it.addPlanLine(id, (section ?: 0).toUInt(), text, cents!!) }
            }) { Text(stringResource(R.string.save)) }
        }, dismissButton = { TextButton(onClick = { lineToEdit = null; addSection = null }) { Text(stringResource(R.string.cancel)) } })
    }
}

fun itemText(item: ItemDetail, locale: String): String = buildString {
    appendLine("# ${item.title}")
    item.plan?.let { plan ->
        appendLine(plan.summary)
        plan.sections.forEach { section ->
            appendLine("\n## ${section.title}")
            section.lines.forEach { line -> appendLine("- [${if (line.done) "x" else " "}] ${line.text}" + if (line.costCents > 0) " · ${formatMoney(line.costCents, locale)}" else "") }
        }
    } ?: appendLine(item.text ?: item.app?.headline.orEmpty())
}
