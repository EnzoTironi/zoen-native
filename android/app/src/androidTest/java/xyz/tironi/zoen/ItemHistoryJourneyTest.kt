package xyz.tironi.zoen

import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import android.os.ParcelFileDescriptor
import androidx.compose.ui.semantics.SemanticsProperties
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
import xyz.tironi.zoen.core.*

@RunWith(AndroidJUnit4::class)
class ItemHistoryJourneyTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val app get() = ApplicationProvider.getApplicationContext<ZoenApplication>()
    private lateinit var scenario: ActivityScenario<MainActivity>
    private lateinit var item: ItemDetail

    @Before fun prepareOwnedDemo() {
        app.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        runBlocking { app.repository.boot(true); app.repository.resetDemo() }
        item = app.repository.state.value.items.first { it.kindId == "plan" && it.plan != null }
    }

    private fun detail() = runBlocking { app.repository.query { it.item(item.id) } }
    private fun page() = runBlocking { app.repository.query { it.planPage(item.id) } }
    private fun open() {
        scenario = ActivityScenario.launch(Intent(app, MainActivity::class.java).putExtra("demo", true)
            .setAction(Intent.ACTION_VIEW).setData(Uri.parse("zoen://item/${item.id}")))
        compose.waitUntil(15_000) { compose.onAllNodesWithTag("plan-lines").fetchSemanticsNodes().isNotEmpty() }
        compose.waitUntil(10_000) { compose.onAllNodes(hasTestTag("item-history") and isEnabled()).fetchSemanticsNodes().isNotEmpty() }
    }
    private fun history() {
        compose.onNodeWithTag("item-history").performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("item-versions").fetchSemanticsNodes().isNotEmpty() }
    }
    private fun signedEdits(count: Int) {
        val line = page().blocks.first { it.content is PlanPageContentDto.Line }.content as PlanPageContentDto.Line
        repeat(count) { index ->
            val observed = page().contentHash
            runBlocking { app.repository.change {
                it.editPlanLine(item.id, line.legacyId, "History checkpoint ${index + 1} café 😀", (index + 1).toLong(), observed)
            } }
        }
    }
    private fun shell(command: String) {
        ParcelFileDescriptor.AutoCloseInputStream(
            InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)
        ).use { it.readBytes() }
        compose.waitForIdle()
    }
    private fun capture(name: String) {
        compose.waitForIdle()
        val bitmap = checkNotNull(InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot())
        try { Evidence.outputFile("item-history", "$name.png").outputStream().use {
            check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it))
        } } finally { bitmap.recycle() }
    }
    private fun assertDismissed(before: ItemDetail) {
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("item-versions").fetchSemanticsNodes().isEmpty() }
        compose.onNodeWithTag("plan-lines").assertIsDisplayed()
        assertEquals(before, detail())
        capture("dismissed-${before.version}")
    }

    @Test fun longHistoryScrollsToLatestAndSelectsItsReadOnlyContent() {
        signedEdits(12)
        val before = detail()
        assertEquals(item.version + 12u, before.version)
        open(); history()
        val list = compose.onNodeWithTag("item-versions")
        val range = list.fetchSemanticsNode().config[SemanticsProperties.VerticalScrollAxisRange]
        assertTrue("Long real history must have a nonzero scroll range", range.maxValue() > 0f)
        val initial = range.value()
        compose.onNodeWithTag("version:1").assertIsDisplayed()
        capture("long-history-first")
        list.performScrollToNode(hasTestTag("version:${before.version}"))
        compose.onNodeWithTag("version:${before.version}").assertIsDisplayed()
        val scrolled = list.fetchSemanticsNode().config[SemanticsProperties.VerticalScrollAxisRange].value()
        assertTrue("Selecting the last row must actually scroll the list", scrolled > initial)
        Evidence.outputFile("item-history", "scroll-range.txt").writeText(
            "versions=${before.versions.size}; initial=$initial; max=${range.maxValue()}; after=$scrolled\n")
        capture("long-history-last")
        compose.onNodeWithTag("version:${before.version}").performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("version-preview").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText(app.getString(R.string.page_preview_version, before.version.toInt())).assertIsDisplayed()
        compose.onNodeWithTag("version-preview").performScrollToNode(hasText("History checkpoint 12 café 😀"))
        compose.onNodeWithText("History checkpoint 12 café 😀").assertIsDisplayed()
        compose.onAllNodesWithTag("plan-save-line").assertCountEquals(0)
        assertEquals(before, detail())
        capture("latest-read-only-preview")
    }

    @Test fun longHistorySelectsOldestWithoutChangingTheCurrentVersion() {
        signedEdits(12)
        val before = detail()
        val original = runBlocking { app.repository.query { it.itemAt(item.id, 1u) } }
        val line = checkNotNull(original.plan).sections.first { it.lines.isNotEmpty() }.lines.first()
        open(); history()
        compose.onNodeWithTag("item-versions").performScrollToNode(hasTestTag("version:1"))
        compose.onNodeWithTag("version:1").assertIsDisplayed().performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("version-preview").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText(app.getString(R.string.page_preview_version, 1)).assertIsDisplayed()
        compose.onNodeWithTag("version-preview").performScrollToNode(hasText(line.text))
        compose.onNodeWithText(line.text).assertIsDisplayed()
        compose.onNodeWithTag("preview-plan-line:${line.id}").assertIsNotEnabled()
        compose.onNodeWithTag("preview-restore").assertIsEnabled()
        assertEquals(before, detail())
        capture("oldest-read-only-preview")
    }

    @Test fun backDismissesHistoryAndKeepsTheItem() {
        val before = detail()
        open(); history(); capture("before-back-dismiss")
        shell("input keyevent 4")
        assertDismissed(before)
    }

    @Test fun draggingHistoryDownDismissesWithoutStoppingAtAPartialAnchor() {
        val before = detail()
        open(); history(); capture("before-drag-dismiss")
        compose.onNodeWithTag("item-versions").performTouchInput { swipeDown(durationMillis = 500) }
        assertDismissed(before)
    }

    @Test fun backdropDismissesHistoryAndKeepsTheItem() {
        val before = detail()
        open(); history(); capture("before-backdrop-dismiss")
        val title = compose.onNodeWithText(app.getString(R.string.versions)).fetchSemanticsNode().boundsInWindow
        val y = (title.top / 2).toInt()
        assertTrue("Short history must leave a visible backdrop above the sheet", y > 24 && y < title.top - 48)
        shell("input tap ${title.center.x.toInt()} $y")
        assertDismissed(before)
    }

    @Test fun membershipLossDisablesSelectionAndRestoreInAnOpenHistory() {
        open(); history()
        runBlocking { app.repository.change { it.leaveSpace(item.spaceId) } }
        compose.waitUntil(10_000) { !page().legacyEditAdmitted }
        val before = detail()
        compose.onNodeWithTag("version:1").assertIsNotEnabled()
        compose.onNodeWithTag("restore-version:1").assertIsNotEnabled()
        compose.onNodeWithTag("version:1").performTouchInput { click() }
        compose.onNodeWithTag("restore-version:1").performTouchInput { click() }
        compose.onNodeWithTag("item-versions").assertIsDisplayed()
        compose.onAllNodesWithTag("version-preview").assertCountEquals(0)
        compose.onAllNodesWithTag("confirm-restore-version").assertCountEquals(0)
        assertEquals(before, detail())
        capture("open-history-admission-disabled")
    }

    @Test fun planChangedUnderAnOpenHistoryRefusesRestoreThenRestoresAfterReread() {
        open(); history()
        compose.waitUntil(10_000) { compose.onAllNodes(hasTestTag("restore-version:1") and isEnabled()).fetchSemanticsNodes().isNotEmpty() }
        signedEdits(1)
        val changed = detail()
        assertEquals(item.version + 1u, changed.version)
        compose.onNodeWithTag("item-versions").performScrollToNode(hasTestTag("restore-version:1"))
        compose.onNodeWithTag("restore-version:1").performClick()
        compose.onNodeWithTag("confirm-restore-version").performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithText(app.getString(R.string.plan_changed)).fetchSemanticsNodes().isNotEmpty() }
        assertEquals(changed, detail())
        capture("stale-restore-refused")
        // The refusal re-reads the binding; the still-open sheet can now restore what it shows.
        compose.waitUntil(10_000) { compose.onAllNodes(hasTestTag("restore-version:1") and isEnabled()).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("item-versions").performScrollToNode(hasTestTag("restore-version:1"))
        compose.onNodeWithTag("restore-version:1").performClick()
        compose.onNodeWithTag("confirm-restore-version").performClick()
        compose.waitUntil(10_000) { detail().version == changed.version + 1u }
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("item-versions").fetchSemanticsNodes().isEmpty() }
        capture("stale-restore-after-reread")
    }

    @Test fun membershipLossDisablesOpeningHistory() {
        open()
        runBlocking { app.repository.change { it.leaveSpace(item.spaceId) } }
        compose.waitUntil(10_000) { !page().legacyEditAdmitted }
        val before = detail()
        compose.onNodeWithTag("item-history").assertIsNotEnabled().performTouchInput { click() }
        compose.onAllNodesWithTag("item-versions").assertCountEquals(0)
        assertEquals(before, detail())
        capture("history-admission-disabled")
    }

    @After fun close() { if (::scenario.isInitialized) scenario.close() }
}
