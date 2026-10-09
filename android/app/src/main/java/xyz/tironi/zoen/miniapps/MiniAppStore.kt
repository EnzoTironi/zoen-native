package xyz.tironi.zoen.miniapps

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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.json.JSONArray
import org.json.JSONObject
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.ui.Avatar
import xyz.tironi.zoen.ui.EmptyState
import xyz.tironi.zoen.ui.ScreenBar
import xyz.tironi.zoen.ui.SectionLabel
import xyz.tironi.zoen.ui.appIcon

@Composable
fun MiniAppStoreScreen(model: ZoenViewModel, state: AppState, onOpenItem: (String) -> Unit, back: () -> Unit, onOpenAgent: (String) -> Unit = {}) {
    var specs by remember(model.repository.locale) { mutableStateOf<List<AppSpecDto>>(emptyList()) }
    var query by rememberSaveable { mutableStateOf("") }
    var category by rememberSaveable { mutableStateOf("all") }
    var selected by rememberSaveable { mutableStateOf<String?>(null) }
    LaunchedEffect(model.repository.locale) { specs = model.repository.query { it.appSpecs() } }
    val filtered = specs.filter { spec ->
        (query.isBlank() || (spec.name + " " + spec.description).contains(query, true)) && when (category) {
            "games" -> spec.id in setOf("pet", "maptap")
            "plans" -> spec.id in setOf("recipe", "hike", "countdown", "poll")
            "daily" -> spec.id in setOf("pet", "list", "recipe")
            "agents" -> false
            else -> true
        }
    }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.miniapp_store), back) }) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding), contentPadding = PaddingValues(24.dp, 8.dp, 24.dp, 40.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            item {
                Text(stringResource(R.string.miniapp_store_title), style = MaterialTheme.typography.headlineLarge)
                Text(stringResource(R.string.miniapp_store_detail), color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            item { OutlinedTextField(query, { query = it }, Modifier.fillMaxWidth().testTag("store-search"), singleLine = true, leadingIcon = { Icon(Icons.Rounded.Search, null) }, label = { Text(stringResource(R.string.search)) }) }
            item { LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                items(listOf("all" to R.string.all, "daily" to R.string.miniapp_daily, "games" to R.string.miniapp_games, "plans" to R.string.miniapp_plans, "agents" to R.string.agents)) { (id, label) -> FilterChip(category == id, onClick = { category = id }, label = { Text(stringResource(label)) }) }
            } }
            if (query.isBlank() && category == "all") {
                item { SectionLabel(stringResource(R.string.miniapp_collections)) }
                item { LazyRow(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    items(listOf("daily" to R.string.miniapp_collection_daily, "plans" to R.string.miniapp_collection_plans, "games" to R.string.miniapp_collection_games)) { (filter, title) ->
                        Card(onClick = { category = filter }, Modifier.width(220.dp), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer)) {
                            Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                                Icon(if (filter == "games") Icons.Rounded.SportsEsports else if (filter == "plans") Icons.Rounded.Terrain else Icons.Rounded.FavoriteBorder, null, Modifier.size(36.dp))
                                Text(stringResource(title), style = MaterialTheme.typography.titleLarge)
                            }
                        }
                    }
                } }
                item { SectionLabel(stringResource(R.string.miniapp_featured)) }
            }
            if (category != "agents") items(filtered, key = { it.id }) { spec ->
                val installed = state.items.count { it.app?.appId == spec.id }
                Card(onClick = { selected = spec.id }, modifier = Modifier.fillMaxWidth().testTag("store-app:${spec.id}"), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
                    Row(Modifier.padding(18.dp), horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) {
                        Icon(appIcon(spec.id), null, Modifier.size(40.dp), tint = MaterialTheme.colorScheme.primary)
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text(spec.name, style = MaterialTheme.typography.titleMedium)
                            Text(spec.description, style = MaterialTheme.typography.bodySmall, maxLines = 3, overflow = TextOverflow.Ellipsis)
                            Text(stringResource(if (installed > 0) R.string.miniapp_installed else R.string.miniapp_bundled), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
                        }
                        Icon(Icons.Rounded.ChevronRight, null)
                    }
                }
            }
            if (category == "all" || category == "agents") {
                item { SectionLabel(stringResource(R.string.agents)) }
                items(state.agents.filter { query.isBlank() || (it.persona.name + " " + it.runsOn).contains(query, true) }, key = { it.persona.id }) { agent ->
                    Card(onClick = { onOpenAgent(agent.persona.id) }, Modifier.fillMaxWidth()) {
                        Row(Modifier.padding(18.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            Avatar(agent.persona, size = 44)
                            Column(Modifier.weight(1f)) { Text(agent.persona.name, style = MaterialTheme.typography.titleMedium); Text(agent.runsOn, style = MaterialTheme.typography.bodySmall, maxLines = 2, overflow = TextOverflow.Ellipsis) }
                            Icon(Icons.Rounded.ChevronRight, null)
                        }
                    }
                }
            }
            if (filtered.isEmpty() && category != "agents") item { EmptyState(stringResource(R.string.miniapp_no_results), stringResource(R.string.miniapp_search_hint)) }
            item { Text(stringResource(R.string.miniapp_catalog_notice), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
    }
    selected?.let { appId -> specs.firstOrNull { it.id == appId }?.let { spec -> MiniAppInstallSheet(model, state, spec, onOpenItem, onDismiss = { selected = null }) } }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun MiniAppInstallSheet(model: ZoenViewModel, state: AppState, spec: AppSpecDto, onOpenItem: (String) -> Unit, onDismiss: () -> Unit) {
    val existing = state.items.filter { it.app?.appId == spec.id }
    val spaces = state.spaces.filter { it.privacy != PrivacyDto.PUBLIC && it.members.any { member -> member.isMe } }
    var selectedSpace by rememberSaveable(spec.id) { mutableStateOf(spaces.firstOrNull()?.id) }
    var title by rememberSaveable(spec.id) { mutableStateOf(spec.name) }
    var choices by rememberSaveable(spec.id) { mutableStateOf("") }
    var target by rememberSaveable(spec.id) { mutableLongStateOf(System.currentTimeMillis() + 7 * 86_400_000L) }
    var pickDate by remember { mutableStateOf(false) }
    var pickTime by remember { mutableStateOf(false) }
    val dateState = rememberDatePickerState(initialSelectedDateMillis = target)
    val initialTime = remember(target) { java.time.Instant.ofEpochMilli(target).atZone(java.time.ZoneId.systemDefault()) }
    val timeState = rememberTimePickerState(initialHour = initialTime.hour, initialMinute = initialTime.minute)
    var html by rememberSaveable(spec.id) { mutableStateOf(spec.hasView && spec.id !in xyz.tironi.zoen.ui.nativeMiniApps) }
    var manifest by remember(spec.id) { mutableStateOf<MiniAppManifest?>(null) }
    var busy by remember { mutableStateOf(false) }
    var confirmation by remember { mutableStateOf(false) }
    val preferences = model.repository.preferences
    LaunchedEffect(spec.id) { if (spec.hasView) manifest = model.repository.query { core -> MiniAppManifest.decode(core.readAppResource(spec.resourceUri).manifestJson, spec.id) } }
    val start = spec.tools.firstOrNull { "model" in it.visibility && !it.readOnly }
    val requiredValid = title.isNotBlank() && (spec.id != "poll" || choices.lines().count { it.isNotBlank() } >= 2) && (spec.id != "countdown" || target > System.currentTimeMillis())
    ModalBottomSheet(onDismissRequest = { if (!busy) onDismiss() }) {
        LazyColumn(contentPadding = PaddingValues(24.dp, 8.dp, 24.dp, 40.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            item { Icon(appIcon(spec.id), null, Modifier.size(56.dp), tint = MaterialTheme.colorScheme.primary); Text(spec.name, style = MaterialTheme.typography.headlineMedium); Text(spec.description) }
            if (existing.isNotEmpty()) {
                item { SectionLabel(stringResource(R.string.miniapp_already_in)) }
                items(existing, key = { it.id }) { item -> TextButton(onClick = { onDismiss(); onOpenItem(item.id) }) { Text(item.title + " · " + item.spaceTitle) } }
            }
            item { SectionLabel(stringResource(R.string.miniapp_install_where)) }
            if (spaces.isEmpty()) item { Text(stringResource(R.string.miniapp_create_space_first)) }
            items(spaces, key = { it.id }) { space ->
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    RadioButton(selectedSpace == space.id, onClick = { selectedSpace = space.id }, enabled = !busy)
                    Text(space.title, Modifier.weight(1f))
                }
            }
            item {
                if (spec.id !in setOf("maptap", "hike", "recipe")) OutlinedTextField(title, { title = it.take(if (spec.id == "pet") 18 else 32) }, Modifier.fillMaxWidth(), label = { Text(stringResource(R.string.miniapp_name)) }, singleLine = true)
                if (spec.id in setOf("poll", "list")) OutlinedTextField(choices, { choices = it.take(2_000) }, Modifier.fillMaxWidth(), label = { Text(stringResource(if (spec.id == "poll") R.string.miniapp_options else R.string.miniapp_list_items)) }, minLines = 3)
                if (spec.id == "countdown") OutlinedButton(onClick = { pickDate = true }) {
                    Icon(Icons.Rounded.CalendarMonth, null); Spacer(Modifier.width(8.dp))
                    Text(java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.MEDIUM, java.text.DateFormat.SHORT).format(java.util.Date(target)))
                }
            }
            if (spec.hasView && spec.id in xyz.tironi.zoen.ui.nativeMiniApps) item {
                Text(stringResource(R.string.miniapp_runtime), style = MaterialTheme.typography.titleMedium)
                Row(verticalAlignment = Alignment.CenterVertically) { RadioButton(!html, { html = false }); Text(stringResource(R.string.miniapp_native_view)) }
                Row(verticalAlignment = Alignment.CenterVertically) { RadioButton(html, { html = true }); Text(stringResource(R.string.miniapp_html_view)) }
            }
            item {
                SectionLabel(stringResource(R.string.miniapp_manifest))
                Text(stringResource(R.string.miniapp_install_scope), style = MaterialTheme.typography.bodySmall)
                manifest?.capabilities?.forEach { (capability, purpose) -> Text(capabilityName(capability), style = MaterialTheme.typography.labelLarge); Text(purpose, style = MaterialTheme.typography.bodySmall) }
                manifest?.allowedDomains?.takeIf { it.isNotEmpty() }?.let { Text(stringResource(R.string.miniapp_network, it.joinToString()), style = MaterialTheme.typography.bodySmall) }
                Text(stringResource(R.string.miniapp_capabilities_optional), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            item {
                SectionLabel(stringResource(R.string.miniapp_tools))
                spec.tools.filter { "app" in it.visibility }.forEach { tool -> Text(tool.title + " · " + tool.actionLabel, style = MaterialTheme.typography.bodySmall) }
            }
            item { Button(onClick = { confirmation = true }, enabled = selectedSpace != null && requiredValid && start != null && !busy, modifier = Modifier.fillMaxWidth().testTag("store-install")) { if (busy) CircularProgressIndicator(Modifier.size(20.dp)) else Text(stringResource(R.string.miniapp_install)) } }
        }
    }
    if (pickDate) DatePickerDialog(onDismissRequest = { pickDate = false }, confirmButton = {
        TextButton(onClick = { pickDate = false; pickTime = true }, enabled = dateState.selectedDateMillis != null) { Text(stringResource(R.string.next_step)) }
    }, dismissButton = { TextButton(onClick = { pickDate = false }) { Text(stringResource(R.string.cancel)) } }) { DatePicker(dateState) }
    if (pickTime) AlertDialog(onDismissRequest = { pickTime = false }, title = { Text(stringResource(R.string.miniapp_target)) }, text = { TimePicker(timeState) }, confirmButton = {
        TextButton(onClick = {
            val date = java.time.Instant.ofEpochMilli(checkNotNull(dateState.selectedDateMillis)).atZone(java.time.ZoneOffset.UTC).toLocalDate()
            target = date.atTime(timeState.hour, timeState.minute).atZone(java.time.ZoneId.systemDefault()).toInstant().toEpochMilli()
            pickTime = false
        }) { Text(stringResource(R.string.done)) }
    }, dismissButton = { TextButton(onClick = { pickTime = false }) { Text(stringResource(R.string.cancel)) } })
    if (confirmation) AlertDialog(onDismissRequest = { confirmation = false }, title = { Text(stringResource(R.string.miniapp_install_confirm, spec.name)) }, text = { Text(stringResource(R.string.miniapp_install_scope)) }, confirmButton = {
        TextButton(onClick = {
            confirmation = false; busy = true
            model.launch {
                try {
                    val args = when (spec.id) {
                        "pet" -> JSONObject().put("name", title)
                        "poll" -> JSONObject().put("question", title).put("options", JSONArray(choices.lines().map(String::trim).filter(String::isNotEmpty).take(12)))
                        "list" -> JSONObject().put("title", title).put("items", JSONArray(choices.lines().map(String::trim).filter(String::isNotEmpty).take(50)))
                        "countdown" -> JSONObject().put("title", title).put("target_ms", target)
                        else -> JSONObject()
                    }
                    val item = model.repository.change { it.installApp(checkNotNull(selectedSpace), spec.id, args.toString()) }
                    preferences.edit().putBoolean("miniapps.html:${spec.id}", html).apply()
                    MiniAppPins(preferences).show(item.id)
                    MiniAppPins(preferences).show(item.id, MiniAppPins.chat(item.spaceId))
                    onDismiss(); onOpenItem(item.id)
                } finally { busy = false }
            }
        }) { Text(stringResource(R.string.miniapp_install)) }
    }, dismissButton = { TextButton(onClick = { confirmation = false }) { Text(stringResource(R.string.cancel)) } })
}
