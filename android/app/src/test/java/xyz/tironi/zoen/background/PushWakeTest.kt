package xyz.tironi.zoen.background

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PushWakeTest {
    @Test fun onlyTheContentFreeWakeIsAccepted() {
        assertTrue(PushWake.isWake(mapOf("wake" to "1"), hasNotification = false))
        assertFalse(PushWake.isWake(mapOf("wake" to "1"), hasNotification = true))
        assertFalse(PushWake.isWake(mapOf("wake" to "1", "body" to "oi"), hasNotification = false))
        assertFalse(PushWake.isWake(mapOf("wake" to "0"), hasNotification = false))
        assertFalse(PushWake.isWake(emptyMap(), hasNotification = false))
    }

    @Test fun pushIsOffUntilFirebaseIsFullyConfigured() {
        assertFalse(PushConfig("", "", "", "").complete)
        assertFalse(PushConfig("zoen", "1:2:android:3", "", "2").complete)
        assertTrue(PushConfig("zoen", "1:2:android:3", "AIza", "2").complete)
    }
}
