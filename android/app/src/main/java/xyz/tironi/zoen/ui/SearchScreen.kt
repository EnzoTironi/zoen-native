package xyz.tironi.zoen.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Chat
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.navigation3.runtime.NavKey
import java.text.DateFormat
import java.text.Normalizer
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.AppSpecDto
import xyz.tironi.zoen.core.Persona
import xyz.tironi.zoen.core.PersonaKind
import xyz.tironi.zoen.core.UniversalHit
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.miniapps.MiniAppStoreScreen

internal enum class SearchScope(val label: Int, val kind: String?) {
    All(R.string.search_scope_all, null),
    Messages(R.string.search_scope_messages, "message"),
    People(R.string.search_scope_people, "person"),
    Agents(R.string.search_scope_agents, "agent"),
    Spaces(R.string.search_scope_spaces, "space"),
    Apps(R.string.search_scope_apps, "app"),
    Files(R.string.search_scope_files, "item");

    val kinds: List<String> get() = kind?.let(::listOf).orEmpty()
    val limit: UInt get() = if (this == All) 4u else 40u
}

internal fun updatedSearchRecents(previous: List<String>, query: String): List<String> {
    val clean = query.trim()
    if (clean.codePointCount(0, clean.length) < 2) return previous
    return (listOf(clean) + previous.filterNot { it.equals(clean, ignoreCase = true) }).take(8)
}

internal fun decodeSearchRecents(encoded: String?): List<String> = try {
    Json.decodeFromString<List<String>>(encoded ?: "[]").filter { it.isNotBlank() }.take(8)
} catch (_: IllegalArgumentException) {
    emptyList()
}

internal fun catalogSearchMatches(query: String, spec: AppSpecDto): Boolean {
    fun fold(value: String) = Normalizer.normalize(value, Normalizer.Form.NFD)
        .replace(Regex("\\p{M}+"), "").lowercase(Locale.ROOT)
    val word = Regex("[\\p{L}\\p{N}]+")
    val terms = word.findAll(fold(query)).map { it.value }.toList()
    if (terms.isEmpty()) return false
    val words = word.findAll(fold(spec.name + " " + spec.description + " " + spec.id)).map { it.value }.toList()
    return terms.all { term -> words.any { it.startsWith(term) } }
}

internal fun highlightedSearchText(marked: String, color: Color): AnnotatedString = buildAnnotatedString {
    var cursor = 0
    while (cursor < marked.length) {
        val open = marked.indexOf("[[", cursor)
        if (open < 0) { append(marked.substring(cursor)); break }
        val close = marked.indexOf("]]", open + 2)
        if (close < 0) { append(marked.substring(cursor)); break }
        append(marked.substring(cursor, open))
        withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = color)) {
            append(marked.substring(open + 2, close))
        }
        cursor = close + 2
    }
}

private data class SearchOutcome(
    val query: String = "",
    val scope: SearchScope = SearchScope.All,
    val hits: List<UniversalHit> = emptyList(),
    val catalog: List<AppSpecDto> = emptyList(),
    val error: String? = null,
)

@Composable
fun SearchScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit, back: () -> Unit) {
    var query by rememberSaveable { mutableStateOf("") }
    var scopeName by rememberSaveable { mutableStateOf(SearchScope.All.name) }
    val scope = SearchScope.valueOf(scopeName)
    val trimmedQuery = query.trim()
    val preferences = model.repository.preferences
    var recents by remember {
        mutableStateOf(decodeSearchRecents(preferences.getString("search.recents", null)))
    }
    var storeApp by rememberSaveable { mutableStateOf<String?>(null) }
    var storeOpen by rememberSaveable { mutableStateOf(false) }
    var retry by remember { mutableIntStateOf(0) }
    var asking by remember { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    val specsState by produceState<List<AppSpecDto>>(emptyList(), model.repository.locale) {
        value = model.repository.query { it.appSpecs() }
    }
    val specs = specsState
    val outcomeState by produceState(SearchOutcome(), trimmedQuery, scope, state.revision, specs, retry) {
        if (trimmedQuery.isEmpty()) value = SearchOutcome(scope = scope)
        else {
            delay(150)
            value = try {
                val hits = model.repository.query { it.universalSearch(trimmedQuery, scope.kinds, scope.limit).hits }
                val catalog = if (scope == SearchScope.All || scope == SearchScope.Apps)
                    specs.filter { catalogSearchMatches(trimmedQuery, it) }.let { if (scope == SearchScope.All) it.take(3) else it }
                else emptyList()
                SearchOutcome(trimmedQuery, scope, hits, catalog)
            } catch (error: Exception) {
                if (error is CancellationException) throw error
                SearchOutcome(trimmedQuery, scope, error = error.message ?: error.javaClass.simpleName)
            }
        }
    }
    val outcome = outcomeState
    val current = outcome.query == trimmedQuery && outcome.scope == scope
    val loading = trimmedQuery.isNotEmpty() && !current
    val hits = if (current) outcome.hits else emptyList()
    val catalog = if (current) outcome.catalog else emptyList()
    val error = if (current) outcome.error else null
    val people = state.spaces.flatMap { it.members }.filterNot { it.isMe }.distinctBy { it.id }
    val suggested = people.filter { it.kind == PersonaKind.PERSON }.take(4) +
        people.filter { it.kind == PersonaKind.AGENT }.take(4)
    val featured = specs.take(3)
    val recentQueries = recents
    val suggestions = (state.spaces.map { it.title } + suggested.map { it.name } + specs.map { it.name })
        .filter { it.isNotBlank() && !it.equals(trimmedQuery, ignoreCase = true) }.distinct().take(3)
    fun saveQuery() {
        recents = updatedSearchRecents(recents, query)
        preferences.edit().putString("search.recents", Json.encodeToString(recents)).apply()
    }
    fun leave() { saveQuery(); keyboard?.hide(); back() }
    fun openPerson(person: Persona) {
        saveQuery(); keyboard?.hide()
        val route = if (person.kind == PersonaKind.AGENT) Agent(person.id)
        else state.spaces.firstOrNull { it.counterpart?.id == person.id }
            ?.let { Chat(it.id) }
            ?: state.spaces.firstOrNull { space -> space.members.any { it.id == person.id } }?.let { Chat(it.id) }
            ?: Person(person.id)
        navigate(route)
    }
    fun openHit(hit: UniversalHit) {
        saveQuery(); keyboard?.hide()
        if (hit.kind == "person" && hit.persona != null) { openPerson(hit.persona!!); return }
        val route: NavKey? = when (hit.kind) {
            "app", "item" -> Item(hit.refId)
            "space" -> Chat(hit.refId)
            "agent" -> Agent(hit.refId)
            "person" -> Person(hit.refId)
            "message" -> hit.spaceId?.let { Chat(it, hit.refId) }
            else -> null
        }
        route?.let(navigate)
    }
    fun openCatalog(spec: AppSpecDto) {
        saveQuery(); keyboard?.hide()
        storeApp = spec.id; storeOpen = true
    }
    BackHandler(onBack = ::leave)
    LaunchedEffect(Unit) { focus.requestFocus() }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.search), ::leave) }) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).imePadding()) {
            OutlinedTextField(query, { query = it }, Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp)
                .focusRequester(focus).testTag("search"),
                placeholder = { Text(stringResource(R.string.search_hint)) },
                leadingIcon = { Icon(Icons.Rounded.Search, null) }, singleLine = true,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, imeAction = ImeAction.Search),
                keyboardActions = KeyboardActions(onSearch = { saveQuery(); keyboard?.hide() }),
                trailingIcon = {
                    if (query.isNotEmpty()) IconButton(onClick = { query = ""; focus.requestFocus() }) {
                        Icon(Icons.Rounded.Close, stringResource(R.string.search_clear_query))
                    }
                },
            )
            LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                items(SearchScope.entries, key = { it.name }) { choice ->
                    FilterChip(selected = scope == choice, onClick = { scopeName = choice.name },
                        label = { Text(stringResource(choice.label)) }, modifier = Modifier.testTag("search-scope:" + choice.name.lowercase(Locale.ROOT)))
                }
            }
            if (loading) LinearProgressIndicator(Modifier.fillMaxWidth().padding(top = 8.dp))
            LazyColumn(Modifier.weight(1f).testTag("search-results"), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (trimmedQuery.isEmpty()) {
                    if (recentQueries.isNotEmpty()) {
                        item("recents-header") { SearchSectionTitle(stringResource(R.string.search_recent), stringResource(R.string.search_clear_recent)) {
                            recents = emptyList(); preferences.edit().remove("search.recents").apply()
                        } }
                        items(recentQueries, key = { "recent:" + it }) { recent ->
                            ListItem(headlineContent = { Text(recent) }, leadingContent = { Icon(Icons.Rounded.History, null) },
                                trailingContent = { Icon(Icons.Rounded.NorthWest, null) },
                                modifier = Modifier.testTag("search-recent:" + recent).clickable { query = recent })
                        }
                    }
                    if (suggested.isNotEmpty()) {
                        item("suggested-header") { SearchSectionTitle(stringResource(R.string.search_suggested)) }
                        item("suggested-people") {
                            LazyRow(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                                items(suggested, key = { it.id }) { person ->
                                    Column(Modifier.width(76.dp).testTag("search-suggested:" + person.id).clickable { openPerson(person) },
                                        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                                        Avatar(person, size = 56)
                                        Text(person.name, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.labelMedium)
                                    }
                                }
                            }
                        }
                    }
                    if (featured.isNotEmpty()) {
                        item("featured-header") { SearchSectionTitle(stringResource(R.string.search_apps_for_you)) }
                        items(featured, key = { "featured:" + it.id }) { spec -> SearchCatalogRow(spec, state, { openCatalog(spec) }) }
                    }
                    if (suggested.isEmpty() && featured.isEmpty() && recentQueries.isEmpty())
                        item("empty") { EmptyState(stringResource(R.string.search_empty), stringResource(R.string.search_empty_detail)) }
                } else if (error != null) {
                    item("error") {
                        EmptyState(stringResource(R.string.search_failed), error) {
                            OutlinedButton(onClick = { retry++ }) { Text(stringResource(R.string.retry)) }
                        }
                    }
                } else if (!loading && hits.isEmpty() && catalog.isEmpty()) {
                    item("no-results") { Text(stringResource(R.string.search_not_found), style = MaterialTheme.typography.titleLarge) }
                    if (suggestions.isNotEmpty()) {
                        item("suggestions-label") { Text(stringResource(R.string.search_try_suggestion), color = MaterialTheme.colorScheme.onSurfaceVariant) }
                        item("suggestions") {
                            LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                items(suggestions) { suggestion -> SuggestionChip(onClick = { query = suggestion }, label = { Text(suggestion) }) }
                            }
                        }
                    }
                    state.zoenChat?.let { chat ->
                        item("ask-zoen") {
                            Card(onClick = {
                                if (!asking) {
                                    val question = query.trim()
                                    saveQuery(); keyboard?.hide(); asking = true
                                    model.launch {
                                        try { model.send(chat.id, question) }
                                        finally { asking = false }
                                    }
                                    navigate(Chat(chat.id))
                                }
                            }, enabled = !asking && !state.keyMissing, modifier = Modifier.fillMaxWidth().testTag("search-ask-zoen"),
                                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer)) {
                                Row(Modifier.padding(20.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                                    if (asking) CircularProgressIndicator(Modifier.size(32.dp))
                                    else Icon(Icons.Rounded.AutoAwesome, null, Modifier.size(32.dp))
                                    Column(Modifier.weight(1f)) {
                                        Text(stringResource(R.string.ask_zoen), style = MaterialTheme.typography.titleMedium)
                                        Text(query, maxLines = 2, overflow = TextOverflow.Ellipsis)
                                    }
                                    Icon(Icons.Rounded.ChevronRight, null)
                                }
                            }
                        }
                    }
                } else {
                    SearchScope.entries.filterNot { it == SearchScope.All }.forEach { group ->
                        val groupHits = hits.filter { it.kind == group.kind }
                        val groupCatalog = if (group == SearchScope.Apps) catalog else emptyList()
                        if (groupHits.isNotEmpty() || groupCatalog.isNotEmpty()) {
                            item("header:" + group.name) {
                                SearchSectionTitle(stringResource(group.label), if (scope == SearchScope.All) stringResource(R.string.search_see_all) else null) {
                                    scopeName = group.name
                                }
                            }
                            items(groupHits, key = { it.kind + ":" + it.refId }) { hit -> SearchHitRow(hit, state, { openHit(hit) }) }
                            items(groupCatalog, key = { "catalog:" + it.id }) { spec -> SearchCatalogRow(spec, state, { openCatalog(spec) }) }
                        }
                    }
                }
            }
        }
    }
    if (storeOpen) Dialog(onDismissRequest = { storeOpen = false }, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        MiniAppStoreScreen(model, state,
            onOpenItem = { storeOpen = false; navigate(Item(it)) },
            back = { storeOpen = false },
            onOpenAgent = { storeOpen = false; navigate(Agent(it)) },
            initialAppId = storeApp,
        )
    }
}

@Composable
private fun SearchSectionTitle(title: String, action: String? = null, onAction: () -> Unit = {}) {
    Row(Modifier.fillMaxWidth().padding(top = 12.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(title, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
        if (action != null) TextButton(onClick = onAction) { Text(action) }
    }
}

@Composable
private fun SearchHitRow(hit: UniversalHit, state: AppState, onClick: () -> Unit) {
    val title = hit.titleSnippet.ifEmpty { hit.title }
    val snippet = if (hit.kind in listOf("person", "agent") && hit.snippet.isEmpty())
        hit.persona?.bio?.takeIf { it.isNotBlank() } ?: hit.persona?.let { "@" + it.handle }.orEmpty()
    else hit.snippet
    Card(onClick = onClick, modifier = Modifier.fillMaxWidth().testTag("search-result:" + hit.refId),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
        Row(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.Top) {
            val space = if (hit.kind == "space") state.spaces.firstOrNull { it.id == hit.refId } else null
            when {
                hit.persona != null -> Avatar(hit.persona, size = 40)
                space != null -> SpaceAvatar(space, size = 40)
                else -> Icon(if (hit.kind == "message") Icons.AutoMirrored.Rounded.Chat else appIcon(hit.kind), null,
                    Modifier.padding(6.dp).size(28.dp), tint = MaterialTheme.colorScheme.primary)
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.Top) {
                    Text(highlightedSearchText(title, MaterialTheme.colorScheme.primary), Modifier.weight(1f),
                        style = MaterialTheme.typography.titleSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    if (hit.atMs > 0 && hit.kind in listOf("message", "item", "app"))
                        Text(DateFormat.getDateInstance(DateFormat.SHORT).format(Date(hit.atMs)),
                            style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                if (snippet.isNotEmpty()) Text(highlightedSearchText(snippet, MaterialTheme.colorScheme.primary),
                    maxLines = 3, overflow = TextOverflow.Ellipsis, color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodyMedium)
                hit.spaceTitle?.takeIf { hit.kind != "space" }?.let { context ->
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        Icon(Icons.Rounded.Groups, null, Modifier.size(14.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                        Text(context, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
        }
    }
}

@Composable
private fun SearchCatalogRow(spec: AppSpecDto, state: AppState, onClick: () -> Unit) {
    val installed = state.items.any { it.app?.appId == spec.id }
    Card(onClick = onClick, modifier = Modifier.fillMaxWidth().testTag("search-catalog:" + spec.id),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
        Row(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.Top) {
            Icon(appIcon(spec.id), null, Modifier.size(40.dp), tint = MaterialTheme.colorScheme.primary)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(spec.name, style = MaterialTheme.typography.titleSmall)
                Text(spec.description, maxLines = 3, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    Icon(Icons.Rounded.Storefront, null, Modifier.size(14.dp), tint = MaterialTheme.colorScheme.primary)
                    Text(stringResource(if (installed) R.string.search_catalog_installed else R.string.search_catalog_available),
                        style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                }
            }
            Icon(Icons.Rounded.ChevronRight, null)
        }
    }
}
