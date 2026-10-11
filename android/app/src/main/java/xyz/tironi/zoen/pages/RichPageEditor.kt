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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlinx.coroutines.delay
import kotlinx.coroutines.CancellationException
import java.util.UUID
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.data.FileAccess
import xyz.tironi.zoen.ui.EmptyState
import xyz.tironi.zoen.ui.keepSnackbarAbove

@Composable
fun RichPageEditor(model: ZoenViewModel, state: AppState, item: ItemDetail, modifier: Modifier = Modifier) {
    val owner = state.me?.id ?: return
    val context = LocalContext.current
    val unreadableDraft = stringResource(R.string.page_draft_unreadable)
    val draftSaveFailed = stringResource(R.string.page_draft_save_failed)
    val draftCopied = stringResource(R.string.page_draft_copied)
    var page by remember(owner, item.id) { mutableStateOf<PageDto?>(null) }
    var pageReload by remember(owner, item.id) { mutableIntStateOf(0) }
    LaunchedEffect(owner, item.id, state.revision, pageReload) { page = model.repository.query { it.page(item.id) } }
    val draftKey = model.repository.localKey("pageDraft", item.id, owner)
    val encryptedKey = model.repository.localKey("encryptedPageDraft", item.id, owner)
    val draftStore = remember(owner, item.id) { PageDraftStore(model.repository.vault, encryptedKey) }
    val loadedDraft = remember(owner, item.id) { runCatching { draftStore.load(model.repository.preferences, draftKey) } }
    var draft by remember(owner, item.id) { mutableStateOf(loadedDraft.getOrNull()) }
    var draftReadable by remember(owner, item.id) { mutableStateOf(loadedDraft.isSuccess) }
    var recoveryRequired by remember(owner, item.id) { mutableStateOf(false) }
    var discardDraft by rememberSaveable(owner, item.id) { mutableStateOf(false) }
    var saveError by remember(owner, item.id) { mutableStateOf(if (loadedDraft.isFailure) unreadableDraft else null) }
    var deferred by remember(owner, item.id) { mutableStateOf(false) }
    val editorToken = remember(owner, item.id) { Any() }
    var generation by remember(owner, item.id) { mutableLongStateOf(model.pageSaves.generation(item.id)) }
    var restorePaused by remember(owner, item.id) { mutableStateOf(false) }
    var composingBlocks by remember(owner, item.id) { mutableStateOf<Set<String>>(emptySet()) }
    var compositionPage by remember(owner, item.id) { mutableStateOf<PageDto?>(null) }
    var selected by rememberSaveable { mutableStateOf<String?>(null) }
    var range by remember { mutableStateOf(TextRange.Zero) }
    var typingMarks by remember(item.id) { mutableStateOf<Map<String, String?>>(emptyMap()) }
    var menu by remember { mutableStateOf<String?>(null) }
    var linking by remember { mutableStateOf(false) }
    var link by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var historyRevision by remember { mutableIntStateOf(0) }
    var history by remember(owner, item.id) { mutableStateOf(PageEditHistory()) }
    val saved = stringResource(R.string.saved)
    val recoveryBeforeRestore = stringResource(R.string.page_recovery_before_restore)
    LaunchedEffect(owner, item.id, draftReadable, recoveryRequired) {
        model.pageSaves.draftRecovery(item.id, !draftReadable || recoveryRequired)
    }
    fun failed(error: Exception) {
        saveError = when {
            !draftReadable && draft == null -> unreadableDraft
            error is CoreException.Stale -> error.reason
            error is CoreException.Invalid -> error.reason
            error is CoreException.Storage -> error.reason
            else -> error.message
        }
        if (error is CoreException.Stale) recoveryRequired = true
    }
    val current = page
    if (current == null) { Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }; return }
    if (!current.ready) { EmptyState(stringResource(R.string.page_not_ready), "", modifier); return }
    val editable = current.canEdit && draftReadable && !recoveryRequired && !restorePaused
    val rendered = compositionPage ?: current
    val blocks = remember(rendered, draft) { draft?.let { PageEditing.decode(it.content, PageEditing.decode(it.base).orEmpty()) } ?: rendered.blocks }
    fun persist(next: List<PageBlockDto>) {
        if (!editable || model.state.value.me?.id != owner || !model.pageSaves.isCurrent(item.id, editorToken, generation)) return
        val nextDraft = PageDraft(PageEditing.encode(next), draft?.base ?: PageEditing.encode(rendered.blocks),
            draft?.context ?: rendered.editContext, (draft?.revision ?: 0) + 1)
        draft = nextDraft
        runCatching { draftStore.save(nextDraft) }.onFailure {
            draftReadable = false; saveError = draftSaveFailed; model.notify(draftSaveFailed)
        }
    }
    fun change(next: List<PageBlockDto>, editKey: String? = null) {
        if (!editable || next == blocks) return
        history.record(blocks, editKey); historyRevision++; persist(next)
    }
    fun replace(block: PageBlockDto, editKey: String? = null) = change(blocks.map { if (it.id == block.id) block else it }, editKey)
    val active = blocks.firstOrNull { it.id == selected }
    val save: suspend (Boolean) -> Unit = { commit ->
        model.pageSaves.save(item.id, editorToken, generation) {
            check(!recoveryRequired && draftReadable) { recoveryBeforeRestore }
            var savingDraft = draft ?: return@save
            if (draftReadable && composingBlocks.isEmpty() && model.state.value.me?.id == owner) {
                if (savingDraft.context.isEmpty()) {
                    val remote = model.repository.query { core ->
                        check(core.me()?.id == owner) { "The page belongs to another account" }
                        core.page(item.id).also { check(it.canEdit) { it.saveError ?: "This page is read only" } }
                    }
                    if (!model.pageSaves.isCurrent(item.id, editorToken, generation) || composingBlocks.isNotEmpty()) return@save
                    fun migrate(old: PageDraft): PageDraft {
                        val base = requireNotNull(PageEditing.decode(old.base))
                        val pending = requireNotNull(PageEditing.decode(old.content, base))
                        return old.copy(content = PageEditing.encode(PageEditing.recoverLegacy(pending, base, remote.blocks)),
                            base = PageEditing.encode(remote.blocks), context = remote.editContext)
                    }
                    val previous = savingDraft
                    val upgraded = migrate(previous)
                    val newest = draft
                    val upgradedNewest = if (newest == previous) upgraded else newest?.let { if (it.context.isEmpty()) migrate(it) else it }
                    // Persist the one-time recovery and its exact context before the first core mutation.
                    draftStore.save(upgradedNewest ?: upgraded)
                    draft = upgradedNewest ?: upgraded
                    savingDraft = upgraded
                } else {
                    draftStore.save(checkNotNull(draft))
                }
                val base = requireNotNull(PageEditing.decode(savingDraft.base))
                val appliedBlocks = requireNotNull(PageEditing.decode(savingDraft.content, base))
                val originals = base.associateBy { it.id }
                val changed = appliedBlocks.filter { originals[it.id] != it }
                val (receipt, fresh) = model.repository.change { core ->
                    check(core.me()?.id == owner) { "The page belongs to another account" }
                    // An empty observed diff can undo a previous accepted edit whose receipt was lost.
                    val result = core.pageApplyFrom(item.id, savingDraft.editId, savingDraft.context, appliedBlocks.map { it.id }, changed)
                    if (commit) core.pageCommit(item.id, saved)
                    result to core.page(item.id)
                }
                if (model.state.value.me?.id != owner || !model.pageSaves.isCurrent(item.id, editorToken, generation)) return@save
                page = fresh
                saveError = fresh.saveError
                deferred = commit && fresh.unsaved && fresh.pendingSync
                val newest = draft
                if (newest == savingDraft) {
                    if (composingBlocks.isNotEmpty()) {
                        val next = savingDraft.copy(base = PageEditing.encode(appliedBlocks), context = receipt.appliedEditContext,
                            editId = UUID.randomUUID().toString())
                        draftStore.save(next); draft = next
                    } else if (!fresh.unsaved && fresh.saveError == null) {
                        draftStore.delete(); draft = null
                    } else {
                        val next = PageDraft.fromPage(fresh, savingDraft.revision)
                        draftStore.save(next); draft = next
                    }
                } else if (newest != null) {
                    val next = newest.copy(base = PageEditing.encode(appliedBlocks), context = receipt.appliedEditContext)
                    draftStore.save(next); draft = next
                }
            }
        }
    }
    val latestSave by rememberUpdatedState(save)
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(owner, item.id, lifecycle) {
        val flush: suspend () -> Unit = {
            try { latestSave(true) } catch (error: Exception) { if (error is CancellationException) throw error; failed(error); throw error }
        }
        model.pageSaves.register(item.id, editorToken, flush, pause = { paused ->
            restorePaused = paused
            if (paused) { menu = null; linking = false }
        }, restored = { nextGeneration, restored, error ->
            generation = nextGeneration
            if (model.state.value.me?.id == owner) {
                restored?.let {
                    pageReload++
                    page = it.page
                    draft = it.draft
                    draftReadable = it.draftStored
                    recoveryRequired = false
                    deferred = it.page.unsaved && it.page.pendingSync
                    composingBlocks = emptySet(); compositionPage = null
                    selected = null; range = TextRange.Zero; typingMarks = emptyMap()
                    history = PageEditHistory(); historyRevision = 0
                }
                saveError = error ?: restored?.page?.saveError
            }
        })
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_STOP) model.launch { flush() } }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer); model.launch { try { flush() } finally { model.pageSaves.unregister(item.id, editorToken) } } }
    }
    LaunchedEffect(draft?.revision, composingBlocks, restorePaused, draftReadable, recoveryRequired) {
        if (draft != null && draftReadable && !recoveryRequired && composingBlocks.isEmpty() && !restorePaused) try { delay(400); latestSave(false); delay(3600); latestSave(true) }
        catch (error: Exception) { if (error is CancellationException) throw error; failed(error); model.notify(saveError ?: saved) }
    }
    LaunchedEffect(current.pendingSync, state.revision) {
        if (deferred && !current.pendingSync && current.canEdit) {
            deferred = false
            try { latestSave(true) } catch (error: Exception) { if (error is CancellationException) throw error; failed(error) }
        }
    }
    val renderedGeneration = generation
    LaunchedEffect(current.unsaved, owner, item.id, generation, restorePaused, composingBlocks) {
        if (current.unsaved && editable && composingBlocks.isEmpty() && draft == null && page == current &&
            model.state.value.me?.id == owner && model.pageSaves.isCurrent(item.id, editorToken, renderedGeneration)) {
            val restored = PageDraft.fromPage(current)
            draftStore.save(restored); draft = restored
        }
    }
    Column(modifier.fillMaxSize().imePadding()) {
        if (!draftReadable || recoveryRequired) {
            Surface(color = MaterialTheme.colorScheme.errorContainer) {
                Column(Modifier.fillMaxWidth().padding(12.dp).testTag("page-recovery-actions")) {
                    Text(stringResource(R.string.page_recovery_detail), style = MaterialTheme.typography.bodySmall)
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (draft != null) TextButton(onClick = {
                            if (model.state.value.me?.id == owner) {
                                context.getSystemService(android.content.ClipboardManager::class.java).setPrimaryClip(
                                    android.content.ClipData.newPlainText(item.title, blocks.joinToString("\n\n") { it.text }))
                                model.notify(draftCopied)
                            }
                        }, Modifier.testTag("page-copy-draft")) { Text(stringResource(R.string.page_copy_draft)) }
                        else TextButton(onClick = { model.launch {
                            check(model.state.value.me?.id == owner)
                            FileAccess.shareFile(context, "${item.title}.zoen-draft", "application/octet-stream",
                                model.repository.vault.storedCiphertext(encryptedKey))
                        } }, Modifier.testTag("page-keep-draft")) { Text(stringResource(R.string.page_keep_draft)) }
                        TextButton(onClick = { discardDraft = true }, enabled = !busy && composingBlocks.isEmpty(),
                            modifier = Modifier.testTag("page-discard-draft")) { Text(stringResource(R.string.page_discard_draft)) }
                        if (!recoveryRequired) TextButton(onClick = { model.launch {
                            try {
                                check(model.state.value.me?.id == owner)
                                if (draft != null) draftStore.save(checkNotNull(draft))
                                else draft = draftStore.load(model.repository.preferences, draftKey)
                                draftReadable = true; saveError = null
                            } catch (error: Exception) { failed(error) }
                        } }) { Text(stringResource(R.string.retry)) }
                    }
                }
            }
        }
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = { history.undo(blocks)?.let { persist(it) }; typingMarks = emptyMap(); historyRevision++ }, enabled = editable && historyRevision > 0 && history.canUndo) { Icon(Icons.AutoMirrored.Rounded.Undo, stringResource(R.string.undo)) }
            IconButton(onClick = { history.redo(blocks)?.let { persist(it) }; typingMarks = emptyMap(); historyRevision++ }, enabled = editable && historyRevision > 0 && history.canRedo) { Icon(Icons.AutoMirrored.Rounded.Redo, stringResource(R.string.page_redo)) }
            val formats = listOf(Triple("b", Icons.Rounded.FormatBold, R.string.page_bold), Triple("i", Icons.Rounded.FormatItalic, R.string.page_italic), Triple("s", Icons.Rounded.FormatStrikethrough, R.string.page_strike), Triple("c", Icons.Rounded.Code, R.string.page_inline_code))
            formats.forEach { (key, icon, label) ->
                val checked = active?.let { if (range.collapsed && typingMarks.containsKey(key)) typingMarks[key] != null else PageEditing.hasMark(it, key, range.start, range.end) } == true
                IconToggleButton(checked = checked, enabled = editable && active != null, onCheckedChange = { active?.let {
                    if (range.collapsed) typingMarks = typingMarks + (key to if (checked) null else "")
                    else replace(PageEditing.toggle(it, key, "", range.start, range.end))
                } }) { Icon(icon, stringResource(label)) }
            }
            IconButton(enabled = editable && active != null, onClick = { link = active?.let { PageEditing.marksAt(it, range.start)["a"] }.orEmpty(); linking = true }) { Icon(Icons.Rounded.Link, stringResource(R.string.page_link)) }
            IconButton(enabled = editable && active != null && active.kind !in listOf("image", "divider"), onClick = { active?.let {
                replace(PageEditing.hardBreak(it, range.start, range.end)); range = TextRange(minOf(range.start, range.end) + 1)
            } }) { Icon(Icons.Rounded.KeyboardReturn, stringResource(R.string.page_line_break)) }
            IconButton(enabled = editable && active?.kind in listOf("bullet", "numbered", "task"), onClick = { active?.let { replace(it.copy(indent = (it.indent + 1u).coerceAtMost(6u))) } }) { Icon(Icons.AutoMirrored.Rounded.FormatIndentIncrease, stringResource(R.string.page_indent)) }
            IconButton(enabled = editable && active?.indent?.let { it > 0u } == true, onClick = { active?.let { replace(it.copy(indent = it.indent - 1u)) } }) { Icon(Icons.AutoMirrored.Rounded.FormatIndentDecrease, stringResource(R.string.page_outdent)) }
        }
        LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(horizontal = 20.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            items(blocks, key = { it.id }) { block ->
                PageBlockEditor(block, selected == block.id, changed = { replace(it, if (it.kind == block.kind && it.text != block.text) block.id else null) }, enter = { updated, offset ->
                    val (left, right) = PageEditing.split(updated, offset)
                    change(blocks.flatMap { if (it.id == block.id) listOf(left, right) else listOf(it) }); selected = right.id; range = TextRange.Zero
                }, selection = { nextRange, typed ->
                    if (selected != block.id || !typed && range != nextRange) typingMarks = emptyMap()
                    selected = block.id; range = nextRange
                }, format = { if (editable) { if (selected != block.id) typingMarks = emptyMap(); selected = block.id; menu = block.id } },
                    selectedRange = range.takeIf { selected == block.id }, typingMarks = typingMarks.takeIf { selected == block.id }.orEmpty(), editable = editable,
                    composing = { composing ->
                        if (composing && editable) {
                            if (composingBlocks.isEmpty()) compositionPage = rendered
                            composingBlocks = composingBlocks + block.id
                        } else {
                            composingBlocks = composingBlocks - block.id
                            if (composingBlocks.isEmpty()) compositionPage = null
                        }
                    })
            }
            if (editable) item { TextButton(onClick = { val block = PageEditing.blank(); change(blocks + block); selected = block.id }) { Icon(Icons.Rounded.Add, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.add_block)) } }
        }
        Surface(modifier = Modifier.keepSnackbarAbove(), tonalElevation = 2.dp) {
            Row(Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 20.dp, vertical = 12.dp), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
                val status = saveError ?: current.saveError ?: if (!editable) stringResource(R.string.read_only) else if (current.pendingSync) stringResource(R.string.page_pending_sync) else if (draft != null) stringResource(R.string.page_draft_saved) else saved
                Text(status, Modifier.weight(1f), style = MaterialTheme.typography.labelSmall,
                    color = if (saveError != null || current.saveError != null) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant)
                Button(enabled = editable && composingBlocks.isEmpty() && (draft != null || current.unsaved) && !busy, modifier = Modifier.testTag("page-save"), onClick = {
                    busy = true
                    model.launch {
                        try {
                            latestSave(true)
                        } catch (error: Exception) {
                            if (error is CancellationException) throw error
                            failed(error)
                        } finally { busy = false }
                    }
                }) { Text(stringResource(R.string.save)) }
            }
        }
    }
    if (discardDraft) AlertDialog(onDismissRequest = { discardDraft = false },
        title = { Text(stringResource(R.string.page_discard_question)) },
        text = { Text(stringResource(R.string.page_discard_detail)) },
        confirmButton = { TextButton(enabled = !busy, onClick = {
            discardDraft = false; busy = true
            model.launch {
                try {
                    model.pageSaves.save(item.id, editorToken, generation) {
                        val fresh = model.repository.query { core -> check(core.me()?.id == owner); core.page(item.id) }
                        draftStore.replaceAfterRestore(model.repository.preferences, draftKey, null)
                        page = fresh; draft = null; draftReadable = true; recoveryRequired = false
                        saveError = fresh.saveError; selected = null; typingMarks = emptyMap()
                        history = PageEditHistory(); historyRevision = 0
                        model.pageSaves.draftRecovery(item.id, false)
                    }
                } catch (error: Exception) { failed(error) } finally { busy = false }
            }
        }) { Text(stringResource(R.string.page_discard_draft)) } },
        dismissButton = { TextButton(onClick = { discardDraft = false }) { Text(stringResource(R.string.cancel)) } })
    menu?.let { id ->
        val block = blocks.firstOrNull { it.id == id }
        if (block != null && editable) AlertDialog(onDismissRequest = { menu = null }, title = { Text(stringResource(R.string.format)) }, text = {
            Column {
                val kinds = listOf("paragraph" to R.string.paragraph, "heading" to R.string.heading, "bullet" to R.string.page_bullet, "numbered" to R.string.page_numbered, "task" to R.string.task, "quote" to R.string.page_quote, "code" to R.string.page_code, "divider" to R.string.page_divider, "image" to R.string.page_image, "raw" to R.string.page_raw)
                Row(Modifier.horizontalScroll(rememberScrollState())) { kinds.forEach { (kind, label) -> FilterChip(block.kind == kind, { replace(block.copy(kind = kind, level = if (kind == "heading") 2u else 0u)); menu = null }, label = { Text(stringResource(label)) }, modifier = Modifier.padding(end = 4.dp)) } }
                if (block.kind == "heading") Row(Modifier.horizontalScroll(rememberScrollState())) { (1..6).forEach { level -> TextButton(onClick = { replace(block.copy(level = level.toUInt())); menu = null }) { Text("H$level") } } }
                TextButton(onClick = { val index = blocks.indexOf(block); val new = PageEditing.blank(); change(blocks.toMutableList().apply { add(index + 1, new) }); selected = new.id; menu = null }) { Text(stringResource(R.string.add_block)) }
                TextButton(onClick = { change(blocks.filterNot { it.id == id }); selected = null; menu = null }) { Text(stringResource(R.string.delete), color = MaterialTheme.colorScheme.error) }
            }
        }, confirmButton = { TextButton(onClick = { menu = null }) { Text(stringResource(R.string.done)) } })
    }
    if (linking && editable) AlertDialog(onDismissRequest = { linking = false }, title = { Text(stringResource(R.string.page_link)) }, text = { OutlinedTextField(link, { link = it }, label = { Text(stringResource(R.string.page_image_url)) }, singleLine = true) }, confirmButton = {
        TextButton(enabled = link.isBlank() || PageEditing.linkTarget(link) != null, onClick = { active?.let { block ->
            replace(PageEditing.setLink(block, link, range.start, range.end))
            if (range.collapsed && link.isNotBlank()) range = TextRange(range.start + link.trim().length)
            typingMarks = emptyMap()
        }; linking = false }) { Text(stringResource(R.string.save)) }
    }, dismissButton = { TextButton(onClick = { linking = false }) { Text(stringResource(R.string.cancel)) } })
}
