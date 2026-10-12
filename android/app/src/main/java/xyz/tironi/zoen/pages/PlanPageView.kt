package xyz.tironi.zoen.pages

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.Edit
import androidx.compose.material.icons.rounded.Forum
import androidx.compose.material.icons.rounded.Image
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.navigation3.runtime.NavKey
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.ui.Chat
import xyz.tironi.zoen.ui.EmptyState
import xyz.tironi.zoen.ui.Request
import xyz.tironi.zoen.ui.requestStatusLabel

private data class PlanLineEdit(val observed: PlanPageDto, val block: PlanPageBlockDto?, val section: UInt?)

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun PlanPageView(model: ZoenViewModel, item: ItemDetail, page: PlanPageDto?, failure: String?, fresh: Boolean,
                 modifier: Modifier = Modifier, navigate: (NavKey) -> Unit) {
    var editing by remember(item.id) { mutableStateOf<PlanLineEdit?>(null) }
    var removing by remember(item.id) { mutableStateOf<PlanLineEdit?>(null) }
    var busy by remember(item.id) { mutableStateOf(false) }
    val readOnly = stringResource(R.string.plan_read_only)
    val changed = stringResource(R.string.plan_changed)
    val editable = fresh && page?.ready == true && page.legacyEditAdmitted && !busy

    fun mutate(observed: PlanPageDto, action: (RodaEngine) -> EditOutcome, completed: () -> Unit = {}) {
        if (busy) return
        busy = true
        model.launch {
            try {
                val result = model.repository.change { core ->
                    requirePlanEdit(core.planPage(item.id), observed, readOnly, changed)
                    action(core)
                }
                completed()
                model.notifyEdit(result)
            } finally { busy = false }
        }
    }

    if (page == null) {
        if (failure != null) EmptyState(stringResource(R.string.page_not_ready), failure, modifier)
        else Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        return
    }
    if (!page.ready) { EmptyState(stringResource(R.string.page_not_ready), page.saveError ?: readOnly, modifier); return }
    val rows = remember(page) { planPageRows(page) }
    LazyColumn(modifier.testTag("plan-lines"), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        if (page.saveError != null || failure != null || page.pendingSync || page.unsaved || !page.legacyEditAdmitted) item {
            Text(page.saveError ?: failure ?: when {
                page.unsaved -> stringResource(R.string.page_draft_saved)
                page.pendingSync -> stringResource(R.string.page_pending_sync)
                else -> readOnly
            }, style = MaterialTheme.typography.bodySmall, modifier = Modifier.testTag("plan-status"))
        }
        items(rows, key = { it.block.id }) { row ->
            val block = row.block
            val text = styledPlanText(block, MaterialTheme.colorScheme.primary, MaterialTheme.colorScheme.surfaceContainerHighest)
            Column(Modifier.fillMaxWidth().testTag("plan-block:${block.id}")) {
                when (val content = block.content) {
                    is PlanPageContentDto.Title -> {
                        SelectionContainer { Text(text, style = MaterialTheme.typography.displaySmall) }
                        Card(Modifier.padding(top = 16.dp), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer)) {
                            FlowRow(Modifier.fillMaxWidth().padding(20.dp), horizontalArrangement = Arrangement.spacedBy(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                                Column {
                                    Text(stringResource(R.string.total), style = MaterialTheme.typography.labelMedium)
                                    Text(page.totalCents?.let { formatMoney(it, model.repository.locale) } ?: stringResource(R.string.plan_total_unavailable),
                                        style = MaterialTheme.typography.headlineMedium, modifier = Modifier.testTag("plan-total"))
                                }
                                content.budgetCents?.let { budget -> Column {
                                    Text(stringResource(R.string.budget), style = MaterialTheme.typography.labelMedium)
                                    Text(formatMoney(budget, model.repository.locale), style = MaterialTheme.typography.titleLarge)
                                } }
                            }
                        }
                    }
                    is PlanPageContentDto.Summary -> SelectionContainer { Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    is PlanPageContentDto.Section -> {
                        SelectionContainer { Text(text, style = MaterialTheme.typography.titleLarge) }
                        if (page.legacyEditAdmitted) TextButton(enabled = editable, onClick = { editing = PlanLineEdit(page, null, row.sectionIndex) },
                            modifier = Modifier.testTag("plan-add:${block.id}")) {
                            Icon(Icons.Rounded.Add, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.add_line))
                        }
                    }
                    is PlanPageContentDto.Line -> Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
                        Row(Modifier.fillMaxWidth().padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
                            Checkbox(content.done, onCheckedChange = {
                                mutate(page, { it.togglePlanLine(item.id, content.legacyId) })
                            }, enabled = editable && row.uniqueLine,
                                modifier = Modifier.testTag(if (row.uniqueLine) "plan-line:${content.legacyId}" else "plan-line-block:${block.id}"))
                            Column(Modifier.weight(1f)) {
                                SelectionContainer { Text(text) }
                                Text(formatMoney(content.costCents, model.repository.locale), style = MaterialTheme.typography.labelMedium,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.testTag("plan-cost:${block.id}"))
                            }
                            if (page.legacyEditAdmitted) IconButton(enabled = editable && row.uniqueLine,
                                onClick = { editing = PlanLineEdit(page, block, row.sectionIndex) }, modifier = Modifier.testTag("plan-edit:${block.id}")) {
                                Icon(Icons.Rounded.Edit, stringResource(R.string.edit))
                            }
                        }
                        item.linkedRequests.filter { it.lineId == content.legacyId }.forEach { request ->
                            TextButton(onClick = { navigate(Request(request.id)) }, Modifier.padding(start = 48.dp)) { Text(stringResource(requestStatusLabel(request.status))) }
                        }
                    }
                    is PlanPageContentDto.Prose -> PlanProseBlock(block, content.kind)
                }
            }
        }
        item {
            AssistChip(onClick = { navigate(Chat(item.spaceId)) }, label = { Text(item.spaceTitle) }, leadingIcon = { Icon(Icons.Rounded.Forum, null, Modifier.size(16.dp)) })
            HorizontalDivider(Modifier.padding(top = 12.dp))
            Text(stringResource(R.string.origin), style = MaterialTheme.typography.labelMedium, modifier = Modifier.padding(top = 12.dp))
            Text(item.origin, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (page.version > 0u) Text(stringResource(R.string.version, page.version.toInt()), style = MaterialTheme.typography.labelSmall)
        }
    }

    editing?.let { edit ->
        key(edit) {
            var text by remember { mutableStateOf(edit.block?.text.orEmpty()) }
            val line = edit.block?.content as? PlanPageContentDto.Line
            var cost by remember { mutableStateOf(planCostText(line?.costCents ?: 0)) }
            val cents = parsePlanCents(cost)?.takeIf { it >= 0 }
            AlertDialog(onDismissRequest = { if (!busy) editing = null }, title = { Text(stringResource(if (line == null) R.string.add_line else R.string.edit)) }, text = {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    OutlinedTextField(text, { text = it }, label = { Text(stringResource(R.string.task)) }, enabled = !busy,
                        modifier = Modifier.fillMaxWidth().testTag("plan-line-text"))
                    OutlinedTextField(cost, { if (it.length <= 32 && Regex("-?[0-9]*[.,]?[0-9]{0,2}").matches(it)) cost = it },
                        label = { Text(stringResource(R.string.cost)) }, prefix = { Text(if (model.repository.locale.startsWith("pt")) "R$ " else "$ ") },
                        supportingText = { if (cents == null) Text(stringResource(R.string.plan_cost_invalid)) }, isError = cents == null, enabled = !busy,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal), modifier = Modifier.fillMaxWidth().testTag("plan-line-cost"))
                    if (line != null) TextButton(enabled = editable, onClick = { editing = null; removing = edit }, modifier = Modifier.testTag("plan-delete-line")) {
                        Text(stringResource(R.string.delete), color = MaterialTheme.colorScheme.error)
                    }
                }
            }, confirmButton = {
                TextButton(enabled = editable && text.isNotBlank() && cents != null, modifier = Modifier.testTag("plan-save-line"), onClick = {
                    mutate(edit.observed, { core ->
                        if (line != null) core.editPlanLine(item.id, line.legacyId, text, checkNotNull(cents))
                        else core.addPlanLine(item.id, checkNotNull(edit.section), text, checkNotNull(cents))
                    }, { editing = null })
                }) { Text(stringResource(R.string.save)) }
            }, dismissButton = {
                TextButton(enabled = !busy, onClick = { editing = null }, modifier = Modifier.testTag("plan-cancel-line")) { Text(stringResource(R.string.cancel)) }
            })
        }
    }
    removing?.let { edit ->
        val line = checkNotNull(edit.block?.content as? PlanPageContentDto.Line)
        AlertDialog(onDismissRequest = { if (!busy) removing = null }, title = { Text(stringResource(R.string.plan_remove_title)) }, text = {
            Text(stringResource(R.string.plan_remove_detail, checkNotNull(edit.block).text))
        }, confirmButton = {
            TextButton(enabled = editable, onClick = { mutate(edit.observed, { it.removePlanLine(item.id, line.legacyId) }, { removing = null }) },
                modifier = Modifier.testTag("plan-confirm-remove")) { Text(stringResource(R.string.delete), color = MaterialTheme.colorScheme.error) }
        }, dismissButton = {
            TextButton(enabled = !busy, onClick = { removing = null }, modifier = Modifier.testTag("plan-cancel-remove")) { Text(stringResource(R.string.cancel)) }
        })
    }
}

@Composable
private fun PlanProseBlock(block: PlanPageBlockDto, kind: PlanProseKindDto) {
    val text = styledPlanText(block, MaterialTheme.colorScheme.primary, MaterialTheme.colorScheme.surfaceContainerHighest)
    val style: TextStyle = when (kind) {
        is PlanProseKindDto.Heading -> MaterialTheme.typography.headlineMedium.copy(fontSize = when (kind.level) { 1u -> 30.sp; 2u -> 23.sp; 3u -> 19.sp; else -> 17.sp }, fontWeight = FontWeight.Bold)
        is PlanProseKindDto.Code, is PlanProseKindDto.Raw -> MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace)
        is PlanProseKindDto.Quote -> MaterialTheme.typography.bodyLarge.copy(fontStyle = FontStyle.Italic)
        else -> MaterialTheme.typography.bodyLarge
    }
    val indent = when (kind) {
        is PlanProseKindDto.Bullet -> kind.indent
        is PlanProseKindDto.Numbered -> kind.indent
        is PlanProseKindDto.Task -> kind.indent
        else -> 0u
    }
    Row(Modifier.fillMaxWidth().padding(start = (indent.coerceAtMost(8u).toInt() * 12).dp), verticalAlignment = Alignment.Top) {
        when (kind) {
            is PlanProseKindDto.Bullet -> Text("• ")
            is PlanProseKindDto.Numbered -> Text("${kind.number}. ")
            is PlanProseKindDto.Task -> Checkbox(kind.checked, onCheckedChange = {}, enabled = false)
            is PlanProseKindDto.Quote -> VerticalDivider(Modifier.padding(end = 10.dp).height(48.dp), thickness = 3.dp)
            else -> Unit
        }
        when (kind) {
            is PlanProseKindDto.Divider -> HorizontalDivider()
            is PlanProseKindDto.Image -> Column {
                Row(verticalAlignment = Alignment.CenterVertically) { Icon(Icons.Rounded.Image, kind.alt); Text(kind.alt, Modifier.padding(start = 8.dp)) }
                if (kind.title.isNotEmpty()) Text(kind.title, style = MaterialTheme.typography.bodySmall)
                SelectionContainer { Text(kind.url, style = MaterialTheme.typography.bodySmall) }
            }
            else -> Column(Modifier.weight(1f)) {
                if (kind is PlanProseKindDto.Code && kind.lang.isNotEmpty()) Text(kind.lang, style = MaterialTheme.typography.labelSmall)
                SelectionContainer { Text(text, style = style) }
            }
        }
    }
}
