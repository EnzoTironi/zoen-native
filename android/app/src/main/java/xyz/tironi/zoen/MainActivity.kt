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

class MainActivity : ComponentActivity() {
    private val model: ZoenViewModel by viewModels()
    private var deepLink by mutableStateOf<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        deepLink = intent.dataString
        model.boot(BuildConfig.DEBUG && intent.getBooleanExtra("demo", false))
        setContent { ZoenTheme { ZoenApp(model, deepLink) { deepLink = null } } }
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) { model.launch { model.repository.refresh() } }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        deepLink = intent.dataString
    }
}
