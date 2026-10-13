package xyz.tironi.zoen

import android.content.Intent
import android.net.Uri
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.UUID
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AndroidSecretVault
import xyz.tironi.zoen.data.LocalPlanner

@RunWith(AndroidJUnit4::class)
class PlanPageJourneyTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val app get() = ApplicationProvider.getApplicationContext<ZoenApplication>()
    private lateinit var scenario: ActivityScenario<MainActivity>
    private lateinit var item: ItemDetail

    @Before fun prepareOwnedDemo() {
        app.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        runBlocking { app.repository.boot(true); app.repository.resetDemo() }
        item = app.repository.state.value.items.first { it.kindId == "plan" && it.plan != null }
    }

    private fun read() = runBlocking { app.repository.query { it.planPage(item.id) } }
    private fun detail() = runBlocking { app.repository.query { it.item(item.id) } }
    private fun line(page: PlanPageDto, legacyId: String): PlanPageBlockDto = page.blocks.single {
        (it.content as? PlanPageContentDto.Line)?.legacyId == legacyId
    }
    private fun open() {
        scenario = ActivityScenario.launch(Intent(app, MainActivity::class.java).putExtra("demo", true)
            .setAction(Intent.ACTION_VIEW).setData(Uri.parse("zoen://item/${item.id}")))
        compose.waitUntil(15_000) { compose.onAllNodesWithTag("plan-lines").fetchSemanticsNodes().isNotEmpty() }
    }
    private fun edit(block: PlanPageBlockDto) {
        compose.onNodeWithTag("plan-lines").performScrollToNode(hasTestTag("plan-edit:${block.id}"))
        compose.onNodeWithTag("plan-edit:${block.id}").performClick()
        compose.onNodeWithTag("plan-line-text").assertExists()
    }
    private fun capture(name: String) {
        compose.waitForIdle()
        lateinit var activity: MainActivity
        scenario.onActivity { activity = it }
        CommittedWindowCapture.save(activity, "plan-pages", name)
    }

    @Test fun nativeEditorSavesOneCentCancelsRemovesAndRestoresThroughReadOnlyHistory() {
        val before = read()
        val beforeVersion = detail().version
        assertTrue(before.legacyEditAdmitted)
        val original = before.blocks.first { it.content is PlanPageContentDto.Line }
        val legacyId = (original.content as PlanPageContentDto.Line).legacyId
        val exact = "One-cent café e\u0301 😀"
        open()
        edit(original)
        compose.onNodeWithTag("plan-line-text").performTextReplacement(exact)
        compose.onNodeWithTag("plan-line-cost").performTextReplacement("0.01")
        compose.onNodeWithTag("plan-save-line").performClick()
        compose.waitUntil(10_000) { read().blocks.any { it.text == exact && (it.content as? PlanPageContentDto.Line)?.costCents == 1L } }
        val saved = read()
        val savedVersion = detail().version
        assertEquals(beforeVersion + 1u, savedVersion)
        scenario.recreate()
        compose.waitUntil(15_000) { compose.onAllNodesWithTag("plan-lines").fetchSemanticsNodes().isNotEmpty() }
        val savedLine = line(saved, legacyId)
        compose.onNodeWithTag("plan-lines").performScrollToNode(hasTestTag("plan-block:${savedLine.id}"))
        compose.onNodeWithText(exact).assertIsDisplayed()
        capture("one-cent-after-activity-recreation")

        edit(savedLine)
        compose.onNodeWithTag("plan-line-text").performTextReplacement("Uncommitted change")
        compose.onNodeWithTag("plan-line-cost").performTextReplacement("0.99")
        compose.onNodeWithTag("plan-cancel-line").performClick()
        assertEquals(saved.contentHash, read().contentHash)
        assertEquals(saved.version, read().version)

        edit(savedLine)
        compose.onNodeWithTag("plan-delete-line").performClick()
        compose.onNodeWithTag("plan-confirm-remove").assertIsDisplayed()
        capture("native-removal-confirmation")
        compose.onNodeWithTag("plan-cancel-remove").performClick()
        assertEquals(saved.contentHash, read().contentHash)
        edit(savedLine)
        compose.onNodeWithTag("plan-delete-line").performClick()
        compose.onNodeWithTag("plan-confirm-remove").performClick()
        compose.waitUntil(10_000) { read().blocks.none { (it.content as? PlanPageContentDto.Line)?.legacyId == legacyId } }
        val removed = read()
        val removedVersion = detail().version
        assertEquals(savedVersion + 1u, removedVersion)

        compose.onNodeWithTag("item-history").performClick()
        for (version in 1u..removedVersion) {
            compose.onNodeWithTag("item-versions").performScrollToNode(hasTestTag("version:$version"))
            compose.onNodeWithTag("version:$version").assertIsDisplayed()
        }
        compose.onNodeWithTag("item-versions").performScrollToNode(hasTestTag("version:$savedVersion"))
        compose.onNodeWithTag("version:$savedVersion").performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("version-preview").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("version-preview").performScrollToNode(hasText(exact))
        compose.onNodeWithText(exact).assertIsDisplayed()
        compose.onAllNodesWithTag("plan-save-line").assertCountEquals(0)
        compose.onNodeWithTag("preview-plan-line:$legacyId").assertIsNotEnabled()
        assertEquals(removed.version, read().version)
        assertEquals(removedVersion, detail().version)
        assertEquals(removed.contentHash, read().contentHash)
        capture("historical-version-is-read-only")
        compose.onNodeWithTag("preview-restore").performClick()
        compose.onNodeWithTag("preview-confirm-restore").performClick()
        compose.waitUntil(10_000) { detail().version == removedVersion + 1u }
        val restored = read()
        assertEquals(exact, line(restored, legacyId).text)
        assertEquals(1L, (line(restored, legacyId).content as PlanPageContentDto.Line).costCents)
        val detail = detail()
        assertEquals((1u..detail.version).toList().reversed(), detail.versions.map { it.number })
        assertTrue(runBlocking { app.repository.query { it.verifyLog(item.spaceId).valid } })
        compose.onNodeWithTag("plan-lines").performScrollToNode(hasTestTag("plan-block:${line(restored, legacyId).id}"))
        capture("restoration-keeps-full-history")
    }

    @Test fun removedMemberCannotEditOrRestoreTheReadablePlan() {
        val original = read().blocks.first { it.content is PlanPageContentDto.Line }
        val legacyId = (original.content as PlanPageContentDto.Line).legacyId
        open()
        runBlocking { app.repository.change { it.leaveSpace(item.spaceId) } }
        compose.waitUntil(10_000) { !read().legacyEditAdmitted }
        val readonly = read()
        compose.onNodeWithTag("plan-lines").performScrollToNode(hasTestTag("plan-line:$legacyId"))
        compose.onNodeWithTag("plan-line:$legacyId").assertIsNotEnabled()
        compose.onNodeWithTag("plan-line:$legacyId").performTouchInput { click() }
        compose.onAllNodesWithTag("plan-edit:${original.id}").assertCountEquals(0)
        compose.onNodeWithTag("item-history").assertIsNotEnabled()
        assertNotNull(runCatching { runBlocking { app.repository.change { it.togglePlanLine(item.id, legacyId, read().contentHash) } } }.exceptionOrNull())
        lateinit var model: ZoenViewModel
        scenario.onActivity { model = ViewModelProvider(it)[ZoenViewModel::class.java] }
        assertNotNull(runCatching { runBlocking { model.pageSaves.restoreVersion(app.repository, item.id, app.repository.state.value.me!!.id, 1u, readonly.contentHash) } }.exceptionOrNull())
        assertEquals(readonly.contentHash, read().contentHash)
        assertEquals(readonly.version, read().version)
        capture("removed-member-read-only-admission")
    }

    @Test fun typedOfflinePlanAndLargeExactCentsSurviveColdKeystoreReopening() {
        val namespace = "typed-plan-${UUID.randomUUID()}"
        val folder = File(app.noBackupFilesDir, namespace).apply { mkdirs() }
        val vault = AndroidSecretVault(app, "$namespace-vault")
        val database = File(folder, "account.sqlite").absolutePath
        var core = RodaEngine.open(database, "en")
        try {
            core.createAccount("Plan Test", "plan_test", "http://10.0.2.2:18787", vault)
            val chat = core.spaces().first { it.counterpart?.handle == "zoen" }
            val agent = core.agents().first { it.persona.handle == "zoen" }
            core.setTrust(agent.persona.id, chat.id, TrustLevelDto.ACT)
            val created = checkNotNull(core.agentCreatePlan(chat.id, agent.persona.id, "A weekend trip",
                LocalPlanner.plan("A weekend trip", "en"), "Local planner", 0).item)
            val initial = core.planPage(created.id)
            assertTrue(initial.legacyEditAdmitted)
            val line = initial.blocks.first { it.content is PlanPageContentDto.Line }.content as PlanPageContentDto.Line
            val exact = "Keep cafe\u0301 😀\r\nNEL\u0085paragraph\u2029bytes"
            val cents = 9_007_199_254_740_991L
            core.editPlanLine(created.id, line.legacyId, exact, cents, initial.contentHash)
            val saved = core.planPage(created.id)
            core.destroy()
            core = RodaEngine.open(database, "en")
            assertTrue(core.unlock(vault))
            val reopened = core.planPage(created.id)
            assertEquals(saved.blocks, reopened.blocks)
            assertEquals(saved.contentHash, reopened.contentHash)
            assertEquals(saved.source, reopened.source)
            assertEquals(saved.totalCents, reopened.totalCents)
            assertTrue(reopened.legacyEditAdmitted)
            val retained = reopened.blocks.single { (it.content as? PlanPageContentDto.Line)?.legacyId == line.legacyId }
            assertEquals(exact, retained.text)
            assertEquals(cents, (retained.content as PlanPageContentDto.Line).costCents)
            val version = core.item(created.id).version
            core.restorePlanVersion(created.id, 1u, reopened.contentHash)
            assertEquals(version + 1u, core.item(created.id).version)
            assertTrue(core.verifyAll().all { it.valid })
        } finally {
            core.eraseDevice(vault); core.destroy(); folder.deleteRecursively()
            File(app.noBackupFilesDir, "$namespace-vault").deleteRecursively()
        }
    }

    @After fun close() { if (::scenario.isInitialized) scenario.close() }
}
