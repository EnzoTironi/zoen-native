package xyz.tironi.zoen.ui

import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.core.*

class ThreadStateTest {
    @Test fun threadKeysIncludeTheSpaceNotOnlyTheClientId() {
        assertNotEquals(threadListKey("one", "same-id"), threadListKey("two", "same-id"))
        assertNotEquals(threadListKey("one:two", "three"), threadListKey("one", "two:three"))
        assertEquals(threadListKey("one", "same-id"), threadListKey("one", "same-id"))
    }

    @Test fun chatSearchAndArchiveTakePrecedenceOverTheThreadInbox() {
        assertTrue(showThreadInbox(true, "", false))
        assertTrue(showThreadInbox(true, "  ", false))
        assertFalse(showThreadInbox(true, "trip", false))
        assertFalse(showThreadInbox(true, "", true))
        assertFalse(showThreadInbox(false, "", false))
    }

    @Test fun stoppedOrHiddenThreadNeverAcknowledgesLoadedEntries() {
        val entries = listOf(entry(4uL), entry(8uL))
        assertNull(observedThreadReadSeq(entries, started = false, visible = true))
        assertNull(observedThreadReadSeq(entries, started = true, visible = false))
        assertNull(observedThreadReadSeq(emptyList(), started = true, visible = true))
    }

    @Test fun threadReadBoundaryOnlyUsesLoadedConfirmedSequence() {
        val entries = listOf(entry(4uL), entry(8uL), entry(0uL))
        assertEquals(8uL, observedThreadReadSeq(entries, started = true, visible = true))
        assertEquals(4uL, observedThreadReadSeq(entries.take(1), started = true, visible = true))
    }

    private fun entry(seq: ULong) = TimelineEntry(
        id = "entry-$seq", seq = seq, atMs = 1L, kind = EntryKind.Message("reply", null), delivery = Delivery.SENT,
        author = Persona("friend", PersonaKind.PERSON, "Friend", "friend", "F", "#123456", null, "", null, null, null, false, false),
    )
}
