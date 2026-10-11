package xyz.tironi.zoen.ui

import android.net.Uri
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Chat
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.ui.NavDisplay
import xyz.tironi.zoen.ui.ink.ZoenGlyph
import xyz.tironi.zoen.miniapps.MiniAppStoreScreen
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ZoenApp(model: ZoenViewModel, deepLink: String?, consumed: () -> Unit) {
    val state by model.state.collectAsStateWithLifecycle()
    var onboardingLanding by rememberSaveable { mutableStateOf<String?>(null) }
    val snackbar = remember { SnackbarHostState() }
    val undo = stringResource(R.string.undo)
    val haptics = rememberZoenHaptics()
    LaunchedEffect(model) {
        model.notices.collect { notice ->
            val result = snackbar.showSnackbar(notice.text, if (notice.undo != null) undo else null,
                withDismissAction = true, duration = SnackbarDuration.Long)
            if (result == SnackbarResult.ActionPerformed) notice.undo?.let(model::undo)
        }
    }
    if (!state.ready) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                ZoenMascot(Modifier.size(180.dp), animated = true)
                Text(stringResource(R.string.opening), color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        return
    }
    if (state.failure != null) {
        Scaffold(snackbarHost = { SnackbarHost(snackbar) }) { padding ->
            EmptyState(stringResource(R.string.something_wrong), state.failure.orEmpty(), Modifier.padding(padding)) {
                Button(onClick = { model.boot(false) }) { Text(stringResource(R.string.retry)) }
            }
        }
        return
    }
    if (state.me == null || !model.repository.preferences.getBoolean("onboarded", false)) {
        Scaffold(snackbarHost = { SnackbarHost(snackbar) }) { padding ->
            Onboarding(model, Modifier.padding(padding), deepLink) { onboardingLanding = it }
        }
        return
    }

    key(state.demo, state.me?.id, LocalConfiguration.current.locales[0].language) {
        var tab by rememberSaveable { mutableStateOf(Tab.Chats) }
        val chats = rememberNavBackStack(Home)
        val store = rememberNavBackStack(Home)
        val files = rememberNavBackStack(Home)
        val activity = rememberNavBackStack(Home)
        LaunchedEffect(tab) { if (tab == Tab.Spaces) tab = Tab.Chats }
        val stack = when (tab) { Tab.Chats, Tab.Spaces -> chats; Tab.Store -> store; Tab.Files -> files; Tab.Activity -> activity }
        val navigate: (NavKey) -> Unit = { key ->
            if (key == Home) { while (stack.size > 1) stack.removeLastOrNull() }
            else if (stack.lastOrNull() != key) stack.add(key)
        }
        val back: () -> Unit = { if (stack.size > 1) stack.removeLastOrNull() }
        var quickActions by rememberSaveable { mutableStateOf(false) }
        var voiceStart by remember { mutableIntStateOf(0) }
        var voiceRelease by remember { mutableIntStateOf(0) }
        var voiceLock by remember { mutableIntStateOf(0) }
        var voiceCancel by remember { mutableIntStateOf(0) }
        LaunchedEffect(onboardingLanding) {
            onboardingLanding?.let { navigate(Chat(it)); onboardingLanding = null; consumed() }
        }
        LaunchedEffect(deepLink, state.me) {
            val uri = deepLink?.let(Uri::parse) ?: return@LaunchedEffect
            model.repository.query { it.growthCaptureLink(uri.toString()) }
            if (uri.scheme == "zoen") when (uri.host) {
                "app", "item" -> uri.lastPathSegment?.let { navigate(Item(it)) }
                "chat" -> uri.lastPathSegment?.let { navigate(Chat(it)) }
                "join" -> navigate(Join(uri.lastPathSegment.orEmpty()))
                "request" -> uri.lastPathSegment?.let { navigate(Request(it)) }
                "widgets" -> navigate(Widgets)
                "friend" -> uri.lastPathSegment?.let { handle ->
                    model.launch {
                        val friend = model.repository.network { core -> core.findPeople(handle).firstOrNull { it.handle.equals(handle, true) } }
                        friend?.let { navigate(Chat(model.repository.change { core -> core.startDirect(it.id) })) }
                    }
                }
            }
            if (uri.scheme == "https" && (uri.host == "tryzoen.com" || uri.host == "zoen.app" || uri.host?.endsWith(".tryzoen.com") == true)) {
                if (uri.pathSegments.firstOrNull() == "j") navigate(Join(uri.lastPathSegment.orEmpty()))
                else uri.pathSegments.firstOrNull()?.takeIf { it.startsWith("@") }?.removePrefix("@")?.let { handle ->
                    model.launch {
                        val friend = model.repository.network { core -> core.findPeople(handle).firstOrNull { it.handle.equals(handle, true) } }
                        friend?.let { navigate(Chat(model.repository.change { core -> core.startDirect(it.id) })) }
                    }
                }
            }
            consumed()
        }
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val unread = state.spaces.sumOf { it.unread.toLong() }.coerceAtMost(100L).toInt()
            val rail = maxWidth >= 600.dp
            val split = maxWidth >= 840.dp
            val largeNavigationText = LocalConfiguration.current.fontScale > 1.5f
            val navigationHeight = if (largeNavigationText) 112.dp else 80.dp
            val snackbarClearance = remember { SnackbarClearance() }
            val density = LocalDensity.current
            Row {
                if (rail) NavigationRail(modifier = Modifier.fillMaxHeight(), header = {
                    ZoenMascot(Modifier.padding(top = 12.dp).size(64.dp))
                    Spacer(Modifier.height(16.dp))
                    PlusVoiceButton({ quickActions = true }, { voiceStart++ }, { voiceRelease++ }, { voiceLock++ }, { voiceCancel++ }, compact = true)
                }) {
                    Spacer(Modifier.height(24.dp))
                    Tab.main.forEach { target ->
                        NavigationRailItem(selected = tab == target, onClick = { if (tab != target) haptics.perform(ZoenFeedback.Selection); tab = target }, icon = { TabIcon(target, if (target == Tab.Chats) unread else state.pending, tab == target) }, label = { Text(tabName(target)) })
                    }
                    Spacer(Modifier.weight(1f))
                    IconButton(onClick = { navigate(Search) }) { Icon(Icons.Rounded.Search, stringResource(R.string.search)) }
                    IconButton(onClick = { navigate(Context) }) { Icon(Icons.Rounded.Person, stringResource(R.string.context)) }
                }
                Scaffold(
                    modifier = Modifier.weight(1f),
                    snackbarHost = { if (stack.lastOrNull() !is VersionPreview) SnackbarHost(snackbar,
                        Modifier.padding(bottom = with(density) { snackbarClearance.bottomPx.toDp() }).testTag("app-snackbar")) },
                    bottomBar = {
                        if (!rail && stack.size == 1) NavigationBar(modifier = Modifier.heightIn(min = navigationHeight), containerColor = MaterialTheme.colorScheme.surface, tonalElevation = 0.dp) {
                            Tab.main.forEachIndexed { index, target ->
                                if (index == 2) Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
                                    PlusVoiceButton({ quickActions = true }, { voiceStart++ }, { voiceRelease++ }, { voiceLock++ }, { voiceCancel++ }, compact = true)
                                }
                                val item: @Composable RowScope.() -> Unit = {
                                    NavigationBarItem(selected = tab == target, onClick = { if (tab != target) haptics.perform(ZoenFeedback.Selection); tab = target }, icon = { TabIcon(target, if (target == Tab.Chats) unread else state.pending, tab == target) }, label = {
                                        Text(tabName(target), maxLines = 1, overflow = TextOverflow.Ellipsis,
                                            style = if (largeNavigationText) MaterialTheme.typography.labelSmall else MaterialTheme.typography.labelMedium)
                                    }, alwaysShowLabel = !largeNavigationText, modifier = Modifier.testTag("tab:${target.name}"))
                                }
                                if (largeNavigationText) Row(Modifier.weight(if (tab == target) 2.5f else 1f)) { item() }
                                else item()
                            }
                        }
                    },
                    contentWindowInsets = WindowInsets(0, 0, 0, 0),
                ) { padding ->
                    CompositionLocalProvider(LocalSnackbarClearance provides snackbarClearance) { NavDisplay(
                        modifier = Modifier.fillMaxSize().padding(padding), backStack = stack, onBack = back,
                        entryProvider = entryProvider {
                            entry<Home> {
                                when (tab) {
                                    Tab.Chats -> ConversationsScreen(model, state, navigate, split)
                                    Tab.Spaces -> ConversationsScreen(model, state, navigate, split)
                                    Tab.Store -> MiniAppStoreScreen(model, state, { navigate(Item(it)) }, { tab = Tab.Chats }, { navigate(Agent(it)) })
                                    Tab.Files -> FilesScreen(model, state, navigate, null)
                                    Tab.Activity -> ActivityScreen(model, state, navigate)
                                }
                            }
                            entry<Chat> { ChatScreen(model, state, it.id, navigate, back, it.message) }
                            entry<Item> { ItemScreen(model, state, it.id, navigate, back) }
                            entry<Agent> { AgentScreen(model, state, it.id, back, navigate) }
                            entry<Person> { ProfileScreen(model, state, it.id, navigate, back) }
                            entry<Participants> { ParticipantsScreen(model, state, it.id, navigate, back) }
                            entry<Thread> { ThreadScreen(model, state, it.space, it.root, navigate, back) }
                            entry<Files> { FilesScreen(model, state, navigate, back) }
                            entry<Folder> { FilesScreen(model, state, navigate, back, it.space) }
                            entry<Agents> { AgentsScreen(state, navigate, back) }
                            entry<Context> { ContextScreen(model, state, navigate, back) }
                            entry<Search> { SearchScreen(model, state, navigate, back) }
                            entry<History> { HistoryScreen(model, state, back) }
                            entry<Permissions> { PermissionsScreen(model, state, back) }
                            entry<AgentPermissions> { StandingPermissionsScreen(model, state, back, it.agent, it.space) }
                            entry<NewChat> { NewChatScreen(model, state, navigate, back) }
                            entry<NewSpace> { NewSpaceScreen(model, navigate, back) }
                            entry<Join> { JoinScreen(model, it.code, navigate, back) }
                            entry<Request> { RequestScreen(model, state, it.id, navigate, back) }
                            entry<Appearance> { ChatAppearanceScreen(model, it.space, back) }
                            entry<Widgets> { xyz.tironi.zoen.widgets.WidgetsScreen(state, back) }
                            entry<Browser> { xyz.tironi.zoen.agent.AgentBrowserScreen(model.browser, back) }
                            entry<VersionPreview> { route -> VersionPreviewScreen(model, route.id, route.number, back, snackbar.takeIf { stack.lastOrNull() == route }) }
                        },
                    ) }
                }
            }
        }
        state.zoenChat?.let { chat ->
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.BottomEnd) {
                xyz.tironi.zoen.media.VoiceComposer(model, chat.id, Modifier.padding(end = 16.dp, bottom = 140.dp), startSignal = voiceStart, releaseSignal = voiceRelease, lockSignal = voiceLock, cancelSignal = voiceCancel, showTrigger = false, onSent = { navigate(Chat(chat.id)) })
            }
        }
        if (quickActions) ModalBottomSheet(onDismissRequest = { quickActions = false }) {
            Column(Modifier.fillMaxWidth().weight(1f, fill = false).verticalScroll(rememberScrollState()).testTag("quickActions")) {
                Text(stringResource(R.string.app_name), Modifier.padding(horizontal = 24.dp, vertical = 8.dp), style = MaterialTheme.typography.headlineMedium)
                val actions = listOf(
                    Triple(Icons.Rounded.Search, R.string.search, Search),
                    Triple(Icons.Rounded.AutoAwesome, R.string.ask_zoen, state.zoenChat?.let { Chat(it.id) }),
                    Triple(Icons.AutoMirrored.Rounded.Chat, R.string.new_chat, NewChat),
                    Triple(Icons.Rounded.GroupAdd, R.string.new_space, NewSpace),
                    Triple(Icons.Rounded.SmartToy, R.string.agents, Agents),
                    Triple(Icons.Rounded.FolderOpen, R.string.files, Files),
                    Triple(Icons.Rounded.PersonOutline, R.string.context, Context),
                    Triple(Icons.Rounded.Widgets, R.string.widgets, Widgets),
                )
                actions.forEach { (icon, label, route) ->
                    if (route != null) SettingsRow(icon, stringResource(label), onClick = { quickActions = false; navigate(route) })
                }
                Spacer(Modifier.height(24.dp))
            }
        }
    }
}

@Composable
private fun tabName(tab: Tab) = stringResource(when (tab) { Tab.Chats -> R.string.chats; Tab.Spaces -> R.string.spaces; Tab.Store -> R.string.miniapp_store; Tab.Files -> R.string.files; Tab.Activity -> R.string.activity })

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TabIcon(tab: Tab, count: Int, selected: Boolean) {
    val icon = when (tab) { Tab.Chats -> ZoenGlyph.Chats; Tab.Spaces -> ZoenGlyph.Spaces; Tab.Store -> ZoenGlyph.Store; Tab.Files -> ZoenGlyph.Folder; Tab.Activity -> ZoenGlyph.Bell }
    BadgedBox(badge = { if (tab in listOf(Tab.Chats, Tab.Activity) && count > 0) Badge { Text(if (count > 99) "99+" else count.toString()) } }) { ZoenIcon(icon, tabName(tab), selected = selected) }
}
