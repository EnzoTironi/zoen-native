package xyz.tironi.zoen

import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import android.os.SystemClock
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextReplacement
import androidx.lifecycle.ViewModelProvider
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import kotlinx.coroutines.runBlocking
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.theme.AppAppearance

/** Uses the real activity and platform IME; Compose semantics selects the field deterministically. */
@RunWith(AndroidJUnit4::class)
class AndroidParityComposerCaptureTest {
    @get:Rule val compose = createEmptyComposeRule()

    @Test fun composerLight() = capture(false)
    @Test fun composerDark() = capture(true)

    private fun await(condition: () -> Boolean) {
        val deadline = SystemClock.uptimeMillis() + 30_000
        while (!condition()) {
            check(SystemClock.uptimeMillis() < deadline) { "Real composer/IME did not become ready" }
            SystemClock.sleep(100)
        }
    }

    private fun capture(dark: Boolean) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = ApplicationProvider.getApplicationContext<ZoenApplication>()
        NativeParityFixtureSetup.install(app)
        val previous = app.repository.preferences.getString("appearance", "auto") ?: "auto"
        app.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        instrumentation.runOnMainSync { AppAppearance.set(app, if (dark) "dark" else "light") }
        try {
            ActivityScenario.launch<MainActivity>(Intent(app, MainActivity::class.java).putExtra("demo", true)).use { scenario ->
                await { app.repository.state.value.ready && app.repository.state.value.me != null }
                runBlocking { app.repository.resetDemo() }
                val chat = app.repository.state.value.spaces.first { it.counterpart?.handle == "marina" }.id
                scenario.onActivity { activity -> activity.startActivity(Intent(activity, MainActivity::class.java)
                    .setAction(Intent.ACTION_VIEW).setData(Uri.parse("zoen://chat/$chat"))
                    .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)) }
                compose.waitUntil(30_000) {
                    app.repository.activeSpace == chat && compose.onAllNodesWithTag("composer").fetchSemanticsNodes().isNotEmpty()
                }
                compose.onNodeWithTag("composer").performClick().performTextReplacement("See you tomorrow")
                scenario.onActivity { activity ->
                    val manager = activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE) as android.view.inputmethod.InputMethodManager
                    manager.showSoftInput(activity.currentFocus, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT)
                }
                await {
                    var visible = false
                    scenario.onActivity { visible = ViewCompat.getRootWindowInsets(it.window.decorView)?.isVisible(WindowInsetsCompat.Type.ime()) == true }
                    visible
                }
                compose.onNodeWithTag("composer").assertTextEquals("See you tomorrow")
                SystemClock.sleep(1_500)
                val name = "composer-${if (dark) "dark" else "light"}"
                lateinit var activity: MainActivity
                scenario.onActivity {
                    activity = it
                    val model = ViewModelProvider(it)[ZoenViewModel::class.java]
                    NativeParityFixtureSetup.attest(app, model.state.value)
                }
                CommittedWindowCapture.save(activity, "android-parity", "$name-window")
                val screenshot = checkNotNull(instrumentation.uiAutomation.takeScreenshot())
                try {
                    File(app.cacheDir, "device-evidence/android-parity/$name.png").outputStream().use {
                        check(screenshot.compress(Bitmap.CompressFormat.PNG, 100, it))
                    }
                } finally { screenshot.recycle() }
            }
        } finally { instrumentation.runOnMainSync { AppAppearance.set(app, previous) } }
    }
}
