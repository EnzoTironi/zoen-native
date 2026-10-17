package xyz.tironi.zoen.ui

import xyz.tironi.zoen.core.TimelineEntry

// Client IDs are unique within a space, not across the Threads inbox.
internal fun threadListKey(space: String, root: String): String = "t:${space.length}:$space:$root"

internal fun showThreadInbox(threads: Boolean, query: String, archived: Boolean): Boolean =
    threads && query.isBlank() && !archived

internal fun observedThreadReadSeq(entries: List<TimelineEntry>, started: Boolean, visible: Boolean): ULong? =
    if (started && visible) entries.maxOfOrNull { it.seq } else null
