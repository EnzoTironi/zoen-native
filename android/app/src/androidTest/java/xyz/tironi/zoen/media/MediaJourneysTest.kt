package xyz.tironi.zoen.media

import android.Manifest
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.view.WindowManager
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.ByteArrayOutputStream
import java.util.UUID
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.MainActivity
import xyz.tironi.zoen.Evidence
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenApplication
import xyz.tironi.zoen.core.EntryKind

@RunWith(AndroidJUnit4::class)
class MediaJourneysTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val application get() = ApplicationProvider.getApplicationContext<ZoenApplication>()
    private fun capture(name: String) {
        val bitmap = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot() ?: return
        try { Evidence.outputFile("media", "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) } }
        finally { bitmap.recycle() }
    }
    private fun voiceFailure(stage: String, space: String?, entry: String?, voice: String?, error: Throwable) {
        runCatching { capture("voice-failure") }.onFailure(error::addSuppressed)
        runCatching {
            val state = application.repository.state.value
            val playback = VoicePlayback.state.value
            val report = JSONObject().put("stage", stage).put("error", error.stackTraceToString())
                .put("space", space ?: JSONObject.NULL).put("expectedEntry", entry ?: JSONObject.NULL).put("expectedVoice", voice ?: JSONObject.NULL)
                .put("ready", state.ready).put("repositoryFailure", state.failure ?: JSONObject.NULL)
                .put("activeSpace", application.repository.activeSpace ?: JSONObject.NULL)
                .put("observedSpaces", JSONArray(state.timelines.keys.toList()))
                .put("observedTimelineIds", JSONArray(space?.let { state.timelines[it] }.orEmpty().map { it.id }))
                .put("durableTimelineIds", JSONArray(if (space == null) emptyList<String>() else runBlocking { application.repository.query { it.timeline(space).map { entry -> entry.id } } }))
                .put("playback", JSONObject().put("id", playback.id ?: JSONObject.NULL).put("playing", playback.playing).put("duration", playback.duration).put("error", playback.error ?: JSONObject.NULL))
            Evidence.outputFile("media", "voice-failure.json").writeText(report.toString(2))
        }.onFailure(error::addSuppressed)
        runCatching {
            Evidence.outputFile("media", "voice-failure.txt").writeText(compose.onAllNodes(isRoot(), useUnmergedTree = true).printToString(maxDepth = Int.MAX_VALUE))
        }.onFailure(error::addSuppressed)
    }
    private fun open(): ActivityScenario<MainActivity> {
        for (command in listOf("input keyevent 224", "wm dismiss-keyguard")) ParcelFileDescriptor.AutoCloseInputStream(InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)).use { it.readBytes() }
        application.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        val scenario = ActivityScenario.launch<MainActivity>(Intent(application, MainActivity::class.java).putExtra("demo", true))
        scenario.onActivity { it.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) }
        compose.waitUntil(30_000) { application.repository.state.value.ready && application.repository.state.value.me != null }
        return scenario
    }

    @Test fun actualMicrophoneReviewCutAndSendCreatesPlayableSignedAudioAfterRotation() {
        InstrumentationRegistry.getInstrumentation().uiAutomation.grantRuntimePermission(application.packageName, Manifest.permission.RECORD_AUDIO)
        val scenario = open()
        var stage = "recording"
        var spaceId: String? = null
        var sentEntry: String? = null
        var sentVoice: String? = null
        try {
            compose.onNodeWithTag("conversation-list").performScrollToNode(hasTestTag("chat:zoen"))
            compose.onNodeWithTag("chat:zoen").performClick()
            val space = checkNotNull(application.repository.state.value.zoenChat).id
            spaceId = space
            val previous = application.repository.state.value.timelines[space].orEmpty().map { it.id }.toSet()
            compose.onNodeWithTag("voice-record").performClick()
            val timerPrefix = application.getString(R.string.media_recording_locked, "0:00").substringBefore("0:00")
            compose.waitUntil(20_000) { compose.onAllNodes(hasText(timerPrefix, substring = true) and !hasText("0:00", substring = true)).fetchSemanticsNodes().isNotEmpty() }
            compose.onAllNodesWithTag("voice-recording").assertCountEquals(1)
            compose.onAllNodesWithContentDescription(application.getString(R.string.media_stop_review)).assertCountEquals(1)
            compose.onNodeWithContentDescription(application.getString(R.string.media_stop_review)).performClick()
            compose.waitUntil(15_000) { compose.onAllNodesWithTag("voice-waveform").fetchSemanticsNodes().isNotEmpty() }
            compose.onAllNodesWithTag("voice-recording").assertCountEquals(0)
            compose.onAllNodesWithTag("voice-send").assertCountEquals(1)
            compose.onNodeWithText(application.getString(R.string.edit), substring = false).performClick()
            compose.onNodeWithTag("voice-waveform").performTouchInput { swipe(Offset(width * .2f, height * .5f), Offset(width * .6f, height * .5f), durationMillis = 400) }
            compose.onNodeWithTag("voice-cut-selection").performClick()
            compose.onNodeWithTag("voice-cut-summary").assertTextContains("1", substring = true)
            val sendDeadline = SystemClock.elapsedRealtime() + 20_000
            compose.waitUntil(20_000) { compose.onAllNodes(hasTestTag("voice-send") and isEnabled()).fetchSemanticsNodes().size == 1 }
            compose.onNodeWithTag("voice-send").assertIsDisplayed().assertIsEnabled()
            compose.waitForIdle()
            compose.onNodeWithTag("voice-cut-summary").assertTextContains("1", substring = true)
            capture("voice-review-cut")
            stage = "sending"
            compose.onNodeWithTag("voice-send").assertIsEnabled().performClick()
            val remainingSendTime = sendDeadline - SystemClock.elapsedRealtime()
            assertTrue("Readiness and sending must share the original 20-second deadline", remainingSendTime > 0)
            compose.waitUntil(remainingSendTime) { application.repository.state.value.timelines[space].orEmpty().any { it.id !in previous && (it.kind as? EntryKind.Message)?.text?.let(VoiceNoteRef::parse) != null } }
            val entry = application.repository.state.value.timelines[space].orEmpty().last { it.id !in previous && (it.kind as? EntryKind.Message)?.text?.let(VoiceNoteRef::parse) != null }
            val voice = checkNotNull(VoiceNoteRef.parse((entry.kind as EntryKind.Message).text))
            sentEntry = entry.id
            sentVoice = voice.id
            stage = "checking-signed-audio"
            assertTrue(voice.ms >= 300)
            val file = runBlocking { VoiceTransport.localFile(application, application.repository, voice) }
            assertNotNull(file)
            try { assertEquals(voice.ms / 1000.0, runBlocking { MediaExport.duration(file!!) }, .15) } finally { file?.delete() }
            assertTrue(runBlocking { application.repository.query { it.verifyAll().all { report -> report.valid } } })
            val bubble = hasTestTag("voice-message") and hasAnyAncestor(hasTestTag("timeline:${entry.id}"))
            val rotationDeadline = SystemClock.elapsedRealtime() + 10_000
            stage = "before-recreation"
            compose.waitUntil(10_000) { compose.onAllNodes(bubble, useUnmergedTree = true).fetchSemanticsNodes().size == 1 }
            compose.waitForIdle()
            compose.onNode(bubble, useUnmergedTree = true).assertIsDisplayed()
            scenario.recreate()
            stage = "after-recreation"
            val remainingRotationTime = rotationDeadline - SystemClock.elapsedRealtime()
            assertTrue("The exact sent bubble must settle and survive recreation within the original 10-second deadline", remainingRotationTime > 0)
            compose.waitUntil(remainingRotationTime) { compose.onAllNodes(bubble, useUnmergedTree = true).fetchSemanticsNodes().size == 1 }
            compose.onNode(bubble, useUnmergedTree = true).assertIsDisplayed()
            capture("voice-after-rotation")
            stage = "playback"
            compose.onNode(hasContentDescription(application.getString(R.string.media_play_voice)) and hasAnyAncestor(hasTestTag("timeline:${entry.id}")), useUnmergedTree = true).assertIsDisplayed().performClick()
            compose.waitUntil(10_000) { VoicePlayback.state.value.id == voice.id && VoicePlayback.state.value.playing }
            assertTrue(VoicePlayback.state.value.duration > .3)
            capture("voice-playback")
        } catch (error: Throwable) {
            voiceFailure(stage, spaceId, sentEntry, sentVoice, error)
            throw error
        } finally { scenario.close() }
    }

    @Test fun drawingAndSavingThroughNativeUiMakesARealNewImageVersion() {
        val scenario = open()
        try {
            val bitmap = Bitmap.createBitmap(300, 200, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.WHITE) }
            val original = try { ByteArrayOutputStream().use { output -> bitmap.compress(Bitmap.CompressFormat.PNG, 100, output); output.toByteArray() } } finally { bitmap.recycle() }
            val name = "markup-${UUID.randomUUID()}.png"
            val item = runBlocking { application.repository.change { it.fileAdd(checkNotNull(application.repository.state.value.zoenChat).id, name, name, "image/png", original, null) } }
            compose.onNodeWithTag("home-plus").performClick()
            compose.onNodeWithText(application.getString(R.string.files), useUnmergedTree = true).performClick()
            compose.onNodeWithTag("files-list").performScrollToNode(hasText(item.title))
            compose.onNodeWithText(item.title, useUnmergedTree = true).performScrollTo().assertIsDisplayed().performClick()
            compose.waitUntil(15_000) { compose.onAllNodesWithTag("file-markup", useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithTag("file-markup", useUnmergedTree = true).performScrollTo().performClick()
            compose.waitUntil(10_000) { compose.onAllNodesWithTag("markup-canvas").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithTag("markup-canvas").performTouchInput { swipe(Offset(width * .2f, height * .5f), Offset(width * .8f, height * .5f), durationMillis = 700) }
            capture("image-native-ink")
            compose.onNodeWithTag("markup-save").performClick()
            compose.waitUntil(15_000) { application.repository.state.value.items.firstOrNull { it.id == item.id }?.version == 2u }
            val actual = runBlocking { application.repository.query { it.fileBytes(item.id, null)!! } }
            val decoded = runBlocking { DocumentMarkup.decodeImage(actual) }
            try { val color = decoded.getPixel(150, 100); assertTrue(Color.red(color) > 150 && Color.green(color) < 100) } finally { decoded.recycle() }
            assertArrayEquals(original, runBlocking { application.repository.query { it.fileBytes(item.id, 1u) } })
            scenario.recreate()
            compose.waitUntil(10_000) { compose.onAllNodesWithText("v2", substring = true).fetchSemanticsNodes().isNotEmpty() }
            capture("image-saved-version")
        } finally { scenario.close() }
    }

    @Test fun retainedVoiceReviewStaysWithItsOriginalThreadWhenAnotherThreadOpens() {
        InstrumentationRegistry.getInstrumentation().uiAutomation.grantRuntimePermission(application.packageName, Manifest.permission.RECORD_AUDIO)
        val scenario = open()
        try {
            val space = checkNotNull(application.repository.state.value.zoenChat).id
            val texts = listOf("First voice thread ${UUID.randomUUID()}", "Second voice thread ${UUID.randomUUID()}")
            val roots = runBlocking { application.repository.change { core ->
                texts.forEach { core.sendMessage(space, it) }
                val entries = core.timeline(space)
                texts.map { text -> entries.single { (it.kind as? EntryKind.Message)?.text == text }.id }
            } }
            compose.onNodeWithTag("conversation-list").performScrollToNode(hasTestTag("chat:zoen"))
            compose.onNodeWithTag("chat:zoen").performClick()
            fun openThread(root: String) {
                compose.onNodeWithTag("chat-timeline").performScrollToNode(hasTestTag("timeline:$root"))
                compose.onNodeWithTag("timeline:$root").performTouchInput { swipeRight() }
                compose.onNodeWithText(application.getString(R.string.replies), substring = false).assertExists()
            }
            openThread(roots[0])
            compose.onNodeWithTag("voice-record").performClick()
            val timerPrefix = application.getString(R.string.media_recording_locked, "0:00").substringBefore("0:00")
            compose.waitUntil(20_000) { compose.onAllNodes(hasText(timerPrefix, substring = true) and !hasText("0:00", substring = true)).fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithContentDescription(application.getString(R.string.media_stop_review)).performClick()
            compose.waitUntil(15_000) { compose.onAllNodesWithTag("voice-send").fetchSemanticsNodes().isNotEmpty() }
            compose.onAllNodesWithTag("voice-send").assertCountEquals(1)

            // A new chat intent can arrive while the first thread's review is still retained.
            scenario.onActivity { activity ->
                activity.startActivity(Intent(activity, MainActivity::class.java).setAction(Intent.ACTION_VIEW)
                    .setData(Uri.parse("zoen://chat/$space"))
                    .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP))
            }
            compose.waitUntil(10_000) { compose.onAllNodesWithTag("chat-timeline").fetchSemanticsNodes().isNotEmpty() }
            compose.onAllNodesWithTag("voice-send").assertCountEquals(0)
            openThread(roots[1])
            compose.onAllNodesWithTag("voice-send").assertCountEquals(0)
            compose.onAllNodesWithTag("voice-recording").assertCountEquals(0)
            compose.onNodeWithTag("voice-record").assertIsEnabled()

            compose.onNodeWithContentDescription(application.getString(R.string.back)).performClick()
            openThread(roots[0])
            compose.waitUntil(15_000) { compose.onAllNodes(hasTestTag("voice-send") and isEnabled()).fetchSemanticsNodes().isNotEmpty() }
            compose.onAllNodesWithTag("voice-send").assertCountEquals(1)
            compose.onNodeWithTag("voice-send").performClick()
            compose.waitUntil(20_000) { runBlocking { application.repository.query { core ->
                core.thread(space, roots[0]).any { (it.kind as? EntryKind.Message)?.text?.let(VoiceNoteRef::parse) != null }
            } } }
            runBlocking { application.repository.query { core ->
                val first = core.thread(space, roots[0]).filter { (it.kind as? EntryKind.Message)?.text?.let(VoiceNoteRef::parse) != null }
                val second = core.thread(space, roots[1]).filter { (it.kind as? EntryKind.Message)?.text?.let(VoiceNoteRef::parse) != null }
                assertEquals(1, first.size)
                assertTrue(second.isEmpty())
                assertTrue(core.verifyAll().all { it.valid })
            } }
        } finally { scenario.close() }
    }
}
