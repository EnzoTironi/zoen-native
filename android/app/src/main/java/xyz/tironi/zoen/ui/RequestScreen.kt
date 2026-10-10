package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState

@Composable
fun RequestScreen(model: ZoenViewModel, state: AppState, id: String, navigate: (NavKey) -> Unit, back: () -> Unit) {
    val request = state.requests.firstOrNull { it.id == id }
    var confirm by remember { mutableStateOf<RequestDecision?>(null) }
    var busy by remember { mutableStateOf(false) }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.review), back) }) { padding ->
        if (request != null) LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            item { Avatar(request.agent, size = 68, onClick = { navigate(Agent(request.agent.id)) }) }
            item { Text(request.title, style = MaterialTheme.typography.headlineMedium); Text(request.agent.name + " · " + request.spaceTitle, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            item { Text(request.detail, style = MaterialTheme.typography.bodyLarge) }
            item { Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.secondaryContainer)) {
                Column(Modifier.fillMaxWidth().padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Icon(Icons.Rounded.VerifiedUser, null)
                    Text(request.reason)
                    Text(request.audience, style = MaterialTheme.typography.bodyMedium)
                    request.costCents?.let { Text(formatMoney(it, model.repository.locale), style = MaterialTheme.typography.titleLarge) }
                }
            } }
            request.itemId?.let { itemId -> item { OutlinedButton(onClick = { navigate(Item(itemId)) }) { Text(stringResource(R.string.files)) } } }
            item { Text(stringResource(requestStatusLabel(request.status)), color = MaterialTheme.colorScheme.primary) }
            if (request.status == RequestStatus.PENDING && request.agent.isMine) {
                item { Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    OutlinedButton(onClick = { confirm = RequestDecision.DENY }, enabled = !busy, modifier = Modifier.weight(1f)) { Text(stringResource(R.string.deny)) }
                    Button(onClick = { confirm = RequestDecision.APPROVE }, enabled = !busy, modifier = Modifier.weight(1f)) { Text(stringResource(R.string.approve)) }
                } }
                if (request.canAlwaysApprove) item { TextButton(onClick = { confirm = RequestDecision.ALWAYS_APPROVE }, enabled = !busy) { Text(stringResource(R.string.always_approve)) } }
                item { TextButton(onClick = { confirm = RequestDecision.ALWAYS_DENY }, enabled = !busy) { Text(stringResource(R.string.always_deny)) } }
            }
        }
    }
    confirm?.let { decision ->
        val label = when (decision) { RequestDecision.APPROVE -> R.string.approve; RequestDecision.DENY -> R.string.deny; RequestDecision.ALWAYS_APPROVE -> R.string.always_approve; RequestDecision.ALWAYS_DENY -> R.string.always_deny }
        AlertDialog(onDismissRequest = { confirm = null }, title = { Text(stringResource(label)) }, text = { Text(request?.detail.orEmpty()) }, confirmButton = {
            TextButton(onClick = {
                confirm = null; busy = true
                model.launch { try { val result = model.repository.change { it.decideRequest(id, decision) }; model.notify(result.message) } finally { busy = false } }
            }) { Text(stringResource(label)) }
        }, dismissButton = { TextButton(onClick = { confirm = null }) { Text(stringResource(R.string.cancel)) } })
    }
}
