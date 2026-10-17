package xyz.tironi.zoen.background

import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.core.*

class UnreadActivityTest {
    @Test fun oldUnreadDoesNotMakeUnfollowedRepliesNotify() {
        val entries = listOf(message("root", 2uL), message("hidden", 11uL, thread = "root"))
        assertNull(latestNewUnreadActivity(eligibleUnreadActivity(entries, 1u), 10uL))
    }

    @Test fun coalescedReadAndNewArrivalStillNotifyDespiteTheLowerUnreadCount() {
        val entries = listOf(message("old-read", 2uL), message("new", 10uL))
        assertEquals("new", latestNewUnreadActivity(eligibleUnreadActivity(entries, 1u), 10uL)?.id)
    }

    @Test fun coalescedThreadReadAndNewReplyStillNotify() {
        val root = message("root", 1uL).copy(threadFollowing = true, threadUnread = 1u)
        val entries = listOf(root, message("old-read", 3uL, thread = "root"), message("new", 10uL, thread = "root"))
        assertEquals("new", latestNewUnreadActivity(eligibleUnreadActivity(entries, 0u), 10uL)?.id)
    }

    @Test fun followingAnOldBroadcastWithUnrelatedNewEventsDoesNotNotifyAgain() {
        val root = message("root", 1uL).copy(threadFollowing = true, threadUnread = 1u)
        val entries = listOf(
            root, message("other-root", 2uL),
            message("old-broadcast", 9uL, thread = "root", broadcast = true),
            message("new-unfollowed-reply", 10uL, thread = "other-root"),
        )
        assertNull(latestNewUnreadActivity(eligibleUnreadActivity(entries, 1u), 10uL))
    }

    @Test fun notificationUsesTheEligibleReplyNotANewerUnfollowedPreviewOrAuthor() {
        val root = message("followed-root", 1uL).copy(threadFollowing = true, threadUnread = 1u)
        val eligible = message("eligible-body", 10uL, thread = root.id)
        val unrelated = message("wrong-body", 11uL, thread = "unfollowed-root").let {
            it.copy(author = it.author.copy(id = "different-author", name = "Unfollowed author"))
        }
        val entries = listOf(root, message("unfollowed-root", 2uL), eligible, unrelated)
        val activity = latestNewUnreadActivity(eligibleUnreadActivity(entries, 0u), 10uL)

        assertEquals(eligible.id, activity?.id)
        assertEquals(eligible.author.id, activity?.author?.id)
        assertEquals("eligible-body", (activity?.kind as? EntryKind.Message)?.text)
    }

    @Test fun deletedChatAndThreadMessagesConsumeUnreadBeforeTheyAreExcluded() {
        val root = message("root", 1uL).copy(threadFollowing = true, threadUnread = 1u)
        val entries = listOf(
            root, message("old-chat", 2uL), message("old-thread", 3uL, thread = root.id),
            message("deleted-chat", 10uL).copy(deleted = true),
            message("deleted-thread", 11uL, thread = root.id).copy(deleted = true),
        )
        assertTrue(eligibleUnreadActivity(entries, 1u).isEmpty())
    }

    @Test fun aBroadcastIsOnlyOneCandidateEvenWhenBothChatAndThreadAreUnread() {
        val root = message("root", 1uL).copy(threadFollowing = true, threadUnread = 1u)
        val broadcast = message("broadcast", 10uL, thread = root.id, broadcast = true)
        assertEquals(listOf(broadcast), eligibleUnreadActivity(listOf(root, broadcast), 1u))
    }

    @Test fun ownPendingMessagesAndSystemEventsAreNeverNotificationCandidates() {
        val own = message("pending", 0uL).let { it.copy(author = it.author.copy(isMe = true)) }
        val system = message("system", 10uL).copy(kind = EntryKind.System("Changed settings"))
        assertTrue(eligibleUnreadActivity(listOf(own, system), UInt.MAX_VALUE).isEmpty())
    }

    @Test fun eventBoundaryIsInclusiveAndDoesNotRenotifyOlderMessages() {
        val entries = listOf(message("old", 9uL), message("new", 10uL))
        assertEquals("new", latestNewUnreadActivity(entries, 10uL)?.id)
        assertNull(latestNewUnreadActivity(entries, 11uL))
    }

    @Test fun visibleChatOnlySuppressesMessagesShownInTheChat() {
        assertTrue(unreadActivityIsVisible(message("root", 1uL), "space", "space", null))
        assertTrue(unreadActivityIsVisible(message("broadcast", 2uL, thread = "root", broadcast = true), "space", "space", null))
        assertFalse(unreadActivityIsVisible(message("hidden", 3uL, thread = "root"), "space", "space", null))
    }

    @Test fun visibleThreadSuppressesItsRepliesButNotOtherThreadsOrSpaces() {
        val reply = message("reply", 2uL, thread = "root")
        assertTrue(unreadActivityIsVisible(reply, "space", null, "space" to "root"))
        assertFalse(unreadActivityIsVisible(reply, "space", null, "space" to "other-root"))
        assertFalse(unreadActivityIsVisible(reply, "space", null, "other-space" to "root"))
    }

    private fun message(id: String, seq: ULong, thread: String? = null, broadcast: Boolean = false) = TimelineEntry(
        id = id, seq = seq, atMs = 1L, kind = EntryKind.Message(id, null), delivery = Delivery.SENT,
        author = Persona("friend", PersonaKind.PERSON, "Friend", "friend", "F", "#123456", null, "", null, null, null, false, false),
        inThread = thread, alsoInChat = broadcast,
    )
}
