package xyz.tironi.zoen

import android.content.ClipboardManager
import android.content.Intent
import android.net.Uri
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
import xyz.tironi.zoen.pages.PageDraft
import xyz.tironi.zoen.pages.PageDraftStore
import xyz.tironi.zoen.pages.PageEditing
import java.io.File

@RunWith(AndroidJUnit4::class)
class PageDraftRecoveryJourneyTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val app get() = ApplicationProvider.getApplicationContext<ZoenApplication>()
    private lateinit var scenario: ActivityScenario<MainActivity>
    private lateinit var itemId: String
    private lateinit var encryptedKey: String
    private lateinit var legacyKey: String

    @Before fun createPage() {
        app.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        runBlocking { app.repository.boot(true); app.repository.resetDemo() }
        itemId = runBlocking { app.repository.change {
            it.pageImportMarkdown(app.repository.state.value.zoenChat!!.id, "notes/recovery.md", "# Recovery\n\nSaved shared text")
        } }.id
        val owner = app.repository.state.value.me!!.id
        encryptedKey = app.repository.localKey("encryptedPageDraft", itemId, owner)
        legacyKey = app.repository.localKey("pageDraft", itemId, owner)
    }

    private fun open() {
        scenario = ActivityScenario.launch(Intent(app, MainActivity::class.java).putExtra("demo", true)
            .setAction(Intent.ACTION_VIEW).setData(Uri.parse("zoen://item/$itemId")))
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("page-recovery-actions").fetchSemanticsNodes().isNotEmpty() }
    }

    private fun capture(name: String) {
        compose.waitForIdle()
        lateinit var activity: MainActivity
        scenario.onActivity { activity = it }
        CommittedWindowCapture.save(activity, "pages", name)
    }

    private fun discard() {
        compose.onNodeWithTag("page-discard-draft").performClick()
        compose.onNode(hasText(app.getString(R.string.page_discard_draft)) and hasClickAction() and hasAnyAncestor(isDialog())).performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("page-recovery-actions").fetchSemanticsNodes().isEmpty() }
        compose.onNodeWithTag("page-save").assertIsNotEnabled()
    }

    @Test fun rejectedBranchCanBeCopiedAndDiscardedWithoutReplacingTheRestoredPage() {
        val draftText = "Keep this rejected branch ✍️"
        val store = PageDraftStore(app.repository.vault, encryptedKey)
        val saved = runBlocking { app.repository.change { core ->
            val page = core.page(itemId)
            val paragraph = page.blocks.first { it.kind == "paragraph" }
            core.pageApplyFrom(itemId, "recovery-version-two", page.editContext, page.blocks.map { it.id }, listOf(paragraph.copy(text = "Second signed version")))
            core.pageCommit(itemId, "Second version")
            val before = core.page(itemId)
            val pending = before.blocks.map { if (it.id == paragraph.id) it.copy(text = draftText) else it }
            store.save(PageDraft(PageEditing.encode(pending), PageEditing.encode(before.blocks), before.editContext))
            core.restoreVersion(itemId, 1u)
            core.page(itemId)
        } }
        open()
        compose.onNodeWithTag("page-copy-draft").assertIsDisplayed().performClick()
        assertTrue(app.getSystemService(ClipboardManager::class.java).primaryClip!!.getItemAt(0).text.toString().contains(draftText))
        compose.onNodeWithTag("page-save").assertIsNotEnabled()
        capture("recovered-stale-draft-native-actions")
        compose.onNodeWithContentDescription(app.getString(R.string.versions)).performClick()
        compose.onAllNodesWithText(app.getString(R.string.restore)).assertCountEquals(2)
        compose.onAllNodesWithText(app.getString(R.string.restore))[0].assertIsNotEnabled()
        compose.onAllNodesWithText(app.getString(R.string.restore))[1].assertIsNotEnabled()
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand("input keyevent KEYCODE_BACK").close()
        scenario.recreate()
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("page-copy-draft").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("page-discard-draft").performClick()
        compose.onNodeWithText(app.getString(R.string.cancel)).performClick()
        assertNotNull(store.load(app.repository.preferences, legacyKey))
        discard()
        assertNull(store.load(app.repository.preferences, legacyKey))
        val fresh = runBlocking { app.repository.query { it.page(itemId) } }
        assertEquals(saved.contentHash, fresh.contentHash)
        assertEquals(saved.version, fresh.version)
        compose.onNodeWithTag("page-block:${fresh.blocks.first { it.kind == "paragraph" }.id}").performTextReplacement("Editing resumed after explicit discard")
        compose.onNodeWithTag("page-save").performClick()
        compose.waitUntil(10_000) { runBlocking { app.repository.query { it.page(itemId).version > saved.version } } }
        assertTrue(runBlocking { app.repository.query { it.verifyLog(fresh.spaceId).valid } })
    }

    @Test fun unreadableDraftCanBeExportedAsCiphertextAndDiscardedWithoutASharedWrite() {
        assertTrue(app.repository.vault.save(encryptedKey, "unreadable checkpoint".toByteArray()))
        val damaged = app.repository.vault.storedCiphertext(encryptedKey)
        damaged[damaged.lastIndex] = (damaged.last().toInt() xor 1).toByte()
        val hash = java.security.MessageDigest.getInstance("SHA-256").digest(encryptedKey.toByteArray())
            .joinToString("") { "%02x".format(it) }
        File(File(app.noBackupFilesDir, "keys"), hash).writeBytes(damaged)
        assertNull(app.repository.vault.load(encryptedKey))
        val archive = app.repository.vault.storedCiphertext(encryptedKey)
        assertFalse(archive.toString(Charsets.UTF_8).contains("unreadable checkpoint"))
        val before = runBlocking { app.repository.query { it.page(itemId) } }
        open()
        compose.onNodeWithTag("page-keep-draft").assertIsDisplayed()
        capture("unreadable-encrypted-draft-native-actions")
        val exports = File(app.cacheDir, "exports")
        val previous = exports.walkTopDown().filter { it.isFile }.map { it.absolutePath }.toSet()
        compose.onNodeWithTag("page-keep-draft").performClick()
        val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
        compose.waitUntil(10_000) {
            automation.rootInActiveWindow?.packageName?.toString() in setOf("android", "com.android.intentresolver")
        }
        val exported = exports.walkTopDown().first { it.isFile && it.absolutePath !in previous }
        assertArrayEquals(archive, exported.readBytes())
        automation.executeShellCommand("input keyevent KEYCODE_BACK").close()
        compose.onNodeWithTag("page-recovery-actions").assertExists()
        discard()
        assertNull(app.repository.vault.loadStored(encryptedKey))
        val after = runBlocking { app.repository.query { it.page(itemId) } }
        assertEquals(before.contentHash, after.contentHash)
        assertEquals(before.version, after.version)
        exported.delete()
        exported.parentFile?.delete()
    }

    @After fun close() {
        if (::scenario.isInitialized) scenario.close()
        if (::encryptedKey.isInitialized) app.repository.vault.delete(encryptedKey)
    }
}
