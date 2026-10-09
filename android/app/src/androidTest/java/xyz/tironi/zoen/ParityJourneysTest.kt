package xyz.tironi.zoen

import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import android.os.Build
import android.view.WindowManager
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.text.TextRange
import androidx.lifecycle.Lifecycle
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ParityJourneysTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val application get() = ApplicationProvider.getApplicationContext<ZoenApplication>()
    private lateinit var scenario: ActivityScenario<MainActivity>

    @Before fun openDemo() {
        val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
        automation.executeShellCommand("input keyevent KEYCODE_WAKEUP").close()
        automation.executeShellCommand("wm dismiss-keyguard").close()
        application.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        scenario = ActivityScenario.launch(Intent(application, MainActivity::class.java).putExtra("demo", true))
        scenario.onActivity { it.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) }
        compose.waitUntil(30_000) { application.repository.state.value.ready && application.repository.state.value.me != null }
        runBlocking { application.repository.resetDemo() }
        compose.waitForIdle()
    }
    @After fun close() { if (::scenario.isInitialized) scenario.close() }

    private fun open(link: String) {
        scenario.onActivity { activity -> activity.startActivity(Intent(activity, MainActivity::class.java).setAction(Intent.ACTION_VIEW).setData(Uri.parse(link)).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)) }
    }

    private fun capturePageHistory(name: String) {
        Evidence.outputFile("pages", "$name.txt").writeText(
            compose.onAllNodes(isRoot(), useUnmergedTree = true).printToString(maxDepth = Int.MAX_VALUE)
        )
        val bitmap = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot() ?: return
        try { Evidence.outputFile("pages", "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) } }
        finally { bitmap.recycle() }
    }

    @Test fun nativeLinksReuseTheActivityAndConsumedLinksDoNotReplayAfterRecreation() {
        val chat = application.repository.state.value.zoenChat!!
        val otherSpace = application.repository.state.value.spaces.first { it.id != chat.id && it.counterpart?.handle == "marina" }
        scenario.close()
        scenario = ActivityScenario.launch(Intent(application, MainActivity::class.java).putExtra("demo", true)
            .setAction(Intent.ACTION_VIEW).setData(Uri.parse("zoen://chat/${chat.id}")))
        scenario.onActivity { it.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) }
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("composer").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithContentDescription(application.getString(R.string.back)).performClick()
        compose.onNodeWithTag("home-plus").assertExists()
        scenario.recreate()
        compose.onNodeWithTag("home-plus").assertExists()
        compose.onNodeWithTag("composer").assertDoesNotExist()

        var identity = 0
        var task = 0
        scenario.onActivity { identity = System.identityHashCode(it); task = it.taskId }
        open("zoen://chat/${otherSpace.id}")
        compose.waitUntil(10_000) { application.repository.activeSpace == otherSpace.id && compose.onAllNodesWithTag("composer").fetchSemanticsNodes().isNotEmpty() }
        assertEquals(Lifecycle.State.RESUMED, scenario.state)
        scenario.onActivity { assertEquals(identity, System.identityHashCode(it)); assertEquals(task, it.taskId) }
        scenario.moveToState(Lifecycle.State.CREATED)
        assertFalse(application.repository.appVisible)
        scenario.moveToState(Lifecycle.State.RESUMED)
        assertTrue(application.repository.appVisible)
        compose.onNodeWithContentDescription(application.getString(R.string.back)).performClick()
        scenario.recreate()
        compose.onNodeWithTag("home-plus").assertExists()
        compose.onNodeWithTag("composer").assertDoesNotExist()
        scenario.close()
        assertEquals(Lifecycle.State.DESTROYED, scenario.state)
        assertFalse(application.repository.appVisible)
    }

    @Test fun threadRepliesStaySeparateQuotesJumpAndPinnedPlanOpensTheRealItem() {
        val plan = application.repository.state.value.items.first { it.plan != null }
        val space = plan.spaceId
        val root = runBlocking {
            application.repository.change { it.sendMessage(space, "Native quote original") }
            application.repository.query { it.timeline(space).first { row -> (row.kind as? xyz.tironi.zoen.core.EntryKind.Message)?.text == "Native quote original" }.id }
        }
        runBlocking {
            application.repository.change { it.sendReply(space, "Native inline reply", root, false); it.sendReply(space, "Native separate thread", root, true) }
        }
        open("zoen://chat/$space")
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("chat-pinned-plan").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Native separate thread").assertDoesNotExist()
        compose.onNodeWithTag("quote:$root").performScrollTo().performClick()
        compose.onNodeWithTag("timeline:$root").assertIsDisplayed()
        compose.onNodeWithTag("timeline:$root").performTouchInput { swipeLeft() }
        compose.onNodeWithTag("reply-target:$root").assertExists()
        compose.onNodeWithContentDescription(application.getString(R.string.cancel)).performClick()
        compose.onNodeWithTag("timeline:$root").performTouchInput { swipeRight() }
        compose.waitUntil(10_000) { compose.onAllNodesWithText("Native separate thread").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Native separate thread").assertExists()
        compose.onNodeWithContentDescription(application.getString(R.string.back)).performClick()
        compose.onNodeWithTag("chat-pinned-plan").performClick()
        compose.onAllNodesWithText(plan.title, substring = false).onFirst().assertExists()
    }

    @Test fun richFormattingDraftUndoSaveAndOldVersionPreviewUseTheNativeUi() {
        val chat = application.repository.state.value.zoenChat!!
        val item = runBlocking { application.repository.change { it.pageImportMarkdown(chat.id, "notes/native-rich.md", "# Native rich page\n\nEditable paragraph\n\n```kotlin\nval x = 1\n```\n\n![Forest](https://example.com/forest.png)") } }
        val original = runBlocking { application.repository.query { it.page(item.id) } }
        val paragraph = original.blocks.first { it.kind == "paragraph" }
        open("zoen://item/${item.id}")
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("page-block:${paragraph.id}").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("page-block:${paragraph.id}").performTextReplacement("Edited paragraph")
        compose.onNodeWithTag("page-block:${paragraph.id}").performTextInputSelection(TextRange(0, 6))
        compose.onNodeWithContentDescription(application.getString(R.string.page_bold)).performClick()
        compose.onNodeWithContentDescription(application.getString(R.string.undo)).performClick()
        compose.onNodeWithContentDescription(application.getString(R.string.page_redo)).performClick()
        scenario.recreate()
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("page-block:${paragraph.id}").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("page-block:${paragraph.id}").assertTextContains("Edited paragraph")
        if (!compose.onNodeWithTag("page-save").fetchSemanticsNode().config.contains(androidx.compose.ui.semantics.SemanticsProperties.Disabled)) compose.onNodeWithTag("page-save").performClick()
        compose.waitUntil(10_000) { application.repository.state.value.items.first { it.id == item.id }.version == 2u }
        val persisted = runBlocking { application.repository.query { it.page(item.id) } }
        assertTrue(persisted.blocks.first { it.id == paragraph.id }.spans.any { it.key == "b" && it.start == 0u && it.end == 6u })
        assertEquals(original.blocks.filter { it.kind in listOf("code", "image") }, persisted.blocks.filter { it.kind in listOf("code", "image") })
        compose.onNodeWithContentDescription(application.getString(R.string.versions)).performClick()
        compose.onNodeWithText(application.getString(R.string.version, 1)).performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithText("Editable paragraph").fetchSemanticsNodes().size == 1 }
        compose.onNodeWithText("Editable paragraph").assertExists().assertIsDisplayed()
        assertEquals(2u, application.repository.state.value.items.first { it.id == item.id }.version)
        compose.waitUntil(10_000) {
            compose.onAllNodesWithText(application.getString(R.string.page_restore_version)).fetchSemanticsNodes()
                .singleOrNull()?.config?.contains(androidx.compose.ui.semantics.SemanticsProperties.Disabled) == false
        }
        capturePageHistory("historical-version-before-restore")
        val restore = compose.onNodeWithText(application.getString(R.string.page_restore_version)).assertIsDisplayed().assertIsEnabled()
        compose.onAllNodesWithTag("app-snackbar").assertCountEquals(1)
        val noticeBounds = compose.onNodeWithTag("app-snackbar").fetchSemanticsNode().boundsInRoot
        if (noticeBounds.height > 0) assertTrue("A visible snackbar must be above the restore button", noticeBounds.bottom <= restore.fetchSemanticsNode().boundsInRoot.top)
        restore.performClick()
        try {
            compose.waitUntil(10_000) { compose.onAllNodes(isDialog()).fetchSemanticsNodes().size == 1 }
        } catch (failure: Throwable) {
            try { capturePageHistory("historical-version-restore-failure") }
            catch (captureFailure: Throwable) { failure.addSuppressed(captureFailure) }
            throw failure
        }
        compose.onNode(isDialog()).assertExists()
        capturePageHistory("historical-version-restore-confirmation")
        compose.onNode(hasText(application.getString(R.string.restore), substring = false) and hasClickAction() and hasAnyAncestor(isDialog())).performClick()
        compose.waitUntil(10_000) { application.repository.state.value.items.first { it.id == item.id }.version == 3u }
        assertEquals(original.blocks, runBlocking { application.repository.query { it.page(item.id) } }.blocks)
        compose.waitUntil(10_000) { compose.onAllNodes(hasTestTag("page-block:${paragraph.id}") and hasText("Editable paragraph", substring = false)).fetchSemanticsNodes().size == 1 }
        compose.onNodeWithTag("page-block:${paragraph.id}").assertTextContains("Editable paragraph")
        capturePageHistory("historical-version-restored-v3")
    }

    @Test fun homeLongHoldRecordsAndReleaseSendsToZoenWithoutOpeningTheActionSheet() {
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand("pm grant ${application.packageName} android.permission.RECORD_AUDIO").use { android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes() }
        val chat = application.repository.state.value.zoenChat!!
        val before = application.repository.state.value.items.size
        compose.onNodeWithTag("home-plus").performTouchInput { down(center) }
        compose.mainClock.advanceTimeBy(800)
        val timerPrefix = application.getString(R.string.media_recording, "0:00").substringBefore("0:00")
        compose.waitUntil(20_000) { compose.onAllNodes(hasText(timerPrefix, substring = true) and !hasText("0:00", substring = true)).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText(application.getString(R.string.new_chat), substring = false).assertDoesNotExist()
        compose.onNodeWithTag("home-plus").performTouchInput { up() }
        compose.waitUntil(20_000) { application.repository.state.value.items.size > before && application.repository.state.value.items.any { it.file?.path?.startsWith("VoiceNotes/") == true } }
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("composer").fetchSemanticsNodes().isNotEmpty() }
        val entries = runBlocking { application.repository.query { it.timeline(chat.id) } }
        assertTrue(entries.any { entry -> (entry.kind as? xyz.tironi.zoen.core.EntryKind.Message)?.text?.startsWith("⟦voice:") == true })
        assertTrue(runBlocking { application.repository.query { it.verifyAll().all { log -> log.valid } } })
        compose.onNodeWithText(application.getString(R.string.new_chat), substring = false).assertDoesNotExist()
    }

    @Test fun chatMicrophoneStationaryHoldReleaseSendsRealAudio() {
        InstrumentationRegistry.getInstrumentation().uiAutomation.grantRuntimePermission(application.packageName, android.Manifest.permission.RECORD_AUDIO)
        val chat = application.repository.state.value.zoenChat!!
        val before = runBlocking { application.repository.query { it.timeline(chat.id).map { row -> row.id }.toSet() } }
        open("zoen://chat/${chat.id}")
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("voice-record").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("voice-record").performTouchInput { down(center) }
        compose.mainClock.advanceTimeBy(800)
        val timerPrefix = application.getString(R.string.media_recording, "0:00").substringBefore("0:00")
        compose.waitUntil(20_000) { compose.onAllNodes(hasText(timerPrefix, substring = true) and !hasText("0:00", substring = true)).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("voice-record").performTouchInput { up() }
        compose.waitUntil(20_000) {
            application.repository.state.value.timelines[chat.id].orEmpty().any { row -> row.id !in before && (row.kind as? xyz.tironi.zoen.core.EntryKind.Message)?.text?.let(xyz.tironi.zoen.media.VoiceNoteRef::parse) != null }
        }
        val entry = runBlocking { application.repository.query { it.timeline(chat.id) } }.last { row -> row.id !in before && (row.kind as? xyz.tironi.zoen.core.EntryKind.Message)?.text?.let(xyz.tironi.zoen.media.VoiceNoteRef::parse) != null }
        val voice = checkNotNull(xyz.tironi.zoen.media.VoiceNoteRef.parse((entry.kind as xyz.tironi.zoen.core.EntryKind.Message).text))
        assertTrue(voice.ms >= 1000)
        val file = checkNotNull(runBlocking { xyz.tironi.zoen.media.VoiceTransport.localFile(application, application.repository, voice) })
        try { assertEquals(voice.ms / 1000.0, runBlocking { xyz.tironi.zoen.media.MediaExport.duration(file) }, .15) }
        finally { file.delete() }
        assertTrue(runBlocking { application.repository.query { it.verifyAll().all { log -> log.valid } } })
    }

    @Test fun ActivityMentionsTasksAndChatAppearanceAreReachableAndPersist() {
        val mention = runBlocking { application.repository.query { it.mentions().first() } }
        val task = application.repository.state.value.items.first { it.plan?.sections?.any { section -> section.lines.any { !it.done } } == true }
        compose.onNodeWithText(application.getString(R.string.activity), substring = false).performClick()
        compose.onNodeWithTag("activity-tabs").performScrollToNode(hasText(application.getString(R.string.activity_mentions), substring = true))
        compose.onNodeWithText(application.getString(R.string.activity_mentions), substring = true).performClick()
        compose.onNodeWithText(application.getString(R.string.activity_mentioned, mention.entry.author.name), substring = true).assertExists()
        compose.onNodeWithTag("activity-tabs").performScrollToNode(hasText(application.getString(R.string.activity_tasks), substring = true))
        compose.onNodeWithText(application.getString(R.string.activity_tasks), substring = true).performClick()
        compose.onNodeWithText(task.title, substring = false).assertExists()
        val chat = application.repository.state.value.zoenChat!!
        open("zoen://chat/${chat.id}")
        compose.onNodeWithContentDescription(application.getString(R.string.more)).performClick()
        compose.onNodeWithText(application.getString(R.string.chat_appearance), substring = false).performClick()
        compose.waitUntil(10_000) {
            compose.onAllNodes(hasText(application.getString(R.string.save), substring = false) and isEnabled()).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithText(application.getString(R.string.bg_mint), substring = false).performClick()
        compose.onNodeWithTag("background:color:mint").assertIsSelected()
        compose.onNodeWithText(application.getString(R.string.save), substring = false).assertIsEnabled().performClick()
        compose.waitUntil(10_000) {
            compose.onAllNodesWithTag("chat-appearance").fetchSemanticsNodes().isEmpty() &&
                compose.onAllNodesWithTag("chat-timeline").fetchSemanticsNodes().isNotEmpty()
        }
        val chosen = runBlocking { xyz.tironi.zoen.ui.ChatAppearanceStore.load(application.repository, chat.id) }
        assertEquals("color:mint", chosen.style)
        capturePageHistory("chat-appearance-mint-saved")
        scenario.recreate()
        assertEquals(chosen, runBlocking { xyz.tironi.zoen.ui.ChatAppearanceStore.load(application.repository, chat.id) })
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("chat-timeline").fetchSemanticsNodes().isNotEmpty() }
        capturePageHistory("chat-appearance-mint-after-recreation")
    }
}
