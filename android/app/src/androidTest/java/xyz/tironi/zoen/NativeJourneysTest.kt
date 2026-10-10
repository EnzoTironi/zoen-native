package xyz.tironi.zoen

import android.content.Intent
import android.graphics.Bitmap
import android.os.ParcelFileDescriptor
import android.view.WindowManager
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
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
import xyz.tironi.zoen.core.RequestStatus
import xyz.tironi.zoen.core.ItemDetail
import xyz.tironi.zoen.core.EntryKind
import xyz.tironi.zoen.core.SpaceKindDto

@RunWith(AndroidJUnit4::class)
class NativeJourneysTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val application get() = ApplicationProvider.getApplicationContext<ZoenApplication>()
    private lateinit var scenario: ActivityScenario<MainActivity>

    @Before fun openDemo() {
        val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
        for (command in listOf("input keyevent KEYCODE_WAKEUP", "wm dismiss-keyguard")) {
            ParcelFileDescriptor.AutoCloseInputStream(automation.executeShellCommand(command)).use { it.readBytes() }
        }
        application.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        scenario = ActivityScenario.launch(Intent(application, MainActivity::class.java).putExtra("demo", true))
        scenario.onActivity { it.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) }
        compose.waitUntil(30_000) { application.repository.state.value.ready && application.repository.state.value.me != null }
        runBlocking { application.repository.resetDemo() }
        compose.waitForIdle()
    }
    @After fun close() { scenario.close() }

    private fun captureChatViewport(name: String) {
        val bitmap = checkNotNull(InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot())
        try { Evidence.outputFile("navigation", "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) } }
        finally { bitmap.recycle() }
    }

    @Test fun createsAPlanThroughTheComposerAndKeepsItAfterRecreation() {
        val prompt = "Android weekend trip up to $1,500"
        val chat = checkNotNull(application.repository.state.value.zoenChat)
        val before = application.repository.state.value.items.map { it.id }.toSet()
        compose.onNodeWithTag("conversation-list").performScrollToNode(hasTestTag("chat:zoen"))
        compose.onNodeWithTag("chat:zoen").performClick()
        compose.onNodeWithTag("composer").performTextReplacement(prompt)
        compose.onNodeWithTag("send").performClick()
        fun fromThisPrompt(item: ItemDetail) = item.id !in before && item.spaceId == chat.id && item.kindId == "plan" && item.origin.contains(prompt) && item.plan?.budgetCents == 150_000L
        compose.waitUntil(60_000) { application.repository.state.value.items.any(::fromThisPrompt) }
        val item = application.repository.state.value.items.single(::fromThisPrompt)
        val plan = checkNotNull(item.plan)
        assertTrue(item.title.isNotBlank())
        assertEquals(checkNotNull(chat.counterpart).id, item.createdBy.id)
        assertTrue(plan.sections.flatMap { it.lines }.isNotEmpty())
        assertTrue(plan.totalCents <= 150_000L)
        assertTrue(runBlocking { application.repository.query { it.timeline(chat.id) } }.any { it.author.isMe && (it.kind as? EntryKind.Message)?.text == prompt })
        scenario.onActivity { activity ->
            activity.getSystemService(android.view.inputmethod.InputMethodManager::class.java)
                .hideSoftInputFromWindow(activity.window.decorView.windowToken, 0)
        }
        compose.onNodeWithTag("chat-timeline").performScrollToNode(hasTestTag("item:${item.id}"))
        compose.onNodeWithTag("item:${item.id}").performClick()
        val line = plan.sections.flatMap { it.lines }.first()
        val checkbox = "plan-line:${line.id}"
        assertFalse(line.done)
        compose.onNodeWithTag("plan-lines").performScrollToNode(hasTestTag(checkbox))
        compose.onNodeWithTag(checkbox).performClick()
        compose.waitUntil(10_000) { application.repository.state.value.items.first { it.id == item.id }.plan!!.sections.flatMap { it.lines }.first { it.id == line.id }.done }
        scenario.recreate()
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("plan-lines").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("plan-lines").performScrollToNode(hasTestTag(checkbox))
        compose.onNodeWithTag(checkbox).assertIsOn()
        assertTrue(runBlocking { application.repository.query { it.item(item.id) } }.plan!!.sections.flatMap { it.lines }.first { it.id == line.id }.done)
        assertTrue(runBlocking { application.repository.query { it.verifyAll().all { report -> report.valid } } })
    }

    @Test fun approvalsAreReviewedAndRecordedByTheSharedEngine() {
        val request = application.repository.state.value.requests.first { it.status == RequestStatus.PENDING }
        compose.onNodeWithText(application.getString(R.string.activity)).performClick()
        compose.onNodeWithText(request.title).performScrollTo().performClick()
        compose.onNodeWithText(application.getString(R.string.approve), substring = false).performClick()
        compose.onNode(isDialog()).assertExists()
        compose.onAllNodesWithText(application.getString(R.string.approve), substring = false).onLast().performClick()
        compose.waitUntil(10_000) {
            application.repository.state.value.requests.first { it.id == request.id }.status == RequestStatus.APPROVED &&
                compose.onAllNodesWithText(application.getString(R.string.status_approved)).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithText(application.getString(R.string.status_approved)).assertExists()
    }

    @Test fun chatKeepsReadingPositionAndFollowsNewMessagesAfterRecreation() {
        val space = checkNotNull(application.repository.state.value.zoenChat).id
        val texts = (1..8).map { index -> ("Viewport message $index: " + "Keep this conversation readable across Android recreation. ".repeat(3)).trim() }
        val ids = runBlocking { application.repository.change { core ->
            texts.forEach { core.sendMessage(space, it) }
            val entries = core.timeline(space)
            texts.map { text -> entries.single { (it.kind as? EntryKind.Message)?.text == text }.id }
        } }
        compose.onNodeWithTag("conversation-list").performScrollToNode(hasTestTag("chat:zoen"))
        compose.onNodeWithTag("chat:zoen").performClick()
        fun awaitEntry(id: String) {
            compose.waitUntil(10_000) { compose.onAllNodesWithTag("timeline:$id").fetchSemanticsNodes().size == 1 }
            compose.onNodeWithTag("timeline:$id").assertIsDisplayed()
        }
        awaitEntry(ids.last())
        compose.onNodeWithTag("chat-timeline").performScrollToNode(hasTestTag("timeline:${ids.first()}"))
        compose.onNodeWithTag("timeline:${ids.first()}").assertIsDisplayed()
        scenario.recreate()
        awaitEntry(ids.first())
        captureChatViewport("chat-reading-after-recreation")
        val latest = "New message after restoring the historical reading position"
        val latestId = runBlocking { application.repository.change { core ->
            core.sendMessage(space, latest)
            core.timeline(space).single { (it.kind as? EntryKind.Message)?.text == latest }.id
        } }
        awaitEntry(latestId)
        scenario.recreate()
        awaitEntry(latestId)
        captureChatViewport("chat-latest-after-recreation")
        assertTrue(runBlocking { application.repository.query { it.verifyAll().all { report -> report.valid } } })
    }

    @Test fun searchFindsThePersistedStory() {
        compose.onNodeWithTag("home-plus").performClick()
        compose.onNode(hasText(application.getString(R.string.search)) and hasAnyAncestor(hasTestTag("quickActions")), useUnmergedTree = true).performClick()
        compose.onNodeWithTag("search").performTextInput("Paraty")
        val result = hasText("Paraty", substring = true) and hasClickAction() and !hasSetTextAction()
        compose.waitUntil(10_000) { compose.onAllNodes(result).fetchSemanticsNodes().isNotEmpty() }
        compose.onAllNodes(result).onFirst().performClick()
        compose.onNodeWithTag("composer").assertExists()
    }

    @Test fun homeSearchOpensJoinedCommunityAndKeepsItAfterRecreation() {
        val community = application.repository.state.value.spaces.first { it.kind == SpaceKindDto.COMMUNITY }
        compose.onNodeWithContentDescription(application.getString(R.string.search)).performClick()
        compose.onNode(hasSetTextAction()).performTextInput(community.title)
        compose.onNodeWithTag("conversation-list").performScrollToNode(hasTestTag("chat:${community.id}"))
        compose.onNodeWithTag("chat:${community.id}").assertIsDisplayed().performClick()
        compose.onNodeWithTag("composer").assertIsDisplayed()
        scenario.recreate()
        compose.onNodeWithText(community.title).assertIsDisplayed()
        compose.onNodeWithTag("composer").assertIsDisplayed()
        captureChatViewport("joined-community-after-recreation")
    }
}
