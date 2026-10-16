package xyz.tironi.zoen.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.json.JSONObject
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.SpaceKindDto
import xyz.tironi.zoen.data.AppState

private data class LivingReview(val space: String, val title: String, val identity: String, val invite: String? = null)

@Composable
fun LivingSpaceNotifications(model: ZoenViewModel, state: AppState) {
    var reviews by remember { mutableStateOf<List<LivingReview>>(emptyList()) }
    var failure by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    var reload by remember { mutableIntStateOf(0) }
    var lastApproved by remember { mutableStateOf(true) }
    LaunchedEffect(state.revision, reload) {
        try {
            reviews = model.repository.query { core -> state.spaces.filter { it.kind == SpaceKindDto.COMMUNITY }.flatMap { space ->
                val home = JSONObject(core.livingSpaceCommand(space.id, "{\"action\":\"home\"}"))
                if (!home.optBoolean("can_manage")) emptyList() else buildList {
                    home.optJSONArray("pending_requests")?.let { pending -> (0 until pending.length()).forEach { add(LivingReview(space.id, space.title, pending.getString(it))) } }
                    home.optJSONArray("tier_claims")?.let { claims -> (0 until claims.length()).forEach { index -> val claim = claims.getJSONObject(index); add(LivingReview(space.id, space.title, claim.getString("identity_id"), claim.getString("invite_id"))) } }
                }
            } }
            failure = null
        } catch (_: Exception) { failure = "Requests could not be loaded. Check your access and try again." }
    }
    fun decide(request: LivingReview, approve: Boolean) {
        if (busy) return
        busy = true
        model.launch {
            try {
                model.repository.change { core ->
                    val command = JSONObject().put("identity_id", request.identity)
                    if (request.invite != null) command.put("action", "tier_claim_approve").put("invite_id", request.invite)
                    else command.put("action", if (approve) "approve_join" else "reject_join")
                    core.livingSpaceCommand(request.space, command.toString())
                }
                lastApproved = approve; reload++; failure = null
            } catch (_: Exception) { failure = "This request was not updated. Check your access and try again." }
            finally { busy = false }
        }
    }
    Scaffold(topBar = { LivingScreenBar("") }) { padding ->
        Column(Modifier.padding(padding).fillMaxSize().background(MaterialTheme.colorScheme.surfaceContainerLow).verticalScroll(rememberScrollState()).padding(20.dp).testTag("living-space-notifications"), verticalArrangement = Arrangement.spacedBy(24.dp)) {
            Text("Notifications", style = MaterialTheme.typography.headlineLarge.copy(fontSize = 34.sp, lineHeight = 41.sp), fontWeight = FontWeight.Bold)
            if (reload > 0) LivingSpaceFeedback(if (lastApproved) "Request approved" else "Request declined", reload, positive = lastApproved)
            failure?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            if (reviews.isEmpty()) Text("No pending requests.", color = MaterialTheme.colorScheme.onSurfaceVariant)
            reviews.groupBy { it.space }.values.forEach { requests ->
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(requests.first().title, Modifier.padding(horizontal = 20.dp), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    requests.forEach { request ->
                        Surface(shape = RoundedCornerShape(26.dp), color = MaterialTheme.colorScheme.surface) {
                            Column(Modifier.fillMaxWidth().padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) { Icon(Icons.Rounded.PersonAdd, null, tint = MaterialTheme.colorScheme.primary); Text(if (request.invite == null) "Join request" else "Membership request", style = MaterialTheme.typography.titleMedium) }
                                Text(state.spaces.asSequence().flatMap { it.members }.firstOrNull { it.id == request.identity }?.name ?: "Community member", style = MaterialTheme.typography.bodyLarge)
                                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                                    FilledTonalButton(enabled = !busy, shape = RoundedCornerShape(50), onClick = { decide(request, true) }) { Icon(Icons.Rounded.Check, null); Spacer(Modifier.width(8.dp)); Text(if (request.invite == null) "Approve" else "Approve access") }
                                    if (request.invite == null) FilledTonalButton(enabled = !busy, shape = RoundedCornerShape(50), onClick = { decide(request, false) }) { Icon(Icons.Rounded.Close, null); Spacer(Modifier.width(8.dp)); Text("Decline") }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
