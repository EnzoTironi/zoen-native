package xyz.tironi.zoen.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState

@Composable
fun NewChatScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit, back: () -> Unit) {
    var query by rememberSaveable { mutableStateOf("") }
    var group by rememberSaveable { mutableStateOf(false) }
    var title by rememberSaveable { mutableStateOf("") }
    var selected by rememberSaveable { mutableStateOf(listOf<String>()) }
    var people by remember { mutableStateOf(listOf<Persona>()) }
    var loading by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(query, state.demo) {
        error = null
        if (state.demo) people = model.repository.query { it.personas().filter { p -> !p.isMe && p.kind == PersonaKind.PERSON && (p.name.contains(query, true) || p.handle.contains(query, true)) } }
        else if (query.isBlank()) people = emptyList()
        else {
            delay(300); loading = true
            try { people = model.repository.network { it.findPeople(query) } }
            catch (e: Exception) { if (e is CancellationException) throw e; people = emptyList(); error = e.message }
            finally { loading = false }
        }
    }
    Scaffold(topBar = { ScreenBar(stringResource(if (group) R.string.new_group else R.string.new_chat), back, actions = {
        IconButton(onClick = { group = !group }) { Icon(Icons.Rounded.GroupAdd, stringResource(R.string.new_group)) }
    }) }) { padding ->
        Column(Modifier.padding(padding).imePadding().padding(horizontal = 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (group) OutlinedTextField(title, { title = it }, Modifier.fillMaxWidth(), label = { Text(stringResource(R.string.group_name)) })
            OutlinedTextField(query, { query = it }, Modifier.fillMaxWidth(), label = { Text(stringResource(R.string.find_people)) }, singleLine = true)
            if (loading) LinearProgressIndicator(Modifier.fillMaxWidth())
            error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            LazyColumn(Modifier.weight(1f)) {
                items(people, key = { it.id }) { person ->
                    ListItem(headlineContent = { Text(person.name) }, supportingContent = { Text("@${person.handle}") }, leadingContent = { Avatar(person, size = 40) }, trailingContent = { if (group) Checkbox(person.id in selected, onCheckedChange = { selected = if (person.id in selected) selected - person.id else selected + person.id }) },
                        modifier = Modifier.clickable {
                            if (group) selected = if (person.id in selected) selected - person.id else selected + person.id
                            else model.launch {
                                val existing = state.spaces.firstOrNull { it.counterpart?.id == person.id }
                                val id = existing?.id ?: model.repository.change { it.startDirect(person.id) }
                                back(); navigate(Chat(id))
                            }
                        },
                    )
                }
            }
            if (group) Button(onClick = { model.launch { val id = model.repository.change { it.createGroup(title, selected) }; back(); navigate(Chat(id)) } }, enabled = title.isNotBlank() && selected.isNotEmpty(), modifier = Modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = 16.dp)) { Text(stringResource(R.string.create)) }
        }
    }
}

@Composable
fun NewSpaceScreen(model: ZoenViewModel, navigate: (NavKey) -> Unit, back: () -> Unit) {
    var title by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.new_space), back) }) { padding ->
        Column(Modifier.padding(padding).imePadding().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            ZoenMascot(Modifier.size(160.dp), pose = MascotPose.Map)
            Text(stringResource(R.string.spaces_empty_title), style = MaterialTheme.typography.headlineMedium)
            Text(stringResource(R.string.spaces_empty_detail), color = MaterialTheme.colorScheme.onSurfaceVariant)
            OutlinedTextField(title, { title = it }, Modifier.fillMaxWidth(), label = { Text(stringResource(R.string.space_name)) })
            Button(onClick = { busy = true; model.launch { try { val id = model.repository.change { it.createCommunity(title) }; back(); navigate(Chat(id)) } finally { busy = false } } }, enabled = title.isNotBlank() && !busy, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.create)) }
        }
    }
}

@Composable
fun JoinScreen(model: ZoenViewModel, initial: String, navigate: (NavKey) -> Unit, back: () -> Unit) {
    var code by rememberSaveable { mutableStateOf(initial) }
    var preview by remember { mutableStateOf<InvitePreviewDto?>(null) }
    var loading by remember { mutableStateOf(false) }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.join_space), back) }) { padding ->
        Column(Modifier.padding(padding).imePadding().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            OutlinedTextField(code, { code = it; preview = null }, Modifier.fillMaxWidth(), label = { Text(stringResource(R.string.invite_code)) })
            preview?.let { info -> Card { Column(Modifier.fillMaxWidth().padding(20.dp)) { Text(info.title, style = MaterialTheme.typography.titleLarge); info.inviter?.let { Text(it.name) } } } }
            Button(onClick = {
                loading = true
                model.launch {
                    try {
                        if (preview == null) preview = model.repository.network { it.previewInvite(code) }
                        else { val id = model.repository.network { it.joinInvite(code) }; back(); navigate(Chat(id)) }
                    } finally { loading = false }
                }
            }, enabled = code.isNotBlank() && !loading, modifier = Modifier.fillMaxWidth()) { Text(stringResource(if (preview == null) R.string.review else R.string.join)) }
        }
    }
}
