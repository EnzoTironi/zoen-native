package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import java.text.DateFormat
import java.util.Date
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.DeviceGrantDto
import xyz.tironi.zoen.core.StandingDecisionDto
import xyz.tironi.zoen.data.AppState

internal data class PermissionItemScope(val spaceId: String, val creatorId: String)

internal fun standingInScope(grants: List<StandingDecisionDto>, agentId: String?, spaceId: String?): List<StandingDecisionDto> =
    grants.filter { (agentId == null || it.agent.id == agentId) && (spaceId == null || it.spaceId == spaceId) }

internal fun deviceInScope(
    grants: List<DeviceGrantDto>,
    scopes: Map<String, PermissionItemScope>,
    agentId: String?,
    spaceId: String?,
): List<DeviceGrantDto> = grants.filter { grant ->
    if (spaceId != null) scopes[grant.itemId]?.spaceId == spaceId
    else if (agentId != null) scopes[grant.itemId]?.creatorId == agentId
    else true
}

private data class PermissionData(
    val standing: List<StandingDecisionDto> = emptyList(),
    val device: List<DeviceGrantDto> = emptyList(),
    val loading: Boolean = true,
    val error: String? = null,
)

@Composable
fun StandingPermissionsScreen(model: ZoenViewModel, state: AppState, back: () -> Unit, agentId: String? = null, spaceId: String? = null) {
    var retry by remember { mutableIntStateOf(0) }
    var revoking by remember(state.me?.id) { mutableStateOf(emptySet<String>()) }
    val loadFailure = stringResource(R.string.ap_permission_load_failed)
    val loaded by produceState(PermissionData(), state.me?.id, state.revision, agentId, spaceId, retry) {
        value = PermissionData()
        try {
            do {
                value = model.repository.query { core ->
                    val devices = core.appDeviceGrants()
                    val scopes = if (agentId == null && spaceId == null) emptyMap() else devices.mapNotNull { grant ->
                        try {
                            val item = core.item(grant.itemId)
                            grant.itemId to PermissionItemScope(item.spaceId, item.createdBy.id)
                        } catch (error: Exception) {
                            if (error is CancellationException) throw error
                            null
                        }
                    }.toMap()
                    PermissionData(standingInScope(core.standingDecisions(), agentId, spaceId), deviceInScope(devices, scopes, agentId, spaceId), loading = false)
                }
                val once = value.device.filter { !it.always }
                if (once.isNotEmpty()) {
                    val remaining = once.minOf { it.atMs + 60_000L - System.currentTimeMillis() }
                    delay(remaining.coerceIn(500L, 60_000L))
                }
            } while (value.device.any { !it.always })
        } catch (error: Exception) {
            if (error is CancellationException) throw error
            value = PermissionData(loading = false, error = error.message ?: loadFailure)
        }
    }
    val snapshot = loaded
    val standing = snapshot.standing
    val device = snapshot.device
    val pending = revoking
    val agent = state.agents.firstOrNull { it.persona.id == agentId }?.persona
    val space = state.spaces.firstOrNull { it.id == spaceId }
    val locale = LocalConfiguration.current.locales[0]
    val time = remember(locale) { DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT, locale) }
    val revokedNotice = stringResource(R.string.ap_permission_revoked)
    val ownerId = state.me?.id
    val revoke: (String, Boolean) -> Unit = { id, app ->
        if (id !in revoking) {
            revoking = revoking + id
            model.launch {
                try {
                    if (model.state.value.me?.id != ownerId) return@launch
                    model.repository.change { core -> if (app) core.revokeAppDevice(id) else core.revokeStanding(id) }
                    retry++
                    model.notify(revokedNotice)
                } finally { revoking = revoking - id }
            }
        }
    }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.permissions), back) }) { padding ->
        LazyColumn(
            Modifier.padding(padding).fillMaxSize(),
            contentPadding = PaddingValues(24.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            if (agent != null || space != null) item {
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    agent?.let { Avatar(it, size = 48) }
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        agent?.let { Text(it.name, style = MaterialTheme.typography.titleLarge) }
                        space?.let { Text(it.title, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    }
                }
            }
            if (snapshot.loading) item { LinearProgressIndicator(Modifier.fillMaxWidth()) }
            if (snapshot.error != null) item {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(stringResource(R.string.ap_permission_load_failed), color = MaterialTheme.colorScheme.error)
                    Text(snapshot.error, style = MaterialTheme.typography.bodySmall)
                    TextButton(onClick = { retry++ }) { Text(stringResource(R.string.ap_retry)) }
                }
            }
            item { SectionLabel(stringResource(R.string.ap_standing_title)) }
            if (!snapshot.loading && snapshot.error == null && standing.isEmpty()) item { Text(stringResource(R.string.ap_standing_empty), color = MaterialTheme.colorScheme.onSurfaceVariant) }
            items(standing, key = { "standing:${it.grantId}" }) { grant ->
                Card {
                    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Avatar(grant.agent, size = 36)
                            Column {
                                Text(grant.agent.name, style = MaterialTheme.typography.titleMedium)
                                Text(grant.spaceTitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                        Text(grant.actionLabel, style = MaterialTheme.typography.bodyLarge)
                        Text(stringResource(if (grant.allow) R.string.always_approve else R.string.always_deny), color = if (grant.allow) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error)
                        Text(time.format(Date(grant.atMs)), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        PermissionRevoke(grant.grantId in pending) { revoke(grant.grantId, false) }
                    }
                }
            }
            item { Text(stringResource(R.string.ap_standing_explanation), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            item { SectionLabel(stringResource(R.string.ap_device_title)) }
            if (spaceId != null || agentId != null) item {
                Text(stringResource(if (spaceId != null) R.string.ap_device_chat_scope else R.string.ap_device_agent_scope), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            if (!snapshot.loading && snapshot.error == null && device.isEmpty()) item { Text(stringResource(R.string.ap_device_empty), color = MaterialTheme.colorScheme.onSurfaceVariant) }
            items(device, key = { "device:${it.grantId}" }) { grant ->
                Card {
                    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Icon(permissionIcon(grant.capability), null, Modifier.size(28.dp), tint = MaterialTheme.colorScheme.primary)
                            Column {
                                Text(permissionCapability(grant.capability), style = MaterialTheme.typography.titleMedium)
                                Text(stringResource(R.string.ap_device_source, grant.appName, grant.spaceTitle), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                        Text(grant.itemTitle, style = MaterialTheme.typography.bodyMedium)
                        if (grant.purpose.isNotBlank()) Text(grant.purpose, style = MaterialTheme.typography.bodySmall)
                        Text(stringResource(if (grant.always) R.string.ap_device_always else R.string.ap_device_once), color = MaterialTheme.colorScheme.primary)
                        Text(time.format(Date(grant.atMs)), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        PermissionRevoke(grant.grantId in pending) { revoke(grant.grantId, true) }
                    }
                }
            }
            item { Text(stringResource(R.string.ap_device_explanation), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
    }
}

@Composable
private fun PermissionRevoke(pending: Boolean, revoke: () -> Unit) {
    TextButton(onClick = revoke, enabled = !pending) {
        if (pending) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
        else Text(stringResource(R.string.revoke), color = MaterialTheme.colorScheme.error)
    }
}

private fun permissionIcon(capability: String) = when (capability) {
    "location", "location.approximate" -> Icons.Rounded.LocationOn
    "photos.pick" -> Icons.Rounded.PhotoLibrary
    "camera.capture" -> Icons.Rounded.PhotoCamera
    "calendar.freebusy", "calendar.events", "calendar.add" -> Icons.Rounded.CalendarMonth
    "contacts.pick" -> Icons.Rounded.Person
    "health.steps" -> Icons.Rounded.DirectionsWalk
    else -> Icons.Rounded.Language
}

@Composable
private fun permissionCapability(capability: String): String = when (capability) {
    "location.approximate" -> stringResource(R.string.ap_cap_location_approximate)
    "location" -> stringResource(R.string.ap_cap_location_precise)
    "photos.pick" -> stringResource(R.string.ap_cap_photos)
    "camera.capture" -> stringResource(R.string.ap_cap_camera)
    "calendar.freebusy" -> stringResource(R.string.ap_cap_calendar_busy)
    "calendar.events" -> stringResource(R.string.ap_cap_calendar_events)
    "calendar.add" -> stringResource(R.string.ap_cap_calendar_add)
    "contacts.pick" -> stringResource(R.string.ap_cap_contacts)
    "health.steps" -> stringResource(R.string.ap_cap_steps)
    else -> if (capability.startsWith("net:")) stringResource(R.string.ap_cap_network, capability.removePrefix("net:")) else capability
}
