package xyz.tironi.zoen.miniapps

import android.content.Intent
import android.net.Uri
import android.view.HapticFeedbackConstants
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlinx.coroutines.CompletableDeferred
import org.json.JSONArray
import org.json.JSONObject
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.ItemDetail
import xyz.tironi.zoen.core.PersonaKind
import xyz.tironi.zoen.data.AppState

data class PendingMiniAppPrompt(val prompt: MiniAppPrompt, val result: CompletableDeferred<MiniAppConsent>, val grant: Boolean)

@Composable
fun McpAppView(
    model: ZoenViewModel,
    state: AppState,
    item: ItemDetail,
    modifier: Modifier = Modifier,
    displayMode: String = "fullscreen",
    onClose: () -> Unit = {},
    onFullscreen: () -> Unit = {},
) {
    if (!MiniAppWebProviderGate(modifier)) return
    val context = LocalContext.current
    val configuration = LocalConfiguration.current
    val density = LocalDensity.current
    val failed = stringResource(R.string.miniapp_failed)
    val localView = LocalView.current
    val owner = LocalLifecycleOwner.current
    val device = rememberMiniAppDeviceActions()
    var pending by remember(item.id) { mutableStateOf<PendingMiniAppPrompt?>(null) }
    var ready by remember(item.id) { mutableStateOf(false) }
    var failure by remember(item.id) { mutableStateOf<String?>(null) }
    var reportedHeight by remember(item.id) { mutableFloatStateOf(360f) }
    val session = remember(item.id, model.repository) {
        McpAppSession(item.id, RepositoryMiniAppGateway(model.repository),
            confirm = { prompt ->
                val decision = CompletableDeferred<MiniAppConsent>()
                pending = PendingMiniAppPrompt(prompt, decision, false)
                try { decision.await() != MiniAppConsent.DENY } finally { if (pending?.result === decision) pending = null }
            },
            consent = { prompt ->
                val decision = CompletableDeferred<MiniAppConsent>()
                pending = PendingMiniAppPrompt(prompt, decision, true)
                try { decision.await() } finally { if (pending?.result === decision) pending = null }
            },
            native = { capability, params ->
                if (capability == "haptics") {
                    localView.performHapticFeedback(HapticFeedbackConstants.CONTEXT_CLICK)
                    JSONObject().put("performed", true)
                } else device.perform(capability, params)
            }, nativeAvailable = device.available,
            openLink = { link -> context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(link)).addCategory(Intent.CATEGORY_BROWSABLE)) },
            onDisplay = { mode -> if (mode == "inline" && displayMode == "fullscreen") onClose() else if (mode == "fullscreen" && displayMode == "inline") onFullscreen() },
            haptic = { kind -> localView.performHapticFeedback(if (android.os.Build.VERSION.SDK_INT >= 30 && kind == "warning") HapticFeedbackConstants.REJECT else if (android.os.Build.VERSION.SDK_INT >= 30 && kind == "success") HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.CONTEXT_CLICK) },
            onError = { failure = it }, onHeight = { reportedHeight = it.coerceIn(120f, 640f) },
            onToolCompleted = { tool -> if (tool == "hike_decide") model.planHikeIfReady(item.id) },
        ).also { it.displayMode = displayMode }
    }
    val members = JSONArray().apply {
        val people = state.spaces.firstOrNull { it.id == item.spaceId }?.members.orEmpty().filter { it.kind != PersonaKind.AGENT }.toMutableList()
        state.me?.let { me -> if (people.none { it.id == me.id }) people.add(0, me) }
        people.sortedByDescending { it.isMe }.forEach { person -> put(JSONObject().put("id", person.id).put("name", person.name).put("initials", person.initials).put("color", person.tintHex).put("isMe", person.isMe)) }
    }
    val dark = isSystemInDarkTheme()
    val locale = configuration.locales[0].toLanguageTag()
    LaunchedEffect(session) {
        try {
            session.update(item, locale, dark, 360f, density.fontScale, members)
            session.prepare()
            ready = true
        } catch (error: Exception) {
            if (error is kotlinx.coroutines.CancellationException) throw error
            failure = error.message ?: failed
        }
    }
    DisposableEffect(session, owner) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_START) session.visible(true)
            if (event == Lifecycle.Event.ON_STOP) session.visible(false)
        }
        owner.lifecycle.addObserver(observer)
        onDispose { owner.lifecycle.removeObserver(observer); pending?.result?.cancel(); session.dispose() }
    }
    BoxWithConstraints(modifier.testTag("mcp-app:${item.id}")) {
        SideEffect { session.update(item, locale, dark, maxWidth.value, density.fontScale, members) }
        when {
            failure != null -> Column(Modifier.fillMaxWidth().padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.miniapp_failed), style = MaterialTheme.typography.titleLarge)
                Text(failure.orEmpty(), color = MaterialTheme.colorScheme.error)
            }
            !ready -> Box(Modifier.fillMaxSize(), contentAlignment = androidx.compose.ui.Alignment.Center) { CircularProgressIndicator() }
            else -> AndroidView(factory = { session.createWebView(it) }, modifier = if (displayMode == "inline") Modifier.fillMaxWidth().height(reportedHeight.dp) else Modifier.fillMaxSize())
        }
    }
    pending?.let { request ->
        fun decide(value: MiniAppConsent) { request.result.complete(value); if (pending === request) pending = null }
        AlertDialog(onDismissRequest = { decide(MiniAppConsent.DENY) },
            title = { Text(if (request.grant) stringResource(R.string.miniapp_allow_capability, request.prompt.appName) else request.prompt.title) },
            text = { Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(request.prompt.appName, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                request.prompt.capability?.let { Text(capabilityName(it), style = MaterialTheme.typography.titleMedium) }
                Text(request.prompt.detail)
            } },
            confirmButton = {
                if (request.grant) Column {
                    TextButton(onClick = { decide(MiniAppConsent.ALWAYS) }) { Text(stringResource(R.string.miniapp_always)) }
                    TextButton(onClick = { decide(MiniAppConsent.ONCE) }) { Text(stringResource(R.string.miniapp_allow_once)) }
                } else TextButton(onClick = { decide(MiniAppConsent.ONCE) }) { Text(stringResource(R.string.confirm_action)) }
            },
            dismissButton = { TextButton(onClick = { decide(MiniAppConsent.DENY) }) { Text(stringResource(if (request.grant) R.string.miniapp_deny else R.string.cancel)) } })
    }
}

@Composable
fun capabilityName(capability: String): String = when (capability) {
    "photos.pick" -> stringResource(R.string.miniapp_photos)
    "camera.capture" -> stringResource(R.string.miniapp_camera)
    "location", "location.approximate" -> stringResource(R.string.miniapp_location)
    "calendar.freebusy" -> stringResource(R.string.miniapp_calendar_busy)
    "calendar.events" -> stringResource(R.string.miniapp_calendar_events)
    "calendar.add" -> stringResource(R.string.miniapp_calendar_add)
    "contacts.pick" -> stringResource(R.string.miniapp_contacts)
    "clipboard.read", "clipboard.write" -> stringResource(R.string.miniapp_clipboard)
    else -> if (capability.startsWith("net:")) stringResource(R.string.miniapp_network, capability.removePrefix("net:")) else capability
}
