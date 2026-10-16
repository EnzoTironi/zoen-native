package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.agent.OnDeviceAiSettings
import xyz.tironi.zoen.core.CoreStats
import xyz.tironi.zoen.core.DeviceGrantDto
import xyz.tironi.zoen.miniapps.capabilityName
import xyz.tironi.zoen.core.formatMoney
import xyz.tironi.zoen.data.AppState

/** Profile uses the same genuine ownership and item projections as native YouScreen. */
@Composable
internal fun YouScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit, chats: () -> Unit) {
    val stats by produceState<CoreStats?>(null, state.revision) { value = model.repository.query { it.stats() } }
    val grants by produceState<List<DeviceGrantDto>?>(null, state.revision) {
        value = model.repository.query { it.appDeviceGrants() }
    }
    var reset by remember { mutableStateOf(false) }
    Scaffold(topBar = { HomeBar("Profile", state, navigate) }, contentWindowInsets = WindowInsets(0, 0, 0, 0)) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding).testTag("profile-list"), contentPadding = PaddingValues(20.dp, 8.dp, 20.dp, 112.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            state.me?.let { me -> item {
                GlassCard { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                        Avatar(me, size = 56)
                        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(me.name, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                            Text("Your personal space · @${me.handle}", style = MaterialTheme.typography.bodyMedium)
                            Text("Identity ${me.id.take(8)}…${me.id.takeLast(4)}", style = MaterialTheme.typography.bodySmall)
                        }
                    }
                    Text("Organize what’s yours and decide who can use it.", style = MaterialTheme.typography.bodyMedium)
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Icon(Icons.Rounded.Lock, null, tint = MaterialTheme.colorScheme.primary)
                        Column {
                            Text("Only you and authorized agents", fontWeight = FontWeight.SemiBold)
                            Text(if (state.demo) "Everything stays on this device, signed with your key." else "Your space stays on this device. Chats sync, signed with your key.", style = MaterialTheme.typography.bodySmall)
                        }
                    }
                } }
            } }
            item { SettingsRow(Icons.Rounded.Settings, "Settings", onClick = { navigate(SettingsRoute()) }) }
            state.account?.let { account -> item {
                Column { Text("@${account.handle}"); Text(state.connection.state, style = MaterialTheme.typography.bodySmall) }
            } }
            item { SectionLabel("Your data") }
            item { SettingsRow(Icons.Rounded.Layers, "Items", stats?.items?.toString().orEmpty(), onClick = { navigate(Files) }) }
            item { SettingsRow(Icons.Rounded.ChatBubbleOutline, "Chats", state.spaces.size.toString(), onClick = chats) }
            if (stats?.allLogsValid == false) item { SettingsRow(Icons.Rounded.WarningAmber, "Some chats look damaged", "Open to see which ones need attention.", onClick = { navigate(History) }) }
            item { SectionLabel("Access grants") }
            items(state.agents.filter { it.persona.isMine }, key = { it.persona.id }) { agent ->
                GlassCard(onClick = { navigate(Agent(agent.persona.id)) }) {
                    Row(Modifier.fillMaxWidth().padding(14.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Avatar(agent.persona, size = 40)
                        Column(Modifier.weight(1f)) {
                            Text(agent.persona.name, fontWeight = FontWeight.SemiBold)
                            Text(if (agent.spaces.isEmpty()) "No Spaces" else "${agent.spaces.size} Space(s)", style = MaterialTheme.typography.bodySmall)
                        }
                        if (agent.budgetSpentCents != null && agent.budgetLimitCents != null) Text(formatMoney(agent.budgetSpentCents!!, model.repository.locale) + " / " + formatMoney(agent.budgetLimitCents!!, model.repository.locale), style = MaterialTheme.typography.labelSmall)
                    }
                }
            }
            item { Text("Each of your agents has a trust level per chat and a monthly budget.", style = MaterialTheme.typography.bodySmall) }
            item { SectionLabel("Mini-app access") }
            if (grants?.isEmpty() == true) item {
                Text("No mini-app has asked for anything yet.", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            items(grants.orEmpty(), key = { it.grantId }) { grant ->
                ListItem(
                    headlineContent = { Text(capabilityName(grant.capability)) },
                    supportingContent = { Text("${grant.appName} · ${grant.spaceTitle} · ${if (grant.always) "always" else "once"}") },
                    trailingContent = {
                        TextButton(onClick = { model.launch { model.repository.change { it.revokeAppDevice(grant.grantId) } } }) {
                            Text("Revoke", color = MaterialTheme.colorScheme.error)
                        }
                    },
                )
            }
            item { Text("Each permission is for one mini-app in one chat. Android keeps its own switch in Settings too.", style = MaterialTheme.typography.bodySmall) }
            item { GlassCard { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Protect what’s yours", style = MaterialTheme.typography.titleMedium)
                Text("A passkey keeps your identity on all your devices.", style = MaterialTheme.typography.bodySmall)
                TextButton(onClick = {}, enabled = false) { Text("Soon (weeks 5–6)") }
            } } }
            item { SectionLabel("Settings") }
            item { OnDeviceAiSettings(model) }
            if (state.demo) {
                item { TextButton(onClick = { reset = true }) { Text("Restart the demo", color = MaterialTheme.colorScheme.error) } }
                item { Text("Demo · illustrative data. Agent payments and messages are simulated.", style = MaterialTheme.typography.bodySmall) }
            } else item { Text("Your chats stay in sync across your devices.", style = MaterialTheme.typography.bodySmall) }
        }
    }
    if (reset) AlertDialog(onDismissRequest = { reset = false }, title = { Text("Erase everything and restart the demo story?") }, confirmButton = { TextButton(onClick = { reset = false; model.launch { model.repository.resetDemo() } }) { Text("Restart") } }, dismissButton = { TextButton(onClick = { reset = false }) { Text("Cancel") } })
}
