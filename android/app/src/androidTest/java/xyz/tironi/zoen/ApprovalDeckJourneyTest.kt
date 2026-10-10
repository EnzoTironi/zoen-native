package xyz.tironi.zoen

import android.content.Intent
import android.graphics.Bitmap
import android.os.ParcelFileDescriptor
import android.view.WindowManager
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.lifecycle.ViewModelProvider
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
import xyz.tironi.zoen.core.*

@RunWith(AndroidJUnit4::class)
class ApprovalDeckJourneyTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val app get() = ApplicationProvider.getApplicationContext<ZoenApplication>()
    private lateinit var scenario: ActivityScenario<MainActivity>
    private lateinit var model: ZoenViewModel
    private var originalScale = "0"

    @Before fun open() {
        originalScale = shell("settings get global animator_duration_scale").trim()
        shell("settings put global animator_duration_scale 1")
        app.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        scenario = ActivityScenario.launch(Intent(app, MainActivity::class.java).putExtra("demo", true))
        scenario.onActivity {
            it.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            model = ViewModelProvider(it)[ZoenViewModel::class.java]
        }
        compose.waitUntil(30_000) { app.repository.state.value.ready && app.repository.state.value.me != null }
        runBlocking { app.repository.resetDemo() }
        compose.waitForIdle()
    }
    @After fun close() {
        scenario.close()
        if (originalScale == "null") shell("settings delete global animator_duration_scale") else shell("settings put global animator_duration_scale $originalScale")
    }

    @Test fun swipeThresholdsUndoAndRotationKeepDecisionsUnsignedUntilTheWindowCloses() {
        val request = app.repository.state.value.requests.filter { it.agent.isMine && it.status == RequestStatus.PENDING }.minBy { it.openedMs }
        assertFalse("The original money request is a red line", request.canAlwaysApprove)
        val before = runBlocking { app.repository.query { it.logEvents(request.spaceId).size } }
        compose.onNodeWithText(app.getString(R.string.activity)).performClick()
        compose.onNodeWithTag("approval-up").assertIsNotEnabled()
        capture("cards-before-drag")
        compose.onNodeWithTag("approval-card-top").performTouchInput {
            swipe(center, center + Offset(width * .15f, 0f), 450)
        }
        compose.waitForIdle()
        assertNull(model.approvals.state.value.pending)
        assertEquals(RequestStatus.PENDING, status(request.id))
        val rejected = feedback("Reject")
        swipe("up")
        compose.onNodeWithText(app.getString(R.string.approval_always_asks)).assertExists()
        assertEquals(rejected + 1, feedback("Reject"))
        assertNull(model.approvals.state.value.pending)
        swipe("right")
        compose.waitUntil(3_000) { model.approvals.state.value.pending?.request?.id == request.id }
        assertEquals(RequestStatus.PENDING, status(request.id))
        assertEquals(before, runBlocking { app.repository.query { it.logEvents(request.spaceId).size } })
        scenario.recreate()
        scenario.onActivity { model = ViewModelProvider(it)[ZoenViewModel::class.java] }
        val undo = compose.onNodeWithTag("approval-undo")
        if (runCatching { undo.assertIsDisplayed() }.isFailure) undo.performScrollTo()
        undo.performClick()
        compose.waitForIdle()
        assertNull(model.approvals.state.value.pending)
        assertEquals(RequestStatus.PENDING, status(request.id))
        assertEquals(before, runBlocking { app.repository.query { it.logEvents(request.spaceId).size } })
        compose.onNodeWithText(request.title).assertIsDisplayed()
        compose.onNodeWithTag("approval-undo").assertDoesNotExist()
        capture("undo-returned-card")
        val nextTitle = app.repository.state.value.requests.filter { it.agent.isMine && it.status == RequestStatus.PENDING && it.id != request.id }.minBy { it.openedMs }.title
        val right = button("approval-right")
        compose.mainClock.autoAdvance = false
        try {
            right.performClick()
            assertEquals(request.id, model.approvals.state.value.pending?.request?.id)
            shell("settings put global animator_duration_scale 0")
            compose.mainClock.advanceTimeBy(32)
            compose.waitUntil(2_000) {
                compose.mainClock.advanceTimeByFrame()
                runCatching { compose.onNodeWithText(nextTitle).assertIsDisplayed() }.isSuccess
            }
            compose.mainClock.autoAdvance = true
            button("approval-undo").performClick()
            assertNull(model.approvals.state.value.pending)
            assertEquals(before, runBlocking { app.repository.query { it.logEvents(request.spaceId).size } })
        } finally {
            compose.mainClock.autoAdvance = true
            shell("settings put global animator_duration_scale 1")
        }
        compose.onNodeWithTag("approval-left").assertIsEnabled()
        val confirmed = feedback("Confirm")
        swipe("left")
        compose.waitUntil(10_000) { status(request.id) == RequestStatus.DENIED }
        compose.waitUntil(2_000) { feedback("Confirm") == confirmed + 1 }
        assertEquals(confirmed + 1, feedback("Confirm"))
        assertTrue(runBlocking { app.repository.query { it.logEvents(request.spaceId).size } } > before)
        assertTrue(runBlocking { app.repository.query { it.verifyAll().all { report -> report.valid } } })
        compose.onNodeWithTag("activity-show-list").performClick()
        compose.onNodeWithTag("activity-list").performScrollToNode(hasText(request.title))
        compose.onNodeWithText(app.getString(R.string.status_denied)).assertIsDisplayed()
        capture("signed-denial-in-list")
        Evidence.outputFile("approvals", "undo-recreation-receipt.txt").writeText("PASS: short drag returns, sensitive upward swipe rejected once, right swipe waits unsigned, recreation retains undo, undo adds no signed event, left swipe signs after 4.5 seconds and requests one platform confirmation.\n")
    }

    @Test fun reducedMotionStandingSwipesCreateRevocableRulesAndResolveMatchingRequests() {
        shell("settings put global animator_duration_scale 0")
        val ids = createPlanRequests("Native standing approval")
        val reviewed = app.repository.state.value.requests.filter { it.id in ids }.minBy { it.openedMs }.id
        compose.onNodeWithText(app.getString(R.string.activity)).performClick()
        compose.onNodeWithTag("approval-up").assertIsEnabled()
        swipe("up")
        compose.waitUntil(10_000) { ids.all { status(it) == RequestStatus.APPROVED } }
        compose.waitForIdle()
        val rule = runBlocking { app.repository.query { it.standingDecisions().single { rule -> rule.allow } } }
        assertTrue(app.repository.state.value.requests.filter { it.id in ids && it.id != reviewed }.all { it.byStanding })
        capture("standing-approval-caught-up")
        runBlocking { app.repository.change { it.revokeStanding(rule.grantId) } }
        val denied = createPlanRequests("Native standing denial")
        swipe("down")
        compose.waitUntil(10_000) { denied.all { status(it) == RequestStatus.DENIED } }
        compose.waitForIdle()
        val deny = runBlocking { app.repository.query { it.standingDecisions().single { rule -> !rule.allow } } }
        assertEquals(rule.agent.id, deny.agent.id)
        assertEquals(rule.spaceId, deny.spaceId)
        assertEquals(rule.actionKey, deny.actionKey)
        compose.onNodeWithTag("activity-show-list").performClick()
        capture("standing-denial-in-list")
        assertTrue(runBlocking { app.repository.query { it.verifyAll().all { report -> report.valid } } })
        Evidence.outputFile("approvals", "standing-decision-receipt.txt").writeText("PASS: actual upward and downward gestures under reduced motion create engine-backed, scoped revocable grants; both matching pending requests settle, signed logs verify.\n")
    }

    @Test fun finishingActivityStoresItsAcceptedChoiceWithoutBackgroundFeedback() {
        val request = app.repository.state.value.requests.filter { it.agent.isMine && it.status == RequestStatus.PENDING }.minBy { it.openedMs }
        val before = runBlocking { app.repository.query { it.logEvents(request.spaceId).size } }
        compose.onNodeWithText(app.getString(R.string.activity)).performClick()
        val confirmed = feedback("Confirm")
        button("approval-left").performClick()
        assertEquals(request.id, model.approvals.state.value.pending?.request?.id)
        assertEquals(RequestStatus.PENDING, status(request.id))
        scenario.close()
        compose.waitUntil(10_000) {
            runBlocking { app.repository.query { it.requests().single { it.id == request.id }.status == RequestStatus.DENIED } }
        }
        assertTrue(runBlocking { app.repository.query { it.logEvents(request.spaceId).size } } > before)
        assertTrue(runBlocking { app.repository.query { it.verifyAll().all { report -> report.valid } } })
        assertEquals(confirmed, feedback("Confirm"))
        Evidence.outputFile("approvals", "activity-finish-receipt.txt").writeText("PASS: leaving the Activity stores the accepted decision before the ViewModel disappears; signed logs verify and no confirmation is requested in the background.\n")
    }

    private fun createPlanRequests(prefix: String): List<String> = runBlocking {
        app.repository.change { core ->
            core.requests().filter { it.agent.isMine && it.status == RequestStatus.PENDING }.forEach { core.decideRequest(it.id, RequestDecision.DENY) }
            val chat = core.spaces().first { it.kind == SpaceKindDto.GROUP && it.members.any { member -> member.handle == "zoen" && member.isMine } }
            val agent = chat.members.single { it.handle == "zoen" }
            core.setTrust(agent.id, chat.id, TrustLevelDto.SUGGEST)
            val existing = core.requests().map { it.id }.toSet()
            repeat(2) { index ->
                val title = "$prefix $index"
                val plan = PlanDto(title = title, summary = "An actual permission request", budgetCents = null, sections = listOf(PlanSectionDto("Plan", listOf(PlanLineDto("", "Bring water", 0, false)))), totalCents = 0)
                assertEquals(DecisionKind.REQUEST, core.agentCreatePlan(chat.id, agent.id, title, plan, "Native test", 0).kind)
            }
            core.requests().filter { it.id !in existing && it.status == RequestStatus.PENDING }.also { requests ->
                assertEquals(2, requests.size)
                assertTrue(requests.all { it.canAlwaysApprove })
            }.map { it.id }
        }
    }
    private fun status(id: String) = app.repository.state.value.requests.single { it.id == id }.status
    private fun button(tag: String): SemanticsNodeInteraction {
        val node = compose.onNodeWithTag(tag)
        val scrollable = generateSequence(node.fetchSemanticsNode().parent) { it.parent }.any { it.config.contains(SemanticsActions.ScrollBy) }
        if (scrollable) node.performScrollTo()
        compose.waitForIdle()
        return node.assertIsDisplayed()
    }
    private fun swipe(direction: String) {
        compose.onNodeWithTag("approval-card-top").performTouchInput {
            val start = when (direction) {
                "left" -> Offset(width - 12f, center.y)
                "right" -> Offset(12f, center.y)
                "up" -> Offset(center.x, height - 12f)
                else -> Offset(center.x, 12f)
            }
            val end = when (direction) {
                "left" -> Offset(12f, center.y)
                "right" -> Offset(width - 12f, center.y)
                "up" -> Offset(center.x, 12f)
                else -> Offset(center.x, height - 12f)
            }
            swipe(start, end, 700)
        }
        compose.waitForIdle()
    }
    private fun feedback(effect: String) = shell("logcat -d -v brief -s ZoenFeedback:D *:S").lineSequence().count { "$effect effect=" in it }
    private fun shell(command: String): String = ParcelFileDescriptor.AutoCloseInputStream(InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)).use { it.readBytes().toString(Charsets.UTF_8) }
    private fun capture(name: String) {
        val bitmap = checkNotNull(InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot())
        try { Evidence.outputFile("approvals", "$name.png").outputStream().use { assertTrue(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) } }
        finally { bitmap.recycle() }
    }
}
