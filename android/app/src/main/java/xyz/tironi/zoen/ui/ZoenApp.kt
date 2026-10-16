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
                GlassButton(onClick = { model.boot(false) }) { Text(stringResource(R.string.retry)) }
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
        val stack = when (tab) { Tab.Chats -> chats; Tab.Store -> store }
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
            fun openAgent(target: String) = model.launch {
                navigate(Chat(model.repository.network { it.openAgentLink(target) }))
            }
            if (uri.scheme == "zoen") when (uri.host) {
                "app", "item" -> uri.lastPathSegment?.let { navigate(Item(it)) }
                "chat" -> uri.lastPathSegment?.let { navigate(Chat(it)) }
                "join" -> navigate(Join(uri.lastPathSegment.orEmpty()))
                "request" -> uri.lastPathSegment?.let { navigate(Request(it)) }
                "widgets" -> navigate(Widgets)
                "agent" -> uri.lastPathSegment?.let { openAgent(it) }
                "friend" -> uri.lastPathSegment?.let { handle ->
                    model.launch {
                        val friend = model.repository.network { core -> core.findPeople(handle).firstOrNull { it.handle.equals(handle, true) } }
                        friend?.let { navigate(Chat(model.repository.change { core -> core.startDirect(it.id) })) }
                    }
                }
            }
            if (uri.scheme == "https" && (uri.host == "tryzoen.com" || uri.host == "zoen.app" || uri.host?.endsWith(".tryzoen.com") == true)) {
                if (uri.pathSegments.firstOrNull() == "j") navigate(Join(uri.lastPathSegment.orEmpty()))
                else if (uri.pathSegments.firstOrNull() == "a") uri.lastPathSegment?.let { openAgent(it) }
                else uri.pathSegments.firstOrNull()?.takeIf { it.startsWith("@") }?.removePrefix("@")?.let { handle ->
                    model.launch {
                        val friend = model.repository.network { core -> core.findPeople(handle).firstOrNull { it.handle.equals(handle, true) } }
                        friend?.let { navigate(Chat(model.repository.change { core -> core.startDirect(it.id) })) }
                    }
                }
            }
            consumed()
        }
        ZoenGlassBackdrop { BoxWithConstraints(Modifier.fillMaxSize()) {
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
                        if (!rail && stack.size == 1) AppleBottomBar(tab, unread, { target ->
                            if (tab != target) haptics.perform(ZoenFeedback.Selection)
                            tab = target
                        }, { navigate(Search) }) {
                            PlusVoiceButton({ quickActions = true }, { voiceStart++ }, { voiceRelease++ }, { voiceLock++ }, { voiceCancel++ }, compact = true)
                        }
                    },
                    contentWindowInsets = WindowInsets(0, 0, 0, 0),
                ) { padding ->
                    CompositionLocalProvider(LocalSnackbarClearance provides snackbarClearance) { NavDisplay(
                        modifier = Modifier.fillMaxSize().padding(top = padding.calculateTopPadding()).zoenGlassSource(), backStack = stack, onBack = back,
                        entryProvider = entryProvider {
                            entry<Home> {
                                when (tab) {
                                    Tab.Chats -> ConversationsScreen(model, state, navigate, split)
                                    Tab.Store -> MiniAppStoreScreen(model, state, { navigate(Item(it)) }, { tab = Tab.Chats }, { navigate(Agent(it)) })
                                }
                            }
                            entry<Chat> { ChatScreen(model, state, it.id, navigate, back, it.message) }
                            entry<Item> { ItemScreen(model, state, it.id, navigate, back) }
                            entry<Agent> { AgentScreen(model, state, it.id, back, navigate) }
                            entry<Person> { ProfileScreen(model, state, it.id, navigate, back) }
                            entry<Participants> { ParticipantsScreen(model, state, it.id, navigate, back) }
                            entry<Thread> { ThreadScreen(model, state, it.space, it.root, navigate, back) }
                            entry<Activity> { ActivityScreen(model, state, navigate, back) }
                            entry<Files> { FilesScreen(model, state, navigate, back) }
                            entry<Folder> { FilesScreen(model, state, navigate, back, it.space) }
                            entry<Agents> { AgentsScreen(model, state, navigate, back) }
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
                            entry<SettingsRoute> { SettingsScreen(model, state, it.page, navigate, back) { while (stack.size > 1 && stack.lastOrNull() is SettingsRoute) stack.removeLastOrNull() } }
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
        if (quickActions) {
            AppleActionFan(onDismiss = { quickActions = false }, actions = listOf(
                FanAction(Icons.Rounded.AutoAwesome, stringResource(R.string.ask_zoen)) { state.zoenChat?.let { navigate(Chat(it.id)) } },
                FanAction(Icons.Rounded.Group, stringResource(R.string.agents)) { navigate(Agents) },
                FanAction(Icons.Rounded.FolderOpen, stringResource(R.string.files)) { navigate(Files) },
                FanAction(Icons.Rounded.PersonOutline, stringResource(R.string.context)) { navigate(Context) },
            ))
        }
        }
    }
}

@Composable
private fun tabName(tab: Tab) = stringResource(when (tab) { Tab.Chats -> R.string.chats; Tab.Store -> R.string.miniapp_store })

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TabIcon(tab: Tab, count: Int, selected: Boolean) {
    val icon = when (tab) { Tab.Chats -> ZoenGlyph.Chats; Tab.Store -> ZoenGlyph.Store }
    BadgedBox(badge = { if (tab == Tab.Chats && count > 0) Badge { Text(if (count > 99) "99+" else count.toString()) } }) { ZoenIcon(icon, tabName(tab), selected = selected) }
}
