package xyz.tironi.zoen.ui

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NativeIconMotionTest {
    @Test fun existingFixtureDoesNotWiggleOnRefresh() {
        assertFalse(hasNewPendingRequest(setOf("rq-a", "rq-b"), setOf("rq-b", "rq-a")))
    }
    @Test fun genuineNewRequestWigglesEvenWhenCountUnchanged() {
        assertTrue(hasNewPendingRequest(setOf("rq-a"), setOf("rq-b")))
    }
    @Test fun approvalAndRemovalDoNotWiggle() {
        assertFalse(hasNewPendingRequest(setOf("rq-a", "rq-b"), setOf("rq-b")))
    }
    @Test fun firstNewRequestWiggles() {
        assertTrue(hasNewPendingRequest(emptySet(), setOf("rq-a")))
    }
}
