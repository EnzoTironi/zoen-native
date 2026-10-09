package xyz.tironi.zoen.ui

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Chat
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.data.FileAccess
import java.text.Normalizer

enum class FileScope { ALL, PERSONAL, SHARED, AGENTS }

fun filterFiles(state: AppState, scope: FileScope, query: String, folder: String? = null): List<ItemDetail> {
    fun folded(value: String) = Normalizer.normalize(value, Normalizer.Form.NFD).replace(Regex("\\p{M}+"), "").lowercase(java.util.Locale.ROOT)
    val search = folded(query)
    val spaces = state.spaces.associateBy { it.id }
    return state.items.filter { item ->
        val people = spaces[item.spaceId]?.members?.count { it.kind == PersonaKind.PERSON } ?: 0
        val matchesScope = when (scope) {
            FileScope.ALL -> true
            FileScope.PERSONAL -> people <= 1
            FileScope.SHARED -> people > 1
            FileScope.AGENTS -> item.createdBy.kind == PersonaKind.AGENT
        }
        (folder == null || item.spaceId == folder) && matchesScope && (search.isBlank() || folded("${item.title} ${item.spaceTitle}").contains(search))
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FilesScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit, back: () -> Unit, folder: String? = null) {
    var query by rememberSaveable(folder) { mutableStateOf("") }
    var scope by rememberSaveable(folder) { mutableStateOf(FileScope.ALL) }
    var add by rememberSaveable { mutableStateOf(false) }
    var createIn by rememberSaveable { mutableStateOf<String?>(null) }
    var importInto by rememberSaveable { mutableStateOf<String?>(null) }
    var title by rememberSaveable { mutableStateOf("") }
    val context = LocalContext.current
    val filtered = remember(state.items, state.spaces, query, scope, folder) { filterFiles(state, scope, query, folder) }
    val folders = filtered.groupBy { it.spaceId }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        val space = importInto
        importInto = null
        if (space != null && uris.isNotEmpty()) model.launch {
            val made = FileAccess.importMany(context, model.repository, space, uris)
            if (made.size == 1) navigate(Item(made.single().id))
        }
    }
    val startImport: (String) -> Unit = { id -> importInto = id; add = false; picker.launch(arrayOf("*/*")) }
    Scaffold(topBar = { ScreenBar(state.spaces.firstOrNull { it.id == folder }?.title ?: stringResource(R.string.files), back, actions = {
        IconButton({ add = true }, enabled = state.spaces.isNotEmpty(), modifier = Modifier.testTag("files-add")) { Icon(Icons.Rounded.Add, stringResource(R.string.files_add)) }
    }) }) { padding ->
        LazyColumn(Modifier.padding(padding).fillMaxSize(), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            item { OutlinedTextField(query, { query = it }, Modifier.fillMaxWidth().testTag("files-search"), singleLine = true, label = { Text(stringResource(R.string.files_search)) }, leadingIcon = { Icon(Icons.Rounded.Search, null) }) }
            if (folder == null) {
                item { LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) { items(FileScope.entries) { option ->
                    FilterChip(scope == option, { scope = option }, label = { Text(stringResource(when (option) { FileScope.ALL -> R.string.all; FileScope.PERSONAL -> R.string.files_personal; FileScope.SHARED -> R.string.files_shared; FileScope.AGENTS -> R.string.files_agents })) })
                } } }
                if (folders.isNotEmpty()) item { SectionLabel(stringResource(R.string.files_folders)) }
                items(folders.keys.toList(), key = { "folder:$it" }) { id ->
                    val space = state.spaces.firstOrNull { it.id == id }
                    if (space != null) Card(onClick = { navigate(Folder(id)) }, modifier = Modifier.testTag("folder:$id")) {
                        ListItem(headlineContent = { Text(space.title, fontWeight = FontWeight.SemiBold) }, supportingContent = {
                            val count = folders[id]?.size ?: 0
                            Text(pluralStringResource(R.plurals.files_items, count, count))
                        }, leadingContent = { Icon(Icons.Rounded.Folder, null, tint = MaterialTheme.colorScheme.tertiary) })
                    }
                }
                if (filtered.isNotEmpty()) item { SectionLabel(stringResource(R.string.files_recent)) }
            }
            items(filtered, key = { it.id }) { item -> FileItemRow(model, item) { navigate(Item(item.id)) } }
            if (filtered.isEmpty()) item { EmptyState(stringResource(R.string.no_files), stringResource(R.string.no_files_detail)) }
            if (folder != null) item {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(stringResource(R.string.files_versions), style = MaterialTheme.typography.bodySmall)
                    TextButton({ createIn = folder; title = "" }) { Icon(Icons.Rounded.NoteAdd, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.new_page)) }
                    TextButton({ startImport(folder) }) { Icon(Icons.Rounded.FileUpload, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.files_import_many)) }
                    TextButton({ navigate(Chat(folder)) }) { Icon(Icons.AutoMirrored.Rounded.Chat, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.files_open_chat)) }
                }
            }
        }
    }
    if (add) ModalBottomSheet(onDismissRequest = { add = false }) {
        Text(stringResource(R.string.files_add_to), Modifier.padding(24.dp), style = MaterialTheme.typography.titleLarge)
        LazyColumn(contentPadding = PaddingValues(horizontal = 24.dp, vertical = 8.dp)) {
            items(state.spaces.filter { folder == null || it.id == folder }, key = { it.id }) { space ->
                Column(Modifier.padding(bottom = 16.dp)) {
                    Text(space.title, style = MaterialTheme.typography.titleMedium)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        TextButton({ add = false; createIn = space.id; title = "" }) { Text(stringResource(R.string.new_page)) }
                        TextButton({ startImport(space.id) }) { Text(stringResource(R.string.files_import_many)) }
                    }
                }
            }
        }
    }
    if (createIn != null) AlertDialog(onDismissRequest = { createIn = null }, title = { Text(stringResource(R.string.new_page)) }, text = {
        OutlinedTextField(title, { title = it }, label = { Text(stringResource(R.string.page_title)) })
    }, confirmButton = {
        TextButton({ val space = checkNotNull(createIn); createIn = null; model.launch { navigate(Item(model.repository.change { it.pageCreate(space, title) }.id)) } }) { Text(stringResource(R.string.create)) }
    }, dismissButton = { TextButton({ createIn = null }) { Text(stringResource(R.string.cancel)) } })
}

@Composable
private fun FileItemRow(model: ZoenViewModel, item: ItemDetail, open: () -> Unit) {
    val context = LocalContext.current
    val thumbnail by produceState<android.graphics.Bitmap?>(null, item.id, item.version) {
        if (item.file?.hasThumbnail == true) value = model.repository.query { core -> core.fileThumbnail(item.id)?.let { android.graphics.BitmapFactory.decodeByteArray(it, 0, it.size) } }
    }
    val preview = thumbnail
    val latest = item.versions.maxByOrNull { it.number }
    val who = latest?.author?.let { if (it.isMe) stringResource(R.string.files_you) else it.name } ?: item.createdBy.name
    Card(onClick = open, colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
        ListItem(headlineContent = { Text(item.title, fontWeight = FontWeight.SemiBold) }, supportingContent = {
            Text(if (item.file?.ready == false) stringResource(R.string.file_not_ready) else listOfNotNull(item.spaceTitle, item.file?.let { android.text.format.Formatter.formatShortFileSize(context, it.bytes.toLong()) }, who, latest?.let { java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.SHORT, java.text.DateFormat.SHORT).format(java.util.Date(it.atMs)) }).joinToString(" · "))
        }, leadingContent = {
            if (preview != null) Image(preview.asImageBitmap(), null, Modifier.size(40.dp))
            else Icon(appIcon(item.app?.appId ?: item.kindId), null, tint = MaterialTheme.colorScheme.primary)
        }, trailingContent = { Text("v${item.version}", style = MaterialTheme.typography.labelSmall) })
    }
}
