package xyz.tironi.zoen.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import java.text.DateFormat
import java.util.Date
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.miniapps.MiniAppTileStrip
import xyz.tironi.zoen.miniapps.MiniAppStoreScreen
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties

@Composable
private fun HomeBar(title: String, state: AppState, navigate: (NavKey) -> Unit, onStore: (() -> Unit)? = null) {
    ScreenBar(title, actions = {
        if (onStore != null) IconButton(onClick = onStore) { Icon(Icons.Rounded.Storefront, stringResource(R.string.miniapp_store)) }
        if (state.demo) AssistChip(onClick = { navigate(Context) }, label = { Text(stringResource(R.string.demo)) }, modifier = Modifier.padding(end = 4.dp))
        IconButton(onClick = { navigate(Search) }) { Icon(Icons.Rounded.Search, stringResource(R.string.search)) }
        IconButton(onClick = { navigate(Context) }, modifier = Modifier.testTag("open-context")) { Avatar(state.me, size = 34) }
    })
}

@Composable
fun ConversationsScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit, split: Boolean) {
    var store by rememberSaveable { mutableStateOf(false) }
    var selectedChat by rememberSaveable { mutableStateOf<String?>(null) }
    var filter by rememberSaveable { mutableIntStateOf(0) }
    var query by rememberSaveable { mutableStateOf("") }
    var pinned by remember { mutableStateOf(model.repository.preferences.getStringSet("pins", setOf("zoen"))!!.toSet()) }
    val chats = state.spaces.filter { it.kind != SpaceKindDto.COMMUNITY }
        .filter { when (filter) { 1 -> it.counterpart?.kind == PersonaKind.PERSON; 2 -> it.unread > 0u; else -> true } }
        .filter { matchesSpace(it, query) }
        .sortedByDescending { it.lastAtMs }
        .sortedByDescending { it.id in pinned || it.counterpart?.handle in pinned }
    val list: @Composable () -> Unit = {
        Scaffold(topBar = { HomeBar(stringResource(R.string.app_name), state, navigate, onStore = { store = true }) }, contentWindowInsets = WindowInsets(0, 0, 0, 0)) { padding ->
            LazyColumn(Modifier.fillMaxSize().padding(padding).testTag("conversation-list"), contentPadding = PaddingValues(bottom = 100.dp)) {
                item {
                    OutlinedTextField(query, { query = it }, Modifier.fillMaxWidth().padding(horizontal = 24.dp), singleLine = true, label = { Text(stringResource(R.string.search)) }, leadingIcon = { Icon(Icons.Rounded.Search, null) })
                }
                item {
                    Column(Modifier.padding(horizontal = 24.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(stringResource(R.string.chats), style = MaterialTheme.typography.headlineLarge)
                        Text(stringResource(R.string.your_day), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                val apps = state.items.filter { it.app != null }
                if (apps.isNotEmpty()) {
                    item { SectionLabel(stringResource(R.string.live_apps), Modifier.padding(horizontal = 24.dp)) }
                    item {
                        MiniAppTileStrip(model, state, apps, onOpenItem = { navigate(Item(it)) })
                    }
                }
                item {
                    LazyRow(contentPadding = PaddingValues(horizontal = 24.dp, vertical = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        items(3) { index -> FilterChip(selected = filter == index, onClick = { filter = index }, label = { Text(stringResource(listOf(R.string.all, R.string.people, R.string.unread)[index])) }) }
                    }
                }
                items(chats, key = { it.id }) { chat ->
                    val isPinned = chat.id in pinned || chat.counterpart?.handle in pinned
                    ConversationRow(chat, isPinned, selected = split && selectedChat == chat.id,
                        onClick = { if (split) selectedChat = chat.id else navigate(Chat(chat.id)) },
                        onPin = {
                            val key = if (chat.counterpart?.handle == "zoen") "zoen" else chat.id
                            pinned = if (isPinned) pinned - key else pinned + key
                            model.repository.preferences.edit().putStringSet("pins", pinned).apply()
                        },
                        onRead = { model.launch { model.repository.change { it.markRead(chat.id) } } },
                    )
                }
                if (chats.isEmpty()) item {
                    EmptyState(stringResource(R.string.chat_empty_title), stringResource(R.string.chat_empty_detail)) {
                        Button(onClick = { navigate(NewChat) }) { Text(stringResource(R.string.new_chat)) }
                    }
                }
            }
        }
    }
    if (split) Row {
        Box(Modifier.width(340.dp).fillMaxHeight()) { list() }
        VerticalDivider()
        Box(Modifier.weight(1f).fillMaxHeight()) {
            val chat = selectedChat?.let { id -> state.spaces.firstOrNull { it.id == id } }
            if (chat != null) ChatScreen(model, state, chat.id, navigate, { selectedChat = null })
            else Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { EmptyState(stringResource(R.string.chat_empty_title), stringResource(R.string.chat_empty_detail)) }
        }
    } else list()
    if (store) Dialog(onDismissRequest = { store = false }, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        MiniAppStoreScreen(model, state, onOpenItem = { store = false; navigate(Item(it)) }, back = { store = false }, onOpenAgent = { store = false; navigate(Agent(it)) })
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun ConversationRow(chat: SpaceSummary, pinned: Boolean, selected: Boolean, onClick: () -> Unit, onPin: () -> Unit, onRead: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    Box {
        Row(Modifier.fillMaxWidth().testTag("chat:${chat.counterpart?.handle ?: chat.id}").background(if (selected) MaterialTheme.colorScheme.primaryContainer else Color.Transparent)
            .combinedClickable(onClick = onClick, onLongClick = { menu = true }).padding(horizontal = 24.dp, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            if (chat.counterpart != null) Avatar(chat.counterpart, size = 58)
            else SpaceAvatar(chat, size = 58)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(chat.title, style = MaterialTheme.typography.titleMedium, fontWeight = if (chat.unread > 0u) FontWeight.Bold else FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(conversationPreview(chat), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(chat.lastAtMs)), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                if (chat.unread > 0u) Badge(containerColor = MaterialTheme.colorScheme.primary) { Text(chat.unread.toString()) }
                else if (pinned) Icon(Icons.Rounded.PushPin, stringResource(R.string.pinned), Modifier.size(16.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(text = { Text(stringResource(if (pinned) R.string.unpin else R.string.pin)) }, leadingIcon = { Icon(Icons.Rounded.PushPin, null) }, onClick = { menu = false; onPin() })
            DropdownMenuItem(text = { Text(stringResource(R.string.chat_mark_read)) }, leadingIcon = { Icon(Icons.Rounded.DoneAll, null) }, onClick = { menu = false; onRead() })
        }
    }
}

@Composable
private fun conversationPreview(space: SpaceSummary): String {
    val preview = if (xyz.tironi.zoen.media.VoiceNoteRef.parse(space.lastPreview) != null) stringResource(R.string.media_voice_message) else space.lastPreview
    val author = space.lastAuthor?.let { if (it.isMe) stringResource(R.string.files_you) else if (space.counterpart == null) it.name else "" }.orEmpty()
    return if (author.isBlank()) preview else "$author: $preview"
}

private fun matchesSpace(space: SpaceSummary, query: String): Boolean {
    fun folded(value: String) = java.text.Normalizer.normalize(value, java.text.Normalizer.Form.NFD).replace(Regex("\\p{M}+"), "").lowercase(java.util.Locale.ROOT)
    return folded((listOf(space.title, space.lastPreview) + space.members.map { it.name }).joinToString(" ")).contains(folded(query.trim()))
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun SpacesScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit) {
    var query by rememberSaveable { mutableStateOf("") }
    var pinned by remember { mutableStateOf(model.repository.preferences.getStringSet("pins", emptySet())!!.toSet()) }
    val groups = state.spaces.filter { it.kind == SpaceKindDto.COMMUNITY && matchesSpace(it, query) }.sortedByDescending { it.lastAtMs }.sortedByDescending { it.id in pinned }
    Scaffold(topBar = { HomeBar(stringResource(R.string.spaces), state, navigate) }, contentWindowInsets = WindowInsets(0, 0, 0, 0)) { padding ->
        LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(24.dp, 8.dp, 24.dp, 100.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            item { Text(stringResource(R.string.spaces_empty_title), style = MaterialTheme.typography.headlineLarge) }
            item { OutlinedTextField(query, { query = it }, Modifier.fillMaxWidth(), singleLine = true, label = { Text(stringResource(R.string.search)) }, leadingIcon = { Icon(Icons.Rounded.Search, null) }) }
            item { Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilledTonalButton(onClick = { navigate(NewSpace) }) { Icon(Icons.Rounded.Add, null); Spacer(Modifier.width(6.dp)); Text(stringResource(R.string.new_space)) }
                OutlinedButton(onClick = { navigate(Join()) }) { Text(stringResource(R.string.join)) }
            } }
            items(groups, key = { it.id }) { space ->
                var menu by remember { mutableStateOf(false) }
                Box {
                Card(modifier = Modifier.combinedClickable(onClick = { navigate(Chat(space.id)) }, onLongClick = { menu = true }), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
                    Column(Modifier.fillMaxWidth().padding(22.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                            SpaceAvatar(space, size = 48)
                            Spacer(Modifier.weight(1f))
                            if (space.unread > 0u) Badge { Text(space.unread.toString()) }
                            else if (space.id in pinned) Icon(Icons.Rounded.PushPin, stringResource(R.string.pinned))
                        }
                        Text(space.title, style = MaterialTheme.typography.titleLarge)
                        Text(conversationPreview(space), color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) { space.members.take(6).forEach { Avatar(it, size = 28) } }
                    }
                }
                DropdownMenu(menu, { menu = false }) {
                    DropdownMenuItem(text = { Text(stringResource(if (space.id in pinned) R.string.unpin else R.string.pin)) }, onClick = { menu = false; pinned = if (space.id in pinned) pinned - space.id else pinned + space.id; model.repository.preferences.edit().putStringSet("pins", pinned).apply() })
                    DropdownMenuItem(text = { Text(stringResource(R.string.chat_mark_read)) }, onClick = { menu = false; model.launch { model.repository.change { it.markRead(space.id) } } })
                }
                }
            }
            if (groups.isEmpty()) item { EmptyState(stringResource(R.string.spaces_empty_title), stringResource(R.string.spaces_empty_detail)) }
        }
    }
}

@Composable
fun ActivityScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit) {
    var section by rememberSaveable { mutableIntStateOf(0) }
    val loadedMentions by produceState<List<Mention>>(emptyList(), state.revision) { value = model.repository.query { it.mentions() } }
    val mentions = loadedMentions
    val tasks = state.items.filter { it.kindId == "task" || it.plan?.sections?.any { section -> section.lines.any { line -> !line.done } } == true }
    val pending = state.requests.filter { it.agent.isMine && (it.status == RequestStatus.PENDING || it.status == RequestStatus.STALE) }
    val resolved = state.requests.filter { it.status == RequestStatus.APPROVED || it.status == RequestStatus.DENIED }
    var review by remember { mutableStateOf<List<AgentRequestDto>?>(null) }
    var busy by remember { mutableStateOf(false) }
    Scaffold(topBar = { HomeBar(stringResource(R.string.activity), state, navigate) }, contentWindowInsets = WindowInsets(0, 0, 0, 0)) { padding ->
        LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(24.dp, 8.dp, 24.dp, 100.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { LazyRow(Modifier.testTag("activity-tabs"), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                items(3) { index ->
                    val count = listOf(pending.size, mentions.size, tasks.size)[index]
                    FilterChip(section == index, { section = index }, label = { Text(stringResource(listOf(R.string.activity_approvals, R.string.activity_mentions, R.string.activity_tasks)[index]) + if (count > 0) " · $count" else "") })
                }
            } }
            when (section) {
                1 -> {
                    if (mentions.isEmpty()) item { EmptyState(stringResource(R.string.activity_no_mentions), "") }
                    items(mentions, key = { it.entry.id }) { mention ->
                        Card(onClick = { navigate(Chat(mention.spaceId, mention.entry.id)) }) {
                            ListItem(leadingContent = { Avatar(mention.entry.author, size = 40) }, headlineContent = { Text(stringResource(R.string.activity_mentioned, mention.entry.author.name)) }, supportingContent = {
                                Column { Text((mention.entry.kind as? EntryKind.Message)?.text.orEmpty(), maxLines = 3, overflow = TextOverflow.Ellipsis); Text(mention.spaceTitle, style = MaterialTheme.typography.labelMedium) }
                            })
                        }
                    }
                }
                2 -> {
                    if (tasks.isEmpty()) item { EmptyState(stringResource(R.string.activity_no_tasks), "") }
                    items(tasks, key = { it.id }) { task -> Card(onClick = { navigate(Item(task.id)) }) { ListItem(headlineContent = { Text(task.title) }, supportingContent = { Text(task.spaceTitle) }, leadingContent = { Icon(Icons.Rounded.Checklist, null) }) } }
                }
                else -> {
            items(state.agents.filter { it.nearLimit }, key = { "budget:${it.persona.id}" }) { agent ->
                Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.tertiaryContainer)) {
                    Column(Modifier.fillMaxWidth().padding(16.dp)) {
                        Text(stringResource(R.string.activity_near_cap, agent.persona.name), style = MaterialTheme.typography.titleMedium)
                        Text(formatMoney(agent.budgetSpentCents ?: 0, model.repository.locale) + " / " + formatMoney(agent.budgetLimitCents ?: 0, model.repository.locale))
                        TextButton(onClick = { model.launch { model.repository.change { it.raiseBudget(agent.persona.id, 2000) } } }) { Text("+" + formatMoney(2000, model.repository.locale)) }
                    }
                }
            }
            item { Text(stringResource(R.string.pending), style = MaterialTheme.typography.headlineMedium) }
            val approvable = pending.filter { it.status == RequestStatus.PENDING }
            if (approvable.size > 1) item {
                OutlinedButton(onClick = { review = approvable.toList() }, enabled = !busy) { Text(stringResource(R.string.approve_all)) }
            }
            if (pending.isEmpty()) item { EmptyState(stringResource(R.string.all_clear), stringResource(R.string.all_clear_detail)) }
            items(pending, key = { it.id }) { request -> RequestCard(request, { navigate(Request(request.id)) }) }
            if (resolved.isNotEmpty()) item { SectionLabel(stringResource(R.string.resolved)) }
            items(resolved, key = { it.id }) { request -> RequestCard(request, { navigate(Request(request.id)) }) }
                }
            }
        }
    }
    review?.let { requests ->
        val recorded = stringResource(R.string.approvals_recorded, requests.size)
        AlertDialog(onDismissRequest = { review = null }, title = { Text(stringResource(R.string.review)) }, text = {
            LazyColumn(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                items(requests, key = { it.id }) { request ->
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(request.title, fontWeight = FontWeight.SemiBold)
                        Text(request.agent.name + " · " + request.spaceTitle, style = MaterialTheme.typography.labelMedium)
                        Text(request.detail)
                        request.costCents?.let { Text(formatMoney(it, model.repository.locale)) }
                    }
                }
            }
        }, confirmButton = {
            TextButton(onClick = {
                review = null; busy = true
                model.launch {
                    try {
                        for (request in requests) model.repository.change { it.decideRequest(request.id, RequestDecision.APPROVE) }
                        model.notify(recorded)
                    } finally { busy = false }
                }
            }) { Text(stringResource(R.string.approve_all)) }
        }, dismissButton = { TextButton(onClick = { review = null }) { Text(stringResource(R.string.cancel)) } })
    }
}

@Composable
fun RequestCard(request: AgentRequestDto, onClick: () -> Unit) {
    Card(onClick = onClick, colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
        Column(Modifier.fillMaxWidth().padding(20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Avatar(request.agent, size = 38)
                Column { Text(request.agent.name, style = MaterialTheme.typography.titleSmall); Text(request.spaceTitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            }
            Text(request.title, style = MaterialTheme.typography.titleMedium)
            Text(request.detail, style = MaterialTheme.typography.bodyMedium, maxLines = 3, overflow = TextOverflow.Ellipsis)
            Text(stringResource(requestStatusLabel(request.status)), style = MaterialTheme.typography.labelMedium, color = if (request.status == RequestStatus.STALE) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary)
        }
    }
}

fun requestStatusLabel(status: RequestStatus): Int = when (status) {
    RequestStatus.PENDING -> R.string.status_pending
    RequestStatus.APPROVED -> R.string.status_approved
    RequestStatus.DENIED -> R.string.status_denied
    RequestStatus.STALE -> R.string.status_stale
}
