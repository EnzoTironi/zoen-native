package xyz.tironi.zoen.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.net.Uri
import android.os.SystemClock
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.automirrored.rounded.Reply
import androidx.compose.material.icons.automirrored.rounded.Send
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import java.text.DateFormat
import java.util.Date
import kotlinx.coroutines.delay
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.data.FileAccess

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatScreen(model: ZoenViewModel, state: AppState, spaceId: String, navigate: (NavKey) -> Unit, back: () -> Unit, focusMessage: String? = null) {
    val space = state.spaces.firstOrNull { it.id == spaceId }
    val timeline = state.timelines[spaceId].orEmpty()
    val list = rememberLazyListState()
    var firstLoad by rememberSaveable(spaceId) { mutableStateOf(true) }
    var previousCount by remember(spaceId) { mutableIntStateOf(0) }
    var replyId by rememberSaveable(spaceId) { mutableStateOf<String?>(null) }
    var draft by rememberSaveable(spaceId) { mutableStateOf(model.repository.preferences.getString("draft:$spaceId", "").orEmpty()) }
    var sending by remember { mutableStateOf(false) }
    var attachments by remember { mutableStateOf(false) }
    var overflow by remember { mutableStateOf(false) }
    val context = LocalContext.current
    val imported = stringResource(R.string.file_imported)
    val newPageTitle = stringResource(R.string.new_page)
    val encrypted by produceState(false, spaceId, space != null, state.revision) {
        value = space != null && model.repository.query { it.groupKeys(spaceId) != null }
    }
    LaunchedEffect(spaceId, space != null) { model.repository.observe(spaceId) }
    var lastTypingAt by remember(spaceId) { mutableLongStateOf(0L) }
    LaunchedEffect(draft, spaceId) {
        val now = SystemClock.elapsedRealtime()
        if (draft.isEmpty() || now - lastTypingAt > 1_500) {
            model.repository.query { it.setTyping(spaceId, draft.isNotEmpty()) }
            lastTypingAt = now
        }
        if (draft.isNotEmpty()) { delay(4_000); model.repository.query { it.setTyping(spaceId, false) } }
    }
    DisposableEffect(spaceId) { onDispose { model.launch { model.repository.query { it.setTyping(spaceId, false) } } } }
    LaunchedEffect(spaceId, timeline.size) {
        if (timeline.isNotEmpty()) {
            val atBottom = list.layoutInfo.visibleItemsInfo.lastOrNull()?.index?.let { it >= previousCount - 2 } ?: true
            if (firstLoad || (timeline.size > previousCount && (atBottom || timeline.last().author.isMe))) {
                list.scrollToItem(if (firstLoad && focusMessage != null) timeline.indexOfFirst { it.id == focusMessage }.takeIf { it >= 0 } ?: timeline.lastIndex else timeline.lastIndex)
                firstLoad = false
            }
            previousCount = timeline.size
        }
    }
    val import: (Uri) -> Unit = { uri ->
        model.launch {
            val item = FileAccess.import(context, model.repository, spaceId, uri)
            model.notify(imported)
            navigate(Item(item.id))
        }
    }
    val filePicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { it?.let(import) }
    val photoPicker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { it?.let(import) }
    var cameraUri by rememberSaveable { mutableStateOf<String?>(null) }
    val camera = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { success ->
        if (success) cameraUri?.let { import(Uri.parse(it)) }
    }
    val send: () -> Unit = {
        if (draft.isNotBlank() && !sending) {
            val text = draft
            val reply = replyId
            sending = true
            model.launch {
                try {
                    model.send(spaceId, text, reply)
                    if (draft == text) { draft = ""; model.repository.preferences.edit().remove("draft:$spaceId").apply() }
                    replyId = null
                } finally { sending = false }
            }
        }
    }
    if (space == null) {
        Scaffold(topBar = { ScreenBar(stringResource(R.string.chats), back) }) { padding ->
            EmptyState(stringResource(R.string.unavailable), stringResource(R.string.unavailable_detail), Modifier.padding(padding)) {
                OutlinedButton(onClick = { model.launch { model.repository.refresh() } }) { Text(stringResource(R.string.retry)) }
            }
        }
        return
    }
    Scaffold(
        topBar = {
            TopAppBar(title = {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Avatar(space.counterpart, size = 40, onClick = { if (space.counterpart != null) navigate(if (space.counterpart!!.kind == PersonaKind.AGENT) Agent(space.counterpart!!.id) else Person(space.counterpart!!.id)) else navigate(Participants(space.id)) })
                    Column { Text(space.title, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis); PrivacyLabel(space, encrypted) }
                }
            }, navigationIcon = { IconButton(onClick = back) { Icon(Icons.AutoMirrored.Rounded.ArrowBack, stringResource(R.string.back)) } }, actions = {
                IconButton(onClick = { overflow = true }) { Icon(Icons.Rounded.MoreVert, stringResource(R.string.more)) }
                DropdownMenu(overflow, onDismissRequest = { overflow = false }) {
                    DropdownMenuItem(text = { Text(stringResource(R.string.participants)) }, onClick = { overflow = false; navigate(Participants(space.id)) }, leadingIcon = { Icon(Icons.Rounded.Groups, null) })
                    DropdownMenuItem(text = { Text(stringResource(R.string.files)) }, onClick = { overflow = false; navigate(Files) }, leadingIcon = { Icon(Icons.Rounded.Folder, null) })
                    DropdownMenuItem(text = { Text(stringResource(R.string.signed_log)) }, onClick = { overflow = false; navigate(History) }, leadingIcon = { Icon(Icons.Rounded.VerifiedUser, null) })
                }
            })
        },
        bottomBar = {
            Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
                Column(Modifier.navigationBarsPadding().imePadding().padding(horizontal = 12.dp, vertical = 8.dp)) {
                    state.typing[spaceId]?.let { identity ->
                        val name = space.members.firstOrNull { it.id == identity }?.name.orEmpty()
                        Text(stringResource(R.string.person_typing, name), Modifier.padding(start = 16.dp, bottom = 8.dp), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                    }
                    replyId?.let { id ->
                        val original = timeline.firstOrNull { it.id == id }
                        Row(Modifier.fillMaxWidth().padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.AutoMirrored.Rounded.Reply, null, tint = MaterialTheme.colorScheme.primary)
                            Text(original?.author?.name.orEmpty() + " · " + (original?.kind as? EntryKind.Message)?.text.orEmpty(), Modifier.weight(1f).padding(horizontal = 12.dp), maxLines = 1, overflow = TextOverflow.Ellipsis)
                            IconButton(onClick = { replyId = null }) { Icon(Icons.Rounded.Close, stringResource(R.string.cancel)) }
                        }
                    }
                    Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        IconButton(onClick = { attachments = true }, modifier = Modifier.padding(bottom = 4.dp)) { Icon(Icons.Rounded.Add, stringResource(R.string.attach)) }
                        OutlinedTextField(value = draft, onValueChange = {
                            draft = it
                            model.repository.preferences.edit().putString("draft:$spaceId", it).apply()
                        }, modifier = Modifier.weight(1f).testTag("composer"), shape = RoundedCornerShape(26.dp),
                            placeholder = { Text(stringResource(if (space.counterpart?.handle == "zoen") R.string.message_zoen else R.string.message), maxLines = 1) },
                            maxLines = 5, keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Send),
                            keyboardActions = KeyboardActions(onSend = { send() }),
                        )
                        FilledIconButton(onClick = send, enabled = draft.isNotBlank() && !sending && !state.keyMissing, modifier = Modifier.padding(bottom = 4.dp).size(48.dp).testTag("send")) {
                            if (sending) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                            else Icon(Icons.AutoMirrored.Rounded.Send, stringResource(R.string.send))
                        }
                    }
                }
            }
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            if (!state.demo && state.connection.state != "online" && space.counterpart?.handle != "zoen") {
                Text(stringResource(if (state.connection.state == "connecting") R.string.connecting else R.string.offline), Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.secondaryContainer).padding(12.dp), style = MaterialTheme.typography.bodySmall)
            }
            val apps = state.items.filter { it.spaceId == spaceId && it.app != null }
            if (apps.isNotEmpty()) LazyRow(contentPadding = PaddingValues(horizontal = 16.dp, vertical = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                items(apps, key = { it.id }) { item -> AssistChip(onClick = { navigate(Item(item.id)) }, label = { Text(item.title) }, leadingIcon = { Icon(appIcon(item.app!!.appId), null, Modifier.size(18.dp)) }) }
            }
            Box(Modifier.weight(1f)) {
                LazyColumn(state = list, modifier = Modifier.fillMaxSize(), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    items(timeline, key = { it.id }) { entry -> TimelineRow(entry, navigate, onReply = { replyId = entry.id }, onThread = { navigate(Thread(spaceId, entry.id)) }) }
                }
                val lastVisible = list.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0
                if (timeline.size > 5 && lastVisible < timeline.lastIndex - 1) SmallFloatingActionButton(onClick = { model.launch { list.animateScrollToItem(timeline.lastIndex) } }, Modifier.align(Alignment.BottomEnd).padding(16.dp)) {
                    Icon(Icons.Rounded.ArrowDownward, stringResource(R.string.jump_latest))
                }
            }
        }
    }
    if (attachments) ModalBottomSheet(onDismissRequest = { attachments = false }) {
        SettingsRow(Icons.Rounded.PhotoLibrary, stringResource(R.string.photo), onClick = { attachments = false; photoPicker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) })
        SettingsRow(Icons.Rounded.CameraAlt, stringResource(R.string.camera), onClick = { attachments = false; val uri = FileAccess.cameraUri(context); cameraUri = uri.toString(); camera.launch(uri) })
        SettingsRow(Icons.Rounded.AttachFile, stringResource(R.string.import_file), onClick = { attachments = false; filePicker.launch(arrayOf("*/*")) })
        SettingsRow(Icons.Rounded.Article, stringResource(R.string.new_page), onClick = {
            attachments = false
            model.launch { val item = model.repository.change { it.pageCreate(spaceId, newPageTitle) }; navigate(Item(item.id)) }
        })
        Spacer(Modifier.height(24.dp))
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun TimelineRow(entry: TimelineEntry, navigate: (NavKey) -> Unit, onReply: () -> Unit, onThread: (() -> Unit)?) {
    val context = LocalContext.current
    var menu by remember { mutableStateOf(false) }
    val copied = stringResource(R.string.copied)
    val locale = LocalConfiguration.current.locales[0].toLanguageTag()
    when (val kind = entry.kind) {
        is EntryKind.Message -> {
            val mine = entry.author.isMe
            Column(Modifier.fillMaxWidth(), horizontalAlignment = if (mine) Alignment.End else Alignment.Start) {
                if (!mine) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Avatar(entry.author, size = 26, onClick = { navigate(if (entry.author.kind == PersonaKind.AGENT) Agent(entry.author.id) else Person(entry.author.id)) })
                    Text(entry.author.name, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                Box {
                    Surface(shape = RoundedCornerShape(22.dp, 22.dp, if (mine) 6.dp else 22.dp, if (mine) 22.dp else 6.dp),
                        color = if (mine) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainer,
                        modifier = Modifier.padding(top = 6.dp).widthIn(max = 360.dp).combinedClickable(onClick = { kind.card?.let { navigate(Item(it.itemId)) } }, onLongClick = { menu = true })) {
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            entry.replyTo?.let { quote ->
                                Surface(color = MaterialTheme.colorScheme.surface.copy(alpha = .6f), shape = RoundedCornerShape(8.dp)) {
                                    Column(Modifier.padding(10.dp)) { Text(quote.author.name, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary); Text(quote.text, maxLines = 2, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodySmall) }
                                }
                            }
                            if (kind.text.isNotBlank()) Text(kind.text, style = MaterialTheme.typography.bodyLarge)
                            kind.card?.let { card ->
                                Card(onClick = { navigate(Item(card.itemId)) }, modifier = Modifier.testTag("item:${card.itemId}"), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
                                    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                        Icon(appIcon(card.app?.appId ?: card.kindId), null, tint = MaterialTheme.colorScheme.primary)
                                        Text(card.title, style = MaterialTheme.typography.titleMedium)
                                        Text(card.app?.headline ?: card.summary, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                        card.totalCents?.let { Text(formatMoney(it, locale), style = MaterialTheme.typography.titleSmall) }
                                        Text(stringResource(R.string.version, card.version.toInt()), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
                                    }
                                }
                            }
                        }
                    }
                    DropdownMenu(menu, onDismissRequest = { menu = false }) {
                        DropdownMenuItem(text = { Text(stringResource(R.string.reply)) }, onClick = { menu = false; onReply() }, leadingIcon = { Icon(Icons.AutoMirrored.Rounded.Reply, null) })
                        if (onThread != null) DropdownMenuItem(text = { Text(stringResource(R.string.thread)) }, onClick = { menu = false; onThread() }, leadingIcon = { Icon(Icons.Rounded.Forum, null) })
                        DropdownMenuItem(text = { Text(stringResource(R.string.copy)) }, onClick = { menu = false; context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText(copied, kind.text)) }, leadingIcon = { Icon(Icons.Rounded.ContentCopy, null) })
                    }
                }
                Row(Modifier.padding(top = 5.dp, start = 8.dp, end = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                    Text(DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(entry.atMs)), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    if (mine) Icon(when (entry.delivery) { Delivery.FAILED -> Icons.Rounded.ErrorOutline; Delivery.SENDING -> Icons.Rounded.Schedule; Delivery.SENT -> Icons.Rounded.DoneAll; Delivery.LOCAL -> Icons.Rounded.Done }, null, Modifier.size(14.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                if (entry.threadReplies > 0u && onThread != null) TextButton(onClick = onThread) { Text("${entry.threadReplies} " + stringResource(R.string.replies)) }
            }
        }
        is EntryKind.ItemEdited -> TextButton(onClick = { navigate(Item(kind.itemId)) }, Modifier.fillMaxWidth()) { Icon(Icons.Rounded.History, null, Modifier.size(16.dp)); Spacer(Modifier.width(8.dp)); Text(kind.note, style = MaterialTheme.typography.bodySmall) }
        is EntryKind.Request -> OutlinedCard(onClick = { navigate(Request(kind.requestId)) }) { Row(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) { Icon(Icons.Rounded.VerifiedUser, null); Text(kind.title) } }
        is EntryKind.System -> Text(kind.text, Modifier.fillMaxWidth().padding(horizontal = 20.dp), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        is EntryKind.Background -> Text(stringResource(R.string.saved), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
fun ThreadScreen(model: ZoenViewModel, state: AppState, space: String, root: String, navigate: (NavKey) -> Unit, back: () -> Unit) {
    val entries by produceState<List<TimelineEntry>>(emptyList(), state.revision, root) { value = model.repository.query { it.thread(space, root) } }
    var text by rememberSaveable { mutableStateOf("") }
    var replyTo by rememberSaveable(root) { mutableStateOf(root) }
    var sending by remember { mutableStateOf(false) }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.replies), back) }, bottomBar = {
        Row(Modifier.navigationBarsPadding().imePadding().padding(16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(text, { text = it }, Modifier.weight(1f), placeholder = { Text(stringResource(R.string.reply)) }, maxLines = 4,
                label = entries.firstOrNull { it.id == replyTo }?.let { { Text(it.author.name) } })
            FilledIconButton(onClick = {
                val draft = text; val target = replyTo; sending = true
                model.launch { try { model.send(space, draft, target, true); if (text == draft) text = ""; replyTo = root } finally { sending = false } }
            }, enabled = text.isNotBlank() && !sending) { Icon(Icons.AutoMirrored.Rounded.Send, stringResource(R.string.send)) }
        }
    }) { padding -> LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        items(entries, key = { it.id }) { entry -> TimelineRow(entry, navigate, onReply = { replyTo = entry.id }, onThread = null) }
    } }
}
