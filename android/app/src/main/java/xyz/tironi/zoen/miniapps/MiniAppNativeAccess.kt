package xyz.tironi.zoen.miniapps

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CompletableDeferred
import org.json.JSONObject
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.ItemDetail

/** Native views use the same signed, item-scoped access as MCP views, before Android's picker. */
@Composable
fun rememberMiniAppNativeAccess(model: ZoenViewModel, item: ItemDetail): suspend (String, JSONObject) -> Any {
    val device = rememberMiniAppDeviceActions()
    val denied = stringResource(R.string.miniapp_permission_denied)
    var request by remember(item.id) { mutableStateOf<PendingMiniAppPrompt?>(null) }
    DisposableEffect(item.id) { onDispose { request?.result?.cancel() } }
    request?.let { pending ->
        fun decide(value: MiniAppConsent) { pending.result.complete(value); if (request === pending) request = null }
        AlertDialog(onDismissRequest = { decide(MiniAppConsent.DENY) },
            title = { Text(stringResource(R.string.miniapp_allow_capability, pending.prompt.appName)) },
            text = { Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(capabilityName(pending.prompt.capability.orEmpty()), style = MaterialTheme.typography.titleMedium)
                Text(pending.prompt.detail)
            } },
            confirmButton = { Column {
                TextButton(onClick = { decide(MiniAppConsent.ALWAYS) }) { Text(stringResource(R.string.miniapp_always)) }
                TextButton(onClick = { decide(MiniAppConsent.ONCE) }) { Text(stringResource(R.string.miniapp_allow_once)) }
            } },
            dismissButton = { TextButton(onClick = { decide(MiniAppConsent.DENY) }) { Text(stringResource(R.string.miniapp_deny)) } })
    }
    return remember(item.id, model.repository, device) {
        { capability, params ->
            check(capability in device.available)
            val app = requireNotNull(item.app)
            val purpose = model.repository.query { core ->
                val resource = core.readAppResource(app.resourceUri)
                val manifest = requireNotNull(MiniAppManifest.decode(resource.manifestJson, app.appId))
                require(manifest.verifies(resource.text))
                requireNotNull(manifest.capabilities[capability])
            }
            if (!model.repository.query { it.appDeviceAllowed(item.id, capability) }) {
                val result = CompletableDeferred<MiniAppConsent>()
                request = PendingMiniAppPrompt(MiniAppPrompt(app.name, "$purpose\n\n${item.spaceTitle}", app.name, capability), result, true)
                val decision = try { result.await() } finally { if (request?.result === result) request = null }
                check(decision != MiniAppConsent.DENY) { denied }
                model.repository.change { it.grantAppDevice(item.id, capability, purpose, decision == MiniAppConsent.ALWAYS) }
            }
            device.perform(capability, params)
        }
    }
}
