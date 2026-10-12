package xyz.tironi.zoen.miniapps

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.webkit.WebViewFeature
import xyz.tironi.zoen.R

/** Provider capabilities, rather than the Android API level, decide whether HTML is safe. */
object MiniAppWebProvider {
    fun supported(): Boolean = runCatching {
        WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER) &&
            WebViewFeature.isFeatureSupported(WebViewFeature.MULTI_PROFILE)
    }.getOrDefault(false)

    fun requireSupported() {
        check(supported()) { "Update Android System WebView to open this mini-app" }
    }
}

/** This gate runs before creating a host, WebView, or requesting any mini-app capability. */
@Composable
fun MiniAppWebProviderGate(modifier: Modifier = Modifier): Boolean {
    val supported = remember { MiniAppWebProvider.supported() }
    if (!supported) {
        val context = LocalContext.current
        Column(modifier.testTag("mcp-web-unsupported").fillMaxWidth().padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text(stringResource(R.string.miniapp_webview_update_title), style = MaterialTheme.typography.titleLarge)
            Text(stringResource(R.string.miniapp_webview_update_detail))
            OutlinedButton(onClick = {
                try { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://play.google.com/store/apps/details?id=com.google.android.webview"))) }
                catch (_: ActivityNotFoundException) { /* The visible instruction remains usable without a browser. */ }
            }, modifier = Modifier.testTag("mcp-web-update")) { Text(stringResource(R.string.miniapp_webview_update)) }
        }
    }
    return supported
}
