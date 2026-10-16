package xyz.tironi.zoen.ui

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class MessageMutationTest {
    @Test fun failedEditKeepsDraftAndDialogThenRetryCommitsLatestDraft() = runBlocking {
        var draft = "ação revisada"
        var persisted = "original"
        var dismissed = false
        val error = performMessageMutation({ throw IllegalStateException("Sem permissão") }, { dismissed = true })
        assertEquals("Sem permissão", error)
        assertFalse(dismissed)
        assertEquals("ação revisada", draft)
        assertEquals("original", persisted)
        draft += " novamente"
        val retried = performMessageMutation({ persisted = draft }, { dismissed = true })
        assertNull(retried)
        assertEquals("ação revisada novamente", persisted)
        assertTrue(dismissed)
    }

    @Test fun successfulDeletionDismissesOnlyAfterRepositoryReturns() = runBlocking {
        val events = mutableListOf<String>()
        val error = performMessageMutation({ events += "committed" }, { events += "dismissed" })
        assertNull(error)
        assertEquals(listOf("committed", "dismissed"), events)
    }

    @Test fun failedDeletionDoesNotDismissAndCanBeRetried() = runBlocking {
        var dismissed = false
        assertNotNull(performMessageMutation({ throw IllegalStateException() }, { dismissed = true }))
        assertFalse(dismissed)
        assertNull(performMessageMutation({}, { dismissed = true }))
        assertTrue(dismissed)
    }

    @Test fun cancellationPropagatesWithoutDismissal() = runBlocking {
        var dismissed = false
        try {
            performMessageMutation({ throw CancellationException("cancelled") }, { dismissed = true })
            fail("Cancellation must propagate")
        } catch (_: CancellationException) {
            assertFalse(dismissed)
        }
    }
}
