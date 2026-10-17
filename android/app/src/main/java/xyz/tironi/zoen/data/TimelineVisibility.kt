package xyz.tironi.zoen.data

import xyz.tironi.zoen.core.TimelineEntry

/** Broadcast replies have their own chat row; other thread replies do not. */
internal fun TimelineEntry.isVisibleInChat(): Boolean = inThread == null || alsoInChat
