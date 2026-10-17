package xyz.tironi.zoen.background

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class UnreadActivityTest {
    @Test fun oldUnreadDoesNotMakeUnfollowedRepliesNotify() {
        assertFalse(hasNewUnreadActivity(2u, 0u, 11u, 2u, 0u, 10u))
        assertFalse(hasNewUnreadActivity(0u, 2u, 11u, 0u, 2u, 10u))
    }

    @Test fun newRootAndFollowedReplyNotifyIndependently() {
        assertTrue(hasNewUnreadActivity(3u, 1u, 11u, 2u, 2u, 10u))
        assertTrue(hasNewUnreadActivity(1u, 3u, 11u, 2u, 2u, 10u))
        assertTrue(hasNewUnreadActivity(1u, 0u, 1u, 0u, 0u, 0u))
    }

    @Test fun readsAndPreferenceChangesWithoutNewEventsDoNotNotify() {
        assertFalse(hasNewUnreadActivity(0u, 0u, 11u, 2u, 2u, 10u))
        assertFalse(hasNewUnreadActivity(2u, 3u, 10u, 2u, 2u, 10u))
        assertFalse(hasNewUnreadActivity(2u, 3u, 9u, 2u, 2u, 10u))
    }
}
