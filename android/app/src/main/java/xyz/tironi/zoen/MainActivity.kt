package xyz.tironi.zoen

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.launch
import xyz.tironi.zoen.theme.ZoenTheme
import xyz.tironi.zoen.ui.ZoenApp
import xyz.tironi.zoen.background.MessagingService

class MainActivity : ComponentActivity() {
    private val model: ZoenViewModel by viewModels()
    private var deepLink by mutableStateOf<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        deepLink = if (savedInstanceState == null) intent.dataString else savedInstanceState.getString(PENDING_LINK)
        model.boot(BuildConfig.DEBUG && intent.getBooleanExtra("demo", false))
        setContent { ZoenTheme { ZoenApp(model, deepLink) { deepLink = null } } }
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) { model.launch { model.repository.refresh() } }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        // Keep the launch identity stable; incoming links are consumable navigation events.
        deepLink = intent.dataString
    }

    override fun onSaveInstanceState(outState: Bundle) {
        // A saved null means the UI already handled the link, including after a cold launch.
        outState.putString(PENDING_LINK, deepLink)
        super.onSaveInstanceState(outState)
    }

    override fun onStart() {
        super.onStart()
        model.repository.setAppVisible(true, this)
        if (model.repository.preferences.getBoolean(MessagingService.PREFERENCE, false) && !model.state.value.demo) MessagingService.start(this)
    }

    override fun onStop() { model.repository.setAppVisible(false, this); super.onStop() }

    private companion object { const val PENDING_LINK = "zoen.pending-link" }
}
