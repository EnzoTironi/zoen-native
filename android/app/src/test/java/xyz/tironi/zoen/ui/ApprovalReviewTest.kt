package xyz.tironi.zoen.ui

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.*

@OptIn(ExperimentalCoroutinesApi::class)
class ApprovalReviewTest {
    private fun persona(id: String, mine: Boolean = false) = Persona(id, if (mine) PersonaKind.PERSON else PersonaKind.AGENT, id, id, "Z", "#123456", null, "", "me", "Me", null, mine, true)
    private fun request(id: String, chat: String = "chat", action: String = "reversible", always: Boolean = true) = AgentRequestDto(
        id, persona("agent"), chat, "Chat", id, "Detail", "Chat members", "Edit", null, "Ask first", RequestStatus.PENDING,
        1, null, null, null, action, always, false,
    )
    private fun app(vararg requests: AgentRequestDto) = MutableStateFlow(AppState(me = persona("me", true), requests = requests.toList()))

    @Test fun undoBeforeTheDeadlineMakesNoSignedDecision() = runTest {
        val app = app(request("a"))
        var writes = 0
        val review = ApprovalReview(backgroundScope, app, { writes++; true }, { throw it })
        assertTrue(review.queue("a", RequestDecision.APPROVE))
        runCurrent(); advanceTimeBy(4_499); runCurrent()
        assertEquals(0, writes)
        assertTrue(review.undo())
        advanceTimeBy(10_000); runCurrent()
        assertEquals(0, writes)
        assertTrue(review.state.value.hidden.isEmpty())
        assertEquals(0, review.state.value.decided)
    }

    @Test fun expiryAndRepeatedFlushCommitExactlyOnce() = runTest {
        val app = app(request("a"))
        val writes = mutableListOf<PendingApproval>()
        val review = ApprovalReview(backgroundScope, app, { writes += it; true }, { throw it })
        assertTrue(review.queue("a", RequestDecision.DENY))
        assertFalse(review.queue("a", RequestDecision.APPROVE))
        runCurrent(); advanceTimeBy(4_500); runCurrent()
        review.flush(); review.flush(); advanceTimeBy(10_000); runCurrent()
        assertEquals(1, writes.size)
        assertEquals(RequestDecision.DENY, writes.single().decision)
        assertNull(review.state.value.pending)
    }

    @Test fun anotherCardClosesThePreviousUndoWindowAndHasItsOwnUndo() = runTest {
        val app = app(request("a"), request("b"))
        val writes = mutableListOf<String>()
        val review = ApprovalReview(backgroundScope, app, { writes += it.request.id; true }, { throw it })
        assertTrue(review.queue("a", RequestDecision.APPROVE))
        assertTrue(review.queue("b", RequestDecision.DENY))
        assertTrue(review.undo())
        runCurrent(); advanceTimeBy(5_000); runCurrent()
        assertEquals(listOf("a"), writes)
    }

    @Test fun standingRulesCoverOnlyMatchingOwnedPendingRequestsAndPreserveSensitivePrompts() = runTest {
        val app = app(request("a"), request("same"), request("sensitive", always = false), request("other-chat", chat = "other"), request("other-action", action = "money"))
        val writes = mutableListOf<PendingApproval>()
        val review = ApprovalReview(backgroundScope, app, { writes += it; true }, { throw it })
        assertTrue(review.queue("a", RequestDecision.ALWAYS_APPROVE))
        assertEquals(setOf("a", "same"), review.state.value.hidden)
        review.flush(); runCurrent()
        assertEquals(1, writes.size)
        assertEquals(setOf("a", "same"), writes.single().ids)
    }

    @Test fun alwaysDenyIncludesSensitiveRequestsButAlwaysApproveCannotBypassThem() = runTest {
        val app = app(request("a", always = false), request("same"))
        val review = ApprovalReview(backgroundScope, app, { true }, { throw it })
        assertFalse(review.queue("a", RequestDecision.ALWAYS_APPROVE))
        assertTrue(review.queue("a", RequestDecision.ALWAYS_DENY))
        assertEquals(setOf("a", "same"), review.state.value.hidden)
    }

    @Test fun accountChangeAndLockedKeysDiscardTheUncommittedChoice() = runTest {
        val app = app(request("a"))
        var writes = 0
        val review = ApprovalReview(backgroundScope, app, { writes++; true }, { throw it })
        review.queue("a", RequestDecision.APPROVE)
        app.value = app.value.copy(me = persona("other", true))
        review.reconcile(app.value)
        runCurrent(); advanceTimeBy(10_000); runCurrent()
        assertEquals(0, writes)
        app.value = app.value.copy(keyMissing = true)
        assertFalse(review.queue("a", RequestDecision.APPROVE))
        assertTrue(review.state.value.hidden.isEmpty())
    }

    @Test fun aFailedCoreWriteReturnsTheCardAndKeepsTheDecisionUnstored() = runTest {
        val app = app(request("a"))
        val errors = mutableListOf<Exception>()
        val review = ApprovalReview(backgroundScope, app, { error("stale request") }, { errors += it })
        assertTrue(review.queue("a", RequestDecision.APPROVE))
        review.flush(); runCurrent()
        assertEquals("stale request", errors.single().message)
        assertTrue(review.state.value.hidden.isEmpty())
        assertEquals(0, review.state.value.decided)
    }
}
