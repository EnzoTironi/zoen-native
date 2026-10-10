package xyz.tironi.zoen.pages

import android.content.SharedPreferences
import java.lang.reflect.Proxy
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.core.PageBlockDto
import xyz.tironi.zoen.core.PageDto
import xyz.tironi.zoen.core.SecretVault
import kotlin.time.Duration.Companion.seconds

@OptIn(ExperimentalCoroutinesApi::class)
class PageSaveCoordinatorTest {
    private fun page(text: String, pending: Boolean = false, error: String? = null) = PageDto(
        itemId = "page", spaceId = "space", title = "Notes", path = "notes.md", version = 2u,
        ready = true, unsaved = pending, pendingSync = pending, saveError = error, canEdit = true,
        contentHash = "hash-$text", editContext = "context-$text",
        blocks = listOf(PageBlockDto("block", "paragraph", 0u, 0u, 0u, false, "", "", "", text, emptyList())),
    )

    private class Editor(private val coordinator: PageSaveCoordinator) {
        val token = Any()
        var generation = coordinator.generation("page")
        var text = "Unsent draft"
        var paused = false
        var draftReadable = true
        var error: String? = null

        fun register() = coordinator.register("page", token,
            save = { coordinator.save("page", token, generation) { text = "Flushed draft" } },
            pause = { paused = it }, restored = { next, result, failure ->
                generation = next
                result?.let { text = it.page.blocks.single().text; draftReadable = it.draftStored }
                error = failure ?: result?.page?.saveError
            })
    }

    @Test fun restoreWaitsForSaveAndObsoleteQueuedFlushCannotUndoIt() = runTest(timeout = 3.seconds) {
        val coordinator = PageSaveCoordinator()
        val editor = Editor(coordinator).also { it.register() }
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        launch {
            coordinator.save("page", editor.token, 0) {
                entered.complete(Unit); release.await(); editor.text = "Earlier save"
            }
        }
        runCurrent(); entered.await()
        launch { coordinator.restore("page") { publish -> publish(PageRestoreResult(page("Restored target"), null)) } }
        launch { coordinator.save("page", editor.token, 0) { editor.text = "Obsolete disposed flush" } }
        runCurrent()
        release.complete(Unit); advanceUntilIdle()
        assertEquals("Restored target", editor.text)
        assertFalse(editor.paused)
        coordinator.flush("page")
        assertEquals("Flushed draft", editor.text)
    }

    @Test fun cancelledRestoreWaitingForSaveDoesNotDiscardDraft() = runTest(timeout = 3.seconds) {
        val coordinator = PageSaveCoordinator()
        val editor = Editor(coordinator).also { it.register() }
        val release = CompletableDeferred<Unit>()
        launch { coordinator.save("page", editor.token, 0) { release.await(); editor.text = "Saved original" } }
        runCurrent()
        val restore = launch { coordinator.restore("page") { publish -> publish(PageRestoreResult(page("Should not restore"), null)) } }
        runCurrent(); restore.cancel(); runCurrent()
        release.complete(Unit); advanceUntilIdle()
        assertEquals("Saved original", editor.text)
        assertFalse(editor.paused)
        coordinator.save("page", editor.token, 0) { editor.text = "Original context still works" }
        assertEquals("Original context still works", editor.text)
    }

    @Test fun cancellationAfterRestoreAcceptedStillCompletesDraftHandoff() = runTest(timeout = 3.seconds) {
        val coordinator = PageSaveCoordinator()
        val editor = Editor(coordinator).also { it.register() }
        val accepted = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        var secureDraft = "Unsent draft"
        val restore = launch {
            coordinator.restore("page") { publish ->
                val fresh = page("Restored pending", pending = true)
                accepted.complete(Unit); release.await()
                secureDraft = fresh.blocks.single().text
                publish(PageRestoreResult(fresh, PageDraft.fromPage(fresh)))
            }
        }
        runCurrent(); accepted.await(); restore.cancel(); runCurrent()
        assertTrue(editor.paused)
        release.complete(Unit); advanceUntilIdle()
        assertEquals("Restored pending", secureDraft)
        assertEquals("Restored pending", editor.text)
        assertFalse(editor.paused)
        coordinator.save("page", editor.token, 0) { editor.text = "Cancelled old flush" }
        assertEquals("Restored pending", editor.text)
    }

    @Test fun commitRefusalKeepsRestoredDraftAndSurfacesFailure() = runTest(timeout = 3.seconds) {
        val coordinator = PageSaveCoordinator()
        val editor = Editor(coordinator).also { it.register() }
        try {
            coordinator.restore("page") { publish ->
                val fresh = page("Restored after refusal", pending = true)
                publish(PageRestoreResult(fresh, PageDraft.fromPage(fresh)))
                error("Version commit refused")
            }
            fail("The refused commit must be reported")
        } catch (error: IllegalStateException) {
            assertEquals("Version commit refused", error.message)
        }
        assertEquals("Restored after refusal", editor.text)
        assertEquals("Version commit refused", editor.error)
        assertFalse(editor.paused)
        coordinator.save("page", editor.token, 0) { editor.text = "Pre-restore draft" }
        assertEquals("Restored after refusal", editor.text)
        coordinator.save("page", editor.token, editor.generation) { editor.text = "Retried restored draft" }
        assertEquals("Retried restored draft", editor.text)
    }

    @Test fun replacedEditorAndLateUnregisterCannotFlushTheOldDraft() = runTest(timeout = 3.seconds) {
        val coordinator = PageSaveCoordinator()
        val old = Editor(coordinator).also { it.register() }
        val active = Editor(coordinator).also { it.register() }
        coordinator.unregister("page", old.token)
        coordinator.save("page", old.token, old.generation) { active.text = "Obsolete editor" }
        coordinator.flush("page")
        assertEquals("Flushed draft", active.text)
        assertEquals("Unsent draft", old.text)
    }

    @Test fun editorOpenedDuringRestoreReceivesItsResultBeforeUnpausing() = runTest(timeout = 3.seconds) {
        val coordinator = PageSaveCoordinator()
        val old = Editor(coordinator).also { it.register() }
        val release = CompletableDeferred<Unit>()
        launch {
            coordinator.restore("page") { publish ->
                release.await(); publish(PageRestoreResult(page("Restored replacement"), null))
            }
        }
        runCurrent()
        val active = Editor(coordinator).also { it.register() }
        assertTrue(active.paused)
        release.complete(Unit); advanceUntilIdle()
        assertEquals("Restored replacement", active.text)
        assertFalse(active.paused)
        coordinator.save("page", old.token, old.generation) { active.text = "Old editor draft" }
        assertEquals("Restored replacement", active.text)
    }

    @Test fun restoreCleanupFailureDoesNotPublishReadableOldDraft() = runTest(timeout = 3.seconds) {
        val coordinator = PageSaveCoordinator()
        val editor = Editor(coordinator).also { it.register() }
        val vault = MemoryVault()
        val store = PageDraftStore(vault, "draft", vault::load)
        val preferences = Preferences()
        store.save(PageDraft.fromPage(page("Before restore")))
        vault.deleteWorks = false
        try {
            coordinator.restore("page") { publish ->
                publish(PageRestoreResult(page("Clean restored target"), null, draftStored = false))
                store.replaceAfterRestore(preferences.value, "legacy", null)
            }
            fail("A failed deletion must be reported")
        } catch (error: IllegalStateException) {
            assertEquals("Could not remove the previous draft", error.message)
        }
        assertEquals("Clean restored target", editor.text)
        assertFalse(editor.draftReadable)
        assertEquals("Could not remove the previous draft", editor.error)
        assertEquals("Before restore", PageEditing.decode(store.load(preferences.value, "legacy")!!.content)!!.single().text)
        vault.deleteWorks = true
        store.replaceAfterRestore(preferences.value, "legacy", null)
        assertNull(store.load(preferences.value, "legacy"))
    }

    @Test fun legacyRestoreFencePreservesRecoveryBytesAndFailedWriteAbortsBeforeMutation() = runTest(timeout = 3.seconds) {
        val coordinator = PageSaveCoordinator()
        val vault = MemoryVault()
        val store = PageDraftStore(vault, "draft", vault::load)
        val preferences = Preferences()
        val old = PageDraft("[]", "[]", "", editId = "legacy-edit")
        store.save(old)
        vault.writeWorks = false
        var liveText = "Before restore"
        try {
            coordinator.restore("page") { _ ->
                store.save(old.bindBeforeRestore(page("Observed before restore")))
                liveText = "Restored target"
            }
            fail("The failed durable fence must stop restore")
        } catch (error: IllegalStateException) {
            assertEquals("Could not save the draft on this device", error.message)
        }
        assertEquals("Before restore", liveText)
        assertEquals("legacy-edit", store.load(preferences.value, "legacy")!!.editId)
        vault.writeWorks = true
        coordinator.restore("page") { _ ->
            store.save(old.bindBeforeRestore(page("Observed before restore")))
            liveText = "Restored target"
        }
        val fenced = store.load(preferences.value, "legacy")!!
        assertEquals("Restored target", liveText)
        assertEquals("[]", fenced.content)
        assertEquals("[]", fenced.base)
        assertEquals("context-Observed before restore", fenced.context)
        assertNotEquals("legacy-edit", fenced.editId)
        assertEquals(fenced, fenced.bindBeforeRestore(page("Later observed state")))
    }

    @Test fun interruptedLegacyCleanupFinishesFromEncryptedCopyBeforeItCanReappear() {
        val vault = MemoryVault()
        val store = PageDraftStore(vault, "draft", vault::load)
        val preferences = Preferences(mutableMapOf("legacy" to "[]", "legacy:base" to "[]"))
        preferences.commitWorks = false
        try {
            store.load(preferences.value, "legacy")
            fail("Plaintext cleanup failure must be reported")
        } catch (error: IllegalStateException) {
            assertEquals("Could not finish moving the draft", error.message)
        }
        preferences.commitWorks = true
        val migrated = store.load(preferences.value, "legacy")!!
        assertEquals("[]", migrated.content)
        assertEquals("[]", migrated.base)
        assertFalse(preferences.value.contains("legacy"))
        assertFalse(preferences.value.contains("legacy:base"))
        store.delete()
        assertNull(store.load(preferences.value, "legacy"))
    }

    @Test fun lockedEncryptedDraftNeverFallsBackToPlaintextOrDeletesRecoveryData() {
        val vault = MemoryVault()
        val readable = PageDraftStore(vault, "draft", vault::load)
        readable.save(PageDraft.fromPage(page("Encrypted recovery content")))
        val preferences = Preferences(mutableMapOf("legacy" to "[]", "legacy:base" to "[]"))
        val locked = PageDraftStore(vault, "draft") { error("Saved draft is locked") }
        try {
            locked.load(preferences.value, "legacy")
            fail("A locked encrypted draft must be reported")
        } catch (error: IllegalStateException) {
            assertEquals("Saved draft is locked", error.message)
        }
        assertTrue(preferences.value.contains("legacy"))
        val recovered = readable.load(preferences.value, "legacy")!!
        assertEquals("Encrypted recovery content", PageEditing.decode(recovered.content)!!.single().text)
        assertEquals("context-Encrypted recovery content", recovered.context)
        assertFalse(preferences.value.contains("legacy"))
    }

    private class MemoryVault : SecretVault {
        private val bytes = mutableMapOf<String, ByteArray>()
        var writeWorks = true
        var deleteWorks = true
        override fun load(key: String): ByteArray? = bytes[key]?.copyOf()
        override fun save(key: String, value: ByteArray): Boolean {
            if (!writeWorks) return false
            bytes[key] = value.copyOf(); return true
        }
        override fun delete(key: String) { if (deleteWorks) bytes.remove(key) }
    }

    private class Preferences(private val strings: MutableMap<String, String> = mutableMapOf()) {
        var commitWorks = true
        val value = Proxy.newProxyInstance(SharedPreferences::class.java.classLoader, arrayOf(SharedPreferences::class.java)) { _, method, args ->
            when (method.name) {
                "contains" -> strings.containsKey(args!![0] as String)
                "getString" -> strings[args!![0] as String] ?: args[1]
                "edit" -> editor()
                else -> error("Unexpected preference operation ${method.name}")
            }
        } as SharedPreferences

        private fun editor(): SharedPreferences.Editor {
            val removed = mutableListOf<String>()
            return Proxy.newProxyInstance(SharedPreferences.Editor::class.java.classLoader, arrayOf(SharedPreferences.Editor::class.java)) { proxy, method, args ->
                when (method.name) {
                    "remove" -> { removed.add(args!![0] as String); proxy }
                    "commit" -> { if (commitWorks) removed.forEach { strings.remove(it) }; commitWorks }
                    else -> error("Unexpected preference edit ${method.name}")
                }
            } as SharedPreferences.Editor
        }
    }
}
