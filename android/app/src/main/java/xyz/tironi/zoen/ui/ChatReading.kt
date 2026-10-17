package xyz.tironi.zoen.ui

import xyz.tironi.zoen.core.EntryKind
import xyz.tironi.zoen.core.TimelineEntry
import xyz.tironi.zoen.data.isVisibleInChat

internal fun chatScrollTarget(entry: TimelineEntry): String =
    if (entry.isVisibleInChat()) entry.id else entry.inThread ?: entry.id

internal fun firstUnreadMessage(entries: List<TimelineEntry>, unread: Long): String? = entries
    .filter { it.isVisibleInChat() && it.kind is EntryKind.Message && !it.author.isMe }
    .takeLast(unread.coerceIn(0L, Int.MAX_VALUE.toLong()).toInt())
    .firstOrNull()?.id

internal data class ChatAppend(val mine: Boolean, val incoming: Int)

internal fun appendedMessages(entries: List<TimelineEntry>, known: Set<String>): ChatAppend {
    val added = entries.filter { it.id !in known && it.isVisibleInChat() && it.kind is EntryKind.Message }
    return ChatAppend(added.any { it.author.isMe }, added.count { !it.author.isMe })
}
