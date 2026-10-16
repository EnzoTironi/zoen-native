package xyz.tironi.zoen

import android.content.ContentValues
import android.content.Intent
import android.database.sqlite.SQLiteDatabase
import android.net.Uri
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.util.UUID
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.FileAccess
import xyz.tironi.zoen.ui.readFileVersion

@RunWith(AndroidJUnit4::class)
class FileVersionJourneyTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val app get() = ApplicationProvider.getApplicationContext<ZoenApplication>()
    private lateinit var folder: File
    private lateinit var database: File
    private lateinit var core: ZoenEngine
    private var scenario: ActivityScenario<MainActivity>? = null
    private val tooLarge get() = app.getString(R.string.file_open_too_large)

    @Before fun createOwnedFixture() {
        folder = File(app.noBackupFilesDir, "file-version-${UUID.randomUUID()}").apply { mkdirs() }
        database = File(folder, "demo.sqlite")
        openCore()
        assertTrue(core.seedDemoIfEmpty())
    }

    private fun openCore() {
        core = ZoenEngine.open(database.absolutePath, "en")
    }

    private fun closeCore() {
        if (::core.isInitialized) core.destroy()
    }

    private fun <T> whileClosed(block: () -> T): T {
        closeCore()
        try {
            return block()
        } finally {
            openCore()
        }
    }

    private fun addFile(name: String, mime: String, bytes: ByteArray) =
        core.fileAdd(core.spaces().first { it.counterpart?.handle == "zoen" }.id, name, name, mime, bytes, null)

    private fun reopen() {
        closeCore()
        openCore()
    }

    private fun cache(database: File, sha: String): ContentValues {
        SQLiteDatabase.openDatabase(database.absolutePath, null, SQLiteDatabase.OPEN_READWRITE).use { sqlite ->
            sqlite.rawQuery("SELECT mime, bytes, at_ms FROM media WHERE sha256 = ?", arrayOf(sha)).use { cursor ->
                assertTrue("The fixture must own the chunk being removed", cursor.moveToFirst())
                return ContentValues().apply {
                    put("sha256", sha); put("mime", cursor.getString(0)); put("bytes", cursor.getBlob(1)); put("at_ms", cursor.getLong(2))
                }
            }
        }
    }

    private fun removeCache(database: File, sha: String) {
        SQLiteDatabase.openDatabase(database.absolutePath, null, SQLiteDatabase.OPEN_READWRITE).use {
            assertEquals(1, it.delete("media", "sha256 = ?", arrayOf(sha)))
            it.rawQuery("PRAGMA wal_checkpoint(TRUNCATE)", null).use { cursor -> cursor.moveToFirst() }
        }
    }

    private fun putCache(database: File, row: ContentValues) {
        SQLiteDatabase.openDatabase(database.absolutePath, null, SQLiteDatabase.OPEN_READWRITE).use {
            assertTrue(it.insertWithOnConflict("media", null, row, SQLiteDatabase.CONFLICT_REPLACE) >= 0)
            it.rawQuery("PRAGMA wal_checkpoint(TRUNCATE)", null).use { cursor -> cursor.moveToFirst() }
        }
    }

    @Test fun historicalReaderPinsNameMimeAndBytesWithoutRestoringAcrossColdReopen() {
        val original = "original exact café e\u0301 😀".toByteArray()
        val item = addFile("original.txt", "text/plain", original)
        val replacement = "{\"version\":2}".toByteArray()
        core.fileNewVersionTyped(item.id, replacement, null, "Changed encoding", "encoded.json", "application/json")
        val history = core.item(item.id).versions
        repeat(2) { pass ->
            if (pass == 1) reopen()
            val selected = readFileVersion(core, item.id, item.version, tooLarge)
            assertEquals("original.txt", selected.info.name)
            assertEquals("text/plain", selected.info.mime)
            assertEquals(original.size.toULong(), selected.info.bytes)
            assertArrayEquals(original, selected.bytes)
            val latest = readFileVersion(core, item.id, 2u, tooLarge)
            assertEquals("encoded.json", latest.info.name)
            assertEquals("application/json", latest.info.mime)
            assertArrayEquals(replacement, latest.bytes)
            assertEquals(2u, core.item(item.id).version)
            assertEquals(history, core.item(item.id).versions)
        }
        assertTrue(core.verifyLog(item.spaceId).valid)
    }

    @Test fun over20MiBReaderRefusesThePayloadAndKeepsTheSignedVersion() {
        val bytes = ByteArray(FileAccess.MAX_BYTES + 1) { (it * 31 % 251).toByte() }
        val item = addFile("external-large.bin", "application/octet-stream", bytes)
        assertTrue(core.fileInfoAt(item.id, item.version).ready)
        val failure = runCatching { readFileVersion(core, item.id, item.version, tooLarge) }.exceptionOrNull()
        assertTrue(failure is IllegalArgumentException)
        assertEquals(tooLarge, failure?.message)
        assertTrue(runCatching { core.fileBytesBounded(item.id, item.version, FileAccess.MAX_BYTES.toULong()) }.exceptionOrNull() is CoreException.Invalid)
        assertEquals(item.version, core.item(item.id).version)
        assertEquals(item.versions, core.item(item.id).versions)
        assertTrue(core.verifyLog(item.spaceId).valid)
    }

    @Test fun missingChunksRemainPendingAndMalformedCacheIsAnError() {
        val bytes = "One uniquely owned chunk ${UUID.randomUUID()}".toByteArray()
        val item = addFile("pending.txt", "text/plain", bytes)
        val sha = checkNotNull(item.file).sha256
        val row = whileClosed { cache(database, sha).also { removeCache(database, sha) } }
        val pending = readFileVersion(core, item.id, item.version, tooLarge)
        assertFalse(pending.info.ready)
        assertNull(pending.bytes)
        assertEquals(item.versions, core.item(item.id).versions)
        whileClosed { putCache(database, row) }
        assertArrayEquals(bytes, readFileVersion(core, item.id, item.version, tooLarge).bytes)
        val malformed = ContentValues(row).apply { put("bytes", ByteArray(bytes.size + 1)) }
        whileClosed { putCache(database, malformed) }
        assertFalse(core.fileInfoAt(item.id, item.version).ready)
        assertTrue(runCatching { readFileVersion(core, item.id, item.version, tooLarge) }.exceptionOrNull() is CoreException.Invalid)
        assertEquals(item.version, core.item(item.id).version)
        assertEquals(item.versions, core.item(item.id).versions)
        assertTrue(core.verifyLog(item.spaceId).valid)
    }

    @Test fun nativePreviewRetriesMissingPiecesAndHistoryUsesTheSelectedVersion() {
        app.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        runBlocking { app.repository.boot(true); app.repository.resetDemo() }
        assertTrue(app.repository.state.value.demo)
        val bytes = "A pending original café 😀 ${UUID.randomUUID()}".toByteArray()
        val name = "original-${UUID.randomUUID()}.txt"
        val item = runBlocking { app.repository.change {
            it.fileAdd(checkNotNull(app.repository.state.value.zoenChat).id, name, name, "text/plain", bytes, null)
        } }
        val sha = checkNotNull(item.file).sha256
        val row = runBlocking { app.repository.withDemoDatabaseClosed { file -> cache(file, sha).also { removeCache(file, sha) } } }
        try {
            scenario = ActivityScenario.launch(Intent(app, MainActivity::class.java).putExtra("demo", true)
                .setAction(Intent.ACTION_VIEW).setData(Uri.parse("zoen://item/${item.id}")))
            compose.waitUntil(15_000) { compose.onAllNodesWithTag("file-pending-pieces").fetchSemanticsNodes().isNotEmpty() }
            compose.onAllNodesWithTag("file-export").assertCountEquals(0)
            compose.onAllNodesWithTag("file-open-with").assertCountEquals(0)
            capture("pending-piece-has-no-export")
            runBlocking { app.repository.withDemoDatabaseClosed { file -> putCache(file, row) } }
            compose.waitUntil(10_000) { compose.onAllNodesWithTag("file-export").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText(bytes.toString(Charsets.UTF_8)).assertIsDisplayed()
            assertEquals(item.version, app.repository.state.value.items.single { it.id == item.id }.version)
            capture("same-version-retries-arriving-piece")

            runBlocking { app.repository.withDemoDatabaseClosed { file -> putCache(file, ContentValues(row).apply { put("bytes", ByteArray(bytes.size + 1)) }) } }
            compose.waitUntil(10_000) { compose.onAllNodesWithTag("file-read-error").fetchSemanticsNodes().isNotEmpty() }
            compose.onAllNodesWithTag("file-export").assertCountEquals(0)
            compose.onAllNodesWithTag("file-open-with").assertCountEquals(0)
            capture("malformed-cache-is-an-error")
            runBlocking { app.repository.withDemoDatabaseClosed { file -> putCache(file, row) } }
            compose.waitUntil(10_000) { compose.onAllNodesWithTag("file-export").fetchSemanticsNodes().isNotEmpty() }

            val changedName = "encoded-${UUID.randomUUID()}.json"
            val replacement = "{\"version\":2}".toByteArray()
            runBlocking { app.repository.change { it.fileNewVersionTyped(item.id, replacement, null, "Changed encoding", changedName, "application/json") } }
            compose.waitUntil(10_000) { compose.onAllNodesWithText(replacement.toString(Charsets.UTF_8)).fetchSemanticsNodes().isNotEmpty() }
            compose.onAllNodesWithText(changedName).onFirst().assertIsDisplayed()
            compose.onNodeWithTag("item-history").performClick()
            compose.onNodeWithTag("item-versions").performScrollToNode(hasTestTag("version:${item.version}"))
            compose.onNodeWithTag("version:${item.version}").performClick()
            compose.waitUntil(10_000) { compose.onAllNodesWithTag("version-preview").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithTag("version-preview").performScrollToNode(hasText(bytes.toString(Charsets.UTF_8)))
            compose.onNodeWithText(bytes.toString(Charsets.UTF_8)).assertIsDisplayed()
            compose.onAllNodesWithText(name).onFirst().assertIsDisplayed()
            compose.onNodeWithText("text/plain").assertIsDisplayed()
            compose.onNodeWithTag("preview-share").assertIsEnabled()
            assertEquals(2u, app.repository.state.value.items.single { it.id == item.id }.version)
            capture("historical-name-mime-and-payload")
        } finally { runBlocking { app.repository.withDemoDatabaseClosed { file -> putCache(file, row) } } }
    }

    private fun capture(name: String) {
        compose.waitForIdle()
        lateinit var activity: MainActivity
        checkNotNull(scenario).onActivity { activity = it }
        CommittedWindowCapture.save(activity, "file-versions", name)
    }

    @After fun close() {
        scenario?.close()
        closeCore()
        if (::folder.isInitialized) folder.deleteRecursively()
    }
}
