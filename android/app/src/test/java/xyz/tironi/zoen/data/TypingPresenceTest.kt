package xyz.tironi.zoen.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class TypingPresenceTest {
    @Test fun concurrentPeopleStopIndependently() {
        val typing = TypingPresence()
        typing.update("room", "alice", true)
        typing.update("room", "bob", true)
        assertEquals(setOf("alice", "bob"), typing.identities["room"])
        typing.update("room", "alice", false)
        assertEquals(setOf("bob"), typing.identities["room"])
    }

    @Test fun expiryIsScopedToSpaceAndIdentity() {
        val typing = TypingPresence()
        val first = typing.update("one", "alice", true)
        typing.update("one", "bob", true)
        typing.update("two", "alice", true)
        typing.expire("one", "alice", first)
        assertEquals(mapOf("one" to setOf("bob"), "two" to setOf("alice")), typing.identities)
    }

    @Test fun renewedTypingOutlivesOldTimer() {
        val typing = TypingPresence()
        val old = typing.update("room", "alice", true)
        val current = typing.update("room", "alice", true)
        typing.expire("room", "alice", old)
        assertEquals(setOf("alice"), typing.identities["room"])
        typing.expire("room", "alice", current)
        assertTrue(typing.identities.isEmpty())
    }

    @Test fun stoppedThenRestartedTypingOutlivesOldTimer() {
        val typing = TypingPresence()
        val old = typing.update("room", "alice", true)
        typing.update("room", "alice", false)
        typing.update("room", "alice", true)
        typing.expire("room", "alice", old)
        assertEquals(setOf("alice"), typing.identities["room"])
    }

    @Test fun reconnectClearsAllAndInvalidatesOldTimers() {
        val typing = TypingPresence()
        val old = typing.update("room", "alice", true)
        typing.update("other", "bob", true)
        typing.clear()
        assertTrue(typing.identities.isEmpty())
        typing.update("room", "alice", true)
        typing.expire("room", "alice", old)
        assertEquals(mapOf("room" to setOf("alice")), typing.identities)
    }
}
