package xyz.tironi.zoen.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import java.text.DateFormat
import java.util.Date
import kotlinx.coroutines.CancellationException
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.data.FileAccess
import xyz.tironi.zoen.miniapps.MiniAppDetailsButton
import xyz.tironi.zoen.pages.PlanPageView
import xyz.tironi.zoen.pages.planPageText
import xyz.tironi.zoen.pages.planWrite

private data class PlanReadState(val page: PlanPageDto? = null, val revision: Long = -1, val failure: String? = null)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ItemScreen(model: ZoenViewModel, state: AppState, id: String, navigate: (NavKey) -> Unit, back: () -> Unit) {
    val item = state.items.firstOrNull { it.id == id }
    val living = model.repository.preferences.getBoolean("LivingSpacesEnabled", false)
    var versions by rememberSaveable(id) { mutableStateOf(false) }
    var restore by remember(id) { mutableStateOf<UInt?>(null) }
    // The binding the version sheet saw when it opened. Restore never re-reads it at tap time.
    var sheetHash by remember(id) { mutableStateOf<String?>(null) }
    // Bumped by a stale refusal so the sheet re-reads the binding before the next restore.
    var sheetReload by remember(id) { mutableIntStateOf(0) }
    LaunchedEffect(id, versions, item?.kindId, sheetReload) {
        sheetHash = null
        if (versions && item?.kindId == "plan") sheetHash = try { model.repository.query { it.planPage(id).contentHash } }
            catch (error: Exception) { if (error is CancellationException) throw error; null }
    }
    var planRead by remember(id) { mutableStateOf(PlanReadState()) }
    LaunchedEffect(id, item?.kindId, state.revision) {
        if (item?.kindId == "plan") {
            planRead = try { PlanReadState(model.repository.query { it.planPage(id) }, state.revision) }
            catch (error: Exception) {
                if (error is CancellationException) throw error
                PlanReadState(revision = state.revision, failure = error.message)
            }
        } else planRead = PlanReadState()
    }
    val planFresh = planRead.revision == state.revision
    val historyAvailable = (!living || item?.app == null) && (item?.kindId != "plan" || planFresh && planRead.page?.legacyEditAdmitted == true)
    val restoreAllowed = historyAvailable && (item?.kindId != "page" || model.pageSaves.canRestore(id)) &&
        (item?.kindId != "plan" || sheetHash != null)
    val context = LocalContext.current
    val focus = LocalFocusManager.current
    val fileUnavailable = stringResource(R.string.file_not_ready)
    val fileTooLarge = stringResource(R.string.file_open_too_large)
    val saved = stringResource(R.string.saved)
    val pendingSync = stringResource(R.string.page_pending_sync)
    val draftSaved = stringResource(R.string.page_draft_saved)
    val draftRecovery = stringResource(R.string.page_recovery_before_restore)
    val changed = stringResource(R.string.plan_changed)
    DisposableEffect(id) { model.viewingItem(id); onDispose { model.viewingItem(null) } }
    if (item == null) {
        Scaffold(topBar = { ScreenBar(stringResource(R.string.files), back) }) { padding -> EmptyState(stringResource(R.string.unavailable), stringResource(R.string.unavailable_detail), Modifier.padding(padding)) }
        return
    }
    Scaffold(topBar = {
        ScreenBar(item.title, back, actions = {
            if (!living && item.app != null) MiniAppDetailsButton(model, state, item)
            IconButton(enabled = historyAvailable, onClick = { focus.clearFocus(force = true); model.launch {
                try { model.pageSaves.flush(id) } catch (error: Exception) {
                    model.notify(if (error is CoreException.Stale) draftRecovery else error.message.orEmpty())
                }
                versions = true
            } }, modifier = Modifier.testTag("item-history")) { Icon(Icons.Rounded.History, stringResource(R.string.versions)) }
            IconButton(onClick = {
                model.launch {
                    model.pageSaves.flush(id)
                    when {
                        item.file != null -> {
                            val file = model.repository.query { readFileVersion(it, id, item.version, fileTooLarge) }
                            FileAccess.shareFile(context, file.info.name, file.info.mime, file.bytes ?: error(fileUnavailable))
                        }
                        item.kindId == "page" -> FileAccess.shareFile(context, "${item.title}.md", "text/markdown", model.repository.query { it.pageMarkdown(id) }.toByteArray())
                        item.kindId == "plan" -> FileAccess.shareText(context, planPageText(model.repository.query { it.planPage(id) }, model.repository.locale))
                        else -> FileAccess.shareText(context, itemText(item, model.repository.locale))
                    }
                }
            }, modifier = Modifier.testTag("item-share")) { Icon(Icons.Rounded.Share, stringResource(R.string.share)) }
        })
    }) { padding ->
        when {
            living && item.app != null -> Column(Modifier.padding(padding).padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Text(item.title, style = MaterialTheme.typography.headlineLarge)
                Text(item.app!!.headline, style = MaterialTheme.typography.bodyLarge)
                if (!item.text.isNullOrBlank()) Text(item.text.orEmpty())
                Text("Mini-apps are paused in this Space.", color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            item.app != null -> MiniAppScreen(model, state, item, Modifier.padding(padding), onClose = back)
            item.kindId == "page" -> xyz.tironi.zoen.pages.RichPageEditor(model, state, item, Modifier.padding(padding))
            item.kindId == "plan" -> PlanPageView(model, item, planRead.page, planRead.failure, planFresh, Modifier.padding(padding), navigate)
            item.file != null -> FileScreen(model, state, item, Modifier.padding(padding))
            else -> LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                item {
                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Icon(appIcon(item.kindId), null, Modifier.size(32.dp), tint = MaterialTheme.colorScheme.primary)
                        Text(item.title, style = MaterialTheme.typography.displaySmall)
                        Text(item.text.orEmpty(), color = MaterialTheme.colorScheme.onSurfaceVariant)
                        AssistChip(onClick = { navigate(Chat(item.spaceId)) }, label = { Text(item.spaceTitle) }, leadingIcon = { Icon(Icons.Rounded.Forum, null, Modifier.size(16.dp)) })
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
    if (versions) {
        // Back, the backdrop and a downward drag all ask the sheet to hide. On slow devices that
        // request can arrive while the opening animation is still running, and the opening
        // animation then wins and cancels the hide, leaving the sheet open. Remember the request
        // and close the sheet once the hide ends, whether it finished or was cancelled.
        var closing by remember { mutableStateOf(false) }
        val historySheet = rememberModalBottomSheetState(skipPartiallyExpanded = true,
            confirmValueChange = { target -> if (target == SheetValue.Hidden) closing = true; true })
        LaunchedEffect(closing) {
            if (closing) try { historySheet.hide() } finally { versions = false }
        }
        ModalBottomSheet(onDismissRequest = { versions = false }, sheetState = historySheet) {
        Text(stringResource(R.string.versions), Modifier.padding(horizontal = 24.dp, vertical = 12.dp), style = MaterialTheme.typography.headlineMedium)
        LazyColumn(Modifier.testTag("item-versions"), contentPadding = PaddingValues(bottom = 24.dp)) {
            items(item.versions.reversed(), key = { it.number.toInt() }) { version ->
                ListItem(modifier = Modifier.testTag("version:${version.number}").clickable(enabled = historyAvailable) { versions = false; navigate(VersionPreview(id, version.number)) },
                    headlineContent = { Text(stringResource(R.string.version, version.number.toInt())) }, supportingContent = {
                    Text(version.note + "\n" + version.author.name + " · " + DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(version.atMs)))
                }, leadingContent = { Icon(if (version.isUndo) Icons.Rounded.Undo else Icons.Rounded.History, null) }, trailingContent = {
                    if (version.number != item.version) TextButton(onClick = { restore = version.number }, enabled = restoreAllowed,
                        modifier = Modifier.testTag("restore-version:${version.number}")) { Text(stringResource(R.string.restore)) }
                    else Icon(Icons.Rounded.Check, stringResource(R.string.saved))
                })
            }
        }
        }
    }
    restore?.let { number -> AlertDialog(onDismissRequest = { restore = null }, title = { Text(stringResource(R.string.restore)) }, text = { Text(stringResource(R.string.restore_question)) }, confirmButton = {
        TextButton(enabled = restoreAllowed, modifier = Modifier.testTag("confirm-restore-version"), onClick = {
            val observed = sheetHash
            restore = null
            val owner = state.me?.id ?: return@TextButton
            model.launch {
                model.planWrite(changed, onStale = { sheetReload++ }) {
                    val fresh = model.pageSaves.restoreVersion(model.repository, id, owner, number, observed)
                    versions = false
                    model.notify(fresh?.saveError ?: when {
                        fresh?.pendingSync == true -> pendingSync
                        fresh?.unsaved == true -> draftSaved
                        else -> saved
                    })
                }
            }
        }) { Text(stringResource(R.string.restore)) }
    }, dismissButton = { TextButton(onClick = { restore = null }) { Text(stringResource(R.string.cancel)) } }) }
}

fun itemText(item: ItemDetail, locale: String): String = buildString {
    appendLine("# ${item.title}")
    item.plan?.let { plan ->
        appendLine(plan.summary)
        plan.sections.forEach { section ->
            appendLine("\n## ${section.title}")
            section.lines.forEach { line -> appendLine("- [${if (line.done) "x" else " "}] ${line.text} · ${formatMoney(line.costCents, locale)}") }
        }
    } ?: appendLine(item.text ?: item.app?.headline.orEmpty())
}
