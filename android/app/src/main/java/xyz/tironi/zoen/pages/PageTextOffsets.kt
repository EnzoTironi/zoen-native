package xyz.tironi.zoen.pages

/** Maps UTF-16 editor positions through separate insertions and deletions. */
object PageTextOffsets {
    fun boundary(text: String, offset: Int): Int {
        val at = offset.coerceIn(0, text.length)
        return if (at > 0 && at < text.length && text[at].isLowSurrogate() && text[at - 1].isHighSurrogate()) at - 1 else at
    }

    fun remap(offset: Int, old: String, next: String): Int {
        val at = boundary(old, offset)
        if (old == next) return at
        var prefix = 0
        while (prefix < minOf(old.length, next.length) && old[prefix] == next[prefix]) prefix++
        var suffix = 0
        while (suffix < minOf(old.length, next.length) - prefix && old[old.lastIndex - suffix] == next[next.lastIndex - suffix]) suffix++
        if (at < prefix) return boundary(next, at)
        if (at >= old.length - suffix) return boundary(next, at + next.length - old.length)
        val a = old.substring(prefix, old.length - suffix)
        val b = next.substring(prefix, next.length - suffix)
        val changes = changes(a, b)
        // A completely rewritten paragraph has no stable interior position. Bound
        // diff memory, then keep the nearest valid position in its replacement.
        if (changes == null) return boundary(next, prefix + minOf(at - prefix, b.length))
        var mapped = at - prefix - changes.first.count { it < at - prefix }
        changes.second.sorted().forEach { if (it <= mapped) mapped++ }
        return boundary(next, prefix + mapped)
    }

    private fun changes(old: String, next: String): Pair<List<Int>, List<Int>>? {
        val trace = mutableListOf<IntArray>()
        val limit = minOf(old.length + next.length, 512)
        fun position(depth: Int, diagonal: Int): Int =
            trace.getOrNull(depth)?.getOrNull(depth + diagonal) ?: -1
        for (depth in 0..limit) {
            val row = IntArray(2 * depth + 1) { -1 }
            for (diagonal in -depth..depth step 2) {
                var x = when {
                    depth == 0 -> 0
                    diagonal == -depth || diagonal != depth && position(depth - 1, diagonal - 1) < position(depth - 1, diagonal + 1) -> position(depth - 1, diagonal + 1)
                    else -> position(depth - 1, diagonal - 1) + 1
                }
                var y = x - diagonal
                while (x < old.length && y < next.length && old[x] == next[y]) { x++; y++ }
                row[depth + diagonal] = x
                if (x >= old.length && y >= next.length) {
                    val removed = mutableListOf<Int>()
                    val inserted = mutableListOf<Int>()
                    for (back in depth downTo 1) {
                        val k = x - y
                        val previous = if (k == -back || k != back && position(back - 1, k - 1) < position(back - 1, k + 1)) k + 1 else k - 1
                        val previousX = position(back - 1, previous)
                        val previousY = previousX - previous
                        while (x > previousX && y > previousY) { x--; y-- }
                        if (x == previousX) { y--; inserted.add(y) } else { x--; removed.add(x) }
                    }
                    return removed to inserted
                }
            }
            trace.add(row)
        }
        return null
    }
}
