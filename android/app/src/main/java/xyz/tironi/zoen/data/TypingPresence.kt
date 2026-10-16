package xyz.tironi.zoen.data

/** Called under the repository gate. Tokens survive resets so old timers cannot expire new events. */
internal class TypingPresence {
    private val updates = mutableMapOf<Pair<String, String>, Long>()
    private var sequence = 0L

    val identities: Map<String, Set<String>>
        get() = updates.keys.groupBy({ it.first }, { it.second }).mapValues { it.value.toSet() }

    fun update(space: String, identity: String, typing: Boolean): Long {
        val token = ++sequence
        if (typing) updates[space to identity] = token else updates.remove(space to identity)
        return token
    }

    fun expire(space: String, identity: String, token: Long) {
        if (updates[space to identity] == token) updates.remove(space to identity)
    }

    fun clear() { updates.clear() }
}
