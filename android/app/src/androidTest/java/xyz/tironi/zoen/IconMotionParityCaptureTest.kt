package xyz.tironi.zoen

import android.graphics.Bitmap
import android.os.SystemClock
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import androidx.compose.material3.Surface
import androidx.compose.runtime.getValue
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.core.RequestStatus
import xyz.tironi.zoen.theme.AppAppearance
import xyz.tironi.zoen.theme.ZoenTheme
import xyz.tironi.zoen.ui.ZoenApp

/** One fresh fixture per invocation. Run each of EVENTS × appearances × motion modes.
 * Recording is external (screenrecord, 5–10 seconds); this test never substitutes a
 * still for a recording. phase_ms is measured wall-clock time since the real action,
 * not the requested delay or a Compose animation counter. Native alignment remains
 * a separate recording/witness check. No baseline PNGs or missing-state fallbacks.
 */
@RunWith(AndroidJUnit4::class)
class IconMotionParityCaptureTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun captureActualEvent() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val args = InstrumentationRegistry.getArguments()
        val event = requireNotNull(args.getString("iconEvent"))
        require(event in EVENTS) { "Unknown iconEvent: $event" }
        check(event in AVAILABLE_EVENTS) {
            "Genuine live transport required for $event; offline canonical demo cannot produce this event"
        }
        val appearance = requireNotNull(args.getString("iconAppearance"))
        require(appearance in listOf("light", "dark"))
        val motion = requireNotNull(args.getString("iconMotion"))
        require(motion in listOf("full", "reduced"))
        val delayMs = requireNotNull(args.getString("iconDelayMs")).toLong()
        require(delayMs in 0..10_000)
        val app = ApplicationProvider.getApplicationContext<ZoenApplication>()
        // install() normally resets the static-screen manifest. This independent
        // producer must not invalidate the mandatory 16 static captures.
        val staticManifest = File(app.cacheDir, "device-evidence/android-parity/manifest.json")
        val staticMetadata = staticManifest.takeIf { it.exists() }?.readText()
        NativeParityFixtureSetup.install(app)
        if (staticMetadata != null) staticManifest.writeText(staticMetadata)
        val directory = File(app.cacheDir, "device-evidence/android-icon-parity").apply { mkdirs() }
        val manifestFile = File(directory, "manifest.json")
        val key = "$event-$appearance-$motion"
        // A failed rerun must never leave an earlier frame claiming this event.
        File(directory, "$key.png").delete()
        if (manifestFile.exists()) {
            val previous = JSONObject(manifestFile.readText())
            previous.optJSONObject("icon_states")?.remove(key)
            manifestFile.writeText(previous.toString(2))
        }
        // Never mix sessions/revisions. appliedManifest is the existing fixture attestation.
        app.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        runBlocking { app.repository.boot(true); app.repository.resetDemo() }
        val initial = app.repository.state.value
        val attestation = NativeParityFixture.appliedManifest(initial)
        val previousAppearance = app.repository.preferences.getString("appearance", "auto") ?: "auto"
        val resolver = app.contentResolver
        val previousScale = Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f)
        fun shell(command: String) {
            instrumentation.uiAutomation.executeShellCommand(command).use { descriptor ->
                java.io.FileInputStream(descriptor.fileDescriptor).use { it.readBytes() }
            }
        }
        val model = ViewModelProvider(compose.activity)[ZoenViewModel::class.java]
        try {
            shell("settings put global animator_duration_scale ${if (motion == "reduced") 0 else 1}")
            check(Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE) == if (motion == "reduced") 0f else 1f)
            compose.runOnUiThread {
                compose.activity.enableEdgeToEdge()
                AppAppearance.set(app, appearance)
            }
            var mountedAt = 0L
            compose.runOnUiThread { mountedAt = SystemClock.uptimeMillis() }
            compose.setContent {
                val state by model.state.collectAsStateWithLifecycle()
                // ZoenApp owns the production navigation, handlers and observation.
                ZoenTheme { Surface { ZoenApp(model, null, {}) } }
            }
            compose.waitForIdle()
            var tab = "Chats"
            var draft = ""
            var eventAt = mountedAt
            fun tapTab(target: String) {
                eventAt = SystemClock.uptimeMillis()
                compose.onNodeWithTag("tab:$target").performClick()
                compose.onNodeWithTag("tab:$target").assertIsSelected()
                tab = target
            }
            when (event) {
                "tab-chats-tap" -> { tapTab("Spaces"); tapTab("Chats") }
                "tab-spaces-tap" -> tapTab("Spaces")
                "tab-notifications-tap" -> tapTab("Notifications")
                "tab-profile-tap" -> tapTab("Profile")
                "composer-mic", "composer-send" -> {
                    val direct = initial.spaces.first { it.counterpart?.handle == "marina" }
                    compose.onNodeWithText(direct.title).performClick()
                    compose.onNodeWithTag("composer").assertExists()
                    if (event == "composer-mic") compose.onNodeWithTag("composer").performTextInput("Hi")
                    eventAt = SystemClock.uptimeMillis()
                    if (event == "composer-send") {
                        draft = "Hi"
                        compose.onNodeWithTag("composer").performTextInput(draft)
                        compose.onNodeWithTag("send").assertExists()
                    } else compose.onNodeWithTag("composer").performTextClearance()
                    compose.onNodeWithTag("composer").assertTextEquals(draft).assertIsFocused()
                }
                "bell-new" -> {
                    val owned = initial.agents.first { it.persona.isMine && it.spaces.isNotEmpty() }
                    val spaceId = owned.spaces.first().spaceId
                    eventAt = SystemClock.uptimeMillis()
                    val id = runBlocking { app.repository.change {
                        it.demoOpenRequest(spaceId, owned.persona.handle, "Motion request", "Public parity event", "owner", "external", 0L)
                    } }
                    check(id != null && initial.requests.none { it.id == id } && model.state.value.requests.any {
                        it.id == id && it.status == RequestStatus.PENDING && it.agent.isMine
                    }) { "Policy did not create a genuinely new owned pending request" }
                }
                "typing-active" -> error("Requires a genuine peer typing envelope through the production listener; demo is offline. No synthetic replay supplied.")
                "connecting-active" -> error("Requires a real isolated transport connection; demo account is null. Never relabel offline as connecting.")
                "icons-draw-on" -> Unit // actual first mount above, not forced icon remounts
            }
            val remaining = eventAt + delayMs - SystemClock.uptimeMillis()
            if (remaining > 0) SystemClock.sleep(remaining)
            val current = model.state.value
            check(current.typing.isEmpty() && current.connection.state == "offline") {
                "Unexpected live transport in isolated demo capture"
            }
            val automation = instrumentation.uiAutomation
            check(automation.rootInActiveWindow?.packageName?.toString() == app.packageName)
            val actualPhase = SystemClock.uptimeMillis() - eventAt
            require(actualPhase in 0..10_000) { "Actual phase exceeded contract: $actualPhase" }
            val screenshot = checkNotNull(automation.takeScreenshot())
            try {
                require(screenshot.width > 0 && screenshot.height > 0)
                val colors = mutableSetOf<Int>()
                for (y in 0 until screenshot.height step maxOf(1, screenshot.height / 64)) {
                    for (x in 0 until screenshot.width step maxOf(1, screenshot.width / 64)) {
                        colors.add(screenshot.getPixel(x, y))
                    }
                }
                check(colors.size > 8) { "Blank/unrendered window is not an icon frame" }
                File(directory, "$key.png").outputStream().use {
                    check(screenshot.compress(Bitmap.CompressFormat.PNG, 100, it))
                }
            } finally { screenshot.recycle() }
            val pending = current.requests.filter { it.status == RequestStatus.PENDING && it.agent.isMine }
            check(current.demo && current.account == null) { "Capture must remain isolated and offline" }
            val witness = JSONObject().put("tab", tab).put("draft", draft)
                .put("draft_focused", event.startsWith("composer-"))
                .put("unread", current.spaces.sumOf { it.unread.toLong() }).put("pending_count", pending.size)
                .put("pending_requests", org.json.JSONArray(pending.map {
                    JSONObject().put("agent", it.agent.handle).put("space", it.spaceTitle).put("title", it.title)
                }.sortedBy { it.toString() }))
                .put("typing", JSONObject()).put("connection", current.connection.state)
                .put("synced", current.connection.synced).put("sync_mode", "demo")
            val manifest = if (manifestFile.exists()) JSONObject(manifestFile.readText()) else attestation
            for (field in attestation.keys()) {
                check(manifest.opt(field).toString() == attestation.opt(field).toString()) {
                    "Mixed fixture metadata: $field"
                }
            }
            val states = manifest.optJSONObject("icon_states") ?: JSONObject()
            states.put(key, JSONObject().put("event", event).put("appearance", appearance)
                .put("reduce_motion", motion == "reduced").put("phase_ms", actualPhase)
                .put("observed_state", witness))
            manifest.put("icon_states", states)
            manifestFile.writeText(manifest.toString(2))
        } finally {
            shell("settings put global animator_duration_scale $previousScale")
            compose.runOnUiThread { AppAppearance.set(app, previousAppearance) }
        }
    }

    companion object {
        val EVENTS = listOf("tab-chats-tap", "tab-spaces-tap", "tab-notifications-tap", "tab-profile-tap",
            "composer-mic", "composer-send", "bell-new", "typing-active", "connecting-active", "icons-draw-on")
        // Explicit partial coverage: 8 × 2 appearances × 2 motion modes = 32 frames.
        // Reject unsupported events before resetting the fixture or touching evidence.
        val AVAILABLE_EVENTS = EVENTS.filterNot { it == "typing-active" || it == "connecting-active" }
    }
}
