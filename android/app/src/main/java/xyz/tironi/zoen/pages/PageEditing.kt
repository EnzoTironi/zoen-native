package xyz.tironi.zoen.pages

import java.util.UUID
import kotlinx.serialization.json.*
import xyz.tironi.zoen.core.PageBlockDto
import xyz.tironi.zoen.core.TextSpanDto

object PageEditing {
    /** Older drafts have no CRDT context. Refuse conflicting changes and preserve new remote blocks. */
    fun recoverLegacy(local: List<PageBlockDto>, base: List<PageBlockDto>, remote: List<PageBlockDto>): List<PageBlockDto> {
        val old = base.associateBy { it.id }
        val current = remote.associateBy { it.id }
        val wanted = local.associateBy { it.id }
        val conflict = "This older draft overlaps a newer edit. Your draft is still saved on this device."
        base.forEach { block ->
            if (wanted[block.id] != block && current[block.id] != block && current[block.id] != wanted[block.id]) error(conflict)
        }
        local.filter { it.id !in old }.forEach { block -> require(current[block.id] == null || current[block.id] == block) { conflict } }
        val common = base.map { it.id }.filter { it in current }
        require(remote.filter { it.id in old }.map { it.id } == common) { conflict }
        val surviving = local.filter { it.id !in old || it.id in current }.map { it.id }.toSet()
        val extras = linkedMapOf<String?, MutableList<PageBlockDto>>()
        var anchor: String? = null
        remote.forEach { block ->
            if (block.id in surviving) anchor = block.id
            else if (block.id !in old && block.id !in wanted) extras.getOrPut(anchor) { mutableListOf() }.add(block)
        }
        return buildList {
            addAll(extras[null].orEmpty())
            local.filter { it.id in surviving }.forEach { block ->
                add(if (old[block.id] == block) current[block.id] ?: block else block)
                addAll(extras[block.id].orEmpty())
            }
        }
    }

    fun blank(kind: String = "paragraph") = PageBlockDto(UUID.randomUUID().toString(), kind, 0u, 0u, if (kind == "numbered") 1u else 0u, false, "", "", "", "", emptyList())

    fun encode(blocks: List<PageBlockDto>): String = buildJsonArray {
        blocks.forEach { block -> add(buildJsonObject {
            put("id", block.id); put("kind", block.kind); put("level", block.level.toLong())
            put("indent", block.indent.toLong()); put("number", block.number.toLong()); put("checked", block.checked)
            put("lang", block.lang); put("url", block.url); put("alt", block.alt); put("text", block.text)
            put("spans", buildJsonArray { block.spans.forEach { span -> add(buildJsonObject {
                put("start", span.start.toLong()); put("end", span.end.toLong()); put("key", span.key); put("value", span.value)
            }) } })
        }) }
    }.toString()

    fun decode(source: String, originals: List<PageBlockDto> = emptyList()): List<PageBlockDto>? = runCatching {
        require(source.length <= 4 * 1024 * 1024)
        val array = Json.parseToJsonElement(source).jsonArray
        require(array.size <= 10_000)
        val byId = originals.associateBy { it.id }
        array.map { element ->
            val b = element.jsonObject
            fun text(key: String, fallback: String = "") = b[key]?.jsonPrimitive?.content ?: fallback
            fun number(key: String, fallback: UInt = 0u) = b[key]?.jsonPrimitive?.long?.also { require(it in 0..UInt.MAX_VALUE.toLong()) }?.toUInt() ?: fallback
            val id = text("id").also { require(it.isNotBlank()) }
            val old = byId[id]
            val content = text("text")
            val spans = b["spans"]?.jsonArray?.map { entry ->
                val s = entry.jsonObject
                TextSpanDto(s.getValue("start").jsonPrimitive.long.toUInt(), s.getValue("end").jsonPrimitive.long.toUInt(), s.getValue("key").jsonPrimitive.content, s["value"]?.jsonPrimitive?.content.orEmpty())
            } ?: if (old?.text == content) old.spans else emptyList()
            PageBlockDto(id, text("kind", old?.kind ?: "paragraph"), number("level", old?.level ?: 0u), number("indent", old?.indent ?: 0u), number("number", old?.number ?: 0u), b["checked"]?.jsonPrimitive?.boolean ?: old?.checked ?: false, text("lang", old?.lang.orEmpty()), text("url", old?.url.orEmpty()), text("alt", old?.alt.orEmpty()), content, spans)
        }.also { require(it.map { b -> b.id }.distinct().size == it.size) }
    }.getOrNull()

    fun marksAt(block: PageBlockDto, caret: Int): Map<String, String?> {
        val offset = (caret - 1).coerceIn(0, (block.text.length - 1).coerceAtLeast(0))
        return listOf("b", "i", "s", "c", "a").associateWith { key ->
            block.spans.lastOrNull { it.key == key && offset >= it.start.toInt() && offset < it.end.toInt() }?.value
        }
    }

    fun replaceText(block: PageBlockDto, text: String, typingMarks: Map<String, String?> = emptyMap()): PageBlockDto {
        if (text == block.text) return block
        var prefix = 0
        while (prefix < minOf(text.length, block.text.length) && text[prefix] == block.text[prefix]) prefix++
        prefix = minOf(PageTextOffsets.boundary(text, prefix), PageTextOffsets.boundary(block.text, prefix))
        var suffix = 0
        while (suffix < minOf(text.length, block.text.length) - prefix && text[text.lastIndex - suffix] == block.text[block.text.lastIndex - suffix]) suffix++
        if (suffix > 0 && (PageTextOffsets.boundary(text, text.length - suffix) != text.length - suffix || PageTextOffsets.boundary(block.text, block.text.length - suffix) != block.text.length - suffix)) suffix--
        val oldEnd = block.text.length - suffix
        val newEnd = text.length - suffix
        val delta = text.length - block.text.length
        val spans = block.spans.mapNotNull { s ->
            val start = s.start.toInt(); val end = s.end.toInt()
            val a = when { start < prefix -> start; start >= oldEnd -> start + delta; else -> prefix }
            val b = when { end <= prefix -> end; end >= oldEnd -> end + delta; else -> newEnd }
            val safeA = a.coerceIn(0, text.length); val safeB = b.coerceIn(safeA, text.length)
            if (safeA == safeB) null else s.copy(start = safeA.toUInt(), end = safeB.toUInt())
        }
        var next = block.copy(text = text, spans = spans)
        if (newEnd > prefix) typingMarks.forEach { (key, value) -> next = setMark(next, key, value, prefix, newEnd) }
        return next
    }

    fun setMark(block: PageBlockDto, key: String, value: String?, start: Int, end: Int): PageBlockDto {
        val from = PageTextOffsets.boundary(block.text, minOf(start, end))
        val to = PageTextOffsets.boundary(block.text, maxOf(start, end)).coerceAtLeast(from)
        if (from == to) return block
        val spans = block.spans.flatMap { span ->
            if (span.key != key || span.end.toInt() <= from || span.start.toInt() >= to) listOf(span)
            else buildList {
                if (span.start.toInt() < from) add(span.copy(end = from.toUInt()))
                if (span.end.toInt() > to) add(span.copy(start = to.toUInt()))
            }
        }.toMutableList()
        if (value != null) spans.add(TextSpanDto(from.toUInt(), to.toUInt(), key, value))
        val merged = spans.groupBy { it.key to it.value }.values.flatMap { group ->
            val ranges = mutableListOf<TextSpanDto>()
            group.sortedBy { it.start }.forEach { span ->
                val previous = ranges.lastOrNull()
                if (previous != null && previous.end >= span.start) ranges[ranges.lastIndex] = previous.copy(end = maxOf(previous.end, span.end))
                else ranges.add(span)
            }
            ranges
        }
        return block.copy(spans = merged.sortedWith(compareBy({ it.start }, { it.key })))
    }

    fun hasMark(block: PageBlockDto, key: String, start: Int, end: Int): Boolean {
        val from = minOf(start, end).coerceIn(0, block.text.length)
        val to = maxOf(start, end).coerceIn(from, block.text.length)
        if (from == to) return marksAt(block, from)[key] != null
        var coveredUntil = from
        block.spans.filter { it.key == key }.sortedBy { it.start }.forEach {
            if (it.start.toInt() <= coveredUntil) coveredUntil = maxOf(coveredUntil, it.end.toInt())
        }
        return coveredUntil >= to
    }

    fun toggle(block: PageBlockDto, key: String, value: String, selectionStart: Int, selectionEnd: Int): PageBlockDto {
        val from = PageTextOffsets.boundary(block.text, minOf(selectionStart, selectionEnd))
        val to = PageTextOffsets.boundary(block.text, maxOf(selectionStart, selectionEnd)).coerceAtLeast(from)
        val start = from
        val end = to
        if (start == end) return block
        val matches = block.spans.filter { it.key == key && (key != "a" || it.value == value) && it.end.toInt() > start && it.start.toInt() < end }.sortedBy { it.start }
        var coveredUntil = start
        matches.forEach { if (it.start.toInt() <= coveredUntil) coveredUntil = maxOf(coveredUntil, it.end.toInt()) }
        val remove = coveredUntil >= end
        val next = block.spans.flatMap { s ->
            if (s.key != key || s.end.toInt() <= start || s.start.toInt() >= end) listOf(s)
            else buildList {
                if (s.start.toInt() < start) add(s.copy(end = start.toUInt()))
                if (s.end.toInt() > end) add(s.copy(start = end.toUInt()))
            }
        }.toMutableList()
        if (!remove) next.add(TextSpanDto(start.toUInt(), end.toUInt(), key, value))
        return block.copy(spans = next.sortedWith(compareBy({ it.start }, { it.key })))
    }

    fun linkTarget(address: String): String? {
        val text = address.trim()
        if (text.isEmpty()) return null
        val target = if (text.contains("://") || text.startsWith("mailto:")) text else "https://$text"
        return target.takeIf { it.startsWith("https://") || it.startsWith("http://") || it.startsWith("mailto:") }
    }

    fun setLink(block: PageBlockDto, address: String, start: Int, end: Int): PageBlockDto {
        val from = PageTextOffsets.boundary(block.text, minOf(start, end))
        val to = PageTextOffsets.boundary(block.text, maxOf(start, end)).coerceAtLeast(from)
        if (address.isBlank()) return setMark(block, "a", null, from, to)
        val target = linkTarget(address) ?: return block
        if (from != to) return setMark(block, "a", target, from, to)
        val inserted = address.trim()
        val next = replaceText(block, block.text.substring(0, from) + inserted + block.text.substring(from), marksAt(block, from))
        return setMark(next, "a", target, from, from + inserted.length)
    }

    fun hardBreak(block: PageBlockDto, start: Int, end: Int): PageBlockDto {
        val from = PageTextOffsets.boundary(block.text, minOf(start, end))
        val to = PageTextOffsets.boundary(block.text, maxOf(start, end)).coerceAtLeast(from)
        val next = replaceText(block, block.text.replaceRange(from, to, "\u2028"), marksAt(block, from))
        return setMark(next, "hb", "", from, from + 1)
    }

    fun split(block: PageBlockDto, offset: Int): Pair<PageBlockDto, PageBlockDto> {
        val at = PageTextOffsets.boundary(block.text, offset)
        fun spans(from: Int, to: Int) = block.spans.mapNotNull { s ->
            val a = maxOf(s.start.toInt(), from); val b = minOf(s.end.toInt(), to)
            if (a >= b) null else s.copy(start = (a - from).toUInt(), end = (b - from).toUInt())
        }
        val left = block.copy(text = block.text.take(at), spans = spans(0, at))
        val right = block.copy(id = UUID.randomUUID().toString(), text = block.text.drop(at), spans = spans(at, block.text.length), checked = false, number = if (block.kind == "numbered") block.number + 1u else block.number)
        return left to if (right.kind == "heading") right.copy(kind = "paragraph", level = 0u) else right
    }

    fun shortcut(block: PageBlockDto): PageBlockDto {
        val text = block.text
        val heading = Regex("^(#{1,6}) $").matchEntire(text)
        val numbered = Regex("^(\\d{1,4})[.)] $").matchEntire(text)
        return when {
            heading != null -> block.copy(kind = "heading", level = heading.groupValues[1].length.toUInt(), text = "", spans = emptyList())
            text == "- " || text == "* " || text == "+ " -> block.copy(kind = "bullet", text = "", spans = emptyList())
            text in listOf("[ ] ", "- [ ] ", "[x] ", "- [x] ") -> block.copy(kind = "task", checked = text.contains('x'), text = "", spans = emptyList())
            numbered != null -> block.copy(kind = "numbered", number = numbered.groupValues[1].toUInt().coerceAtLeast(1u), text = "", spans = emptyList())
            text == "> " -> block.copy(kind = "quote", text = "", spans = emptyList())
            text == "```" -> block.copy(kind = "code", text = "", spans = emptyList())
            text == "---" || text == "***" -> block.copy(kind = "divider", text = "", spans = emptyList())
            else -> block
        }
    }
}

class PageEditHistory(private val limit: Int = 80) {
    private val past = ArrayDeque<List<PageBlockDto>>()
    private val future = ArrayDeque<List<PageBlockDto>>()
    private var key: String? = null
    private var lastAt = 0L
    val canUndo get() = past.isNotEmpty()
    val canRedo get() = future.isNotEmpty()
    fun record(before: List<PageBlockDto>, editKey: String? = null, now: Long = System.currentTimeMillis()) {
        if (editKey == null || editKey != key || now - lastAt > 600) {
            past.addLast(before); while (past.size > limit) past.removeFirst()
        }
        key = editKey; lastAt = now; future.clear()
    }
    fun undo(current: List<PageBlockDto>): List<PageBlockDto>? {
        if (past.isEmpty()) return null
        future.addLast(current); key = null; return past.removeLast()
    }
    fun redo(current: List<PageBlockDto>): List<PageBlockDto>? {
        if (future.isEmpty()) return null
        past.addLast(current); key = null; return future.removeLast()
    }
}
