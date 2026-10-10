package xyz.tironi.zoen.miniapps

import java.util.concurrent.ConcurrentHashMap

/** Optional, untrusted UI context for the group's planner. Shared state stays in Rust. */
object MiniAppModelContext {
    private val contexts = ConcurrentHashMap<String, String>()
    internal fun set(itemId: String, json: String) { contexts[itemId] = json }
    fun forItem(itemId: String): String? = contexts[itemId]
    fun clear() { contexts.clear() }
}
