package xyz.tironi.zoen.background

import xyz.tironi.zoen.core.EntryKind
import xyz.tironi.zoen.core.TimelineEntry
import xyz.tironi.zoen.data.isVisibleInChat

/** Reconstruct unread messages from the same suffix counts used by the core. */
internal fun eligibleUnreadActivity(entries: List<TimelineEntry>, unread: UInt): List<TimelineEntry> {
    val incoming = entries.filter { it.kind is EntryKind.Message && !it.author.isMe }
    val chat = incoming.filter { it.isVisibleInChat() }.takeLast(unread.coerceAtMost(Int.MAX_VALUE.toUInt()).toInt())
    val replies = incoming.filter { it.inThread != null }.groupBy { it.inThread }
    val threads = entries.filter { it.inThread == null && it.threadFollowing && it.threadUnread > 0u }.flatMap { root ->
        replies[root.id].orEmpty().takeLast(root.threadUnread.coerceAtMost(Int.MAX_VALUE.toUInt()).toInt())
    }
    // Deleted placeholders still consume the core unread count; filter them only after the suffix.
    return (chat + threads).distinctBy { it.id }.filter { !it.deleted }
}

/** Confirmed per-space event sequences start at zero; eventCount is the next sequence. */
internal fun latestNewUnreadActivity(entries: List<TimelineEntry>, previousEventCount: ULong): TimelineEntry? =
    entries.filter { it.seq >= previousEventCount }.maxByOrNull { it.seq }

internal fun unreadActivityIsVisible(
    entry: TimelineEntry,
    space: String,
    activeChat: String?,
    activeThread: Pair<String, String>?,
): Boolean = (activeChat == space && entry.isVisibleInChat()) ||
    (activeThread?.first == space && activeThread.second == entry.inThread)
