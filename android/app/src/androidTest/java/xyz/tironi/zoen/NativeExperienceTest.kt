package xyz.tironi.zoen

import android.graphics.Bitmap
import android.view.View
import android.view.ViewGroup
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.*
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.runBlocking
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.theme.ZoenTheme
import xyz.tironi.zoen.ui.*

@RunWith(AndroidJUnit4::class)
class NativeExperienceTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    @Test fun sendRequestsOnePlatformHapticAfterTheSignedMessageIsStoredAndDisabledSendRequestsNone() {
        val app = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val repository = app.repository
        repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        runBlocking { repository.boot(true); repository.resetDemo() }
        val space = repository.state.value.spaces.first { it.kind == xyz.tironi.zoen.core.SpaceKindDto.GROUP }.id
        val model = ViewModelProvider(compose.activity)[ZoenViewModel::class.java]
        var locked by mutableStateOf(false)
        compose.setContent {
            val state by model.state.collectAsStateWithLifecycle()
            ZoenTheme { ChatScreen(model, state.copy(keyMissing = locked), space, {}, {}) }
        }
        val text = "A real signed send with platform feedback"
        compose.onNodeWithTag("composer").performTextInput(text)
        fun sends() = shell("logcat -d -v brief -s ZoenFeedback:D *:S").lineSequence().count { "Send effect=" in it }
        val before = sends()
        compose.runOnIdle { locked = true }
        compose.onNodeWithTag("send").assertIsNotEnabled()
        assertEquals(before, sends())
        compose.runOnIdle { locked = false }
        compose.onNodeWithTag("send").performClick()
        compose.waitUntil(10_000) { repository.state.value.timelines[space].orEmpty().any { (it.kind as? xyz.tironi.zoen.core.EntryKind.Message)?.text == text } }
        compose.waitUntil(2_000) { sends() == before + 1 }
        assertEquals(before + 1, sends())
        assertTrue(runBlocking { repository.query { it.verifyLog(space).valid } })
        Evidence.outputFile("experience", "haptic-dispatch-receipt.txt").writeText("PASS: disabled send requests none; one View.performHapticFeedback after signed storage.\n" + shell("logcat -d -v brief -s ZoenFeedback:D *:S"))
        compose.waitUntil(10_000) {
            compose.onAllNodesWithTag("composer").fetchSemanticsNodes().singleOrNull()
                ?.config?.get(androidx.compose.ui.semantics.SemanticsProperties.EditableText)?.text == ""
        }
        compose.runOnIdle {
            compose.activity.currentFocus?.let { view ->
                compose.activity.getSystemService(android.view.inputmethod.InputMethodManager::class.java)
                    .hideSoftInputFromWindow(view.windowToken, 0)
            }
        }
        compose.onNodeWithTag("chat-timeline").performScrollToNode(hasText(text))
        compose.onNodeWithText(text).assertIsDisplayed()
        compose.waitForIdle()
        capture("signed-send-native-feedback")
    }

    @Test fun proceduralMascotRunsPastEightSecondsAndPausesForLifecycleVisibilityAndLiveReducedMotion() {
        val originalScale = shell("settings get global animator_duration_scale").trim()
        try {
            shell("settings put global animator_duration_scale 1")
            compose.setContent { ZoenTheme { Box(Modifier.fillMaxSize()) { ZoenMascot(Modifier.size(240.dp), pose = MascotPose.Map) } } }
            lateinit var mascot: MascotCanvasView
            compose.runOnIdle { mascot = checkNotNull(findMascot(compose.activity.window.decorView)) }
            fun time(): Double { var value = 0.0; instrumentation.runOnMainSync { value = mascot.animationTimeSeconds }; return value }
            fun scheduled(): Boolean { var value = false; instrumentation.runOnMainSync { value = mascot.hasScheduledFrame }; return value }
            compose.waitUntil(12_000) { time() > 8.4 }
            val before = time()
            assertTrue("The procedural rig must run beyond the former eight second loop", before > 8.4)
            assertTrue(scheduled())
            capture("procedural-map-long-running")
            compose.runOnIdle { mascot.visibility = View.INVISIBLE }
            val hidden = time()
            java.lang.Thread.sleep(350)
            assertEquals(hidden, time(), 0.0)
            assertFalse(scheduled())
            compose.runOnIdle { mascot.visibility = View.VISIBLE }
            compose.waitUntil(2_000) { time() > hidden + .2 }
            compose.activityRule.scenario.moveToState(Lifecycle.State.STARTED)
            val paused = time()
            java.lang.Thread.sleep(350)
            assertEquals(paused, time(), 0.0)
            assertFalse(scheduled())
            compose.activityRule.scenario.moveToState(Lifecycle.State.RESUMED)
            compose.waitUntil(2_000) { time() > paused + .2 }
            shell("settings put global animator_duration_scale 0")
            compose.waitUntil(2_000) { !scheduled() }
            val reduced = time()
            java.lang.Thread.sleep(350)
            assertEquals(reduced, time(), 0.0)
            capture("reduced-motion-static-map")
            shell("settings put global animator_duration_scale 1")
            compose.waitUntil(2_000) { time() > reduced + .2 }
            Evidence.outputFile("experience", "mascot-lifecycle-receipt.txt").writeText("PASS: time=$before (>8 s), hidden pause, resumed phase, lifecycle pause, live animation-scale=0 pause and resume.\n")
        } finally {
            if (originalScale == "null") shell("settings delete global animator_duration_scale") else shell("settings put global animator_duration_scale $originalScale")
        }
    }

    private fun findMascot(view: View): MascotCanvasView? {
        if (view is MascotCanvasView) return view
        if (view is ViewGroup) for (index in 0 until view.childCount) findMascot(view.getChildAt(index))?.let { return it }
        return null
    }
    private fun shell(command: String): String = android.os.ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand(command)).use { it.readBytes().toString(Charsets.UTF_8) }
    private fun capture(name: String) {
        val image = checkNotNull(instrumentation.uiAutomation.takeScreenshot())
        Evidence.outputFile("experience", "$name.png").outputStream().use { assertTrue(image.compress(Bitmap.CompressFormat.PNG, 100, it)) }; image.recycle()
    }
}
