package xyz.tironi.zoen.pages

import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import xyz.tironi.zoen.core.PageDto
import xyz.tironi.zoen.data.ZoenRepository

data class PageRestoreResult(val page: PageDto, val draft: PageDraft?, val draftStored: Boolean = true)

class PageSaveCoordinator {
    private class Editor(
        val token: Any,
        val save: suspend () -> Unit,
        val pause: (Boolean) -> Unit,
        val restored: (Long, PageRestoreResult?, String?) -> Unit,
    )

    private class Session {
        val gate = Mutex()
        var generation = 0L
        var restoring = false
        var draftRecovery = false
        var editor: Editor? = null
    }

    private val sessions = mutableMapOf<String, Session>()
    private fun session(id: String) = sessions.getOrPut(id) { Session() }

    fun generation(id: String): Long = session(id).generation
    fun draftRecovery(id: String, required: Boolean) { session(id).draftRecovery = required }
    fun canRestore(id: String): Boolean = !session(id).draftRecovery

    fun register(id: String, token: Any, save: suspend () -> Unit, pause: (Boolean) -> Unit,
                 restored: (Long, PageRestoreResult?, String?) -> Unit) {
        val session = session(id)
        session.editor = Editor(token, save, pause, restored)
        pause(session.restoring)
    }

    fun unregister(id: String, token: Any) {
        val session = session(id)
        if (session.editor?.token === token) session.editor = null
    }

    fun isCurrent(id: String, token: Any, generation: Long): Boolean {
        val session = session(id)
        return !session.restoring && session.generation == generation && session.editor?.token === token
    }

    suspend fun save(id: String, token: Any, generation: Long, action: suspend () -> Unit) {
        session(id).gate.withLock {
            if (isCurrent(id, token, generation)) action()
        }
    }

    suspend fun flush(id: String) { session(id).editor?.save?.invoke() }

    internal suspend fun <T> restore(id: String, action: suspend (publish: (PageRestoreResult) -> Unit) -> T): T {
        val session = session(id)
        return session.gate.withLock {
            check(!session.draftRecovery) { "Keep or discard the recovered draft before restoring a version" }
            // Once the core accepts a restore, cancellation cannot abandon the local draft handoff.
            withContext(NonCancellable) {
                session.restoring = true
                var result: PageRestoreResult? = null
                var failure: Exception? = null
                try {
                    session.editor?.pause?.invoke(true)
                    action { result = it }
                } catch (error: Exception) {
                    failure = error
                    throw error
                } finally {
                    session.generation++
                    try {
                        session.editor?.restored?.invoke(session.generation, result, failure?.message)
                    } finally {
                        session.restoring = false
                        session.editor?.pause?.invoke(false)
                    }
                }
            }
        }
    }

    suspend fun restoreVersion(repository: ZoenRepository, id: String, owner: String, number: UInt): PageDto? = restore(id) { publish ->
        var restoredPage: PageDto? = null
        var failure: Exception? = null
        repository.change { core ->
            check(core.me()?.id == owner) { "The page belongs to another account" }
            if (core.item(id).kindId != "page") {
                core.restoreVersion(id, number)
            } else {
                val store = PageDraftStore(repository.vault, repository.localKey("encryptedPageDraft", id, owner))
                val legacyKey = repository.localKey("pageDraft", id, owner)
                val oldDraft = store.load(repository.preferences, legacyKey)
                val before = core.page(id)
                check(before.canEdit) { before.saveError ?: "This page is read only" }
                val targetHash = core.pageAt(id, number).contentHash
                if (oldDraft?.context?.isEmpty() == true) store.save(oldDraft.bindBeforeRestore(before))
                try { core.restoreVersion(id, number) } catch (error: Exception) { failure = error }
                val fresh = core.page(id)
                if (failure == null || fresh.contentHash == targetHash) {
                    val restoredDraft = if (fresh.unsaved || fresh.pendingSync || fresh.saveError != null || failure != null)
                        PageDraft.fromPage(fresh) else null
                    restoredPage = fresh
                    publish(PageRestoreResult(fresh, restoredDraft, draftStored = false))
                    try {
                        store.replaceAfterRestore(repository.preferences, legacyKey, restoredDraft)
                        publish(PageRestoreResult(fresh, restoredDraft))
                    } catch (error: Exception) {
                        failure = error
                    }
                }
            }
        }
        failure?.let { throw it }
        restoredPage
    }
}
