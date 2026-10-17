package xyz.tironi.zoen

import android.app.Dialog
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.getValue
import androidx.compose.material3.Surface
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.test.core.app.ApplicationProvider
import androidx.test.platform.app.InstrumentationRegistry
import android.graphics.Bitmap
import java.io.File
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.core.SpaceKindDto
import xyz.tironi.zoen.theme.AppAppearance
import xyz.tironi.zoen.theme.ZoenTheme
import xyz.tironi.zoen.ui.*

/** Fresh demo frames for comparison against the same iOS demo, never a real account. */
@RunWith(AndroidJUnit4::class)
class AndroidParityScreensTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @org.junit.Before fun requireParityOptIn() = NativeParityFixtureSetup.assumeOptedIn()

    private fun capture(screen: String, dark: Boolean = false) {
        val app = ApplicationProvider.getApplicationContext<ZoenApplication>()
        NativeParityFixtureSetup.install(app)
        val previous = app.repository.preferences.getString("appearance", "auto") ?: "auto"
        app.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        runBlocking { app.repository.boot(true); app.repository.resetDemo() }
        val state = app.repository.state.value
        val group = state.spaces.first { it.title == "Saturday Crew" }.id
        val direct = state.spaces.first { it.counterpart?.handle == "marina" }.id
        val community = state.spaces.first { it.kind == SpaceKindDto.COMMUNITY }.id
        val root = runBlocking { app.repository.query { it.timeline(group).first().id } }
        val person = requireNotNull(state.me).id
        val model = ViewModelProvider(compose.activity)[ZoenViewModel::class.java]
        try {
            compose.runOnUiThread {
                compose.activity.enableEdgeToEdge()
                AppAppearance.set(app, if (dark) "dark" else "light")
            }
            compose.setContent {
                val current by model.state.collectAsStateWithLifecycle()
                ZoenTheme {
                    Surface {
                        when (screen) {
                            "onboarding" -> Onboarding(model)
                            "chats" -> ZoenApp(model, null, {})
                            "spaces" -> ChatScreen(model, current, community, {}, {})
                            "notifications" -> ActivityScreen(model, current, {}, {}, root = true)
                            "chat" -> ChatScreen(model, current, direct, {}, {})
                            "group" -> ChatScreen(model, current, group, {}, {})
                            "thread" -> ThreadScreen(model, current, group, root, {}, {})
                            "profile" -> ProfileScreen(model, current, person, {}, {})
                            "settings" -> SettingsScreen(model, current, null, {}, {}, {})
                            "devices" -> SettingsScreen(model, current, "Devices", {}, {}, {})
                            "search" -> SearchScreen(model, current, {}, {})
                            else -> error("Unknown parity screen: $screen")
                        }
                    }
                }
            }
            compose.waitForIdle()
            compose.waitForIdle()
            android.os.SystemClock.sleep(1_500)
            NativeParityFixtureSetup.attest(app, model.state.value)
            val name = "$screen-${if (dark) "dark" else "light"}"
            CommittedWindowCapture.save(compose.activity, "android-parity", "$name-window")
            val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
            compose.waitUntil(10_000) {
                NativeParityFixtureSetup.captureWindowReady(compose.activity, automation.rootInActiveWindow?.packageName)
            }
            val screenshot = checkNotNull(automation.takeScreenshot())
            try {
                val destination = File(app.cacheDir, "device-evidence/android-parity/$name.png")
                destination.outputStream().use { output ->
                    check(screenshot.compress(Bitmap.CompressFormat.PNG, 100, output))
                }
            } finally {
                screenshot.recycle()
            }
        } finally {
            compose.runOnUiThread { AppAppearance.set(app, previous) }
        }
    }

    @Test fun onboardingLight() = capture("onboarding")
    @Test fun onboardingDark() = capture("onboarding", true)
    @Test fun chatsLight() = capture("chats")
    @Test fun chatsDark() = capture("chats", true)
    @Test fun chatLight() = capture("chat")
    @Test fun chatDark() = capture("chat", true)
    @Test fun groupLight() = capture("group")
    @Test fun groupDark() = capture("group", true)
    @Test fun threadLight() = capture("thread")
    @Test fun threadDark() = capture("thread", true)
    @Test fun spacesLight() = capture("spaces")
    @Test fun spacesDark() = capture("spaces", true)
    @Test fun notificationsLight() = capture("notifications")
    @Test fun notificationsDark() = capture("notifications", true)
    @Test fun profileLight() = capture("profile")
    @Test fun profileDark() = capture("profile", true)
    @Test fun settingsLight() = capture("settings")
    @Test fun settingsDark() = capture("settings", true)
    @Test fun devicesLight() = capture("devices")
    @Test fun devicesDark() = capture("devices", true)
    @Test fun searchLight() = capture("search")
    @Test fun searchDark() = capture("search", true)

    @Test fun captureReadinessAcceptsOwnedDialogsAndRejectsForeignWindows() {
        val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
        compose.waitUntil(10_000) { NativeParityFixtureSetup.captureWindowReady(compose.activity) }
        lateinit var dialog: Dialog
        compose.runOnUiThread {
            dialog = Dialog(compose.activity).apply {
                setContentView(TextView(context).apply { text = "App-owned window" })
                show()
            }
        }
        try {
            compose.waitUntil(10_000) {
                !compose.activity.window.decorView.hasWindowFocus() &&
                    automation.rootInActiveWindow?.packageName?.toString() == compose.activity.packageName
            }
            check(NativeParityFixtureSetup.captureWindowReady(compose.activity, automation.rootInActiveWindow?.packageName))
            check(!NativeParityFixtureSetup.captureWindowReady(compose.activity, "android"))
        } finally {
            compose.runOnUiThread { dialog.dismiss() }
        }
        compose.waitUntil(10_000) { NativeParityFixtureSetup.captureWindowReady(compose.activity) }
    }
}
