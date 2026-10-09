package xyz.tironi.zoen.ui

import android.net.Uri
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Chat
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.ui.NavDisplay
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ZoenApp(model: ZoenViewModel, deepLink: String?, consumed: () -> Unit) {
    val state by model.state.collectAsStateWithLifecycle()
    val snackbar = remember { SnackbarHostState() }
    val undo = stringResource(R.string.undo)
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
            Onboarding(model, Modifier.padding(padding))
        }
        return
    }

    key(state.demo, state.me?.id, LocalConfiguration.current.locales[0].language) {
        var tab by rememberSaveable { mutableStateOf(Tab.Chats) }
        val chats = rememberNavBackStack(Home)
        val spaces = rememberNavBackStack(Home)
        val activity = rememberNavBackStack(Home)
        val stack = when (tab) { Tab.Chats -> chats; Tab.Spaces -> spaces; Tab.Activity -> activity }
        val navigate: (NavKey) -> Unit = { key -> if (stack.lastOrNull() != key) stack.add(key) }
        val back: () -> Unit = { if (stack.size > 1) stack.removeLastOrNull() }
        var quickActions by rememberSaveable { mutableStateOf(false) }
        LaunchedEffect(deepLink, state.me) {
            val uri = deepLink?.let(Uri::parse) ?: return@LaunchedEffect
            if (uri.scheme == "zoen") when (uri.host) {
                "app", "item" -> uri.lastPathSegment?.let { navigate(Item(it)) }
                "chat" -> uri.lastPathSegment?.let { navigate(Chat(it)) }
                "join" -> navigate(Join(uri.lastPathSegment.orEmpty()))
            }
            consumed()
        }
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val rail = maxWidth >= 600.dp
            val split = maxWidth >= 840.dp
            Row {
                if (rail) NavigationRail(modifier = Modifier.fillMaxHeight(), header = {
                    ZoenMascot(Modifier.padding(top = 12.dp).size(64.dp))
                    Spacer(Modifier.height(16.dp))
                    FloatingActionButton(onClick = { quickActions = true }) { Icon(Icons.Rounded.Add, stringResource(R.string.more)) }
                }) {
                    Spacer(Modifier.height(24.dp))
                    Tab.entries.forEach { target ->
                        NavigationRailItem(selected = tab == target, onClick = { tab = target }, icon = { TabIcon(target, state.pending) }, label = { Text(tabName(target)) })
                    }
                    Spacer(Modifier.weight(1f))
                    IconButton(onClick = { navigate(Search) }) { Icon(Icons.Rounded.Search, stringResource(R.string.search)) }
                    IconButton(onClick = { navigate(Context) }) { Icon(Icons.Rounded.Person, stringResource(R.string.context)) }
                }
                Scaffold(
                    modifier = Modifier.weight(1f),
                    snackbarHost = { SnackbarHost(snackbar) },
                    bottomBar = {
                        if (!rail && stack.size == 1) NavigationBar {
                            Tab.entries.forEach { target ->
                                NavigationBarItem(selected = tab == target, onClick = { tab = target }, icon = { TabIcon(target, state.pending) }, label = { Text(tabName(target)) })
                            }
                        }
                    },
                    floatingActionButton = {
                        if (!rail && stack.size == 1) ExtendedFloatingActionButton(onClick = { quickActions = true }, icon = { Icon(Icons.Rounded.Add, null) }, text = { Text(stringResource(R.string.ask_zoen)) })
                    },
                    contentWindowInsets = WindowInsets(0, 0, 0, 0),
                ) { padding ->
                    NavDisplay(
                        modifier = Modifier.fillMaxSize().padding(padding), backStack = stack, onBack = back,
                        entryProvider = entryProvider {
                            entry<Home> {
                                when (tab) {
                                    Tab.Chats -> ConversationsScreen(model, state, navigate, split)
                                    Tab.Spaces -> SpacesScreen(model, state, navigate)
                                    Tab.Activity -> ActivityScreen(model, state, navigate)
                                }
                            }
                            entry<Chat> { ChatScreen(model, state, it.id, navigate, back, it.message) }
                            entry<Item> { ItemScreen(model, state, it.id, navigate, back) }
                            entry<Agent> { AgentScreen(model, state, it.id, back) }
                            entry<Person> { ProfileScreen(model, state, it.id, navigate, back) }
                            entry<Participants> { ParticipantsScreen(model, state, it.id, navigate, back) }
                            entry<Thread> { ThreadScreen(model, state, it.space, it.root, navigate, back) }
                            entry<Files> { FilesScreen(model, state, navigate, back) }
                            entry<Agents> { AgentsScreen(state, navigate, back) }
                            entry<Context> { ContextScreen(model, state, navigate, back) }
                            entry<Search> { SearchScreen(model, state, navigate, back) }
                            entry<History> { HistoryScreen(model, state, back) }
                            entry<Permissions> { PermissionsScreen(model, state, back) }
                            entry<NewChat> { NewChatScreen(model, state, navigate, back) }
                            entry<NewSpace> { NewSpaceScreen(model, navigate, back) }
                            entry<Join> { JoinScreen(model, it.code, navigate, back) }
                            entry<Request> { RequestScreen(model, state, it.id, navigate, back) }
                        },
                    )
                }
            }
        }
        if (quickActions) ModalBottomSheet(onDismissRequest = { quickActions = false }) {
            Text(stringResource(R.string.app_name), Modifier.padding(horizontal = 24.dp, vertical = 8.dp), style = MaterialTheme.typography.headlineMedium)
            val actions = listOf(
                Triple(Icons.Rounded.AutoAwesome, R.string.ask_zoen, state.zoenChat?.let { Chat(it.id) }),
                Triple(Icons.AutoMirrored.Rounded.Chat, R.string.new_chat, NewChat),
                Triple(Icons.Rounded.GroupAdd, R.string.new_space, NewSpace),
                Triple(Icons.Rounded.SmartToy, R.string.agents, Agents),
                Triple(Icons.Rounded.FolderOpen, R.string.files, Files),
                Triple(Icons.Rounded.PersonOutline, R.string.context, Context),
            )
            actions.forEach { (icon, label, route) ->
                if (route != null) SettingsRow(icon, stringResource(label), onClick = { quickActions = false; navigate(route) })
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}

@Composable
private fun tabName(tab: Tab) = stringResource(when (tab) { Tab.Chats -> R.string.chats; Tab.Spaces -> R.string.spaces; Tab.Activity -> R.string.activity })

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TabIcon(tab: Tab, count: Int) {
    val icon = when (tab) { Tab.Chats -> Icons.AutoMirrored.Rounded.Chat; Tab.Spaces -> Icons.Rounded.Groups; Tab.Activity -> Icons.Rounded.NotificationsNone }
    BadgedBox(badge = { if (tab == Tab.Activity && count > 0) Badge { Text(count.toString()) } }) { Icon(icon, tabName(tab)) }
}
