package xyz.tironi.zoen.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.unit.sp
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import java.text.DateFormat
import java.util.Date
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.miniapps.MiniAppTileStrip
import xyz.tironi.zoen.miniapps.WidgetSnapshot
import org.json.JSONObject
import xyz.tironi.zoen.miniapps.MiniAppStoreScreen
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties

@Composable
@OptIn(ExperimentalMaterial3Api::class)
internal fun HomeBar(title: String, state: AppState, navigate: (NavKey) -> Unit, onBack: (() -> Unit)? = null) {
    // Root destinations are entirely bottom navigated, just like the iPhone shell.
    if (onBack == null) return
    TopAppBar(title = {
        Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }, navigationIcon = {
        if (onBack != null) NativeIconButton(onClick = onBack) { MotionVectorIcon(Icons.AutoMirrored.Rounded.ArrowBack, stringResource(R.string.back)) }
    }, actions = {
        if (state.account != null && state.connection.state == "offline") {
            NativeIconButton(onClick = { navigate(SettingsRoute("Devices")) }, modifier = Modifier.testTag("connection-status")) {
                MotionVectorIcon(Icons.Rounded.Circle, stringResource(if (state.connection.state == "connecting") R.string.connecting else R.string.offline_short), Modifier.size(8.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }, colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.background))
}

@Composable
fun ConversationsScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit, split: Boolean) {
    var selectedChat by rememberSaveable { mutableStateOf<String?>(null) }
    var searching by rememberSaveable { mutableStateOf(false) }
    var query by rememberSaveable { mutableStateOf("") }
    var filter by rememberSaveable { mutableStateOf(ChatInboxFilter.All) }
    // Pin, archive and mute are mine on every device: they come from the core.
    val archived = state.spaces.filter { it.archived }.map { it.id }.toSet()
    val muted = state.spaces.filter { it.muted }.map { it.id }.toSet()
    fun prefs(chat: SpaceSummary, pinned: Boolean = chat.pinned, archived: Boolean = chat.archived, mutedUntil: Long? = chat.mutedUntilMs) =
        model.launch { model.repository.change { it.setChatPrefs(chat.id, pinned, archived, mutedUntil) } }
    var showArchived by rememberSaveable { mutableStateOf(false) }
    val focusManager = LocalFocusManager.current
    androidx.activity.compose.BackHandler(showArchived || searching || query.isNotEmpty()) {
        if (searching || query.isNotEmpty()) { focusManager.clearFocus(); searching = false; query = "" } else showArchived = false
    }
    val haptics = rememberZoenHaptics()
    val working by model.working.collectAsStateWithLifecycle()
    val archivedChats = state.spaces.filter { it.id in archived }
    val chats = state.spaces.filter { (it.id in archived) == showArchived && (showArchived || filter.includes(it)) && matchesSpace(it, query) }
        .sortedByDescending { it.lastAtMs }
        .sortedByDescending { it.pinned }
    val list: @Composable () -> Unit = {
        Scaffold(topBar = { HomeBar(stringResource(R.string.chats), state, navigate) }, containerColor = MaterialTheme.colorScheme.background, contentWindowInsets = WindowInsets(0, 0, 0, 0)) { padding ->
            // Bottom: clear the floating tab bar so the last chat is never under it.
            Column(Modifier.fillMaxSize().padding(padding)) {
                ChatInboxSearchBar(query, { query = it }, searching, { searching = it }, filter) {
                    filter = it; haptics.perform(ZoenFeedback.Selection)
                }
            LazyColumn(Modifier.weight(1f).testTag("conversation-list"), contentPadding = PaddingValues(bottom = 112.dp)) {
                if (showArchived) item {
                    Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        NativeIconButton(onClick = { showArchived = false }) { MotionVectorIcon(Icons.AutoMirrored.Rounded.ArrowBack, stringResource(R.string.back)) }
                        Text(stringResource(R.string.archived), style = MaterialTheme.typography.titleLarge)
                    }
                }
                val apps = state.items.filter { item ->
                    item.app?.let { !JSONObject(it.viewJson).optBoolean("released") } == true && WidgetSnapshot.from(item) != null
                }.asReversed()
                if (apps.isNotEmpty() && !showArchived && !searching && query.isEmpty()) {
                    item {
                        MiniAppTileStrip(model, state, apps, onOpenItem = { navigate(Item(it)) })
                    }
                }
                // WhatsApp: "Archived" above the chats while there are any.
                if (!showArchived && archivedChats.isNotEmpty() && query.isBlank()) item(key = "archived") {
                    Row(Modifier.fillMaxWidth().testTag("archived-chats").clickable { haptics.perform(ZoenFeedback.Tap); showArchived = true }.padding(horizontal = 20.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                        Box(Modifier.width(56.dp), contentAlignment = Alignment.Center) { MotionVectorIcon(Icons.Rounded.Archive, null, tint = MaterialTheme.colorScheme.onSurfaceVariant) }
                        Text(stringResource(R.string.archived), Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
                        val unreadArchived = archivedChats.count { it.unread > 0u }
                        Text(if (unreadArchived > 0) "$unreadArchived" else "${archivedChats.size}", style = MaterialTheme.typography.labelMedium,
                            color = if (unreadArchived > 0) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                items(chats, key = { it.id }) { chat ->
                    ConversationRow(chat, chat.pinned && !showArchived, working = working[chat.id], selected = split && selectedChat == chat.id,
                        muted = chat.muted, archived = chat.archived,
                        onMute = { prefs(chat, mutedUntil = if (chat.muted) null else Long.MAX_VALUE) },
                        onClick = { if (split) selectedChat = chat.id else navigate(Chat(chat.id)) },
                        onPin = { prefs(chat, pinned = !chat.pinned) },
                        onRead = { model.launch { model.repository.change { it.markRead(chat.id) } } },
                        onArchive = { prefs(chat, archived = !chat.archived) },
                        modifier = Modifier.animateItem(),
                    )
                }
                if (chats.isEmpty()) item {
                    when {
                        query.isNotBlank() -> EmptyState("Nenhuma conversa encontrada", "Tente outro nome ou mensagem.")
                        showArchived -> EmptyState("Nenhuma conversa arquivada", "")
                        filter != ChatInboxFilter.All -> EmptyState(stringResource(R.string.chat_filter_empty), "")
                        else -> EmptyState(stringResource(R.string.chat_empty_title), stringResource(R.string.chat_empty_detail)) {
                            Button(onClick = { navigate(NewChat) }) { Text(stringResource(R.string.new_chat)) }
                        }
                    }
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
            else Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { EmptyState(stringResource(R.string.chat_select_title), stringResource(R.string.chat_select_detail)) }
        }
    } else list()
}

@Composable
internal fun ChatInboxSearchBar(
    query: String, onQuery: (String) -> Unit, focused: Boolean, onFocus: (Boolean) -> Unit,
    filter: ChatInboxFilter, selectFilter: (ChatInboxFilter) -> Unit,
) {
    Surface(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 12.dp),
        shape = CircleShape, color = MaterialTheme.colorScheme.surfaceContainer) {
        Row(Modifier.heightIn(min = 52.dp).padding(start = 16.dp, end = 12.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            MotionVectorIcon(Icons.Rounded.Search, null, Modifier.size(17.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
            BasicTextField(query, onQuery, Modifier.weight(1f).heightIn(min = 48.dp).onFocusChanged { onFocus(it.isFocused) }.testTag("chat-search"),
                singleLine = true, textStyle = MaterialTheme.typography.bodyLarge.copy(fontSize = 16.sp, color = MaterialTheme.colorScheme.onSurface),
                cursorBrush = androidx.compose.ui.graphics.SolidColor(MaterialTheme.colorScheme.primary),
                decorationBox = { input ->
                    Box(contentAlignment = Alignment.CenterStart) { if (query.isEmpty()) Text(stringResource(R.string.chat_search_hint), fontSize = 16.sp, color = MaterialTheme.colorScheme.onSurfaceVariant); input() }
                })
            if (query.isNotEmpty()) NativeIconButton(onClick = { onQuery("") }, modifier = Modifier.size(32.dp).testTag("chat-search-clear")) {
                MotionVectorIcon(Icons.Rounded.Cancel, stringResource(R.string.chat_search_clear), Modifier.size(17.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            VerticalDivider(Modifier.height(22.dp), thickness = 0.5.dp)
            ChatFilterMenu(filter, compact = focused || query.isNotEmpty(), select = selectFilter)
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun ConversationRow(chat: SpaceSummary, pinned: Boolean, working: String?, selected: Boolean, onClick: () -> Unit, onPin: () -> Unit, onRead: () -> Unit,
    muted: Boolean = false, archived: Boolean = false, onMute: () -> Unit = {}, onArchive: () -> Unit = {}, modifier: Modifier = Modifier) {
    var menu by remember { mutableStateOf(false) }
    val haptics = rememberZoenHaptics()
    Box(modifier) {
        Row(Modifier.fillMaxWidth().testTag("chat:${chat.counterpart?.handle ?: chat.id}").background(if (selected) MaterialTheme.colorScheme.primaryContainer else Color.Transparent)
            .combinedClickable(hapticFeedbackEnabled = false, onClick = { haptics.perform(ZoenFeedback.Tap); onClick() }, onLongClick = { haptics.perform(ZoenFeedback.Open); menu = true }).padding(horizontal = 20.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            if (chat.counterpart != null) Avatar(chat.counterpart, size = 56, contact = true, working = working != null)
            else SpaceAvatar(chat, size = 56)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(chat.title, style = MaterialTheme.typography.titleMedium, fontWeight = if (chat.unread > 0u) FontWeight.Bold else FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    if (working == null) chat.lastDelivery?.let { ReceiptTicks(it) }
                    Text(if (working != null) stringResource(R.string.person_typing, working) else conversationPreview(chat), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(chat.lastAtMs)), style = MaterialTheme.typography.labelSmall,
                    color = if (chat.unread > 0u && !muted) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    if (muted) MotionVectorIcon(Icons.Rounded.NotificationsOff, stringResource(R.string.muted), Modifier.size(16.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                    if (pinned) MotionVectorIcon(Icons.Rounded.PushPin, stringResource(R.string.pinned), Modifier.size(16.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                    // Muted unread goes grey (WhatsApp); unmuted is the accent.
                    if (chat.unread > 0u) Badge(containerColor = if (muted) MaterialTheme.colorScheme.outline else MaterialTheme.colorScheme.primary, contentColor = MaterialTheme.colorScheme.onPrimary) { Text(chat.unread.toString()) }
                }
            }
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(text = { Text(stringResource(if (pinned) R.string.unpin else R.string.pin)) }, leadingIcon = { MotionVectorIcon(Icons.Rounded.PushPin, null) }, onClick = { menu = false; onPin() })
            DropdownMenuItem(text = { Text(stringResource(R.string.chat_mark_read)) }, leadingIcon = { MotionVectorIcon(Icons.Rounded.DoneAll, null) }, onClick = { menu = false; onRead() })
            DropdownMenuItem(text = { Text(stringResource(if (muted) R.string.unmute else R.string.mute)) }, leadingIcon = { Icon(if (muted) Icons.Rounded.Notifications else Icons.Rounded.NotificationsOff, null) }, onClick = { menu = false; onMute() })
            DropdownMenuItem(text = { Text(stringResource(if (archived) R.string.unarchive else R.string.archive)) }, leadingIcon = { Icon(if (archived) Icons.Rounded.Unarchive else Icons.Rounded.Archive, null) }, onClick = { menu = false; onArchive() })
        }
    }
}

@Composable
private fun conversationPreview(space: SpaceSummary): String {
    val preview = if (xyz.tironi.zoen.media.VoiceNoteRef.parse(space.lastPreview) != null) stringResource(R.string.media_voice_message) else space.lastPreview
    // Mine: the real ticks sit before the preview (WhatsApp) instead of "You:".
    if (space.lastAuthor?.isMe == true) return preview
    val author = space.lastAuthor?.let { if (space.counterpart == null) it.name else "" }.orEmpty()
    return if (author.isBlank()) preview else "$author: $preview"
}

private fun matchesSpace(space: SpaceSummary, query: String): Boolean {
    fun folded(value: String) = java.text.Normalizer.normalize(value, java.text.Normalizer.Form.NFD).replace(Regex("\\p{M}+"), "").lowercase(java.util.Locale.ROOT)
    return folded((listOf(space.title, space.lastPreview) + space.members.map { it.name }).joinToString(" ")).contains(folded(query.trim()))
}

@Composable
fun ActivityScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit, back: () -> Unit, root: Boolean = false) {
    var showingList by rememberSaveable(state.me?.id) { mutableStateOf(root) }
    if (showingList) ActivityList(model, state, navigate) { showingList = false }
    else Scaffold(topBar = { HomeBar("Notifications", state, navigate, onBack = back) }, contentWindowInsets = WindowInsets(0, 0, 0, 0)) { padding ->
        ApprovalsDeck(model, state, navigate, { model.approvals.flush(); showingList = true }, Modifier.padding(padding))
    }
}

@Composable
private fun ActivityList(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit, onShowCards: () -> Unit) {
    var section by rememberSaveable { mutableIntStateOf(0) }
    val loadedMentions by produceState<List<Mention>>(emptyList(), state.revision) { value = model.repository.query { it.mentions() } }
    val mentions = loadedMentions
    val tasks = state.items.filter { it.kindId == "task" || it.plan?.sections?.any { section -> section.lines.any { line -> !line.done } } == true }
    val pending = state.requests.filter { it.agent.isMine && (it.status == RequestStatus.PENDING || it.status == RequestStatus.STALE) }
    val resolved = state.requests.filter { it.status == RequestStatus.APPROVED || it.status == RequestStatus.DENIED }
    var review by remember { mutableStateOf<List<AgentRequestDto>?>(null) }
    var busy by remember { mutableStateOf(false) }
    Scaffold(topBar = { HomeBar("Notifications", state, navigate) }, contentWindowInsets = WindowInsets(0, 0, 0, 0)) { padding ->
        LazyColumn(Modifier.padding(padding).testTag("activity-list"), contentPadding = PaddingValues(24.dp, 8.dp, 24.dp, 100.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                TextButton(onClick = onShowCards, modifier = Modifier.testTag("activity-show-cards")) { Text(stringResource(R.string.approval_cards)) }
            } }
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
                    items(tasks, key = { it.id }) { task -> Card(onClick = { navigate(Item(task.id)) }) { ListItem(headlineContent = { Text(task.title) }, supportingContent = { Text(task.spaceTitle) }, leadingContent = { MotionVectorIcon(Icons.Rounded.Checklist, null) }) } }
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

/** Chats muted on this device; ActivityNotifications skips them, wakes included. */
