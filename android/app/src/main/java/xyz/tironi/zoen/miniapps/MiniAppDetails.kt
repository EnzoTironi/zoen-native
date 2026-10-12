package xyz.tironi.zoen.miniapps

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Info
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import java.text.DateFormat
import java.util.Date
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.DeviceGrantDto
import xyz.tironi.zoen.core.ItemDetail
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.data.FileAccess

@Composable
fun MiniAppDetailsButton(model: ZoenViewModel, state: AppState, item: ItemDetail) {
    var details by remember { mutableStateOf(false) }
    IconButton(onClick = { details = true }) { Icon(Icons.Rounded.Info, stringResource(R.string.miniapp_details)) }
    if (details) MiniAppDetailsSheet(model, state, item, { details = false })
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MiniAppDetailsSheet(model: ZoenViewModel, state: AppState, item: ItemDetail, onClose: () -> Unit) {
    val app = item.app ?: return
    val context = LocalContext.current
    val pins = remember(model.repository) { MiniAppPins(model.repository.preferences) }
    var grants by remember(item.id) { mutableStateOf<List<DeviceGrantDto>>(emptyList()) }
    var manifest by remember(item.id) { mutableStateOf<MiniAppManifest?>(null) }
    var home by remember(item.id) { mutableStateOf(item.id !in pins.hidden()) }
    var chat by remember(item.id) { mutableStateOf(item.id !in pins.hidden(MiniAppPins.chat(item.spaceId))) }
    var restore by remember { mutableStateOf<UInt?>(null) }
    LaunchedEffect(item.id, state.revision) {
        grants = model.repository.query { it.appDeviceGrants().filter { grant -> grant.itemId == item.id } }
        manifest = model.repository.query { core -> runCatching { core.readAppResource(app.resourceUri).manifestJson }.getOrNull()?.let { MiniAppManifest.decode(it, app.appId) } }
    }
    ModalBottomSheet(onDismissRequest = onClose) {
        LazyColumn(contentPadding = PaddingValues(24.dp, 8.dp, 24.dp, 40.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            item {
                Text(app.name, style = MaterialTheme.typography.headlineMedium)
                Text(item.spaceTitle + " · " + app.trust.name, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(app.headline, Modifier.padding(top = 8.dp))
            }
            item {
                Row(Modifier.fillMaxWidth(), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                    Text(stringResource(R.string.miniapp_show_home), Modifier.weight(1f))
                    Switch(home, onCheckedChange = { home = it; if (it) pins.show(item.id) else pins.hide(item.id); model.launch { model.repository.refresh() } })
                }
                Row(Modifier.fillMaxWidth(), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                    Text(stringResource(R.string.miniapp_show_chat), Modifier.weight(1f))
                    Switch(chat, onCheckedChange = { chat = it; if (it) pins.show(item.id, MiniAppPins.chat(item.spaceId)) else pins.hide(item.id, MiniAppPins.chat(item.spaceId)); model.launch { model.repository.refresh() } })
                }
                TextButton(onClick = { FileAccess.shareText(context, "${item.title} · zoen://app/${item.id}") }) { Text(stringResource(R.string.share)) }
            }
            item { Text(stringResource(R.string.miniapp_access), style = MaterialTheme.typography.titleMedium) }
            if (grants.isEmpty()) item { Text(stringResource(R.string.miniapp_no_grants), style = MaterialTheme.typography.bodySmall) }
            items(grants, key = { it.grantId }) { grant ->
                ListItem(headlineContent = { Text(capabilityName(grant.capability)) }, supportingContent = { Text(grant.purpose + "\n" + stringResource(if (grant.always) R.string.miniapp_always else R.string.miniapp_allow_once)) },
                    trailingContent = { TextButton(onClick = { model.launch { model.repository.change { it.revokeAppDevice(grant.grantId) } } }) { Text(stringResource(R.string.miniapp_revoke)) } })
            }
            if (manifest != null) {
                item { Text(stringResource(R.string.miniapp_manifest), style = MaterialTheme.typography.titleMedium) }
                manifest!!.capabilities.forEach { (capability, purpose) -> item { Text(capabilityName(capability), style = MaterialTheme.typography.labelLarge); Text(purpose, style = MaterialTheme.typography.bodySmall) } }
                if (manifest!!.allowedDomains.isNotEmpty()) item { Text(manifest!!.allowedDomains.joinToString("\n"), style = MaterialTheme.typography.bodySmall) }
                item { Text(stringResource(R.string.miniapp_verified_bundle), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary) }
            }
            item { Text(stringResource(R.string.versions), style = MaterialTheme.typography.titleMedium) }
            items(item.versions.reversed(), key = { it.number.toInt() }) { version ->
                ListItem(headlineContent = { Text(stringResource(R.string.version, version.number.toInt())) }, supportingContent = { Text(version.note + "\n" + version.author.name + " · " + DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(version.atMs))) },
                    trailingContent = { if (version.number != item.version) TextButton(onClick = { restore = version.number }) { Text(stringResource(R.string.restore)) } })
            }
            item { Text(item.origin, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
    }
    restore?.let { number -> AlertDialog(onDismissRequest = { restore = null }, title = { Text(stringResource(R.string.restore)) }, text = { Text(stringResource(R.string.restore_question)) }, confirmButton = {
        TextButton(onClick = { restore = null; model.launch { model.repository.change { it.restoreVersion(item.id, number) } } }) { Text(stringResource(R.string.restore)) }
    }, dismissButton = { TextButton(onClick = { restore = null }) { Text(stringResource(R.string.cancel)) } }) }
}
