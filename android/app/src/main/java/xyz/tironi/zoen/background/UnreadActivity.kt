package xyz.tironi.zoen.background

/** Whether a new event introduced activity eligible for a chat notification. */
internal fun hasNewUnreadActivity(
    unread: UInt,
    threadsUnread: UInt,
    eventCount: ULong,
    previousUnread: UInt,
    previousThreadsUnread: UInt,
    previousEventCount: ULong,
): Boolean = (unread > previousUnread || threadsUnread > previousThreadsUnread) && eventCount > previousEventCount
