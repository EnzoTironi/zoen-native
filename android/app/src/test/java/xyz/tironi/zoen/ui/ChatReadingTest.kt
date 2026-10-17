package xyz.tironi.zoen.ui

import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.core.Delivery
import xyz.tironi.zoen.core.EntryKind
import xyz.tironi.zoen.core.Persona
import xyz.tironi.zoen.core.PersonaKind
import xyz.tironi.zoen.core.TimelineEntry

class ChatReadingTest {
    @Test fun hiddenRepliesAfterUnreadChatsDoNotConsumeTheUnreadSuffix() {
        val entries = listOf(
            message("read"), message("first-unread"), message("second-unread"),
            message("hidden-one", thread = "read"), message("hidden-two", thread = "read"),
        )

        assertEquals("first-unread", firstUnreadMessage(entries, 2))
    }

    @Test fun broadcastCanBeTheFirstUnreadMessage() {
        val entries = listOf(
            message("read"), message("broadcast", thread = "read", broadcast = true),
            message("own", mine = true), message("hidden", thread = "read"),
            message("last-unread"), system("system"),
        )

        assertEquals("broadcast", firstUnreadMessage(entries, 2))
    }

    @Test fun unreadIndexSelectsTheFirstVisibleIncomingMessageNotTheEndOfTheChat() {
        val entries = listOf(
            message("read"), message("own", mine = true), message("first-unread"),
            message("broadcast", thread = "read", broadcast = true),
            message("hidden", thread = "read"), system("system"),
        )
        val chat = entries.filter { it.inThread == null || it.alsoInChat }
        val unread = firstUnreadMessage(entries, 2)

        assertEquals(2, chat.indexOfFirst { it.id == unread })
        assertEquals("first-unread", unread)
    }

    @Test fun noUnreadDoesNotRecreateAReadBoundary() {
        val entries = listOf(message("read"), message("broadcast", thread = "read", broadcast = true))

        assertNull(firstUnreadMessage(entries, 0))
        assertNull(firstUnreadMessage(entries, -1))
        assertEquals("read", firstUnreadMessage(entries, Long.MAX_VALUE))
        assertNull(firstUnreadMessage(listOf(message("hidden", thread = "read")), 1))
    }

    @Test fun incomingBroadcastsCountTowardTheNewMessagesButtonWhileReading() {
        val entries = listOf(
            message("known"), message("new-root"),
            message("broadcast", thread = "known", broadcast = true),
            message("hidden", thread = "known"),
        )

        // ChatScreen keeps the scroll position and adds incoming to unseen when away from the bottom.
        assertEquals(ChatAppend(mine = false, incoming = 2), appendedMessages(entries, setOf("known")))
    }

    @Test fun ownBroadcastRequestsScrollToLatestButOwnHiddenReplyDoesNot() {
        assertEquals(
            ChatAppend(mine = true, incoming = 0),
            appendedMessages(listOf(message("broadcast", mine = true, thread = "root", broadcast = true)), emptySet()),
        )
        assertEquals(
            ChatAppend(mine = false, incoming = 0),
            appendedMessages(listOf(message("hidden", mine = true, thread = "root")), emptySet()),
        )
    }

    @Test fun atBottomIncomingBroadcastUsesTheSameFollowSignalAsAnIncomingChat() {
        val chat = appendedMessages(listOf(message("root")), emptySet())
        val broadcast = appendedMessages(listOf(message("broadcast", thread = "root", broadcast = true)), emptySet())

        assertEquals(ChatAppend(mine = false, incoming = 1), chat)
        assertEquals(chat, broadcast)
    }

    @Test fun metadataUpdatesDoNotCreateUnseenMessagesOrChangeTheUnreadBoundary() {
        val original = listOf(message("root"), message("broadcast", thread = "root", broadcast = true))
        val updated = original.map { it.copy(threadUnread = 0u, threadFollowing = true, delivery = Delivery.READ) }
        val known = original.map { it.id }.toSet()

        assertEquals(ChatAppend(mine = false, incoming = 0), appendedMessages(updated, known))
        assertEquals(firstUnreadMessage(original, 1), firstUnreadMessage(updated, 1))
        assertNull(firstUnreadMessage(updated, 0))
    }

    @Test fun replayingKnownEntriesAndSystemEventsDoesNotAddUnseenMessages() {
        val root = message("root")
        val broadcast = message("broadcast", thread = "root", broadcast = true)
        val known = setOf(root.id, broadcast.id)

        assertEquals(
            ChatAppend(mine = false, incoming = 0),
            appendedMessages(listOf(root, broadcast, system("new-system"), message("hidden", thread = "root")), known),
        )
    }

    @Test fun broadcastQuoteAndSearchFocusKeepTheirOwnVisibleRow() {
        assertEquals("root", chatScrollTarget(message("root")))
        assertEquals("broadcast", chatScrollTarget(message("broadcast", thread = "root", broadcast = true)))
        assertEquals("root", chatScrollTarget(message("hidden", thread = "root")))
    }

    private fun message(id: String, mine: Boolean = false, thread: String? = null, broadcast: Boolean = false) = TimelineEntry(
        id = id,
        seq = 1uL,
        author = Persona(
            id = if (mine) "me" else "friend", kind = PersonaKind.PERSON,
            name = "Reader", handle = "reader", initials = "R", tintHex = "#123456", glyph = null,
            bio = "", ownerId = null, ownerName = null, ownerTintHex = null, isMe = mine, isMine = mine,
        ),
        atMs = 1L,
        kind = EntryKind.Message(id, null),
        delivery = Delivery.SENT,
        inThread = thread,
        alsoInChat = broadcast,
    )

    private fun system(id: String) = message(id).copy(kind = EntryKind.System("Changed chat settings"))
}
