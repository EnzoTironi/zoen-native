package xyz.tironi.zoen.ui

import org.junit.Assert.*
import org.junit.Test

class RecoveryKeyInputTest {
    @Test fun confirmsOnlyTheSavedKey() {
        val key = "1".repeat(64) // Deliberately non-secret fixture.
        assertTrue(RecoveryKeyInput.matches(key, key.chunked(4).joinToString(" ")))
        assertFalse(RecoveryKeyInput.matches(key, "2".repeat(64)))
        listOf("", key.dropLast(1), key + "1", key + "x", key + "-", "١".repeat(64)).forEach {
            assertFalse(RecoveryKeyInput.valid(it))
            assertFalse(RecoveryKeyInput.matches(key, it))
        }
    }

    @Test fun requiresSecureRelayWithoutCredentials() {
        assertTrue(RecoveryKeyInput.validRelay("https://relay.example"))
        assertTrue(RecoveryKeyInput.validRelay("http://127.0.0.1:8787"))
        listOf("http://relay.example", "https://user:pass@relay.example", "https://relay.example/path", "https://relay.example?key=value").forEach {
            assertFalse(RecoveryKeyInput.validRelay(it))
        }
    }
}
