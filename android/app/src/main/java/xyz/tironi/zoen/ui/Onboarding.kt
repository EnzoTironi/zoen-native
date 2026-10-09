package xyz.tironi.zoen.ui

import android.Manifest
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.text.KeyboardOptions
import xyz.tironi.zoen.BuildConfig
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.TrustLevelDto
import xyz.tironi.zoen.data.LocalPlanner

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun Onboarding(model: ZoenViewModel, modifier: Modifier = Modifier) {
    var step by rememberSaveable { mutableIntStateOf(0) }
    var selected by rememberSaveable { mutableStateOf(listOf<Int>()) }
    var trust by rememberSaveable { mutableIntStateOf(2) }
    var location by rememberSaveable { mutableStateOf(false) }
    var name by rememberSaveable { mutableStateOf("") }
    var handle by rememberSaveable { mutableStateOf("") }
    var relay by rememberSaveable { mutableStateOf(BuildConfig.RELAY_URL) }
    var connectionSettings by rememberSaveable { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    val notifications = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { step = 5 }
    val prompt = stringResource(R.string.first_prompt)
    val localLabel = stringResource(R.string.local_planner)
    val plan = remember(prompt) { LocalPlanner.plan(prompt, model.repository.locale) }
    val titles = listOf(R.string.welcome_title, R.string.areas_title, R.string.plan_title, R.string.trust_title, R.string.notifications_title, R.string.location_title, R.string.ready_title)
    val details = listOf(R.string.welcome_detail, R.string.areas_detail, R.string.plan_detail, R.string.trust_detail, R.string.notifications_detail, R.string.location_detail, R.string.account_detail)
    val areaLabels = listOf(R.string.life_work, R.string.life_study, R.string.life_travel, R.string.life_home, R.string.life_health, R.string.life_friends)
    BackHandler(enabled = step > 0 && !busy) { step-- }
    Column(modifier.fillMaxSize().safeDrawingPadding().imePadding()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = { if (step > 0) step-- }, enabled = step > 0 && !busy) { Icon(Icons.AutoMirrored.Rounded.ArrowBack, stringResource(R.string.back)) }
            Row(Modifier.weight(1f).padding(end = 28.dp), horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                repeat(7) { index -> LinearProgressIndicator(progress = { if (index <= step) 1f else 0f }, modifier = Modifier.weight(1f).height(4.dp)) }
            }
        }
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).widthIn(max = 560.dp).align(Alignment.CenterHorizontally).padding(horizontal = 28.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            ZoenMascot(Modifier.fillMaxWidth().height(if (step == 6) 150.dp else 208.dp), animated = true, pose = step)
            Text(stringResource(titles[step]), style = if (step == 0) MaterialTheme.typography.displaySmall else MaterialTheme.typography.headlineMedium)
            Text(stringResource(details[step]), style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
            when (step) {
                1 -> FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    areaLabels.forEachIndexed { index, label ->
                        FilterChip(selected = index in selected, onClick = { selected = if (index in selected) selected - index else selected + index }, label = { Text(stringResource(label)) })
                    }
                }
                2 -> Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer)) {
                    Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) { Icon(Icons.Rounded.AutoAwesome, null); Text(stringResource(R.string.local_draft), style = MaterialTheme.typography.labelLarge) }
                        Text(plan.title, style = MaterialTheme.typography.titleLarge)
                        plan.sections.last().lines.forEach { line -> Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) { Icon(Icons.Rounded.CheckCircleOutline, null, Modifier.size(18.dp)); Text(line.text) } }
                    }
                }
                3 -> {
                    val levels = listOf(R.string.listen to R.string.listen_detail, R.string.suggest to R.string.suggest_detail, R.string.act to R.string.act_detail, R.string.autonomous to R.string.autonomous_detail)
                    levels.forEachIndexed { index, labels ->
                        Row(Modifier.fillMaxWidth().selectable(trust == index, role = Role.RadioButton, onClick = { trust = index }).padding(vertical = 5.dp), verticalAlignment = Alignment.CenterVertically) {
                            RadioButton(trust == index, onClick = null)
                            Column(Modifier.padding(start = 12.dp)) { Text(stringResource(labels.first), style = MaterialTheme.typography.titleSmall); Text(stringResource(labels.second), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                        }
                    }
                }
                5 -> Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) { Text(stringResource(R.string.location_preference), Modifier.weight(1f)); Switch(location, { location = it }) }
                6 -> {
                    OutlinedTextField(name, { name = it }, label = { Text(stringResource(R.string.name)) }, singleLine = true, modifier = Modifier.fillMaxWidth(), keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words))
                    OutlinedTextField(handle, { handle = it.lowercase(java.util.Locale.ROOT).filter { c -> c in 'a'..'z' || c in '0'..'9' || c == '_' || c == '.' }.take(24) }, label = { Text(stringResource(R.string.handle)) }, prefix = { Text("@") },
                        supportingText = { Text(stringResource(R.string.handle_rules)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                    TextButton(onClick = { connectionSettings = !connectionSettings }) { Text(stringResource(R.string.connection_settings)) }
                    if (connectionSettings) OutlinedTextField(relay, { relay = it }, label = { Text(stringResource(R.string.relay)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                }
            }
            Spacer(Modifier.height(8.dp))
        }
        Column(Modifier.widthIn(max = 560.dp).fillMaxWidth().align(Alignment.CenterHorizontally).padding(24.dp), verticalArrangement = Arrangement.spacedBy(4.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            Button(onClick = {
                when (step) {
                    4 -> if (Build.VERSION.SDK_INT >= 33) notifications.launch(Manifest.permission.POST_NOTIFICATIONS) else step++
                    6 -> {
                        busy = true
                        model.launch {
                            try {
                                if (model.state.value.account == null) model.repository.createAccount(name, handle, relay)
                                model.repository.change { core ->
                                    val agent = core.agents().first { it.persona.handle == "zoen" }
                                    agent.spaces.forEach { core.setTrust(agent.persona.id, it.spaceId, TrustLevelDto.entries[trust]) }
                                    val chat = core.spaces().first { it.counterpart?.handle == "zoen" }
                                    if (trust > 0) core.agentCreatePlan(chat.id, agent.persona.id, prompt, plan, localLabel, 0)
                                }
                                model.repository.preferences.edit().putBoolean("onboarded", true).putBoolean("location", location).putString("areas", selected.joinToString(",")).commit()
                                model.repository.refresh()
                            } finally { busy = false }
                        }
                    }
                    else -> step++
                }
            }, enabled = !busy && (step != 6 || (name.isNotBlank() && Regex("[a-z][a-z0-9._]{2,23}").matches(handle) && (relay.startsWith("https://") || (BuildConfig.DEBUG && relay.startsWith("http://"))))), modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) {
                if (busy) { CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp); Spacer(Modifier.width(12.dp)) }
                Text(stringResource(if (busy) R.string.creating else if (step == 4) R.string.enable_notifications else if (step == 6) R.string.finish else R.string.continue_label))
            }
            if (step == 4 || step == 5) TextButton(onClick = { step++ }) { Text(stringResource(R.string.not_now)) }
            if (step == 0 && BuildConfig.DEBUG) TextButton(onClick = { model.launch { model.repository.useDemo() } }) { Text(stringResource(R.string.explore_demo)) }
        }
    }
}
