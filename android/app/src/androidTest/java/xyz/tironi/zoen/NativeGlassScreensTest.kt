package xyz.tironi.zoen

import androidx.activity.ComponentActivity
import androidx.compose.runtime.getValue
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.core.SpaceKindDto
import xyz.tironi.zoen.theme.ZoenTheme
import xyz.tironi.zoen.ui.ChatScreen
import xyz.tironi.zoen.ui.ConversationsScreen
import xyz.tironi.zoen.ui.Onboarding

/** Reproducible demo-only screen evidence; never opens a real account. */
@RunWith(AndroidJUnit4::class)
class NativeGlassScreensTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    private fun capture(screen: String) {
        val app = ApplicationProvider.getApplicationContext<ZoenApplication>()
        app.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        runBlocking { app.repository.boot(true); app.repository.resetDemo() }
        val group = app.repository.state.value.spaces.first { it.kind == SpaceKindDto.GROUP }.id
        val model = ViewModelProvider(compose.activity)[ZoenViewModel::class.java]
        compose.setContent {
            val state by model.state.collectAsStateWithLifecycle()
            ZoenTheme {
                when (screen) {
                    "onboarding" -> Onboarding(model)
                    "chats" -> ConversationsScreen(model, state, {}, false)
                    else -> ChatScreen(model, state, group, {}, {})
                }
            }
        }
        compose.waitForIdle()
        // Native animated artwork draws asynchronously after Compose is idle.
        android.os.SystemClock.sleep(1_500)
        CommittedWindowCapture.save(compose.activity, "native-glass-screens", screen)
    }

    @Test fun onboarding() = capture("onboarding")
    @Test fun chats() = capture("chats")
    @Test fun group() = capture("group")
}
