package xyz.tironi.zoen

import android.content.Intent
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
        lateinit var activity: MainActivity
        scenario.onActivity { activity = it }
        CommittedWindowCapture.save(activity, "pages", name)
    }

    @Test fun filesAreAPrimaryDestinationAndFolderBackReturnsToFiles() {
        val item = application.repository.state.value.items.first()
        compose.onNodeWithTag("tab:Files").performClick()
        compose.onNodeWithTag("files-list").assertExists()
        compose.onNodeWithContentDescription(application.getString(R.string.back)).assertDoesNotExist()
        compose.onNodeWithTag("files-list").performScrollToNode(hasTestTag("folder:${item.spaceId}"))
        compose.onNodeWithTag("folder:${item.spaceId}").performClick()
        compose.onNodeWithTag("files-search").performTextReplacement(item.title)
        scenario.recreate()
        compose.onNodeWithTag("files-search").assertTextContains(item.title)
        compose.onNodeWithContentDescription(application.getString(R.string.back)).performClick()
        compose.onNodeWithTag("files-list").assertExists()
        compose.onNodeWithTag("tab:Files").assertIsSelected()
        capturePageHistory("primary-files-after-folder-back")
        compose.onNodeWithTag("tab:Chats").performClick()
        compose.onNodeWithTag("conversation-list").assertExists()
        compose.onNodeWithTag("home-plus").assertIsDisplayed()
        compose.onNodeWithTag("tab:Files").performClick()
        compose.onNodeWithTag("files-list").assertExists()
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

    @Test fun pinnedPlanUsesTheSharedCardAndUnpinPersistsWithoutDeletingIt() {
        val plan = application.repository.state.value.items.first { it.plan != null }
        val pins = xyz.tironi.zoen.miniapps.MiniAppPins(application.repository.preferences)
        val scope = xyz.tironi.zoen.miniapps.MiniAppPins.chat(plan.spaceId)
        val tile = "miniapp-tile:${plan.id}#0"
        pins.show(plan.id, scope)
        try {
            open("zoen://chat/${plan.spaceId}")
            compose.waitUntil(10_000) { compose.onAllNodesWithTag(tile).fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithTag("chat-timeline").performScrollToIndex(0)
            val summary = application.getString(R.string.chat_plan_count, plan.plan!!.sections.sumOf { it.lines.size },
                xyz.tironi.zoen.core.formatMoney(plan.plan!!.totalCents, application.repository.locale))
            compose.onNodeWithText(summary).assertIsDisplayed()
            capturePageHistory("chat-pinned-plan-shared-card")
            compose.onNodeWithTag(tile).performClick()
            compose.onAllNodesWithText(plan.title).onFirst().assertIsDisplayed()
            compose.onNodeWithContentDescription(application.getString(R.string.back)).performClick()
            compose.waitUntil(10_000) { compose.onAllNodesWithTag(tile).fetchSemanticsNodes().isNotEmpty() }
            val unpin = compose.onNodeWithTag(tile).fetchSemanticsNode().config[androidx.compose.ui.semantics.SemanticsActions.CustomActions]
                .first { it.label == application.getString(R.string.unpin) }
            compose.runOnIdle { assertTrue(unpin.action()) }
            compose.onNodeWithText(application.getString(R.string.cancel)).performClick()
            compose.onNodeWithTag(tile).assertIsDisplayed()
            compose.runOnIdle { assertTrue(unpin.action()) }
            compose.onNodeWithText(application.getString(R.string.unpin)).performClick()
            compose.waitUntil(10_000) { compose.onAllNodesWithTag(tile).fetchSemanticsNodes().isEmpty() }
            scenario.recreate()
            compose.onNodeWithTag(tile).assertDoesNotExist()
            val retained = runBlocking { application.repository.query { it.item(plan.id) } }
            assertEquals(plan.version, retained.version)
            assertEquals(plan.plan, retained.plan)
            capturePageHistory("chat-pinned-plan-unpinned-after-recreation")
        } finally {
            pins.show(plan.id, scope)
        }
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
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("miniapp-tile:${plan.id}#0").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Native separate thread").assertDoesNotExist()
        compose.onNodeWithTag("chat-timeline").performScrollToNode(hasTestTag("quote:$root"))
        compose.onNodeWithTag("quote:$root").assertIsDisplayed()
        capturePageHistory("chat-inline-quote-before-jump")
        compose.onNodeWithTag("quote:$root").performClick()
        compose.onNodeWithTag("timeline:$root").assertIsDisplayed()
        capturePageHistory("chat-quote-jump-to-original")
        compose.onNodeWithTag("timeline:$root").performTouchInput { swipeLeft() }
        compose.onNodeWithTag("reply-target:$root").assertExists()
        compose.onNodeWithContentDescription(application.getString(R.string.cancel)).performClick()
        compose.onNodeWithTag("timeline:$root").performTouchInput { swipeRight() }
        compose.waitUntil(10_000) { compose.onAllNodesWithText("Native separate thread").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Native separate thread").assertExists()
        compose.onNodeWithContentDescription(application.getString(R.string.back)).performClick()
        compose.onNodeWithTag("miniapp-tile:${plan.id}#0").performClick()
        compose.onAllNodesWithText(plan.title, substring = false).onFirst().assertExists()
    }

    @Test fun richFormattingDraftUndoSaveAndOldVersionPreviewUseTheNativeUi() {
        val chat = application.repository.state.value.zoenChat!!
        val item = runBlocking { application.repository.change { it.pageImportMarkdown(chat.id, "notes/native-rich.md", "# Native rich page\n\nEditable paragraph\n\n```kotlin\nval x = 1\n```\n\n![Forest](https://example.com/forest.png)") } }
        val original = runBlocking { application.repository.query { it.page(item.id) } }
        val paragraph = original.blocks.first { it.kind == "paragraph" }
        val capturePhases = InstrumentationRegistry.getArguments().getString("zoenFormattingEvidence") == "true"
        fun captureFormatting(stage: String, force: Boolean = false) {
            if (!capturePhases && !force) return
            val field = compose.onNodeWithTag("page-block:${paragraph.id}").fetchSemanticsNode().config
            val selection = androidx.compose.ui.semantics.SemanticsProperties.TextSelectionRange
            val bold = compose.onNodeWithContentDescription(application.getString(R.string.page_bold)).fetchSemanticsNode().config
            val toggle = androidx.compose.ui.semantics.SemanticsProperties.ToggleableState
            val key = application.repository.localKey("pageDraft", item.id, application.repository.state.value.me!!.id)
            val draft = xyz.tironi.zoen.pages.PageDraftStore(application.repository.vault,
                application.repository.localKey("encryptedPageDraft", item.id, application.repository.state.value.me!!.id))
                .load(application.repository.preferences, key)?.content
            val spans = draft?.let { xyz.tironi.zoen.pages.PageEditing.decode(it) }
                ?.firstOrNull { it.id == paragraph.id }?.spans.orEmpty()
            val record = org.json.JSONObject(mapOf(
                "stage" to stage, "api" to Build.VERSION.SDK_INT,
                "selection" to if (field.contains(selection)) field[selection].toString() else "absent",
                "bold" to if (bold.contains(toggle)) bold[toggle].toString() else "absent",
                "draft_present" to (draft != null),
                "spans" to spans.map { mapOf("key" to it.key, "start" to it.start.toLong(), "end" to it.end.toLong()) },
            )).toString(2)
            Evidence.outputFile("pages", "rich-formatting-$stage.json").writeText(record)
            android.util.Log.i("RichEditorJourney", record)
            capturePageHistory("rich-formatting-$stage")
        }
        open("zoen://item/${item.id}")
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("page-block:${paragraph.id}").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("page-block:${paragraph.id}").performTextReplacement("Edited paragraph")
        compose.onNodeWithTag("page-block:${paragraph.id}").performTextInputSelection(TextRange(0, 6))
        captureFormatting("selected")
        compose.onNodeWithContentDescription(application.getString(R.string.page_bold)).performClick()
        captureFormatting("bold")
        compose.onNodeWithContentDescription(application.getString(R.string.undo)).performClick()
        compose.onNodeWithContentDescription(application.getString(R.string.page_redo)).performClick()
        captureFormatting("redo")
        scenario.recreate()
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("page-block:${paragraph.id}").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("page-block:${paragraph.id}").assertTextContains("Edited paragraph")
        captureFormatting("recreated")
        if (!compose.onNodeWithTag("page-save").fetchSemanticsNode().config.contains(androidx.compose.ui.semantics.SemanticsProperties.Disabled)) compose.onNodeWithTag("page-save").performClick()
        compose.waitUntil(10_000) { application.repository.state.value.items.first { it.id == item.id }.version == 2u }
        val persisted = runBlocking { application.repository.query { it.page(item.id) } }
        val savedSpans = persisted.blocks.first { it.id == paragraph.id }.spans
        if (savedSpans.none { it.key == "b" && it.start == 0u && it.end == 6u }) captureFormatting("save-failure", force = true)
        assertTrue("Expected bold [0, 6) after save; actual spans: $savedSpans", savedSpans.any { it.key == "b" && it.start == 0u && it.end == 6u })
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
        compose.onNodeWithTag("activity-show-list").performClick()
        compose.onNodeWithTag("activity-tabs").performScrollToNode(hasText(application.getString(R.string.activity_mentions), substring = true))
        compose.onNodeWithText(application.getString(R.string.activity_mentions), substring = true).performClick()
        compose.onNodeWithText(application.getString(R.string.activity_mentioned, mention.entry.author.name), substring = true).assertExists()
        compose.onNodeWithTag("activity-tabs").performScrollToNode(hasText(application.getString(R.string.activity_tasks), substring = true))
        compose.onNodeWithText(application.getString(R.string.activity_tasks), substring = true).performClick()
        compose.onNodeWithText(task.title, substring = false).assertExists()
        val chat = application.repository.state.value.zoenChat!!
        open("zoen://chat/${chat.id}")
        compose.waitUntil(10_000) {
            scenario.state == Lifecycle.State.RESUMED && application.repository.activeSpace == chat.id &&
                compose.onAllNodesWithTag("composer").fetchSemanticsNodes().isNotEmpty()
        }
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
