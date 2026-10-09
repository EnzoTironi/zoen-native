package xyz.tironi.zoen.agent

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.view.WindowManager
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ui.Avatar
import xyz.tironi.zoen.ui.ScreenBar

private fun Context.activity(): Activity? = when (this) { is Activity -> this; is ContextWrapper -> baseContext.activity(); else -> null }

@Composable
fun AgentBrowserCard(browser: AgentBrowser, spaceId: String, open: () -> Unit) {
    val state by browser.state.collectAsStateWithLifecycle()
    val session = state.session?.takeIf { it.spaceId == spaceId } ?: return
    Card(onClick = open, modifier = Modifier.fillMaxWidth().testTag("browser-card"), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Avatar(session.agent, size = 28)
                Text(browserTitle(state), Modifier.weight(1f), style = MaterialTheme.typography.titleSmall)
                Icon(if (state.phase == BrowserPhase.Finished) Icons.Rounded.CheckCircle else Icons.Rounded.Lock, null, tint = MaterialTheme.colorScheme.primary)
            }
            state.frame?.let { Image(it.asImageBitmap(), stringResource(R.string.agent_browser_screen), Modifier.fillMaxWidth().height(160.dp).clip(RoundedCornerShape(14.dp)), contentScale = ContentScale.Crop, alignment = Alignment.TopCenter) }
            Text(stringResource(R.string.agent_browser_demo), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            when (state.phase) {
                BrowserPhase.NeedsYou, BrowserPhase.Waiting -> {
                    Text(stringResource(R.string.agent_browser_needs_password, session.agent.name), style = MaterialTheme.typography.bodySmall)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (state.phase == BrowserPhase.NeedsYou) OutlinedButton(onClick = browser::notNow, Modifier.weight(1f).testTag("browser-not-now")) { Text(stringResource(R.string.agent_browser_not_now)) }
                        Button(onClick = { browser.takeOver(); open() }, Modifier.weight(1f).testTag("browser-takeover")) { Text(stringResource(R.string.agent_browser_take_over)) }
                    }
                }
                BrowserPhase.Driving -> Text(stringResource(R.string.agent_browser_stopped, session.agent.name), style = MaterialTheme.typography.bodySmall)
                BrowserPhase.Finished -> Text(stringResource(R.string.agent_browser_finished), style = MaterialTheme.typography.bodySmall)
                BrowserPhase.Browsing -> Text(stringResource(R.string.agent_browser_watching), style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}

@Composable
private fun browserTitle(state: BrowserState): String = stringResource(when (state.phase) {
    BrowserPhase.Browsing -> R.string.agent_browser_browsing
    BrowserPhase.NeedsYou -> R.string.agent_browser_needs_you
    BrowserPhase.Waiting -> R.string.agent_browser_waiting
    BrowserPhase.Driving -> R.string.agent_browser_driving
    BrowserPhase.Finished -> R.string.agent_browser_completed
}, state.session?.agent?.name.orEmpty())

@Composable
fun AgentBrowserScreen(browser: AgentBrowser, back: () -> Unit) {
    val state by browser.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    var password by remember { mutableStateOf("") }
    val close = { password = ""; browser.clearTypedInput(); back() }
    BackHandler(onBack = close)
    DisposableEffect(browser) {
        val window = context.activity()?.window
        val secured = window != null && window.attributes.flags.and(WindowManager.LayoutParams.FLAG_SECURE) != 0
        window?.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        onDispose {
            password = ""
            browser.clearTypedInput()
            if (!secured) window?.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        }
    }
    Scaffold(topBar = { ScreenBar(state.session?.site ?: stringResource(R.string.agent_browser), close) }) { padding ->
        Box(Modifier.fillMaxSize().padding(padding).imePadding(), contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 640.dp).fillMaxWidth().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Text(browserTitle(state), style = MaterialTheme.typography.titleMedium)
                Text(stringResource(R.string.agent_browser_demo), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                if (state.failure) Text(stringResource(R.string.agent_browser_error), color = MaterialTheme.colorScheme.error)
                state.frame?.let { frame ->
                    Image(frame.asImageBitmap(), stringResource(R.string.agent_browser_screen), Modifier.fillMaxWidth().aspectRatio(360f / 480f).clip(RoundedCornerShape(18.dp)).testTag("browser-screen")
                        .pointerInput(state.phase) { detectTapGestures { point -> browser.click(point.x / size.width.toDouble() * 360, point.y / size.height.toDouble() * 480) } }, contentScale = ContentScale.Fit)
                }
                when (state.phase) {
                    BrowserPhase.Driving -> {
                        Text(stringResource(R.string.agent_browser_stopped, state.session?.agent?.name.orEmpty()), style = MaterialTheme.typography.bodyMedium)
                        OutlinedTextField(password, { new -> if (new.codePointCount(0, new.length) <= 1024) { browser.typeChanged(password, new); password = new } },
                            Modifier.fillMaxWidth().testTag("browser-password"), label = { Text(stringResource(R.string.agent_browser_password)) }, singleLine = true,
                            visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false))
                        Button(onClick = { password = ""; browser.done() }, enabled = state.ownership.typed > 0 && !state.failure, modifier = Modifier.fillMaxWidth().testTag("browser-done")) { Text(stringResource(R.string.agent_browser_done)) }
                    }
                    BrowserPhase.NeedsYou, BrowserPhase.Waiting -> Button(onClick = browser::takeOver, modifier = Modifier.fillMaxWidth().testTag("browser-takeover-screen"), enabled = !state.failure) { Text(stringResource(R.string.agent_browser_take_over)) }
                    BrowserPhase.Finished -> {
                        Text(stringResource(R.string.agent_browser_finished))
                        OutlinedButton(onClick = close, Modifier.fillMaxWidth().testTag("browser-back-to-chat")) { Text(stringResource(R.string.agent_browser_back)) }
                    }
                    BrowserPhase.Browsing -> Text(stringResource(R.string.agent_browser_watching))
                }
            }
        }
    }
}
