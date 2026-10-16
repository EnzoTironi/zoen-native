package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import org.json.JSONArray
import org.json.JSONObject
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.data.AppState

private fun JSONArray?.tierRows(): List<JSONObject> = if (this == null) emptyList() else (0 until length()).mapNotNull { optJSONObject(it) }
private fun JSONArray?.tierIds(): List<String> = if (this == null) emptyList() else (0 until length()).map { optString(it) }

/** Tier labels describe key-backed access. Every change is decided by the shared core. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SpaceTiersScreen(model: ZoenViewModel, state: AppState, spaceId: String, initialInvite: String = "", navigate: (NavKey) -> Unit, back: () -> Unit) {
    var home by remember(spaceId) { mutableStateOf<JSONObject?>(null) }
    var reload by remember { mutableIntStateOf(0) }
    var failure by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    var success by remember { mutableStateOf<String?>(null) }
    var name by rememberSaveable { mutableStateOf("") }
    var expiry by rememberSaveable { mutableLongStateOf(System.currentTimeMillis() + 7 * 86400000L) }
    var chooseExpiryDate by remember { mutableStateOf(false) }
    var chooseExpiryTime by remember { mutableStateOf(false) }
    var token by rememberSaveable { mutableStateOf(initialInvite) }
    var invite by remember { mutableStateOf<String?>(null) }
    val context = androidx.compose.ui.platform.LocalContext.current
    var groupNames by remember { mutableStateOf<Map<String, String>>(emptyMap()) }
    var pageNames by remember { mutableStateOf<Map<String, String>>(emptyMap()) }
    LaunchedEffect(spaceId, state.revision, reload) {
        try { home = model.repository.query { JSONObject(it.livingSpaceCommand(spaceId, "{\"action\":\"home\"}")) }; failure = null }
        catch (error: Exception) { failure = error.message }
    }
    fun command(value: JSONObject) {
        if (busy) return
        busy = true; success = null
        model.launch {
            try {
                val result = model.repository.change { it.livingSpaceCommand(spaceId, value.toString()) }
                if (value.optString("action") == "tier_invite") {
                    val parsed = runCatching { JSONObject(result) }.getOrNull()
                    invite = parsed?.optString("invite_url")?.takeIf { it.isNotBlank() } ?: result.trim('"')
                }
                if (value.optString("action") == "tier_invite_revoke") { invite = null }
                reload++; failure = null; success = when (value.optString("action")) { "tier_assign" -> "Access assigned"; "tier_revoke" -> "Access removed"; "tier_invite" -> "Invite created"; "tier_redeem" -> "Access requested"; "tier_claim_approve" -> "Access approved"; else -> "Updated" }
            } catch (error: Exception) { failure = error.message }
            finally { busy = false }
        }
    }
    if (chooseExpiryDate) {
        val zone = java.time.ZoneId.systemDefault()
        val old = java.time.Instant.ofEpochMilli(expiry).atZone(zone)
        val picker = rememberDatePickerState(initialSelectedDateMillis = old.toLocalDate().atStartOfDay(java.time.ZoneOffset.UTC).toInstant().toEpochMilli())
        DatePickerDialog(onDismissRequest = { chooseExpiryDate = false }, confirmButton = { TextButton({
            picker.selectedDateMillis?.let { chosen -> expiry = java.time.Instant.ofEpochMilli(chosen).atZone(java.time.ZoneOffset.UTC).toLocalDate().atTime(old.toLocalTime()).atZone(zone).toInstant().toEpochMilli() }
            chooseExpiryDate = false
        }) { Text("Done") } }, dismissButton = { TextButton({ chooseExpiryDate = false }) { Text("Cancel") } }) { DatePicker(picker) }
    }
    if (chooseExpiryTime) {
        val zone = java.time.ZoneId.systemDefault(); val old = java.time.Instant.ofEpochMilli(expiry).atZone(zone)
        val picker = rememberTimePickerState(initialHour = old.hour, initialMinute = old.minute)
        AlertDialog(onDismissRequest = { chooseExpiryTime = false }, title = { Text("Invitation expiry") }, text = { TimePicker(picker) }, confirmButton = { TextButton({ expiry = old.toLocalDate().atTime(picker.hour, picker.minute).atZone(zone).toInstant().toEpochMilli(); chooseExpiryTime = false }) { Text("Done") } }, dismissButton = { TextButton({ chooseExpiryTime = false }) { Text("Cancel") } })
    }
    val admin = home?.optBoolean("can_manage") == true
    val members = state.spaces.firstOrNull { it.id == spaceId }?.members.orEmpty()
    val tiers = home?.optJSONArray("tiers").tierRows()
    Scaffold(topBar = { ScreenBar("Memberships", back) }) { padding ->
        Column(Modifier.padding(padding).fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp).testTag("living-space-tiers"), verticalArrangement = Arrangement.spacedBy(24.dp)) {
            Text("Memberships", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
            Text("Named access to conversations, pages and events.", color = MaterialTheme.colorScheme.onSurfaceVariant)
            failure?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            success?.let { LivingSpaceFeedback(it, reload) }
            if (admin) {
                OutlinedTextField(name, { name = it }, label = { Text("Membership name") }, modifier = Modifier.fillMaxWidth())
                Button(enabled = !busy && name.isNotBlank(), onClick = { command(JSONObject().put("action", "tier_create").put("name", name.trim())) }) { Text("Create membership") }
            }
            if (tiers.isEmpty()) Text("No memberships yet.", color = MaterialTheme.colorScheme.onSurfaceVariant)
            tiers.forEach { tier ->
                val id = tier.getString("id")
                HorizontalDivider()
                Surface(shape = androidx.compose.foundation.shape.RoundedCornerShape(50), color = MaterialTheme.colorScheme.surfaceContainerHigh) { Text(tier.optString("name"), Modifier.padding(horizontal = 10.dp, vertical = 4.dp), style = MaterialTheme.typography.labelMedium) }
                tier.optJSONArray("members").tierIds().forEach { who ->
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                        Text(members.firstOrNull { it.id == who }?.name ?: "Member", Modifier.weight(1f).padding(top = 12.dp))
                        if (admin) TextButton(enabled = !busy, onClick = { command(JSONObject().put("action", "tier_revoke").put("tier_id", id).put("identity_id", who)) }) { Text("Remove access") }
                    }
                }
                tier.optJSONArray("groups").tierIds().forEach { group -> TextButton({ navigate(Chat(group)) }) { Text(state.spaces.firstOrNull { it.id == group }?.title ?: "Conversation") } }
                tier.optJSONArray("resources").tierRows().forEach { resource ->
                    state.items.firstOrNull { it.id == resource.optString("id") }?.let { item -> TextButton({ navigate(Item(item.id)) }) { Text(item.title) } }
                }
                if (admin) {
                    Text("Assign access", style = MaterialTheme.typography.titleMedium)
                    members.filter { it.id !in tier.optJSONArray("members").tierIds() }.forEach { person ->
                        TextButton(enabled = !busy, onClick = { command(JSONObject().put("action", "tier_assign").put("tier_id", id).put("identity_id", person.id)) }) { Text("Add ${person.name}") }
                    }
                    Text("Invitation expiry", style = MaterialTheme.typography.titleMedium)
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        TextButton({ chooseExpiryDate = true }) { Text(java.text.DateFormat.getDateInstance().format(java.util.Date(expiry))) }
                        TextButton({ chooseExpiryTime = true }) { Text(java.text.DateFormat.getTimeInstance(java.text.DateFormat.SHORT).format(java.util.Date(expiry))) }
                    }
                    TextButton(enabled = !busy && expiry > System.currentTimeMillis(), onClick = { command(JSONObject().put("action", "tier_invite").put("tier_id", id).put("expires_at_ms", expiry)) }) { Text("Create invitation") }
                    OutlinedTextField(groupNames[id].orEmpty(), { groupNames = groupNames + (id to it) }, label = { Text("Conversation name") }, modifier = Modifier.fillMaxWidth())
                    TextButton(enabled = !busy && !groupNames[id].isNullOrBlank(), onClick = { command(JSONObject().put("action", "tier_group_create").put("tier_id", id).put("title", groupNames[id].orEmpty().trim())) }) { Text("Create conversation") }
                    OutlinedTextField(pageNames[id].orEmpty(), { pageNames = pageNames + (id to it) }, label = { Text("Page title") }, modifier = Modifier.fillMaxWidth())
                    TextButton(enabled = !busy && !pageNames[id].isNullOrBlank(), onClick = {
                        busy = true
                        model.launch {
                            try {
                                val page = model.repository.change { it.pageCreate(tier.getString("space"), pageNames[id].orEmpty().trim()) }
                                model.repository.change { it.livingSpaceCommand(spaceId, JSONObject().put("action", "tier_bind").put("tier_id", id).put("item_id", page.id).put("kind", "page").toString()) }
                                reload++; navigate(Item(page.id))
                            } catch (error: Exception) { failure = error.message }
                            finally { busy = false }
                        }
                    }) { Text("Create membership page") }
                    Text("Unlock pages and events", style = MaterialTheme.typography.titleMedium)
                    val partition = tier.optString("space")
                    state.items.filter { it.spaceId == partition && (it.kindId == "page" || it.app?.appId == "schedule") }.forEach { item ->
                        TextButton(enabled = !busy, onClick = { command(JSONObject().put("action", "tier_bind").put("tier_id", id).put("item_id", item.id).put("kind", if (item.kindId == "page") "page" else "event")) }) { Text(item.title) }
                    }
                }
            }
            invite?.let { invitation -> TextButton({
                context.getSystemService(android.content.ClipboardManager::class.java).setPrimaryClip(android.content.ClipData.newPlainText("Invitation", invitation))
            }) { Text("Copy invitation") } }
            if (admin) {
                val invitations = home?.optJSONArray("invitations").tierRows()
                if (invitations.isNotEmpty()) Text("Invitations", style = MaterialTheme.typography.titleMedium)
                invitations.forEach { invitation ->
                    Text(tiers.firstOrNull { it.optString("id") == invitation.optString("tier_id") }?.optString("name") ?: "Membership", fontWeight = FontWeight.SemiBold)
                    Text(java.text.DateFormat.getDateTimeInstance().format(java.util.Date(invitation.optLong("expires_at_ms"))), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    if (invitation.optBoolean("revoked")) Text("Revoked", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    else TextButton(enabled = !busy, onClick = { command(JSONObject().put("action", "tier_invite_revoke").put("invite_id", invitation.getString("id"))) }) { Text("Revoke invitation") }
                }
            }
            if (admin) home?.optJSONArray("tier_claims").tierRows().forEach { claim ->
                HorizontalDivider(); Text("Membership request", style = MaterialTheme.typography.titleMedium)
                Text(members.firstOrNull { it.id == claim.optString("identity_id") }?.name ?: "New member")
                TextButton(enabled = !busy, onClick = { command(JSONObject().put("action", "tier_claim_approve").put("identity_id", claim.getString("identity_id")).put("invite_id", claim.getString("invite_id"))) }) { Text("Approve access") }
            }
            HorizontalDivider(); Text("Use an invite", style = MaterialTheme.typography.titleMedium)
            OutlinedTextField(token, { token = it }, label = { Text("Invitation") }, visualTransformation = androidx.compose.ui.text.input.PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth())
            Button(enabled = !busy && token.isNotBlank(), onClick = {
                val parsed = android.net.Uri.parse(token.trim())
                if (parsed.getQueryParameter("space") != null && parsed.getQueryParameter("space") != spaceId) failure = "This invitation belongs to another Space."
                else command(JSONObject().put("action", "tier_redeem").put("token", parsed.getQueryParameter("token") ?: token.trim()))
            }) { Text("Request access") }
        }
    }
}
