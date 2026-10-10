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
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.text.selection.SelectionContainer
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
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.IntOffset
import androidx.navigation3.runtime.NavKey
import java.text.DateFormat
import java.util.Date
import kotlin.math.roundToInt
import kotlinx.coroutines.delay
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import xyz.tironi.zoen.BuildConfig
import xyz.tironi.zoen.agent.AgentBrowserCard
import xyz.tironi.zoen.miniapps.MiniAppPins
import xyz.tironi.zoen.miniapps.MiniAppTileStrip
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.data.FileAccess
import xyz.tironi.zoen.media.VoiceBubble
import xyz.tironi.zoen.media.VoiceComposer
import xyz.tironi.zoen.media.VoiceNoteRef
import xyz.tironi.zoen.media.MediaFiles

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatScreen(model: ZoenViewModel, state: AppState, spaceId: String, navigate: (NavKey) -> Unit, back: () -> Unit, focusMessage: String? = null) {
    val space = state.spaces.firstOrNull { it.id == spaceId }
    val allEntries = state.timelines[spaceId].orEmpty()
    val appIds = state.items.filter { it.app != null }.map { it.id }.toSet()
    val timeline = allEntries.filter { it.inThread == null && (it.kind as? EntryKind.ItemEdited)?.itemId !in appIds }
    val list = rememberLazyListState()
    var firstLoad by rememberSaveable(spaceId) { mutableStateOf(true) }
    var previousCount by rememberSaveable(spaceId) { mutableIntStateOf(0) }
    var replyId by rememberSaveable(spaceId) { mutableStateOf<String?>(null) }
    val draftKey = model.repository.localKey("draft", spaceId, state.me?.id.orEmpty())
    var draft by remember(draftKey) { mutableStateOf(model.repository.preferences.getString(draftKey, "").orEmpty()) }
    var sending by remember { mutableStateOf(false) }
    var attachments by remember { mutableStateOf(false) }
    var overflow by remember { mutableStateOf(false) }
    var highlight by remember(spaceId) { mutableStateOf<String?>(null) }
    var jumpTo by remember(spaceId) { mutableStateOf<String?>(focusMessage) }
    val context = LocalContext.current
    val owner = LocalLifecycleOwner.current
    val observationOwner = remember(owner, spaceId) { Any() }
    val working by model.working.collectAsStateWithLifecycle()
    DisposableEffect(owner, spaceId, observationOwner) {
        val repository = model.repository
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_START) repository.activeSpace = spaceId
            if (event == Lifecycle.Event.ON_STOP && repository.activeSpace == spaceId) repository.activeSpace = null
        }
        owner.lifecycle.addObserver(observer)
        if (owner.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) repository.activeSpace = spaceId
        onDispose {
            owner.lifecycle.removeObserver(observer)
            if (repository.activeSpace == spaceId) repository.activeSpace = null
            model.launch { repository.unobserve(spaceId, observationOwner) }
        }
    }
    LaunchedEffect(spaceId, space?.eventCount, space?.unread) {
        if (model.repository.appVisible && model.repository.activeSpace == spaceId && (space?.unread ?: 0u) > 0u) model.repository.change { it.markRead(spaceId) }
    }
    val imported = stringResource(R.string.file_imported)
    val newPageTitle = stringResource(R.string.new_page)
    LaunchedEffect(spaceId, space != null, observationOwner) { model.repository.observe(spaceId, observationOwner) }
    var lastTypingAt by remember(spaceId) { mutableLongStateOf(0L) }
    LaunchedEffect(draft, spaceId) {
        val now = SystemClock.elapsedRealtime()
        if (draft.isEmpty() || now - lastTypingAt > 1_500) {
            model.repository.query { it.setTyping(spaceId, draft.isNotEmpty()) }
            lastTypingAt = now
        }
        if (draft.isNotEmpty()) { delay(4_000); model.repository.query { it.setTyping(spaceId, false) } }
    }
    LaunchedEffect(focusMessage) { if (focusMessage != null) jumpTo = focusMessage }
    LaunchedEffect(jumpTo, allEntries.size) {
        val target = jumpTo ?: return@LaunchedEffect
        val original = allEntries.firstOrNull { it.id == target } ?: return@LaunchedEffect
        val index = timeline.indexOfFirst { it.id == (original.inThread ?: target) }
        if (index >= 0) {
            // Navigation can compose this effect inside its current measure pass.
            withFrameNanos { }
            list.animateScrollToItem(index)
            highlight = original.inThread ?: target
            jumpTo = null
        }
    }
    LaunchedEffect(highlight) { if (highlight != null) { delay(1800); highlight = null } }
    LaunchedEffect(spaceId, timeline.size) {
        if (timeline.isNotEmpty()) {
            val atBottom = list.layoutInfo.visibleItemsInfo.lastOrNull()?.index?.let { it >= previousCount - 2 } ?: true
            if (firstLoad || (timeline.size > previousCount && (atBottom || timeline.last().author.isMe))) {
                if (focusMessage == null && jumpTo == null && highlight == null) {
                    withFrameNanos { }
                    list.scrollToItem(timeline.lastIndex)
                }
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
    var videoUri by rememberSaveable { mutableStateOf<String?>(null) }
    val videoCamera = rememberLauncherForActivityResult(ActivityResultContracts.CaptureVideo()) { success ->
        if (success) videoUri?.let { import(Uri.parse(it)) }
    }
    val send: () -> Unit = {
        if (draft.isNotBlank() && !sending) {
            val text = draft
            val reply = replyId
            sending = true
            model.launch {
                try {
                    model.send(spaceId, text, reply)
                    if (draft == text) { draft = ""; model.repository.preferences.edit().remove(draftKey).apply() }
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
                    if (space.counterpart != null) Avatar(space.counterpart, size = 40, onClick = { navigate(if (space.counterpart!!.kind == PersonaKind.AGENT) Agent(space.counterpart!!.id) else Person(space.counterpart!!.id)) })
                    else SpaceAvatar(space, size = 40, onClick = { navigate(Participants(space.id)) })
                    Column {
                        Text(space.title, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        val remote = state.remoteActivity[spaceId]
                        val name = working[spaceId] ?: state.typing[spaceId]?.let { id -> space.members.firstOrNull { it.id == id }?.name } ?: remote?.let { activity -> space.members.firstOrNull { it.id == activity.identity }?.name }
                        val status = when {
                            working[spaceId] != null -> stringResource(R.string.chat_processing, name.orEmpty())
                            state.typing[spaceId] != null -> stringResource(R.string.person_typing, name.orEmpty())
                            remote != null -> stringResource(when (remote.kind) { "building" -> R.string.chat_building; "call" -> R.string.chat_call; else -> R.string.chat_processing }, name.orEmpty())
                            space.counterpart?.id in state.online -> stringResource(R.string.chat_online)
                            space.counterpart == null -> stringResource(R.string.chat_people_count, space.members.count { it.kind == PersonaKind.PERSON })
                            else -> ""
                        }
                        if (status.isNotBlank()) Text(status, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
                    }
                }
            }, navigationIcon = { IconButton(onClick = back) { Icon(Icons.AutoMirrored.Rounded.ArrowBack, stringResource(R.string.back)) } }, actions = {
                IconButton(onClick = { overflow = true }) { Icon(Icons.Rounded.MoreVert, stringResource(R.string.more)) }
                DropdownMenu(overflow, onDismissRequest = { overflow = false }) {
                    DropdownMenuItem(text = { Text(stringResource(R.string.participants)) }, onClick = { overflow = false; navigate(Participants(space.id)) }, leadingIcon = { Icon(Icons.Rounded.Groups, null) })
                    DropdownMenuItem(text = { Text(stringResource(R.string.files)) }, onClick = { overflow = false; navigate(Folder(space.id)) }, leadingIcon = { Icon(Icons.Rounded.Folder, null) })
                    DropdownMenuItem(text = { Text(stringResource(R.string.chat_appearance)) }, onClick = { overflow = false; navigate(Appearance(space.id)) }, leadingIcon = { Icon(Icons.Rounded.Palette, null) })
                    DropdownMenuItem(text = { Text(stringResource(R.string.signed_log)) }, onClick = { overflow = false; navigate(History) }, leadingIcon = { Icon(Icons.Rounded.VerifiedUser, null) })
                    if (BuildConfig.DEBUG && state.demo) DropdownMenuItem(text = { Text(stringResource(R.string.agent_browser_start)) }, onClick = {
                        overflow = false
                        space.members.firstOrNull { it.kind == PersonaKind.AGENT && it.isMine }?.let { model.browser.startDemo(space.id, it, model.repository.locale); navigate(Browser) }
                    }, leadingIcon = { Icon(Icons.Rounded.Language, null) })
                }
            })
        },
        bottomBar = {
            Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
                Column(Modifier.navigationBarsPadding().imePadding().padding(horizontal = 12.dp, vertical = 8.dp)) {
                    working[spaceId]?.let { text -> Row(Modifier.padding(start = 16.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp); Text(text, style = MaterialTheme.typography.labelMedium)
                    } }
                    state.typing[spaceId]?.let { identity ->
                        val name = space.members.firstOrNull { it.id == identity }?.name.orEmpty()
                        Text(stringResource(R.string.person_typing, name), Modifier.padding(start = 16.dp, bottom = 8.dp), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                    }
                    replyId?.let { id ->
                        val original = timeline.firstOrNull { it.id == id }
                        Row(Modifier.fillMaxWidth().padding(8.dp).testTag("reply-target:$id"), verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.AutoMirrored.Rounded.Reply, null, tint = MaterialTheme.colorScheme.primary)
                            Text(original?.author?.name.orEmpty() + " · " + (original?.kind as? EntryKind.Message)?.text.orEmpty(), Modifier.weight(1f).padding(horizontal = 12.dp), maxLines = 1, overflow = TextOverflow.Ellipsis)
                            IconButton(onClick = { replyId = null }) { Icon(Icons.Rounded.Close, stringResource(R.string.cancel)) }
                        }
                    }
                    Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        IconButton(onClick = { attachments = true }, modifier = Modifier.padding(bottom = 4.dp)) { Icon(Icons.Rounded.Add, stringResource(R.string.attach)) }
                        OutlinedTextField(value = draft, onValueChange = {
                            draft = it
                            model.repository.preferences.edit().putString(draftKey, it).apply()
                        }, modifier = Modifier.weight(1f).testTag("composer"), shape = RoundedCornerShape(26.dp),
                            placeholder = { Text(stringResource(if (space.counterpart?.handle == "zoen") R.string.message_zoen else R.string.message), maxLines = 1) },
                            maxLines = 5, keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Send),
                            keyboardActions = KeyboardActions(onSend = { send() }),
                        )
                        if (draft.isBlank()) VoiceComposer(model, spaceId, Modifier.padding(bottom = 4.dp), reply = replyId, onSent = { replyId = null })
                        else FilledIconButton(onClick = send, enabled = draft.isNotBlank() && !sending && !state.keyMissing, modifier = Modifier.padding(bottom = 4.dp).size(48.dp).testTag("send")) {
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
            state.items.firstOrNull { it.spaceId == spaceId && it.plan != null }?.let { pinned ->
                val lines = pinned.plan!!.sections.flatMap { it.lines }
                Card(onClick = { navigate(Item(pinned.id)) }, modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp).testTag("chat-pinned-plan")) {
                    Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Icon(Icons.Rounded.PushPin, stringResource(R.string.chat_pinned_plan, pinned.title), tint = MaterialTheme.colorScheme.primary)
                        Column(Modifier.weight(1f)) { Text(pinned.title, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.titleSmall); Text(stringResource(R.string.chat_plan_count, lines.size, formatMoney(lines.sumOf { it.costCents }, model.repository.locale)), style = MaterialTheme.typography.labelSmall) }
                        Icon(Icons.Rounded.ChevronRight, null)
                    }
                }
            }
            if (apps.isNotEmpty()) MiniAppTileStrip(model, state, apps, scope = MiniAppPins.chat(spaceId), compact = list.firstVisibleItemIndex > 2, onOpenItem = { navigate(Item(it)) })
            AgentBrowserCard(model.browser, spaceId) { navigate(Browser) }
            Box(Modifier.weight(1f)) {
                ChatBackdrop(model, space.id, state.revision, Modifier.fillMaxSize())
                LazyColumn(state = list, modifier = Modifier.fillMaxSize().testTag("chat-timeline"), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    items(timeline, key = { it.id }) { entry ->
                        Box(Modifier.testTag("timeline:${entry.id}").background(if (highlight == entry.id) MaterialTheme.colorScheme.secondaryContainer else androidx.compose.ui.graphics.Color.Transparent, RoundedCornerShape(16.dp))) {
                            TimelineRow(model, entry, navigate, onReply = { replyId = entry.id }, onThread = { navigate(Thread(spaceId, entry.id)) }, onQuote = { jumpTo = it })
                        }
                    }
                }
                val lastVisible = list.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0
                if (timeline.size > 5 && lastVisible < timeline.lastIndex - 1) SmallFloatingActionButton(onClick = { model.launch { list.animateScrollToItem(timeline.lastIndex) } }, Modifier.align(Alignment.BottomEnd).padding(16.dp)) {
                    Icon(Icons.Rounded.ArrowDownward, stringResource(R.string.jump_latest))
                }
            }
        }
    }
    if (attachments) ModalBottomSheet(onDismissRequest = { attachments = false }) {
        SettingsRow(Icons.Rounded.PhotoLibrary, stringResource(R.string.media_gallery), onClick = { attachments = false; photoPicker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageAndVideo)) })
        SettingsRow(Icons.Rounded.CameraAlt, stringResource(R.string.camera), onClick = { attachments = false; val uri = FileAccess.cameraUri(context); cameraUri = uri.toString(); camera.launch(uri) })
        SettingsRow(Icons.Rounded.Videocam, stringResource(R.string.media_record_video), onClick = { attachments = false; val uri = MediaFiles.videoUri(context); videoUri = uri.toString(); videoCamera.launch(uri) })
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
fun TimelineRow(model: ZoenViewModel, entry: TimelineEntry, navigate: (NavKey) -> Unit, onReply: () -> Unit, onThread: (() -> Unit)?, onQuote: ((String) -> Unit)? = null) {
    val context = LocalContext.current
    var menu by remember { mutableStateOf(false) }
    val copied = stringResource(R.string.copied)
    val locale = LocalConfiguration.current.locales[0].toLanguageTag()
    when (val kind = entry.kind) {
        is EntryKind.Message -> {
            val mine = entry.author.isMe
            var swipe by remember(entry.id) { mutableFloatStateOf(0f) }
            val threshold = with(LocalDensity.current) { 72.dp.toPx() }
            val haptics = LocalHapticFeedback.current
            val reply by rememberUpdatedState(onReply)
            val thread by rememberUpdatedState(onThread)
            Box(Modifier.fillMaxWidth()) {
                if (swipe < -threshold / 3) Icon(Icons.AutoMirrored.Rounded.Reply, stringResource(R.string.reply), Modifier.align(Alignment.CenterEnd).padding(12.dp), tint = MaterialTheme.colorScheme.primary)
                if (swipe > threshold / 3 && onThread != null) Icon(Icons.Rounded.Forum, stringResource(R.string.thread), Modifier.align(Alignment.CenterStart).padding(12.dp), tint = MaterialTheme.colorScheme.primary)
            Column(Modifier.fillMaxWidth().offset { IntOffset(swipe.roundToInt(), 0) }.pointerInput(entry.id, onThread != null, threshold) {
                detectHorizontalDragGestures(onDragEnd = {
                    if (swipe <= -threshold) { haptics.performHapticFeedback(HapticFeedbackType.LongPress); reply() }
                    else if (swipe >= threshold && thread != null) { haptics.performHapticFeedback(HapticFeedbackType.LongPress); thread?.invoke() }
                    swipe = 0f
                }, onDragCancel = { swipe = 0f }) { change, distance ->
                    change.consume()
                    swipe = (swipe + distance).coerceIn(-threshold * 1.5f, if (thread == null) 0f else threshold * 1.5f)
                }
            }, horizontalAlignment = if (mine) Alignment.End else Alignment.Start) {
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
                                Surface(color = MaterialTheme.colorScheme.surface.copy(alpha = .6f), shape = RoundedCornerShape(8.dp), modifier = Modifier.testTag("quote:${quote.id}").clickable { onQuote?.invoke(quote.id) }) {
                                    Column(Modifier.padding(10.dp)) { Text(quote.author.name, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary); Text(quote.text, maxLines = 2, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodySmall) }
                                }
                            }
                            val voice = VoiceNoteRef.parse(kind.text)
                            if (voice != null) VoiceBubble(model, voice)
                            else if (kind.text.isNotBlank()) {
                                val mentionColor = MaterialTheme.colorScheme.primary
                                SelectionContainer { Text(buildAnnotatedString { append(kind.text); Regex("(?<![\\w.])@[\\p{L}\\p{N}_.]+").findAll(kind.text).forEach { match -> addStyle(SpanStyle(color = mentionColor, fontWeight = FontWeight.SemiBold), match.range.first, match.range.last + 1) } }, style = MaterialTheme.typography.bodyLarge) }
                            }
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
        }
        is EntryKind.ItemEdited -> TextButton(onClick = { navigate(Item(kind.itemId)) }, Modifier.fillMaxWidth()) { Icon(Icons.Rounded.History, null, Modifier.size(16.dp)); Spacer(Modifier.width(8.dp)); Text(kind.note, style = MaterialTheme.typography.bodySmall) }
        is EntryKind.Request -> {
            val state by model.state.collectAsStateWithLifecycle()
            val request = state.requests.firstOrNull { it.id == kind.requestId }
            var approving by remember { mutableStateOf(false) }
            OutlinedCard(onClick = { navigate(Request(kind.requestId)) }) {
                Row(Modifier.fillMaxWidth().padding(12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Avatar(request?.agent ?: entry.author, size = 30)
                    Column(Modifier.weight(1f)) {
                        Text(kind.title, style = MaterialTheme.typography.titleSmall, maxLines = 2)
                        request?.let { Text(listOfNotNull(it.agent.name, it.costCents?.let { cost -> formatMoney(cost, locale) }).joinToString(" · "), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    }
                    when (request?.status) {
                        RequestStatus.PENDING -> Button(enabled = request?.agent?.isMine == true && !approving, onClick = {
                            approving = true
                            model.launch { try { val result = model.repository.change { it.approveRequest(kind.requestId) }; model.notify(result.message) } finally { approving = false } }
                        }) { Text(stringResource(R.string.approve)) }
                        RequestStatus.APPROVED -> Icon(Icons.Rounded.CheckCircle, stringResource(R.string.status_approved), tint = MaterialTheme.colorScheme.primary)
                        RequestStatus.DENIED -> Icon(Icons.Rounded.Cancel, stringResource(R.string.status_denied), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                        RequestStatus.STALE -> Icon(Icons.Rounded.Update, stringResource(R.string.status_stale), tint = MaterialTheme.colorScheme.tertiary)
                        null -> Icon(Icons.Rounded.ChevronRight, stringResource(R.string.review))
                    }
                }
            }
        }
        is EntryKind.System -> Text(kind.text, Modifier.fillMaxWidth().padding(horizontal = 20.dp), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        is EntryKind.Background -> Text(stringResource(R.string.saved), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
fun ThreadScreen(model: ZoenViewModel, state: AppState, space: String, root: String, navigate: (NavKey) -> Unit, back: () -> Unit) {
    val loadedEntries by produceState<List<TimelineEntry>>(emptyList(), state.revision, root) { value = model.repository.query { it.thread(space, root) } }
    val entries = loadedEntries
    val list = rememberLazyListState()
    var text by rememberSaveable { mutableStateOf("") }
    var replyTo by rememberSaveable(root) { mutableStateOf(root) }
    var sending by remember { mutableStateOf(false) }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.replies), back) }, bottomBar = {
        Row(Modifier.navigationBarsPadding().imePadding().padding(16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(text, { text = it }, Modifier.weight(1f), placeholder = { Text(stringResource(R.string.reply)) }, maxLines = 4,
                label = entries.firstOrNull { it.id == replyTo }?.let { { Text(it.author.name) } })
            if (text.isBlank()) VoiceComposer(model, space, reply = replyTo, thread = true, threadRoot = root, onSent = { replyTo = root })
            else FilledIconButton(onClick = {
                val draft = text; val target = replyTo; sending = true
                model.launch { try { model.send(space, draft, target, true); if (text == draft) text = ""; replyTo = root } finally { sending = false } }
            }, enabled = text.isNotBlank() && !sending) { Icon(Icons.AutoMirrored.Rounded.Send, stringResource(R.string.send)) }
        }
    }) { padding -> LazyColumn(Modifier.padding(padding), state = list, contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        items(entries, key = { it.id }) { entry -> TimelineRow(model, entry, navigate, onReply = { replyTo = entry.id }, onThread = null, onQuote = { id ->
            val index = entries.indexOfFirst { it.id == id }
            if (index >= 0) model.launch { list.animateScrollToItem(index) } else navigate(Chat(space, id))
        }) }
    } }
}
