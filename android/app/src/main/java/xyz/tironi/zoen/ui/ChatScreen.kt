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
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.automirrored.rounded.Reply
import androidx.compose.material.icons.automirrored.rounded.Send
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowRight
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.layout.HorizontalAlignmentLine
import androidx.compose.ui.layout.layout
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
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
import kotlinx.coroutines.launch
import kotlinx.coroutines.Job
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.spring
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.text
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import xyz.tironi.zoen.ui.ink.ZoenGlyph
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
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.material.icons.rounded.NotificationsNone
import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.background
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.sp
import androidx.compose.material.icons.automirrored.rounded.Forward
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.data.FileAccess
import xyz.tironi.zoen.media.VoiceBubble
import xyz.tironi.zoen.media.VoiceComposer
import xyz.tironi.zoen.theme.ownMessage
import xyz.tironi.zoen.theme.onOwnMessage
import xyz.tironi.zoen.theme.otherMessage
import xyz.tironi.zoen.media.VoiceNoteRef
import xyz.tironi.zoen.media.MediaFiles

private val MessageFace = HorizontalAlignmentLine { first, second -> maxOf(first, second) }

private fun Modifier.messageFace() = layout { measurable, constraints ->
    val placeable = measurable.measure(constraints)
    layout(placeable.width, placeable.height, mapOf(MessageFace to placeable.height)) { placeable.placeRelative(0, 0) }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatScreen(model: ZoenViewModel, state: AppState, spaceId: String, navigate: (NavKey) -> Unit, back: () -> Unit, focusMessage: String? = null) {
    val space = state.spaces.firstOrNull { it.id == spaceId }
    val allEntries = state.timelines[spaceId].orEmpty()
    val appIds = state.items.filter { it.app != null }.map { it.id }.toSet()
    // Thread-only replies live in their thread; one also sent to the chat shows here too.
    val timeline = allEntries.filter { (it.inThread == null || it.alsoInChat) && (it.kind as? EntryKind.ItemEdited)?.itemId !in appIds }
    val list = rememberLazyListState()
    val scrollScope = rememberCoroutineScope()
    var firstLoad by rememberSaveable(spaceId) { mutableStateOf(true) }
    var knownIds by rememberSaveable(spaceId) { mutableStateOf(arrayListOf<String>()) }
    var openingUnread by rememberSaveable(spaceId) { mutableLongStateOf(space?.unread?.toLong() ?: -1L) }
    var firstUnread by rememberSaveable(spaceId) { mutableStateOf<String?>(null) }
    var unseen by rememberSaveable(spaceId) { mutableIntStateOf(0) }
    val motion = rememberMotionEnabled()
    val nearBottomTolerance = with(LocalDensity.current) { 140.dp.toPx() }
    val nearBottom by remember(list, nearBottomTolerance) { derivedStateOf {
        val layout = list.layoutInfo
        val last = layout.visibleItemsInfo.lastOrNull()
        last == null || (last.index == layout.totalItemsCount - 1 && last.offset + last.size <= layout.viewportEndOffset + nearBottomTolerance)
    } }
    val navigationHaptics = rememberZoenHaptics()
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
        if (openingUnread < 0 && space != null) openingUnread = space.unread.toLong()
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
            if (motion) list.animateScrollToItem(index) else list.scrollToItem(index)
            highlight = original.inThread ?: target
            jumpTo = null
        }
    }
    LaunchedEffect(highlight) { if (highlight != null) { delay(1800); highlight = null } }
    LaunchedEffect(spaceId, allEntries.map { it.id }) {
        if (spaceId !in state.timelines || timeline.isEmpty()) return@LaunchedEffect
        if (firstLoad) {
            firstUnread = firstUnreadMessage(allEntries, openingUnread.coerceAtLeast(0))
            knownIds = ArrayList(allEntries.map { it.id })
            firstLoad = false
            if (focusMessage == null && jumpTo == null && highlight == null) {
                withFrameNanos { }
                val unreadIndex = timeline.indexOfFirst { it.id == firstUnread }
                list.scrollToItem(if (unreadIndex >= 0) unreadIndex else timeline.lastIndex)
            }
        } else {
            val appended = appendedMessages(allEntries, knownIds.toSet())
            knownIds = ArrayList(allEntries.map { it.id })
            if (appended.mine || (appended.incoming > 0 && nearBottom)) {
                if (jumpTo == null && highlight == null) {
                    withFrameNanos { }
                    if (motion) list.animateScrollToItem(timeline.lastIndex) else list.scrollToItem(timeline.lastIndex)
                    unseen = 0
                }
            } else unseen += appended.incoming
        }
    }
    LaunchedEffect(list) {
        snapshotFlow { nearBottom }.collect { atBottom -> if (atBottom) unseen = 0 }
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
    val sendHaptics = rememberZoenHaptics()
    val send: () -> Unit = {
        if (draft.isNotBlank() && !sending && !state.keyMissing) {
            val text = draft
            val reply = replyId
            sending = true
            model.launch {
                try {
                    model.send(spaceId, text, reply, onStored = { sendHaptics.perform(ZoenFeedback.Send) })
                    if (draft == text) { draft = ""; model.repository.preferences.edit().remove(draftKey).apply() }
                    replyId = null
                } finally { sending = false }
            }
        }
    }
    if (space == null) {
        Scaffold(topBar = { ScreenBar(stringResource(R.string.chats), back) }) { padding ->
            EmptyState(stringResource(R.string.unavailable), stringResource(R.string.unavailable_detail), Modifier.padding(padding)) {
                GlassOutlinedButton(onClick = { model.launch { model.repository.refresh() } }) { Text(stringResource(R.string.retry)) }
            }
        }
        return
    }
    Scaffold(
        topBar = {
            val enlargedText = LocalDensity.current.fontScale >= 1.5f
            val participants = stringResource(R.string.participants)
            val identity: @Composable () -> Unit = {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    if (space.counterpart != null) Avatar(space.counterpart, size = 40, onClick = { navigate(if (space.counterpart!!.kind == PersonaKind.AGENT) Agent(space.counterpart!!.id) else Person(space.counterpart!!.id)) }, contact = true, working = working[spaceId] != null)
                    else SpaceAvatar(space, size = 40, onClick = { navigate(Participants(space.id)) })
                    Column(Modifier.weight(1f).clickable(role = Role.Button, onClickLabel = participants) { navigate(Participants(space.id)) }) {
                        Text(space.title, style = MaterialTheme.typography.titleMedium, maxLines = if (enlargedText) Int.MAX_VALUE else 1, overflow = TextOverflow.Ellipsis)
                        val remote = state.remoteActivity[spaceId]
                        val name = working[spaceId] ?: state.typing[spaceId]?.let { id -> space.members.firstOrNull { it.id == id }?.name } ?: remote?.let { activity -> space.members.firstOrNull { it.id == activity.identity }?.name }
                        val status = when {
                            working[spaceId] != null -> stringResource(R.string.chat_processing, name.orEmpty())
                            state.typing[spaceId] != null -> stringResource(R.string.person_typing, name.orEmpty())
                            remote != null -> stringResource(when (remote.kind) { "building" -> R.string.chat_building; "call" -> R.string.chat_call; else -> R.string.chat_processing }, name.orEmpty())
                            space.counterpart?.id in state.online -> stringResource(R.string.chat_online)
                            space.counterpart == null -> space.members.count { it.kind == PersonaKind.PERSON }.let { pluralStringResource(R.plurals.chat_people_count, it, it) }
                            else -> ""
                        }
                        if (status.isNotBlank()) Text(status, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = if (enlargedText) Int.MAX_VALUE else 1)
                    }
                }
            }
            Column(Modifier.background(MaterialTheme.colorScheme.surface)) {
                TopAppBar(title = { if (!enlargedText) identity() }, navigationIcon = { InkIconButton(ZoenGlyph.Back, stringResource(R.string.back), { navigationHaptics.perform(ZoenFeedback.Dismiss); back() }) }, actions = {
                    val callsComing = stringResource(R.string.calls_coming)
                    IconButton(onClick = { model.notify(callsComing) }) { Icon(Icons.Rounded.Call, stringResource(R.string.voice_call)) }
                    IconButton(onClick = { model.notify(callsComing) }) { Icon(Icons.Rounded.VideoCall, stringResource(R.string.video_call)) }
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
                if (enlargedText) Box(Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, bottom = 12.dp)) { identity() }
            }
        },
        bottomBar = {
            Surface(color = MaterialTheme.colorScheme.surface) {
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
                        InkIconButton(ZoenGlyph.Plus, stringResource(R.string.attach), { navigationHaptics.perform(ZoenFeedback.Open); attachments = true }, Modifier.padding(bottom = 4.dp))
                        OutlinedTextField(value = draft, onValueChange = {
                            draft = it
                            model.repository.preferences.edit().putString(draftKey, it).apply()
                        }, modifier = Modifier.weight(1f).heightIn(min = 48.dp).testTag("composer"), shape = RoundedCornerShape(24.dp),
                            // Hark: 15/19 text.
                            textStyle = MaterialTheme.typography.bodyLarge.copy(fontSize = 15.sp, lineHeight = 19.sp),
                            placeholder = { Text(stringResource(if (space.counterpart?.handle == "zoen") R.string.message_zoen else R.string.message), maxLines = 1) },
                            maxLines = 5, keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Send),
                            keyboardActions = KeyboardActions(onSend = { send() }),
                        )
                        if (draft.isBlank()) VoiceComposer(model, spaceId, Modifier.padding(bottom = 4.dp), reply = replyId, onSent = { replyId = null })
                        else FilledIconButton(onClick = send, enabled = draft.isNotBlank() && !sending && !state.keyMissing, modifier = Modifier.padding(bottom = 4.dp).size(48.dp).testTag("send")) {
                            if (sending) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                            else ZoenIcon(ZoenGlyph.Send, stringResource(R.string.send))
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
            val apps = state.items.filter { it.spaceId == spaceId && (it.app != null || it.plan != null) }
            if (apps.isNotEmpty()) MiniAppTileStrip(model, state, apps, scope = MiniAppPins.chat(spaceId),
                compact = WindowInsets.ime.getBottom(LocalDensity.current) > 0, onOpenItem = { navigate(Item(it)) })
            AgentBrowserCard(model.browser, spaceId) { navigate(Browser) }
            Box(Modifier.weight(1f)) {
                ChatBackdrop(model, space.id, state.revision, Modifier.fillMaxSize())
                if (spaceId in state.timelines) {
                    LazyColumn(state = list, modifier = Modifier.fillMaxSize().testTag("chat-timeline"), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp, Alignment.Bottom)) {
                        itemsIndexed(timeline, key = { _, entry -> entry.id }) { index, entry ->
                            if (index == 0) Box(Modifier.fillMaxWidth().padding(top = 8.dp, bottom = 12.dp), contentAlignment = Alignment.Center) {
                                if (space.counterpart != null) Avatar(space.counterpart, size = 64, contact = true)
                                else SpaceAvatar(space, size = 64)
                            }
                            if (entry.id == firstUnread) Row(Modifier.fillMaxWidth().padding(vertical = 12.dp).testTag("unread-divider"), verticalAlignment = Alignment.CenterVertically) {
                                HorizontalDivider(Modifier.weight(1f))
                                Text(stringResource(R.string.chat_unread_divider), Modifier.padding(horizontal = 12.dp), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                                HorizontalDivider(Modifier.weight(1f))
                            }
                            Box(Modifier.animateItem(fadeInSpec = if (motion) androidx.compose.animation.core.tween(180) else null, placementSpec = null, fadeOutSpec = null).testTag("timeline:${entry.id}").background(if (highlight == entry.id) MaterialTheme.colorScheme.secondaryContainer else androidx.compose.ui.graphics.Color.Transparent, RoundedCornerShape(16.dp))) {
                                TimelineRow(model, spaceId, entry, navigate, onReply = { replyId = entry.id }, onThread = { navigate(Thread(spaceId, entry.id)) }, onQuote = { jumpTo = it }, direct = space.counterpart != null)
                            }
                        }
                    }
                } else CircularProgressIndicator(Modifier.align(Alignment.Center))
                if (unseen > 0) FilledTonalButton(onClick = {
                    navigationHaptics.perform(ZoenFeedback.Selection)
                    scrollScope.launch {
                        if (motion) list.animateScrollToItem(timeline.lastIndex) else list.scrollToItem(timeline.lastIndex)
                        unseen = 0
                    }
                }, modifier = Modifier.align(Alignment.BottomCenter).padding(16.dp).testTag("chat-new-messages").semantics { liveRegion = LiveRegionMode.Polite }) {
                    Icon(Icons.Rounded.ArrowDownward, null); Spacer(Modifier.width(8.dp))
                    Text(pluralStringResource(R.plurals.chat_new_messages, unseen, unseen))
                } else if (timeline.size > 5 && !nearBottom) SmallFloatingActionButton(onClick = {
                    navigationHaptics.perform(ZoenFeedback.Selection)
                    scrollScope.launch { if (motion) list.animateScrollToItem(timeline.lastIndex) else list.scrollToItem(timeline.lastIndex) }
                }, Modifier.align(Alignment.BottomEnd).padding(16.dp)) {
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
fun TimelineRow(model: ZoenViewModel, spaceId: String, entry: TimelineEntry, navigate: (NavKey) -> Unit, onReply: () -> Unit, onThread: (() -> Unit)?, onQuote: ((String) -> Unit)? = null, direct: Boolean = false) {
    val context = LocalContext.current
    var menu by remember { mutableStateOf(false) }
    var editing by rememberSaveable(entry.id) { mutableStateOf(false) }
    var editText by rememberSaveable(entry.id) { mutableStateOf("") }
    var forwarding by remember { mutableStateOf(false) }
    var confirmDelete by rememberSaveable(entry.id) { mutableStateOf(false) }
    val copied = stringResource(R.string.copied)
    val locale = LocalConfiguration.current.locales[0].toLanguageTag()
    when (val kind = entry.kind) {
        is EntryKind.Message -> {
            // Only your own agent's notes become cards, and only the core decides that. A
            // candidate (agent author, fenced JSON) stays hidden until the core has answered,
            // so the raw JSON never flashes in the chat.
            val candidate = entry.author.kind == PersonaKind.AGENT && !entry.author.isMe && kind.text.trimStart().startsWith("```json")
            var noteChecked by remember(entry.id) { mutableStateOf(!candidate) }
            var ownerNote by remember(entry.id) { mutableStateOf<OwnerAgentNote?>(null) }
            // Asked again when the state moves (the agent's verified profile may sync after
            // its message); the last answer stays on screen meanwhile, so nothing flickers.
            val noteRevision = if (candidate) model.state.collectAsStateWithLifecycle().value.revision else 0L
            LaunchedEffect(entry.id, noteRevision) {
                if (candidate && ownerNote == null) {
                    ownerNote = runCatching { model.repository.query { it.ownerAgentNote(entry.author.id, kind.text) } }.getOrNull()?.let(OwnerAgentNote::parse)
                    noteChecked = true
                }
            }
            if (!noteChecked) return
            ownerNote?.let { note ->
                val state by model.state.collectAsStateWithLifecycle()
                Row(Modifier.fillMaxWidth().padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Spacer(Modifier.width(36.dp))
                    OwnerAgentNoteCard(entry.author, note,
                        personName = { id -> state.spaces.asSequence().flatMap { it.members }.firstOrNull { it.id == id }?.name },
                        fresh = System.currentTimeMillis() - entry.atMs < 15_000,
                        open = { navigate(Chat(it)) })
                }
                return
            }
            val mine = entry.author.isMe
            var swipe by remember(entry.id) { mutableFloatStateOf(0f) }
            val threshold = with(LocalDensity.current) { 72.dp.toPx() }
            val haptics = rememberZoenHaptics()
            val motion = rememberMotionEnabled()
            val scope = rememberCoroutineScope()
            val settling = remember { Animatable(0f) }
            var returnJob by remember { mutableStateOf<Job?>(null) }
            var returning by remember { mutableStateOf(false) }
            var armed by remember { mutableStateOf(false) }
            val shownSwipe = if (returning) settling.value else swipe
            fun settle() {
                if (!motion) { swipe = 0f; returning = false; return }
                returnJob = scope.launch {
                    settling.snapTo(swipe); returning = true
                    settling.animateTo(0f, spring(dampingRatio = .7f, stiffness = 400f))
                    swipe = 0f; returning = false
                }
            }
            val reply by rememberUpdatedState(onReply)
            val thread by rememberUpdatedState(onThread)
            Box(Modifier.fillMaxWidth()) {
                if (swipe < -threshold / 3) Icon(Icons.AutoMirrored.Rounded.Reply, stringResource(R.string.reply), Modifier.align(Alignment.CenterEnd).padding(12.dp), tint = MaterialTheme.colorScheme.primary)
                if (swipe > threshold / 3 && onThread != null) Icon(Icons.Rounded.Forum, stringResource(R.string.thread), Modifier.align(Alignment.CenterStart).padding(12.dp), tint = MaterialTheme.colorScheme.primary)
            Row(Modifier.fillMaxWidth().offset { IntOffset(shownSwipe.roundToInt(), 0) }.pointerInput(entry.id, onThread != null, threshold) {
                detectHorizontalDragGestures(onDragStart = {
                    swipe = shownSwipe; returnJob?.cancel(); returning = false; armed = false
                }, onDragEnd = {
                    if (swipe <= -threshold) reply()
                    else if (swipe >= threshold && thread != null) thread?.invoke()
                    settle()
                }, onDragCancel = { settle() }) { change, distance ->
                    change.consume()
                    swipe = (swipe + distance).coerceIn(-threshold * 1.5f, if (thread == null) 0f else threshold * 1.5f)
                    val reached = kotlin.math.abs(swipe) >= threshold
                    if (reached && !armed) haptics.perform(ZoenFeedback.Selection)
                    armed = reached
                }
            }, verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (mine) Spacer(Modifier.width(40.dp))
                else Avatar(entry.author, modifier = Modifier.alignBy { it.measuredHeight }, size = 36, onClick = { navigate(if (entry.author.kind == PersonaKind.AGENT) Agent(entry.author.id) else Person(entry.author.id)) }, contact = true)
                Column(Modifier.weight(1f).then(if (mine) Modifier else Modifier.alignBy(MessageFace)), horizontalAlignment = if (mine) Alignment.End else Alignment.Start) {
                if (!mine && !direct) Text(entry.author.name, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Box {
                    Column(horizontalAlignment = if (mine) Alignment.End else Alignment.Start, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (entry.deleted) Surface(shape = RoundedCornerShape(24.dp), color = androidx.compose.ui.graphics.Color.Transparent, border = androidx.compose.foundation.BorderStroke(0.8.dp, MaterialTheme.colorScheme.outlineVariant),
                        modifier = Modifier.padding(top = 6.dp).messageFace().combinedClickable(onClick = {}, onLongClick = { menu = true })) {
                        Text(stringResource(R.string.message_deleted), Modifier.padding(horizontal = 14.dp, vertical = 10.dp), style = MaterialTheme.typography.bodyMedium.copy(fontStyle = androidx.compose.ui.text.font.FontStyle.Italic), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    else if (entry.voice != null || kind.text.isNotBlank() || entry.replyTo != null) {
                    Surface(shape = RoundedCornerShape(24.dp, 24.dp, if (mine) 6.dp else 24.dp, if (mine) 24.dp else 6.dp),
                        color = if (mine) MaterialTheme.colorScheme.ownMessage else MaterialTheme.colorScheme.otherMessage,
                        contentColor = if (mine) MaterialTheme.colorScheme.onOwnMessage else MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.padding(top = 6.dp).messageFace().widthIn(max = 360.dp).combinedClickable(onClick = { kind.card?.let { navigate(Item(it.itemId)) } }, onLongClick = { menu = true })) {
                        Column(Modifier.padding(horizontal = 14.dp, vertical = 10.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            entry.replyTo?.let { quote ->
                                Surface(color = MaterialTheme.colorScheme.surface, contentColor = MaterialTheme.colorScheme.onSurface, shape = RoundedCornerShape(8.dp), modifier = Modifier.testTag("quote:${quote.id}").clickable { onQuote?.invoke(quote.id) }) {
                                    Column(Modifier.padding(10.dp)) { Text(quote.author.name, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary); Text(quote.text, maxLines = 2, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodySmall) }
                                }
                            }
                            if (entry.forwarded) Text(stringResource(R.string.forwarded), style = MaterialTheme.typography.labelSmall.copy(fontStyle = androidx.compose.ui.text.font.FontStyle.Italic), color = LocalContentColor.current.copy(alpha = .65f))
                            val voice = entry.voice?.let(VoiceNoteRef::from) ?: VoiceNoteRef.parse(kind.text)
                            if (voice != null) VoiceBubble(model, voice)
                            else if (kind.text.isNotBlank()) {
                                val mentionColor = if (mine) LocalContentColor.current else MaterialTheme.colorScheme.primary
                                Box { SelectionContainer { Text(buildAnnotatedString { append(kind.text); Regex("(?<![\\w.])@[\\p{L}\\p{N}_.]+").findAll(kind.text).forEach { match -> addStyle(SpanStyle(color = mentionColor, fontWeight = FontWeight.SemiBold), match.range.first, match.range.last + 1) }
                                    // A clear run as wide as the meta, so it never sits on the last line's words.
                                    withStyle(SpanStyle(color = androidx.compose.ui.graphics.Color.Transparent, fontSize = MaterialTheme.typography.labelSmall.fontSize)) { append(bubbleMetaReserve(entry.atMs, mine)) } }, style = MaterialTheme.typography.bodyLarge,
                                        // Readers and tests see the message, not the meta reserve.
                                        modifier = Modifier.clearAndSetSemantics { text = androidx.compose.ui.text.AnnotatedString(kind.text) }) }
                                    // WhatsApp: time and tick inside the bubble, bottom right.
                                    BubbleMeta(entry.atMs, if (mine) entry.delivery else null, Modifier.align(Alignment.BottomEnd).offset(y = 3.dp)) }
                            }
                            if (voice != null) BubbleMeta(entry.atMs, if (mine) entry.delivery else null, Modifier.align(Alignment.End))
                        }
                    }
                    }
                    kind.card?.let { card ->
                                GlassCard(onClick = { navigate(Item(card.itemId)) }, modifier = Modifier.testTag("item:${card.itemId}").then(if (kind.text.isBlank() && entry.replyTo == null) Modifier.messageFace() else Modifier), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
                                    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                        Row(verticalAlignment = Alignment.CenterVertically) {
                                            Text(card.title, Modifier.weight(1f), style = MaterialTheme.typography.titleLarge,
                                                fontFamily = if (card.app == null) FontFamily.Serif else FontFamily.Default, fontWeight = FontWeight.Bold)
                                            Icon(Icons.AutoMirrored.Rounded.KeyboardArrowRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                                        }
                                        Text(card.app?.headline ?: card.summary, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                        card.totalCents?.let { Text(formatMoney(it, locale), style = MaterialTheme.typography.titleSmall) }
                                        if (card.app != null) Text(stringResource(R.string.version, card.version.toInt()), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
                                    }
                                }
                    }
                    }
                    DropdownMenu(menu, onDismissRequest = { menu = false }, shape = RoundedCornerShape(24.dp)) {
                        // Reactions on top, then WhatsApp's order.
                        val myReaction = entry.reactions.firstOrNull { it.mine }?.emoji
                        Row(Modifier.padding(horizontal = 8.dp), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                            QUICK_REACTIONS.forEach { emoji ->
                                Box(Modifier.size(42.dp).clip(CircleShape).background(if (emoji == myReaction) MaterialTheme.colorScheme.primary.copy(alpha = .16f) else androidx.compose.ui.graphics.Color.Transparent)
                                    .clickable { menu = false; haptics.perform(ZoenFeedback.Selection); scope.launch { runCatching { model.repository.change { it.react(spaceId, entry.id, if (emoji == myReaction) null else emoji) } } } }
                                    .testTag("react:$emoji"), contentAlignment = Alignment.Center) { Text(emoji, fontSize = 24.sp) }
                            }
                        }
                        DropdownMenuItem(text = { Text(stringResource(R.string.reply)) }, onClick = { menu = false; onReply() }, leadingIcon = { Icon(Icons.AutoMirrored.Rounded.Reply, null) })
                        if (onThread != null) DropdownMenuItem(text = { Text(stringResource(R.string.thread)) }, onClick = { menu = false; onThread() }, leadingIcon = { Icon(Icons.Rounded.Forum, null) })
                        if (!entry.deleted) {
                            DropdownMenuItem(text = { Text(stringResource(R.string.forward)) }, onClick = { menu = false; forwarding = true }, leadingIcon = { Icon(Icons.AutoMirrored.Rounded.Forward, null) })
                            if (kind.text.isNotBlank()) DropdownMenuItem(text = { Text(stringResource(R.string.copy)) }, onClick = { menu = false; context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText(copied, kind.text)) }, leadingIcon = { Icon(Icons.Rounded.ContentCopy, null) })
                            if (mine && entry.voice == null && kind.text.isNotBlank() && VoiceNoteRef.parse(kind.text) == null) DropdownMenuItem(text = { Text(stringResource(R.string.edit)) }, onClick = { menu = false; editText = kind.text; editing = true }, leadingIcon = { Icon(Icons.Rounded.Edit, null) })
                            if (mine) DropdownMenuItem(text = { Text(stringResource(R.string.delete_for_everyone), color = MaterialTheme.colorScheme.error) }, onClick = { menu = false; confirmDelete = true }, leadingIcon = { Icon(Icons.Rounded.Delete, null, tint = MaterialTheme.colorScheme.error) })
                        }
                    }
                    MessageActionDialogs(model, spaceId, entry, editing, editText, { editText = it }, forwarding, confirmDelete) { editing = false; forwarding = false; confirmDelete = false }
                }
                if (entry.reactions.isNotEmpty()) ReactionChips(model, spaceId, entry)
                if (entry.edited) Text(stringResource(R.string.edited), Modifier.padding(top = 2.dp, start = 8.dp, end = 8.dp), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                // Cards without a text bubble keep the meta underneath.
                if (entry.voice == null && kind.text.isBlank() && entry.replyTo == null) BubbleMeta(entry.atMs, if (mine) entry.delivery else null, Modifier.padding(top = 5.dp, start = 8.dp, end = 8.dp))
                if (entry.threadReplies > 0u && onThread != null) ThreadRepliesRow(entry, onThread)
                }
            }
            }
        }
        is EntryKind.ItemEdited -> TextButton(onClick = { navigate(Item(kind.itemId)) }, Modifier.fillMaxWidth()) { Icon(Icons.Rounded.History, null, Modifier.size(16.dp)); Spacer(Modifier.width(8.dp)); Text(kind.note, style = MaterialTheme.typography.bodySmall) }
        is EntryKind.Request -> {
            val state by model.state.collectAsStateWithLifecycle()
            val request = state.requests.firstOrNull { it.id == kind.requestId }
            var approving by remember { mutableStateOf(false) }
            GlassCard(onClick = { navigate(Request(kind.requestId)) }, colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
                Row(Modifier.fillMaxWidth().padding(12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Avatar(request?.agent ?: entry.author, size = 30)
                    Column(Modifier.weight(1f)) {
                        Text(kind.title, style = MaterialTheme.typography.titleSmall, maxLines = 2)
                        request?.let { Text(listOfNotNull(it.agent.name, it.costCents?.let { cost -> formatMoney(cost, locale) }).joinToString(" · "), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    }
                    when (request?.status) {
                        RequestStatus.PENDING -> GlassButton(enabled = request?.agent?.isMine == true && !approving, onClick = {
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
    val scrollScope = rememberCoroutineScope()
    var text by rememberSaveable { mutableStateOf("") }
    var replyTo by rememberSaveable(root) { mutableStateOf(root) }
    var sending by remember { mutableStateOf(false) }
    val threadHaptics = rememberZoenHaptics()
    val motion = rememberMotionEnabled()
    var alsoInChat by rememberSaveable(root) { mutableStateOf(false) }
    val following = entries.firstOrNull()?.threadFollowing == true
    var changingFollow by remember(root) { mutableStateOf(false) }
    LaunchedEffect(entries) { if (entries.isNotEmpty()) runCatching { model.repository.change { it.markThreadRead(space, root) } } }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.replies), back) {
        TextButton(onClick = {
            if (!changingFollow) {
                changingFollow = true
                threadHaptics.perform(ZoenFeedback.Selection)
                model.launch {
                    try { model.repository.change { it.setThreadFollow(space, root, !following) } }
                    finally { changingFollow = false }
                }
            }
        }, enabled = entries.isNotEmpty() && !changingFollow, modifier = Modifier.testTag("thread-follow")) {
            Icon(if (following) Icons.Rounded.Notifications else Icons.Rounded.NotificationsNone, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp))
            Text(stringResource(if (following) R.string.thread_following else R.string.thread_follow))
        }
    } }, bottomBar = {
        Column(Modifier.navigationBarsPadding().imePadding()) {
        Row(Modifier.padding(start = 8.dp).clip(RoundedCornerShape(12.dp)).clickable { alsoInChat = !alsoInChat }.padding(8.dp).testTag("thread-also-in-chat"), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Checkbox(alsoInChat, null, Modifier.size(20.dp)); Text(stringResource(R.string.thread_also_in_chat), style = MaterialTheme.typography.bodySmall)
        }
        Row(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(text, { text = it }, Modifier.weight(1f), placeholder = { Text(stringResource(R.string.thread_reply_hint)) }, maxLines = 4,
                label = entries.firstOrNull { it.id == replyTo }?.let { { Text(it.author.name) } })
            if (text.isBlank()) VoiceComposer(model, space, reply = replyTo, thread = true, threadRoot = root, onSent = { replyTo = root })
            else FilledIconButton(onClick = {
                val draft = text; val target = replyTo; sending = true
                val broadcast = alsoInChat
                model.launch { try {
                    model.repository.change { it.sendThreadReply(space, draft.trim(), target, broadcast) }
                    threadHaptics.perform(ZoenFeedback.Send)
                    if (text == draft) text = ""; replyTo = root; alsoInChat = false
                } finally { sending = false } }
            }, enabled = text.isNotBlank() && !sending && !state.keyMissing) { ZoenIcon(ZoenGlyph.Send, stringResource(R.string.send)) }
        }
        }
    }) { padding -> LazyColumn(Modifier.padding(padding), state = list, contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        items(entries, key = { it.id }) { entry -> TimelineRow(model, space, entry, navigate, onReply = { replyTo = entry.id }, onThread = null, onQuote = { id ->
            val index = entries.indexOfFirst { it.id == id }
            if (index >= 0) scrollScope.launch { if (motion) list.animateScrollToItem(index) else list.scrollToItem(index) } else navigate(Chat(space, id))
        }) }
    } }
}

/** Time's companion: ✓ sent, ✓✓ delivered, blue ✓✓ read (aggregated "read up to"). */
@Composable
fun ReceiptTicks(delivery: Delivery, base: androidx.compose.ui.graphics.Color = MaterialTheme.colorScheme.onSurfaceVariant) {
    val icon = when (delivery) {
        Delivery.FAILED -> Icons.Rounded.ErrorOutline
        Delivery.SENDING, Delivery.LOCAL -> Icons.Rounded.Schedule
        Delivery.SENT -> Icons.Rounded.Done
        Delivery.DELIVERED, Delivery.READ -> Icons.Rounded.DoneAll
    }
    val tint by androidx.compose.animation.animateColorAsState(
        when (delivery) {
            Delivery.READ -> androidx.compose.ui.graphics.Color(0xFF53BDEB)
            Delivery.FAILED -> MaterialTheme.colorScheme.error
            else -> base
        },
        androidx.compose.animation.core.tween(280, easing = androidx.compose.animation.core.CubicBezierEasing(0.25f, 1f, 0.5f, 1f)),
        label = "ticks",
    )
    val label = stringResource(when (delivery) { Delivery.READ -> R.string.receipt_read; Delivery.DELIVERED -> R.string.receipt_delivered; Delivery.FAILED -> R.string.receipt_failed; Delivery.SENT -> R.string.receipt_sent; else -> R.string.receipt_sending })
    Icon(icon, label, Modifier.size(15.dp), tint = tint)
}

/** A clear run as wide as the meta ("12:11 PM ✓"), in figure spaces. */
internal fun bubbleMetaReserve(atMs: Long, mine: Boolean): String =
    "\u2002" + "\u2007".repeat(DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(atMs)).length + if (mine) 3 else 1)

/** Time and delivery tick (sending: clock, sent: one tick, refused: error). The receipts
 *  work adds the double tick and read colour here. */
@Composable
internal fun BubbleMeta(atMs: Long, delivery: Delivery?, modifier: Modifier = Modifier) {
    Row(modifier.clearAndSetSemantics { }, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(3.dp)) {
        Text(DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(atMs)), style = MaterialTheme.typography.labelSmall, color = LocalContentColor.current.copy(alpha = 0.62f))
        // ✓ sent, ✓✓ delivered, blue ✓✓ read (aggregated "read up to").
        if (delivery != null) ReceiptTicks(delivery, LocalContentColor.current.copy(alpha = 0.62f))
    }
}

private val QUICK_REACTIONS = listOf("👍", "❤️", "😂", "😮", "😢", "🙏")

/** One chip per emoji under the bubble; mine is tinted and tapping it takes mine back. */
@Composable
private fun ReactionChips(model: ZoenViewModel, spaceId: String, entry: TimelineEntry) {
    val scope = rememberCoroutineScope()
    val haptics = rememberZoenHaptics()
    Row(Modifier.padding(top = 2.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        entry.reactions.forEach { r ->
            Surface(shape = CircleShape, color = if (r.mine) MaterialTheme.colorScheme.primary.copy(alpha = .16f) else MaterialTheme.colorScheme.otherMessage,
                border = androidx.compose.foundation.BorderStroke(0.5.dp, MaterialTheme.colorScheme.outlineVariant),
                modifier = Modifier.animateContentSize(androidx.compose.animation.core.tween(280, easing = androidx.compose.animation.core.CubicBezierEasing(0.25f, 1f, 0.5f, 1f)))
                    .clickable { haptics.perform(ZoenFeedback.Selection); scope.launch { runCatching { model.repository.change { it.react(spaceId, entry.id, if (r.mine) null else r.emoji) } } } }
                    .semantics { contentDescription = "${r.emoji} " + r.people.joinToString { it.name } }) {
                Row(Modifier.padding(horizontal = 8.dp, vertical = 3.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(3.dp)) {
                    Text(r.emoji, fontSize = 14.sp)
                    if (r.people.size > 1) Text("${r.people.size}", style = MaterialTheme.typography.labelSmall)
                }
            }
        }
    }
}

/** Edit (author only), forward to another chat, delete for everyone (author only). */
@Composable
private fun MessageActionDialogs(model: ZoenViewModel, spaceId: String, entry: TimelineEntry, editing: Boolean, editText: String, onEditText: (String) -> Unit, forwarding: Boolean, confirmDelete: Boolean, done: () -> Unit) {
    val scope = rememberCoroutineScope()
    var running by remember(entry.id) { mutableStateOf(false) }
    var error by rememberSaveable(editing, confirmDelete, forwarding, entry.id) { mutableStateOf<String?>(null) }
    val dismiss = { if (!running) done() }
    fun run(block: (RodaEngine) -> Unit) {
        if (running) return
        running = true
        error = null
        scope.launch {
            try {
                error = performMessageMutation({ model.repository.change(block) }, done)
            } finally {
                running = false
            }
        }
    }
    @Composable fun failure() {
        error?.let {
            Text("Não foi possível concluir. Tente novamente.\n$it", color = MaterialTheme.colorScheme.error,
                modifier = Modifier.testTag("message-mutation-error"))
        }
        if (running) LinearProgressIndicator(Modifier.fillMaxWidth())
    }
    if (editing) AlertDialog(onDismissRequest = dismiss, title = { Text(stringResource(R.string.edit_message)) },
        text = { Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedTextField(editText, onEditText, Modifier.fillMaxWidth().testTag("edit-field"), enabled = !running, shape = RoundedCornerShape(24.dp))
            failure()
        } },
        confirmButton = { TextButton(onClick = { run { it.editMessage(spaceId, entry.id, editText) } }, enabled = !running && editText.isNotBlank()) { Text(stringResource(R.string.save)) } },
        dismissButton = { TextButton(onClick = dismiss, enabled = !running) { Text(stringResource(R.string.cancel)) } })
    if (confirmDelete) AlertDialog(onDismissRequest = dismiss, title = { Text(stringResource(R.string.delete_for_everyone_q)) },
        text = { Column { Text("Essa ação não pode ser desfeita."); failure() } },
        confirmButton = { TextButton(onClick = { run { it.deleteMessage(spaceId, entry.id) } }, enabled = !running) { Text(stringResource(R.string.delete_for_everyone), color = MaterialTheme.colorScheme.error) } },
        dismissButton = { TextButton(onClick = dismiss, enabled = !running) { Text(stringResource(R.string.cancel)) } })
    if (forwarding) {
        val state by model.state.collectAsStateWithLifecycle()
        AlertDialog(onDismissRequest = done, title = { Text(stringResource(R.string.forward_to)) },
            text = {
                androidx.compose.foundation.lazy.LazyColumn {
                    items(state.spaces.filter { it.id != spaceId }, key = { it.id }) { target ->
                        Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).clickable { run { it.forwardMessage(spaceId, entry.id, target.id) } }.padding(vertical = 8.dp),
                            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            if (target.counterpart != null) Avatar(target.counterpart!!, size = 36, contact = true) else SpaceAvatar(target, size = 36)
                            Text(target.title, style = MaterialTheme.typography.bodyLarge)
                        }
                    }
                }
            },
            confirmButton = {}, dismissButton = { TextButton(onClick = done) { Text(stringResource(R.string.cancel)) } })
    }
}

/** Under a thread's root (Slack/Ando): who replied, "N replies", when, and an unread dot. */
@Composable
private fun ThreadRepliesRow(entry: TimelineEntry, open: () -> Unit) {
    Row(Modifier.padding(top = 4.dp).clip(RoundedCornerShape(24.dp)).background(MaterialTheme.colorScheme.primary.copy(alpha = .1f)).clickable(onClick = open).padding(horizontal = 8.dp, vertical = 4.dp).testTag("thread-chip"),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy((-6).dp)) { entry.threadParticipants.take(3).forEach { Avatar(it, size = 18) } }
        Text(pluralStringResource(R.plurals.thread_replies, entry.threadReplies.toInt(), entry.threadReplies.toInt()), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.SemiBold)
        if (entry.threadLastReplyMs > 0) Text(DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(entry.threadLastReplyMs)), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        androidx.compose.animation.AnimatedVisibility(entry.threadFollowing && entry.threadUnread > 0u) { Box(Modifier.size(7.dp).clip(CircleShape).background(MaterialTheme.colorScheme.primary)) }
    }
}
