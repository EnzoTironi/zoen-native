package xyz.tironi.zoen.ui

import android.content.Intent
import android.provider.Settings
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState

@Composable
fun AgentsScreen(state: AppState, navigate: (NavKey) -> Unit, back: () -> Unit) {
    Scaffold(topBar = { ScreenBar(stringResource(R.string.agents), back) }) { padding ->
        LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (state.agents.isEmpty()) item { EmptyState(stringResource(R.string.no_agents), stringResource(R.string.no_agents_detail)) }
            items(state.agents, key = { it.persona.id }) { agent ->
                Card(onClick = { navigate(Agent(agent.persona.id)) }, colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
                    Column(Modifier.fillMaxWidth().padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) { Avatar(agent.persona, size = 54); Column { Text(agent.persona.name, style = MaterialTheme.typography.titleLarge); Text(agent.persona.bio, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) } }
                        if (agent.budgetLimitCents != null) {
                            val ratio = (agent.budgetSpentCents ?: 0).toFloat() / agent.budgetLimitCents!!.coerceAtLeast(1)
                            LinearProgressIndicator(progress = { ratio.coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
                        } else Text(stringResource(R.string.agent_owned, agent.persona.ownerName.orEmpty()), style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
        }
    }
}

fun trustLabel(level: TrustLevelDto): Int = when (level) {
    TrustLevelDto.LISTEN -> R.string.listen
    TrustLevelDto.SUGGEST -> R.string.suggest
    TrustLevelDto.ACT -> R.string.act
    TrustLevelDto.AUTONOMOUS -> R.string.autonomous
}

@Composable
fun AgentScreen(model: ZoenViewModel, state: AppState, id: String, back: () -> Unit) {
    val profile = state.agents.firstOrNull { it.persona.id == id }
    var trustSpace by remember { mutableStateOf<AgentSpaceTrust?>(null) }
    val activity by produceState<List<AgentActivityDto>>(emptyList(), id, state.revision) { value = model.repository.query { it.agentActivity(id) } }
    if (profile == null) return
    Scaffold(topBar = { ScreenBar(profile.persona.name, back) }) { padding ->
        LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            item { Avatar(profile.persona, size = 96); Text(profile.persona.name, style = MaterialTheme.typography.headlineMedium); Text(profile.persona.bio, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            if (profile.persona.handle == "zoen") item { Text(stringResource(R.string.local_planner_detail), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            if (profile.budgetLimitCents != null) item {
                Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer)) {
                    Column(Modifier.fillMaxWidth().padding(20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Text(stringResource(R.string.monthly_budget), style = MaterialTheme.typography.labelMedium)
                        Text(formatMoney(profile.budgetSpentCents ?: 0, model.repository.locale) + " / " + formatMoney(profile.budgetLimitCents!!, model.repository.locale), style = MaterialTheme.typography.titleLarge)
                        LinearProgressIndicator(progress = { ((profile.budgetSpentCents ?: 0).toFloat() / profile.budgetLimitCents!!.coerceAtLeast(1)).coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
                        if (profile.persona.isMine) TextButton(onClick = { model.launch { model.repository.change { it.raiseBudget(id, 1000) } } }) { Text(stringResource(R.string.raise_budget)) }
                    }
                }
            } else item { Text(stringResource(R.string.agent_owned, profile.persona.ownerName.orEmpty())) }
            item { SectionLabel(stringResource(R.string.trust)) }
            items(profile.spaces, key = { it.spaceId }) { space ->
                SettingsRow(Icons.Rounded.Tune, space.spaceTitle, stringResource(trustLabel(space.level)), onClick = { if (profile.persona.isMine) trustSpace = space }, trailing = { if (profile.persona.isMine) Icon(Icons.Rounded.Edit, stringResource(R.string.edit)) else Icon(Icons.Rounded.Lock, stringResource(R.string.read_only)) })
            }
            if (activity.isNotEmpty()) item { SectionLabel(stringResource(R.string.agent_activity)) }
            items(activity) { action -> Column { Text(action.label, style = MaterialTheme.typography.titleSmall); Text(action.detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) } }
        }
    }
    trustSpace?.let { space -> AlertDialog(onDismissRequest = { trustSpace = null }, title = { Text(stringResource(R.string.trust)) }, text = {
        Column { TrustLevelDto.entries.forEach { level ->
            Row(verticalAlignment = Alignment.CenterVertically) { RadioButton(space.level == level, onClick = { trustSpace = null; model.launch { model.repository.change { it.setTrust(id, space.spaceId, level) } } }); Text(stringResource(trustLabel(level))) }
        } }
    }, confirmButton = { TextButton(onClick = { trustSpace = null }) { Text(stringResource(R.string.cancel)) } }) }
}

@Composable
fun ContextScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit, back: () -> Unit) {
    val context = LocalContext.current
    var reset by remember { mutableStateOf(false) }
    var signOut by remember { mutableStateOf(false) }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.context), back) }) { padding ->
        LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                Avatar(state.me, size = 72, onClick = { state.me?.let { navigate(Person(it.id)) } })
                Column { Text(state.me?.name.orEmpty(), style = MaterialTheme.typography.headlineMedium); Text("@${state.me?.handle.orEmpty()}", color = MaterialTheme.colorScheme.onSurfaceVariant) }
            } }
            if (state.demo) item { Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.tertiaryContainer)) { Column(Modifier.padding(20.dp)) { Text(stringResource(R.string.demo), style = MaterialTheme.typography.titleMedium); Text(stringResource(R.string.demo_detail)) } } }
            if (state.keyMissing) item { Text(stringResource(R.string.key_missing), color = MaterialTheme.colorScheme.error) }
            if (state.account != null) item { Text(stringResource(when (state.connection.state) { "online" -> R.string.online; "connecting" -> R.string.connecting; else -> R.string.offline }), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            item { SectionLabel(stringResource(R.string.settings)) }
            item { SettingsRow(Icons.Rounded.PersonOutline, stringResource(R.string.profile), onClick = { state.me?.let { navigate(Person(it.id)) } }) }
            item { SettingsRow(Icons.Rounded.VerifiedUser, stringResource(R.string.signed_log), onClick = { navigate(History) }) }
            item { SettingsRow(Icons.Rounded.Security, stringResource(R.string.permissions), onClick = { navigate(Permissions) }) }
            item { SettingsRow(Icons.Rounded.NotificationsNone, stringResource(R.string.notification_settings), onClick = { context.startActivity(Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)) }) }
            item { SettingsRow(Icons.Rounded.SmartToy, stringResource(R.string.agents), onClick = { navigate(Agents) }) }
            item { SectionLabel(stringResource(R.string.about)); Text(stringResource(R.string.local_planner_detail), style = MaterialTheme.typography.bodyMedium); Text("Zoen Android ${xyz.tironi.zoen.BuildConfig.VERSION_NAME} · Rust ${coreVersion()}", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            if (state.demo) {
                item { SettingsRow(Icons.Rounded.RestartAlt, stringResource(R.string.reset_demo), onClick = { reset = true }) }
                item { OutlinedButton(onClick = { model.launch { model.repository.leaveDemo() } }, Modifier.fillMaxWidth()) { Text(stringResource(R.string.leave_demo)) } }
            } else item { TextButton(onClick = { signOut = true }) { Text(stringResource(R.string.sign_out), color = MaterialTheme.colorScheme.error) } }
        }
    }
    if (reset) AlertDialog(onDismissRequest = { reset = false }, title = { Text(stringResource(R.string.reset_demo)) }, text = { Text(stringResource(R.string.reset_question)) }, confirmButton = { TextButton(onClick = { reset = false; model.launch { model.repository.resetDemo() }; back() }) { Text(stringResource(R.string.reset_demo)) } }, dismissButton = { TextButton(onClick = { reset = false }) { Text(stringResource(R.string.cancel)) } })
    if (signOut) AlertDialog(onDismissRequest = { signOut = false }, title = { Text(stringResource(R.string.sign_out)) }, text = { Text(stringResource(R.string.sign_out_question)) }, confirmButton = { TextButton(onClick = { signOut = false; model.launch { model.repository.signOut() } }) { Text(stringResource(R.string.sign_out)) } }, dismissButton = { TextButton(onClick = { signOut = false }) { Text(stringResource(R.string.cancel)) } })
}

@Composable
fun PermissionsScreen(model: ZoenViewModel, state: AppState, back: () -> Unit) {
    val standing by produceState<List<StandingDecisionDto>>(emptyList(), state.revision) { value = model.repository.query { it.standingDecisions() } }
    val device by produceState<List<DeviceGrantDto>>(emptyList(), state.revision) { value = model.repository.query { it.appDeviceGrants() } }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.permissions), back) }) { padding ->
        LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (standing.isEmpty() && device.isEmpty()) item { EmptyState(stringResource(R.string.no_permissions), stringResource(R.string.trust_detail)) }
            items(standing, key = { it.grantId }) { grant -> Card { Column(Modifier.fillMaxWidth().padding(16.dp)) { Text(grant.agent.name, style = MaterialTheme.typography.titleMedium); Text(grant.actionLabel + " · " + grant.spaceTitle); Text(stringResource(if (grant.allow) R.string.always_approve else R.string.always_deny)); TextButton(onClick = { model.launch { model.repository.change { it.revokeStanding(grant.grantId) } } }) { Text(stringResource(R.string.revoke)) } } } }
            if (device.isNotEmpty()) item { SectionLabel(stringResource(R.string.device_permissions)) }
            items(device, key = { it.grantId }) { grant -> Card { Column(Modifier.fillMaxWidth().padding(16.dp)) { Text(grant.appName, style = MaterialTheme.typography.titleMedium); Text(grant.purpose); TextButton(onClick = { model.launch { model.repository.change { it.revokeAppDevice(grant.grantId) } } }) { Text(stringResource(R.string.revoke)) } } } }
        }
    }
}

@Composable
fun HistoryScreen(model: ZoenViewModel, state: AppState, back: () -> Unit) {
    val reports by produceState<List<LogReport>>(emptyList(), state.revision) { value = model.repository.query { it.verifyAll() } }
    var expanded by rememberSaveable { mutableStateOf<String?>(null) }
    val events by produceState<List<LogEventDto>>(emptyList(), expanded, state.revision) { value = expanded?.let { id -> model.repository.query { it.logEvents(id) } } ?: emptyList() }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.signed_log), back) }) { padding ->
        LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { Icon(if (reports.all { it.valid }) Icons.Rounded.VerifiedUser else Icons.Rounded.GppBad, null, Modifier.size(48.dp), tint = if (reports.all { it.valid }) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error); Text(stringResource(if (reports.all { it.valid }) R.string.verified else R.string.invalid_log), style = MaterialTheme.typography.headlineMedium) }
            items(reports, key = { it.spaceId }) { report ->
                Card(onClick = { expanded = if (expanded == report.spaceId) null else report.spaceId }) {
                    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) { Text(report.spaceTitle, style = MaterialTheme.typography.titleMedium); Text(stringResource(R.string.events, report.events.toInt())); Text(report.error ?: report.headHash, style = MaterialTheme.typography.labelSmall, color = if (report.valid) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.error) }
                }
                if (expanded == report.spaceId) events.takeLast(20).reversed().forEach { event -> Column(Modifier.padding(16.dp)) { Text("${event.seq} · ${event.label}", style = MaterialTheme.typography.titleSmall); Text(event.author.name, style = MaterialTheme.typography.bodySmall); Text(event.hash, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) } }
            }
        }
    }
}

@Composable
fun ProfileScreen(model: ZoenViewModel, state: AppState, id: String, navigate: (NavKey) -> Unit, back: () -> Unit) {
    val persona by produceState<Persona?>(null, id, state.revision) { value = model.repository.query { it.personas().firstOrNull { person -> person.id == id } } }
    val profile by produceState<ProfileDto?>(null, id, state.revision) { value = model.repository.query { it.getProfile(id) } }
    var editing by rememberSaveable { mutableStateOf(false) }
    var name by rememberSaveable { mutableStateOf("") }
    var bio by rememberSaveable { mutableStateOf("") }
    Scaffold(topBar = { ScreenBar(persona?.name ?: stringResource(R.string.profile), back) }) { padding ->
        Column(Modifier.padding(padding).padding(24.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Avatar(persona, size = 100)
            Text(persona?.name.orEmpty(), style = MaterialTheme.typography.headlineLarge)
            Text("@${persona?.handle.orEmpty()}", color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (editing) {
                OutlinedTextField(name, { name = it }, label = { Text(stringResource(R.string.name)) })
                OutlinedTextField(bio, { bio = it }, label = { Text(stringResource(R.string.bio)) })
                Button(onClick = { model.launch { model.repository.change { it.updateMyProfile(name, bio, PhotoChange.Keep) }; editing = false } }, enabled = name.isNotBlank()) { Text(stringResource(R.string.save)) }
            } else {
                Text(profile?.bio ?: persona?.bio.orEmpty())
                if (persona?.isMe == true) OutlinedButton(onClick = { name = profile?.name ?: persona?.name.orEmpty(); bio = profile?.bio ?: persona?.bio.orEmpty(); editing = true }) { Text(stringResource(R.string.edit)) }
                else if (!state.demo && persona != null) Button(onClick = { model.launch { val chat = model.repository.change { it.startDirect(id) }; navigate(Chat(chat)) } }) { Text(stringResource(R.string.message)) }
            }
        }
    }
}

@Composable
fun ParticipantsScreen(model: ZoenViewModel, state: AppState, id: String, navigate: (NavKey) -> Unit, back: () -> Unit) {
    val space = state.spaces.firstOrNull { it.id == id } ?: return
    val context = LocalContext.current
    Scaffold(topBar = { ScreenBar(stringResource(R.string.participants), back, actions = {
        if (!state.demo) IconButton(onClick = { model.launch { val invite = model.repository.network { it.createInvite(id) }; xyz.tironi.zoen.data.FileAccess.shareText(context, invite.link) } }) { Icon(Icons.Rounded.PersonAdd, stringResource(R.string.invite)) }
    }) }) { padding -> LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(16.dp)) {
        items(space.members, key = { it.id }) { person ->
            SettingsRow(if (person.kind == PersonaKind.AGENT) Icons.Rounded.SmartToy else Icons.Rounded.Person, person.name, person.bio, onClick = { navigate(if (person.kind == PersonaKind.AGENT) Agent(person.id) else Person(person.id)) })
        }
        item { SettingsRow(Icons.Rounded.Security, stringResource(R.string.permissions), onClick = { navigate(Permissions) }) }
    } }
}
