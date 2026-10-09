package xyz.tironi.zoen.ui

import android.content.Intent
import android.provider.Settings
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.input.KeyboardType
import kotlinx.coroutines.CancellationException
import xyz.tironi.zoen.agent.*
import xyz.tironi.zoen.data.FileAccess
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
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
    var previewSpace by rememberSaveable(id) { mutableStateOf<String?>(null) }
    val selectedSpace = previewSpace ?: profile?.spaces?.firstOrNull()?.spaceId
    val previews by produceState<List<DecisionPreview>>(emptyList(), id, selectedSpace, state.revision) {
        value = selectedSpace?.let { space -> model.repository.query { it.previewDecisions(id, space) } }.orEmpty()
    }
    val activity by produceState<List<AgentActivityDto>>(emptyList(), id, state.revision) { value = model.repository.query { it.agentActivity(id) } }
    if (profile == null) return
    Scaffold(topBar = { ScreenBar(profile.persona.name, back) }) { padding ->
        LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            item { Avatar(profile.persona, size = 96); Text(profile.persona.name, style = MaterialTheme.typography.headlineMedium); Text(profile.persona.bio, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            if (profile.persona.handle == "zoen") item { OnDeviceAiSettings(model) }
            if (profile.persona.handle != "zoen") item { AgentDrawingChooser(profile.persona) }
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
                SettingsRow(Icons.Rounded.Tune, space.spaceTitle, stringResource(trustLabel(space.level)), onClick = { previewSpace = space.spaceId; if (profile.persona.isMine) trustSpace = space }, trailing = { if (profile.persona.isMine) Icon(Icons.Rounded.Edit, stringResource(R.string.edit)) else Icon(Icons.Rounded.Lock, stringResource(R.string.read_only)) })
            }
            if (previews.isNotEmpty()) item { SectionLabel(stringResource(R.string.agent_decision_preview)) }
            items(previews, key = { it.action }) { decision ->
                Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
                    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                            Text(decision.action, Modifier.weight(1f), style = MaterialTheme.typography.titleSmall)
                            Text(stringResource(when (decision.kind) { DecisionKind.ACT -> R.string.act; DecisionKind.ACT_WITH_UNDO -> R.string.agent_decision_undo; DecisionKind.REQUEST -> R.string.agent_decision_ask; DecisionKind.BLOCK -> R.string.agent_decision_block }), style = MaterialTheme.typography.labelSmall, color = if (decision.kind == DecisionKind.BLOCK || decision.redLine) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary)
                        }
                        Text(decision.example, style = MaterialTheme.typography.bodySmall)
                        Text(decision.explanation, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        if (decision.redLine) Text(stringResource(R.string.agent_decision_redline), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelSmall)
                    }
                }
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
    val stats by produceState<CoreStats?>(null, state.revision) { value = model.repository.query { it.stats() } }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.context), back) }) { padding ->
        LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                Avatar(state.me, size = 72, onClick = { state.me?.let { navigate(Person(it.id)) } })
                Column { Text(state.me?.name.orEmpty(), style = MaterialTheme.typography.headlineMedium); Text("@${state.me?.handle.orEmpty()}", color = MaterialTheme.colorScheme.onSurfaceVariant) }
            } }
            if (state.demo) item { Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.tertiaryContainer)) { Column(Modifier.padding(20.dp)) { Text(stringResource(R.string.demo), style = MaterialTheme.typography.titleMedium); Text(stringResource(R.string.demo_detail)) } } }
            if (state.keyMissing) item { Text(stringResource(R.string.key_missing), color = MaterialTheme.colorScheme.error) }
            if (state.account != null) {
                if (state.connection.error == "handle_taken") item { SettingsRow(Icons.Rounded.WarningAmber, stringResource(R.string.agent_handle_taken, state.account.handle), onClick = { navigate(Person(state.account.identityId)) }) }
                item {
                    val connection = when (state.connection.state) {
                        "online" -> stringResource(if (state.connection.synced) R.string.online else R.string.agent_profile_syncing)
                        "connecting" -> stringResource(R.string.connecting)
                        else -> if (state.connection.pending > 0uL) stringResource(R.string.agent_profile_offline_queued, state.connection.pending.toLong()) else stringResource(R.string.offline)
                    }
                    Text(connection, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(stringResource(R.string.agent_profile_device, state.account.deviceId.take(8)), style = MaterialTheme.typography.labelSmall)
                }
            }
            item { SectionLabel(stringResource(R.string.settings)) }
            if (!state.demo && state.account != null) item { xyz.tironi.zoen.background.BackgroundConnectionSettings(model) }
            item { SettingsRow(Icons.Rounded.Widgets, stringResource(R.string.widgets), onClick = { navigate(Widgets) }) }
            item { SettingsRow(Icons.Rounded.PersonOutline, stringResource(R.string.profile), onClick = { state.me?.let { navigate(Person(it.id)) } }) }
            item { SettingsRow(Icons.Rounded.VerifiedUser, stringResource(R.string.signed_log), onClick = { navigate(History) }) }
            item { SettingsRow(Icons.Rounded.Security, stringResource(R.string.permissions), onClick = { navigate(Permissions) }) }
            item { SettingsRow(Icons.Rounded.NotificationsNone, stringResource(R.string.notification_settings), onClick = { context.startActivity(Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)) }) }
            item { SettingsRow(Icons.Rounded.SmartToy, stringResource(R.string.agents), onClick = { navigate(Agents) }) }
            item { OnDeviceAiSettings(model) }
            stats?.let { local ->
                item {
                    SectionLabel(stringResource(R.string.agent_local_data))
                    Text(stringResource(R.string.agent_local_counts, local.spaces.toInt(), local.items.toInt(), local.events.toLong()), style = MaterialTheme.typography.bodySmall)
                    if (!local.allLogsValid) TextButton(onClick = { navigate(History) }) { Text(stringResource(R.string.agent_integrity_damaged), color = MaterialTheme.colorScheme.error) }
                    else Text(stringResource(R.string.agent_integrity_verified), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            item { SectionLabel(stringResource(R.string.about)); Text("Zoen Android ${xyz.tironi.zoen.BuildConfig.VERSION_NAME} · Rust ${coreVersion()}", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
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
private fun AgentDrawingChooser(persona: Persona) {
    val context = LocalContext.current
    var choosing by remember { mutableStateOf(false) }
    OutlinedButton(onClick = { choosing = true }) { Text(stringResource(R.string.agent_choose_drawing)) }
    if (choosing) AvatarArtPicker("agent", persona.id, AvatarArt.agent(context, persona), close = { choosing = false })
}

@Composable
fun ProfileScreen(model: ZoenViewModel, state: AppState, id: String, navigate: (NavKey) -> Unit, back: () -> Unit) {
    val context = LocalContext.current
    val failureText = stringResource(R.string.something_wrong)
    var failure by remember { mutableStateOf<String?>(null) }
    val persona by produceState<Persona?>(null, id, state.revision) { value = model.repository.query { it.personas().firstOrNull { person -> person.id == id } } }
    val profile by produceState<ProfileDto?>(null, id, state.revision) {
        try { value = model.repository.query { it.getProfile(id) } }
        catch (e: Exception) { if (e is CancellationException) throw e; failure = e.message }
    }
    var editing by rememberSaveable { mutableStateOf(false) }
    var name by rememberSaveable { mutableStateOf("") }
    var bio by rememberSaveable { mutableStateOf("") }
    var handle by rememberSaveable { mutableStateOf("") }
    var saving by remember { mutableStateOf(false) }
    var photoChange by remember { mutableStateOf<PhotoChange>(PhotoChange.Keep) }
    var photoPreview by remember { mutableStateOf<Bitmap?>(null) }
    var photoSource by remember { mutableStateOf<Bitmap?>(null) }
    var photoActions by remember { mutableStateOf(false) }
    var cameraUri by rememberSaveable { mutableStateOf<String?>(null) }
    var confirmBlock by remember { mutableStateOf(false) }
    var demoBlocked by rememberSaveable(id) { mutableStateOf(model.repository.preferences.getBoolean("demo.blocked.$id", false)) }
    var muted by rememberSaveable(id) { mutableStateOf(model.repository.preferences.getBoolean("muted.person.$id", false)) }
    fun readPhoto(uri: Uri) { model.launch { photoSource = ProfilePhotos.read(context, uri) } }
    val photoPicker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri -> uri?.let(::readPhoto) }
    val camera = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { taken -> if (taken) cameraUri?.let { readPhoto(Uri.parse(it)) } }
    val cleanHandle = handle.trim().removePrefix("@")
    val validHandle = Regex("[a-z][a-z0-9._]{2,23}").matches(cleanHandle)
    val common = state.spaces.filter { space -> space.counterpart == null && space.members.any { it.id == id } }
    val blocked = profile?.blocked == true || demoBlocked
    Scaffold(topBar = { ScreenBar(persona?.name ?: stringResource(R.string.profile), back) }) { padding ->
        LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
            item {
                if (editing && photoPreview != null) androidx.compose.foundation.Image(photoPreview!!.asImageBitmap(), stringResource(R.string.profile), Modifier.size(100.dp).clip(androidx.compose.foundation.shape.CircleShape))
                else if (editing && photoChange is PhotoChange.Remove) Text(persona?.initials.orEmpty(), style = MaterialTheme.typography.displayMedium)
                else Avatar(persona, size = 100)
                Text(persona?.name.orEmpty(), style = MaterialTheme.typography.headlineLarge)
                Text("@${persona?.handle.orEmpty()}", color = MaterialTheme.colorScheme.onSurfaceVariant)
                if (profile?.photoSha256 != null && profile?.photoReady == false) Text(stringResource(R.string.agent_profile_photo_pending), style = MaterialTheme.typography.bodySmall)
            }
            if (editing) {
                item {
                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        OutlinedButton(onClick = { photoActions = true }, enabled = !saving) { Text(stringResource(R.string.agent_photo_choose)) }
                        OutlinedTextField(name, { name = it.take(64) }, Modifier.fillMaxWidth(), label = { Text(stringResource(R.string.name)) }, enabled = !saving)
                        OutlinedTextField(bio, { bio = it.take(280) }, Modifier.fillMaxWidth(), label = { Text(stringResource(R.string.bio)) }, enabled = !saving)
                        if (!state.demo) OutlinedTextField(handle, { handle = it.lowercase(java.util.Locale.ROOT).filter { char -> char in 'a'..'z' || char in '0'..'9' || char in "._" }.take(24) }, Modifier.fillMaxWidth(),
                            label = { Text(stringResource(R.string.agent_profile_handle)) }, prefix = { Text("@") }, singleLine = true, enabled = !saving,
                            isError = handle.isNotBlank() && !validHandle, supportingText = { Text(stringResource(R.string.agent_profile_handle_help)) }, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, autoCorrectEnabled = false))
                        failure?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TextButton(onClick = { editing = false; photoChange = PhotoChange.Keep; photoPreview = null }, enabled = !saving) { Text(stringResource(R.string.cancel)) }
                            Button(onClick = {
                                saving = true; failure = null
                                model.launch {
                                    try {
                                        if (!state.demo) model.repository.change { it.updateProfile(name.trim(), cleanHandle, bio.trim()) }
                                        model.repository.change { it.updateMyProfile(name.trim(), bio.trim(), photoChange) }
                                        editing = false; photoChange = PhotoChange.Keep; photoPreview = null
                                    } catch (e: Exception) { if (e is CancellationException) throw e; failure = e.message ?: failureText }
                                    finally { saving = false }
                                }
                            }, enabled = !saving && name.isNotBlank() && (state.demo || validHandle)) { Text(stringResource(R.string.save)) }
                        }
                        if (saving) LinearProgressIndicator(Modifier.fillMaxWidth())
                    }
                }
            } else {
                item {
                    Text(profile?.bio ?: persona?.bio.orEmpty())
                    if (persona?.isMe == true) OutlinedButton(onClick = { name = profile?.name ?: persona?.name.orEmpty(); bio = profile?.bio ?: persona?.bio.orEmpty(); handle = state.account?.handle ?: persona?.handle.orEmpty(); failure = null; editing = true }) { Text(stringResource(R.string.edit)) }
                    else if (persona != null) Button(onClick = { model.launch {
                        val existing = state.spaces.firstOrNull { it.counterpart?.id == id }
                        val chat = existing?.id ?: model.repository.change { it.startDirect(id) }
                        navigate(Chat(chat))
                    } }) { Text(stringResource(R.string.message)) }
                    if (persona?.kind == PersonaKind.AGENT && persona?.handle != "zoen") AgentDrawingChooser(persona!!)
                    if (blocked) Text(stringResource(R.string.agent_profile_blocked), color = MaterialTheme.colorScheme.error)
                }
                if (common.isNotEmpty()) item { SectionLabel(stringResource(R.string.agent_profile_common)) }
                items(common, key = { it.id }) { space -> SettingsRow(Icons.Rounded.Group, space.title, onClick = { navigate(Chat(space.id)) }) }
                if (persona?.kind == PersonaKind.AGENT) item { SettingsRow(Icons.Rounded.Security, stringResource(R.string.permissions), onClick = { navigate(Agent(id)) }) }
                if (persona?.isMe == false && persona?.kind == PersonaKind.PERSON) {
                    item { SettingsRow(if (muted) Icons.Rounded.NotificationsActive else Icons.Rounded.NotificationsOff, stringResource(if (muted) R.string.agent_profile_unmute else R.string.agent_profile_mute), if (muted) stringResource(R.string.agent_profile_muted) else null, onClick = {
                        muted = !muted; model.repository.preferences.edit().putBoolean("muted.person.$id", muted).apply()
                    }) }
                    item { TextButton(onClick = { if (blocked) model.launch { if (state.demo) { demoBlocked = false; model.repository.preferences.edit().remove("demo.blocked.$id").apply() } else model.repository.change { it.unblockPerson(id) } } else confirmBlock = true }) {
                        Text(stringResource(if (blocked) R.string.agent_profile_unblock else R.string.agent_profile_block), color = MaterialTheme.colorScheme.error)
                    } }
                }
                failure?.let { item { Text(it, color = MaterialTheme.colorScheme.error) } }
            }
        }
    }
    if (photoActions) AlertDialog(onDismissRequest = { photoActions = false }, title = { Text(stringResource(R.string.agent_photo_choose)) }, text = { Column {
        TextButton(onClick = { photoActions = false; photoPicker.launch(androidx.activity.result.PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }) { Text(stringResource(R.string.agent_photo_choose)) }
        TextButton(onClick = { photoActions = false; val uri = FileAccess.cameraUri(context); cameraUri = uri.toString(); camera.launch(uri) }) { Text(stringResource(R.string.agent_photo_camera)) }
        TextButton(onClick = { photoActions = false; photoChange = PhotoChange.Remove; photoPreview = null }) { Text(stringResource(R.string.agent_photo_remove)) }
    } }, confirmButton = { TextButton(onClick = { photoActions = false }) { Text(stringResource(R.string.cancel)) } })
    photoSource?.let { source -> ProfilePhotoCrop(source, apply = { zoom, x, y -> model.launch {
        val bytes = ProfilePhotos.jpeg(source, zoom, x, y)
        photoChange = PhotoChange.Set(bytes, "image/jpeg")
        photoPreview = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
        photoSource = null
    } }, cancel = { photoSource = null }) }
    if (confirmBlock) AlertDialog(onDismissRequest = { confirmBlock = false }, title = { Text(stringResource(R.string.agent_profile_block)) }, text = { Text(stringResource(R.string.agent_profile_block_question, persona?.name.orEmpty())) }, confirmButton = {
        TextButton(onClick = { confirmBlock = false; model.launch {
            if (state.demo) { demoBlocked = true; model.repository.preferences.edit().putBoolean("demo.blocked.$id", true).apply() }
            else model.repository.change { it.blockPerson(id) }
        } }) { Text(stringResource(R.string.agent_profile_block)) }
    }, dismissButton = { TextButton(onClick = { confirmBlock = false }) { Text(stringResource(R.string.cancel)) } })
}

private data class ParticipantAction(val kind: String, val person: Persona? = null)

@Composable
fun ParticipantsScreen(model: ZoenViewModel, state: AppState, id: String, navigate: (NavKey) -> Unit, back: () -> Unit) {
    val space = state.spaces.firstOrNull { it.id == id } ?: return
    val context = LocalContext.current
    val roles by produceState<Map<String, String>>(emptyMap(), id, state.revision) {
        value = model.repository.query { it.memberRoles(id).associate { role -> role.identityId to role.role } }
    }
    val synced by produceState(false, id, state.revision) { value = model.repository.query { it.isSynced(id) } }
    val integrity by produceState<LogReport?>(null, id, state.revision) { value = model.repository.query { it.verifyLog(id) } }
    val myRole = roles[state.me?.id]
    val canManage = synced && myRole in setOf("owner", "admin")
    var confirming by remember { mutableStateOf<ParticipantAction?>(null) }
    var drawingPicker by remember { mutableStateOf(false) }
    var menu by remember { mutableStateOf<String?>(null) }
    var inviting by remember { mutableStateOf(false) }
    var invite by remember { mutableStateOf<InviteDto?>(null) }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.participants), back, actions = {
        if (synced && space.kind == SpaceKindDto.GROUP) IconButton(onClick = {
            inviting = true
            model.launch { try { invite = model.repository.network { it.createInvite(id) } } finally { inviting = false } }
        }, enabled = !inviting) { Icon(Icons.Rounded.PersonAdd, stringResource(R.string.invite)) }
    }) }) { padding ->
        LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            item { SpaceAvatar(space, size = 96); Text(space.title, style = MaterialTheme.typography.headlineMedium) }
            if (space.counterpart == null) item { OutlinedButton(onClick = { drawingPicker = true }) { Text(stringResource(R.string.agent_choose_drawing)) } }
            item { SectionLabel(stringResource(R.string.agent_group_people)) }
            items(space.members.filter { it.kind == PersonaKind.PERSON }, key = { it.id }) { person ->
                ListItem(headlineContent = { Text(person.name) }, supportingContent = { Text(when (roles[person.id]) {
                    "owner" -> stringResource(R.string.agent_group_owner)
                    "admin" -> stringResource(R.string.agent_group_role_admin)
                    else -> "@${person.handle}"
                }) }, leadingContent = { Avatar(person, size = 44, onClick = { navigate(Person(person.id)) }) }, trailingContent = {
                    if (canManage && !person.isMe && roles[person.id] != "owner") Box {
                        IconButton(onClick = { menu = person.id }) { Icon(Icons.Rounded.MoreVert, stringResource(R.string.more)) }
                        DropdownMenu(menu == person.id, onDismissRequest = { menu = null }) {
                            if (roles[person.id] != "admin") DropdownMenuItem(text = { Text(stringResource(R.string.agent_group_admin)) }, onClick = { menu = null; confirming = ParticipantAction("admin", person) })
                            DropdownMenuItem(text = { Text(stringResource(R.string.agent_group_remove)) }, onClick = { menu = null; confirming = ParticipantAction("remove", person) })
                        }
                    }
                }, colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface))
            }
            if (space.members.any { it.kind == PersonaKind.AGENT }) item { SectionLabel(stringResource(R.string.agents)) }
            items(space.members.filter { it.kind == PersonaKind.AGENT }, key = { it.id }) { agent ->
                ListItem(headlineContent = { Text(agent.name) }, supportingContent = { Text(agent.ownerName.orEmpty()) }, leadingContent = { Avatar(agent, size = 44, onClick = { navigate(Agent(agent.id)) }) },
                    trailingContent = { TextButton(onClick = { navigate(Agent(agent.id)) }) { Text(stringResource(R.string.permissions)) } }, colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface))
            }
            item { SectionLabel(stringResource(R.string.settings)); SettingsRow(Icons.Rounded.Palette, stringResource(R.string.agent_group_background), onClick = { navigate(Appearance(id)) }) }
            item { Text(stringResource(R.string.agent_group_history), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            item { SettingsRow(Icons.Rounded.Security, stringResource(R.string.permissions), onClick = { navigate(Permissions) }) }
            if (integrity?.valid == false) item { TextButton(onClick = { navigate(History) }) { Text(stringResource(R.string.agent_integrity_damaged), color = MaterialTheme.colorScheme.error) } }
            if (synced && space.privacy == PrivacyDto.CLOSED && (canManage || space.kind == SpaceKindDto.DIRECT)) item {
                OutlinedButton(onClick = { confirming = ParticipantAction("encrypt") }) { Text(stringResource(R.string.agent_group_encrypt)) }
            }
            invite?.let { link -> item {
                Card { Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(link.code, style = MaterialTheme.typography.titleLarge)
                    Text(link.link, style = MaterialTheme.typography.bodySmall)
                    TextButton(onClick = { FileAccess.shareText(context, link.link) }) { Text(stringResource(R.string.share)) }
                } }
            } }
            if (synced && space.kind == SpaceKindDto.GROUP) item {
                TextButton(onClick = { confirming = ParticipantAction("leave") }) { Text(stringResource(R.string.agent_group_leave), color = MaterialTheme.colorScheme.error) }
            }
        }
    }
    if (drawingPicker) AvatarArtPicker("group", id, AvatarArt.group(context, id, space.title), close = { drawingPicker = false })
    confirming?.let { action ->
        val title = stringResource(when (action.kind) { "leave" -> R.string.agent_group_leave; "encrypt" -> R.string.agent_group_encrypt; "admin" -> R.string.agent_group_admin; else -> R.string.agent_group_remove })
        AlertDialog(onDismissRequest = { confirming = null }, title = { Text(title) }, text = { Text(stringResource(when (action.kind) {
            "leave" -> R.string.agent_group_leave_question; "encrypt" -> R.string.agent_group_encrypt_question; "admin" -> R.string.agent_group_admin_question; else -> R.string.agent_group_remove_question
        }, action.person?.name.orEmpty())) }, confirmButton = {
            TextButton(onClick = { confirming = null; model.launch {
                model.repository.change { core -> when (action.kind) {
                    "leave" -> core.leaveSpace(id)
                    "encrypt" -> core.encryptChat(id)
                    "admin" -> core.addAdmin(id, checkNotNull(action.person).id)
                    "remove" -> core.removeMember(id, checkNotNull(action.person).id)
                    else -> Unit
                } }
                if (action.kind == "leave") navigate(Home)
            } }) { Text(title) }
        }, dismissButton = { TextButton(onClick = { confirming = null }) { Text(stringResource(R.string.cancel)) } })
    }
}
