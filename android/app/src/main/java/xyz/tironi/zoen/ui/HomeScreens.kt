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

@Composable
private fun HomeBar(title: String, state: AppState, navigate: (NavKey) -> Unit) {
    ScreenBar(title, actions = {
        if (state.demo) AssistChip(onClick = { navigate(Context) }, label = { Text(stringResource(R.string.demo)) }, modifier = Modifier.padding(end = 4.dp))
        IconButton(onClick = { navigate(Search) }) { Icon(Icons.Rounded.Search, stringResource(R.string.search)) }
        IconButton(onClick = { navigate(Context) }) { Avatar(state.me, size = 34) }
    })
}

@Composable
fun ConversationsScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit, split: Boolean) {
    var selectedChat by rememberSaveable { mutableStateOf<String?>(null) }
    var filter by rememberSaveable { mutableIntStateOf(0) }
    var pinned by remember { mutableStateOf(model.repository.preferences.getStringSet("pins", setOf("zoen"))!!.toSet()) }
    val chats = state.spaces.filter { it.kind != SpaceKindDto.COMMUNITY }
        .filter { when (filter) { 1 -> it.counterpart?.kind == PersonaKind.PERSON; 2 -> it.unread > 0u; else -> true } }
        .sortedByDescending { it.id in pinned || it.counterpart?.handle in pinned }
    val list: @Composable () -> Unit = {
        Scaffold(topBar = { HomeBar(stringResource(R.string.app_name), state, navigate) }, contentWindowInsets = WindowInsets(0, 0, 0, 0)) { padding ->
            LazyColumn(Modifier.fillMaxSize().padding(padding), contentPadding = PaddingValues(bottom = 100.dp)) {
                item {
                    Column(Modifier.padding(horizontal = 24.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(stringResource(R.string.chats), style = MaterialTheme.typography.headlineLarge)
                        Text(stringResource(R.string.your_day), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                val apps = state.items.filter { it.app != null }.take(12)
                if (apps.isNotEmpty()) {
                    item { SectionLabel(stringResource(R.string.live_apps), Modifier.padding(horizontal = 24.dp)) }
                    item {
                        LazyRow(contentPadding = PaddingValues(horizontal = 24.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            items(apps, key = { it.id }) { item -> ItemTile(item, { navigate(Item(item.id)) }) }
                        }
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
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun ConversationRow(chat: SpaceSummary, pinned: Boolean, selected: Boolean, onClick: () -> Unit, onPin: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    Box {
        Row(Modifier.fillMaxWidth().testTag("chat:${chat.counterpart?.handle ?: chat.id}").background(if (selected) MaterialTheme.colorScheme.primaryContainer else Color.Transparent)
            .combinedClickable(onClick = onClick, onLongClick = { menu = true }).padding(horizontal = 24.dp, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            if (chat.counterpart != null) Avatar(chat.counterpart, size = 58)
            else Surface(shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.tertiaryContainer) {
                Icon(Icons.Rounded.Groups, null, Modifier.padding(15.dp).size(28.dp), tint = MaterialTheme.colorScheme.onTertiaryContainer)
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(chat.title, style = MaterialTheme.typography.titleMedium, fontWeight = if (chat.unread > 0u) FontWeight.Bold else FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(chat.lastPreview, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(chat.lastAtMs)), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                if (chat.unread > 0u) Badge(containerColor = MaterialTheme.colorScheme.primary) { Text(chat.unread.toString()) }
                else if (pinned) Icon(Icons.Rounded.PushPin, stringResource(R.string.pinned), Modifier.size(16.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(text = { Text(stringResource(if (pinned) R.string.unpin else R.string.pin)) }, leadingIcon = { Icon(Icons.Rounded.PushPin, null) }, onClick = { menu = false; onPin() })
        }
    }
}

@Composable
fun SpacesScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit) {
    val groups = state.spaces.filter { it.kind != SpaceKindDto.DIRECT }
    Scaffold(topBar = { HomeBar(stringResource(R.string.spaces), state, navigate) }, contentWindowInsets = WindowInsets(0, 0, 0, 0)) { padding ->
        LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(24.dp, 8.dp, 24.dp, 100.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            item { Text(stringResource(R.string.spaces_empty_title), style = MaterialTheme.typography.headlineLarge) }
            item { Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilledTonalButton(onClick = { navigate(NewSpace) }) { Icon(Icons.Rounded.Add, null); Spacer(Modifier.width(6.dp)); Text(stringResource(R.string.new_space)) }
                OutlinedButton(onClick = { navigate(Join()) }) { Text(stringResource(R.string.join)) }
            } }
            items(groups, key = { it.id }) { space ->
                Card(onClick = { navigate(Chat(space.id)) }, colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
                    Column(Modifier.fillMaxWidth().padding(22.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Rounded.Groups, null, Modifier.size(36.dp), tint = MaterialTheme.colorScheme.primary)
                            Spacer(Modifier.weight(1f))
                            if (space.unread > 0u) Badge { Text(space.unread.toString()) }
                        }
                        Text(space.title, style = MaterialTheme.typography.titleLarge)
                        Text(space.lastPreview, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) { space.members.take(6).forEach { Avatar(it, size = 28) } }
                    }
                }
            }
            if (groups.isEmpty()) item { EmptyState(stringResource(R.string.spaces_empty_title), stringResource(R.string.spaces_empty_detail)) }
        }
    }
}

@Composable
fun ActivityScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit) {
    val pending = state.requests.filter { it.status == RequestStatus.PENDING || it.status == RequestStatus.STALE }
    val resolved = state.requests.filter { it.status == RequestStatus.APPROVED || it.status == RequestStatus.DENIED }
    var review by remember { mutableStateOf<List<AgentRequestDto>?>(null) }
    var busy by remember { mutableStateOf(false) }
    Scaffold(topBar = { HomeBar(stringResource(R.string.activity), state, navigate) }, contentWindowInsets = WindowInsets(0, 0, 0, 0)) { padding ->
        LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(24.dp, 8.dp, 24.dp, 100.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
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
