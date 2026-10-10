package xyz.tironi.zoen.pages

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.horizontalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.*
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlinx.coroutines.delay
import kotlinx.coroutines.CancellationException
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.ui.EmptyState

@Composable
fun RichPageEditor(model: ZoenViewModel, state: AppState, item: ItemDetail, modifier: Modifier = Modifier) {
    val owner = state.me?.id ?: return
    val page by produceState<PageDto?>(null, owner, item.id, state.revision) { value = model.repository.query { it.page(item.id) } }
    val draftKey = model.repository.localKey("pageDraft", item.id, owner)
    val baseKey = "$draftKey:base"
    var draft by remember(owner, item.id) { mutableStateOf(model.repository.preferences.getString(draftKey, null)) }
    var original by remember(owner, item.id) { mutableStateOf<List<PageBlockDto>?>(model.repository.preferences.getString(baseKey, null)?.let { PageEditing.decode(it) }) }
    var selected by rememberSaveable { mutableStateOf<String?>(null) }
    var range by remember { mutableStateOf(TextRange.Zero) }
    var typingMarks by remember(item.id) { mutableStateOf<Map<String, String?>>(emptyMap()) }
    var menu by remember { mutableStateOf<String?>(null) }
    var linking by remember { mutableStateOf(false) }
    var link by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var historyRevision by remember { mutableIntStateOf(0) }
    DisposableEffect(model.repository.preferences, item.id) {
        val preferences = model.repository.preferences
        val listener = android.content.SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
            if (key == draftKey && !preferences.contains(draftKey)) { draft = null; original = null }
        }
        preferences.registerOnSharedPreferenceChangeListener(listener)
        onDispose { preferences.unregisterOnSharedPreferenceChangeListener(listener) }
    }
    val history = remember(item.id) { PageEditHistory() }
    val saved = stringResource(R.string.saved)
    val current = page
    if (current == null) { Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }; return }
    if (!current.ready) { EmptyState(stringResource(R.string.page_not_ready), "", modifier); return }
    val blocks = remember(current, draft) { draft?.let { PageEditing.decode(it, original ?: current.blocks) } ?: current.blocks }
    fun persist(next: List<PageBlockDto>) {
        if (model.state.value.me?.id != owner) return
        if (original == null) { original = current.blocks; model.repository.preferences.edit().putString(baseKey, PageEditing.encode(current.blocks)).apply() }
        draft = PageEditing.encode(next)
        model.repository.preferences.edit().putString(draftKey, draft).apply()
    }
    fun change(next: List<PageBlockDto>, editKey: String? = null) {
        if (next == blocks) return
        history.record(blocks, editKey); historyRevision++; persist(next)
    }
    fun replace(block: PageBlockDto, editKey: String? = null) = change(blocks.map { if (it.id == block.id) block else it }, editKey)
    val active = blocks.firstOrNull { it.id == selected }
    val identity = state.me?.id
    val save: suspend (Boolean) -> Unit = { commit ->
        val savingDraft = draft
        if (savingDraft != null && model.state.value.me?.id == identity) {
            val savingBlocks = PageEditing.decode(savingDraft, original ?: current.blocks) ?: blocks
            val originals = (original ?: current.blocks).associateBy { it.id }
            val changed = model.repository.change { core ->
                check(core.me()?.id == identity) { "The page belongs to another account" }
                val remote = core.page(item.id)
                val ids = savingBlocks.map { it.id }
                val additions = remote.blocks.map { it.id }.filter { it !in originals && it !in ids }
                core.pageApply(item.id, ids + additions, savingBlocks.filter { originals[it.id] != it })
                if (commit) core.pageCommit(item.id, saved) else false
            }
            if (commit && draft == savingDraft) {
                draft = null; original = null
                model.repository.preferences.edit().remove(draftKey).remove(baseKey).apply()
                if (changed) model.notify(saved)
            }
        }
    }
    val latestSave by rememberUpdatedState(save)
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(item.id, lifecycle) {
        val flush: suspend () -> Unit = { latestSave(true) }
        model.pageSaves.register(item.id, flush)
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_STOP) model.launch { flush() } }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer); model.launch { try { flush() } finally { model.pageSaves.unregister(item.id, flush) } } }
    }
    LaunchedEffect(draft) {
        if (draft != null) try { delay(400); latestSave(false); delay(3600); latestSave(true) }
        catch (error: Exception) { if (error is CancellationException) throw error; model.notify(error.message ?: saved) }
    }
    Column(modifier.fillMaxSize().imePadding()) {
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = { history.undo(blocks)?.let { persist(it) }; typingMarks = emptyMap(); historyRevision++ }, enabled = historyRevision > 0 && history.canUndo) { Icon(Icons.AutoMirrored.Rounded.Undo, stringResource(R.string.undo)) }
            IconButton(onClick = { history.redo(blocks)?.let { persist(it) }; typingMarks = emptyMap(); historyRevision++ }, enabled = historyRevision > 0 && history.canRedo) { Icon(Icons.AutoMirrored.Rounded.Redo, stringResource(R.string.page_redo)) }
            val formats = listOf(Triple("b", Icons.Rounded.FormatBold, R.string.page_bold), Triple("i", Icons.Rounded.FormatItalic, R.string.page_italic), Triple("s", Icons.Rounded.FormatStrikethrough, R.string.page_strike), Triple("c", Icons.Rounded.Code, R.string.page_inline_code))
            formats.forEach { (key, icon, label) ->
                val checked = active?.let { if (range.collapsed && typingMarks.containsKey(key)) typingMarks[key] != null else PageEditing.hasMark(it, key, range.start, range.end) } == true
                IconToggleButton(checked = checked, enabled = active != null, onCheckedChange = { active?.let {
                    if (range.collapsed) typingMarks = typingMarks + (key to if (checked) null else "")
                    else replace(PageEditing.toggle(it, key, "", range.start, range.end))
                } }) { Icon(icon, stringResource(label)) }
            }
            IconButton(enabled = active != null, onClick = { link = active?.let { PageEditing.marksAt(it, range.start)["a"] }.orEmpty(); linking = true }) { Icon(Icons.Rounded.Link, stringResource(R.string.page_link)) }
            IconButton(enabled = active != null && active.kind !in listOf("image", "divider"), onClick = { active?.let {
                replace(PageEditing.hardBreak(it, range.start, range.end)); range = TextRange(minOf(range.start, range.end) + 1)
            } }) { Icon(Icons.Rounded.KeyboardReturn, stringResource(R.string.page_line_break)) }
            IconButton(enabled = active?.kind in listOf("bullet", "numbered", "task"), onClick = { active?.let { replace(it.copy(indent = (it.indent + 1u).coerceAtMost(6u))) } }) { Icon(Icons.AutoMirrored.Rounded.FormatIndentIncrease, stringResource(R.string.page_indent)) }
            IconButton(enabled = active?.indent?.let { it > 0u } == true, onClick = { active?.let { replace(it.copy(indent = it.indent - 1u)) } }) { Icon(Icons.AutoMirrored.Rounded.FormatIndentDecrease, stringResource(R.string.page_outdent)) }
        }
        LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(horizontal = 20.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            items(blocks, key = { it.id }) { block ->
                PageBlockEditor(block, selected == block.id, changed = { replace(it, if (it.kind == block.kind && it.text != block.text) block.id else null) }, enter = { updated, offset ->
                    val (left, right) = PageEditing.split(updated, offset)
                    change(blocks.flatMap { if (it.id == block.id) listOf(left, right) else listOf(it) }); selected = right.id; range = TextRange.Zero
                }, selection = { nextRange, typed ->
                    if (selected != block.id || !typed && range != nextRange) typingMarks = emptyMap()
                    selected = block.id; range = nextRange
                }, format = { if (selected != block.id) typingMarks = emptyMap(); selected = block.id; menu = block.id }, selectedRange = range.takeIf { selected == block.id }, typingMarks = typingMarks.takeIf { selected == block.id }.orEmpty())
            }
            item { TextButton(onClick = { val block = PageEditing.blank(); change(blocks + block); selected = block.id }) { Icon(Icons.Rounded.Add, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.add_block)) } }
        }
        Surface(tonalElevation = 2.dp) {
            Row(Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 20.dp, vertical = 12.dp), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
                if (draft != null) Text(stringResource(R.string.page_draft_saved), Modifier.weight(1f), style = MaterialTheme.typography.labelSmall)
                Button(enabled = draft != null && !busy, modifier = Modifier.testTag("page-save"), onClick = {
                    busy = true
                    model.launch {
                        try {
                            latestSave(true)
                        } finally { busy = false }
                    }
                }) { Text(stringResource(R.string.save)) }
            }
        }
    }
    menu?.let { id ->
        val block = blocks.firstOrNull { it.id == id }
        if (block != null) AlertDialog(onDismissRequest = { menu = null }, title = { Text(stringResource(R.string.format)) }, text = {
            Column {
                val kinds = listOf("paragraph" to R.string.paragraph, "heading" to R.string.heading, "bullet" to R.string.page_bullet, "numbered" to R.string.page_numbered, "task" to R.string.task, "quote" to R.string.page_quote, "code" to R.string.page_code, "divider" to R.string.page_divider, "image" to R.string.page_image, "raw" to R.string.page_raw)
                Row(Modifier.horizontalScroll(rememberScrollState())) { kinds.forEach { (kind, label) -> FilterChip(block.kind == kind, { replace(block.copy(kind = kind, level = if (kind == "heading") 2u else 0u)); menu = null }, label = { Text(stringResource(label)) }, modifier = Modifier.padding(end = 4.dp)) } }
                if (block.kind == "heading") Row(Modifier.horizontalScroll(rememberScrollState())) { (1..6).forEach { level -> TextButton(onClick = { replace(block.copy(level = level.toUInt())); menu = null }) { Text("H$level") } } }
                TextButton(onClick = { val index = blocks.indexOf(block); val new = PageEditing.blank(); change(blocks.toMutableList().apply { add(index + 1, new) }); selected = new.id; menu = null }) { Text(stringResource(R.string.add_block)) }
                TextButton(onClick = { change(blocks.filterNot { it.id == id }); selected = null; menu = null }) { Text(stringResource(R.string.delete), color = MaterialTheme.colorScheme.error) }
            }
        }, confirmButton = { TextButton(onClick = { menu = null }) { Text(stringResource(R.string.done)) } })
    }
    if (linking) AlertDialog(onDismissRequest = { linking = false }, title = { Text(stringResource(R.string.page_link)) }, text = { OutlinedTextField(link, { link = it }, label = { Text(stringResource(R.string.page_image_url)) }, singleLine = true) }, confirmButton = {
        TextButton(enabled = link.isBlank() || PageEditing.linkTarget(link) != null, onClick = { active?.let { block ->
            replace(PageEditing.setLink(block, link, range.start, range.end))
            if (range.collapsed && link.isNotBlank()) range = TextRange(range.start + link.trim().length)
            typingMarks = emptyMap()
        }; linking = false }) { Text(stringResource(R.string.save)) }
    }, dismissButton = { TextButton(onClick = { linking = false }) { Text(stringResource(R.string.cancel)) } })
}
