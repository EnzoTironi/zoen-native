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
import xyz.tironi.zoen.miniapps.SnapshotCard
import xyz.tironi.zoen.miniapps.WidgetSnapshot

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
        compose.onNodeWithTag("composer").performClick().performTextReplacement(text)
        compose.onNodeWithTag("composer").assertTextEquals(text)
        compose.onNodeWithTag("send").assertIsEnabled()
        fun sends() = shell("logcat -d -v brief -s ZoenFeedback:D *:S").lineSequence().count { "Send effect=" in it }
        val before = sends()
        compose.runOnIdle { locked = true }
        compose.onNodeWithTag("send").assertIsNotEnabled()
        compose.onNodeWithTag("composer").performImeAction()
        compose.waitForIdle()
        assertEquals(before, sends())
        assertFalse(repository.state.value.timelines[space].orEmpty().any { (it.kind as? xyz.tironi.zoen.core.EntryKind.Message)?.text == text })
        compose.runOnIdle { locked = false }
        compose.onNodeWithTag("send").performClick()
        compose.waitUntil(10_000) { repository.state.value.timelines[space].orEmpty().any { (it.kind as? xyz.tironi.zoen.core.EntryKind.Message)?.text == text } }
        compose.waitUntil(2_000) { sends() == before + 1 }
        assertEquals(before + 1, sends())
        assertTrue(runBlocking { repository.query { it.verifyLog(space).valid } })
        Evidence.outputFile("experience", "haptic-dispatch-receipt.txt").writeText("PASS: disabled send and IME send store no message and request no feedback; one View.performHapticFeedback after signed storage.\n" + shell("logcat -d -v brief -s ZoenFeedback:D *:S"))
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
            shell("settings put global animator_duration_scale 0")
            compose.setContent { ZoenTheme { Box(Modifier.fillMaxSize()) { ZoenMascot(Modifier.size(240.dp), pose = MascotPose.Map) } } }
            lateinit var mascot: MascotCanvasView
            compose.runOnIdle { mascot = checkNotNull(findMascot(compose.activity.window.decorView)) }
            fun time(): Double { var value = 0.0; instrumentation.runOnMainSync { value = mascot.animationTimeSeconds }; return value }
            fun scheduled(): Boolean { var value = false; instrumentation.runOnMainSync { value = mascot.hasScheduledFrame }; return value }
            shell("settings put global animator_duration_scale 1")
            awaitNative(12_000) { time() > 8.4 }
            val before = time()
            assertTrue("The procedural rig must run beyond the former eight second loop", before > 8.4)
            assertTrue(scheduled())
            capture("procedural-map-long-running")
            instrumentation.runOnMainSync { mascot.visibility = View.INVISIBLE }
            val hidden = time()
            java.lang.Thread.sleep(350)
            assertEquals(hidden, time(), 0.0)
            assertFalse(scheduled())
            instrumentation.runOnMainSync { mascot.visibility = View.VISIBLE }
            awaitNative(2_000) { time() > hidden + .2 }
            compose.activityRule.scenario.moveToState(Lifecycle.State.STARTED)
            awaitNative(2_000) { !scheduled() }
            val paused = time()
            java.lang.Thread.sleep(350)
            assertEquals(paused, time(), 0.0)
            assertFalse(scheduled())
            compose.activityRule.scenario.moveToState(Lifecycle.State.RESUMED)
            awaitNative(2_000) { time() > paused + .2 }
            shell("settings put global animator_duration_scale 0")
            awaitNative(2_000) { !scheduled() }
            val reduced = time()
            java.lang.Thread.sleep(350)
            assertEquals(reduced, time(), 0.0)
            capture("reduced-motion-static-map")
            shell("settings put global animator_duration_scale 1")
            awaitNative(2_000) { time() > reduced + .2 }
            Evidence.outputFile("experience", "mascot-lifecycle-receipt.txt").writeText("PASS: time=$before (>8 s), hidden pause, resumed phase, lifecycle pause, live animation-scale=0 pause and resume.\n")
            instrumentation.runOnMainSync { mascot.visibility = View.INVISIBLE }
        } finally {
            if (originalScale == "null") shell("settings delete global animator_duration_scale") else shell("settings put global animator_duration_scale $originalScale")
        }
    }

    @Test fun everySnapshotArtRendersNativelyAndLiveArtPausesWhenHiddenAndMotionIsDisabled() {
        val originalScale = shell("settings get global animator_duration_scale").trim()
        var show by mutableStateOf(true)
        val arts = listOf("notepad", "pot", "ballot", "trip", "hike", "pet", "pet.asleep", "pet.gone", "globe")
        val views = mutableListOf<NativeArtCanvasView>()
        try {
            shell("settings put global animator_duration_scale 0")
            compose.setContent {
                ZoenTheme {
                    if (show) Column(Modifier.fillMaxSize().padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        arts.chunked(3).forEach { row ->
                            Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                row.forEach { art ->
                                    val snapshot = checkNotNull(WidgetSnapshot.decode("""{"id":"art-$art","appId":"list","template":"caption","title":"$art","accentHex":"#5CC79E","symbol":"checklist","art":"$art","deepLink":"zoen://app/art-$art"}"""))
                                    SnapshotCard(snapshot, Modifier.weight(1f).fillMaxHeight())
                                }
                            }
                        }
                    }
                }
            }
            compose.runOnIdle {
                fun collect(view: View) {
                    if (view is NativeArtCanvasView) views.add(view)
                    if (view is ViewGroup) for (index in 0 until view.childCount) collect(view.getChildAt(index))
                }
                collect(compose.activity.window.decorView)
            }
            assertEquals(8, views.size)
            compose.onNodeWithTag("native-globe").assertIsDisplayed()
            shell("settings put global animator_duration_scale 1")
            fun times(): List<Double> { var value = emptyList<Double>(); instrumentation.runOnMainSync { value = views.map { it.animationTimeSeconds } }; return value }
            fun scheduled(): List<Boolean> { var value = emptyList<Boolean>(); instrumentation.runOnMainSync { value = views.map { it.hasScheduledFrame } }; return value }
            awaitNative(3_000) { times().all { it > 1.1 } }
            assertTrue(scheduled().all { it })
            CommittedWindowCapture.save(compose.activity, "experience", "native-snapshot-art-live")
            instrumentation.runOnMainSync { views.forEach { it.visibility = View.INVISIBLE } }
            val hidden = times()
            java.lang.Thread.sleep(350)
            assertEquals(hidden, times())
            assertTrue(scheduled().none { it })
            instrumentation.runOnMainSync { views.forEach { it.visibility = View.VISIBLE } }
            awaitNative(2_000) { times().zip(hidden).all { (now, before) -> now > before + .2 } }
            shell("settings put global animator_duration_scale 0")
            awaitNative(2_000) { scheduled().none { it } }
            val stopped = times()
            java.lang.Thread.sleep(350)
            assertEquals(stopped, times())
            CommittedWindowCapture.save(compose.activity, "experience", "native-snapshot-art-reduced-motion")
            Evidence.outputFile("experience", "snapshot-art-receipt.txt").writeText("PASS: all nine snapshot arts, eight original Canvas sprites/doodles plus the land-data globe; live draw-on and native clocks, hidden pause/resume, reduced-motion freeze.\n")
        } finally {
            instrumentation.runOnMainSync { show = false }
            compose.waitForIdle()
            if (originalScale == "null") shell("settings delete global animator_duration_scale") else shell("settings put global animator_duration_scale $originalScale")
        }
    }

    private fun awaitNative(timeout: Long, condition: () -> Boolean) {
        val deadline = android.os.SystemClock.elapsedRealtime() + timeout
        while (!condition()) {
            assertTrue("Native animation did not reach its required state within $timeout ms", android.os.SystemClock.elapsedRealtime() < deadline)
            java.lang.Thread.sleep(10)
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
