package xyz.tironi.zoen.media

import java.util.Locale
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

data class TimeRange(val start: Double, val end: Double) {
    init { require(start.isFinite() && end.isFinite() && start >= 0 && end >= start) }
    val duration: Double get() = end - start
    operator fun contains(time: Double): Boolean = time >= start && time < end
    fun overlaps(other: TimeRange) = start < other.end && other.start < end
}

data class TranscriptWord(val text: String, val start: Double, val duration: Double)
data class VoiceTranscript(val text: String, val words: List<TranscriptWord> = emptyList())
data class VoiceEdit(
    val trimStart: Double = 0.0,
    val trimEnd: Double,
    val removedWords: Set<Int> = emptySet(),
    val cuts: List<TimeRange> = emptyList(),
    val shortenedPauses: Set<Int> = emptySet(),
)

/** The same edit decisions drive preview, transcript filtering and the exported samples. */
class VoiceEditor(val duration: Double, transcript: VoiceTranscript = VoiceTranscript("")) {
    init { require(duration.isFinite() && duration > 0) }
    var transcript = transcript
        private set
    var edit = VoiceEdit(trimEnd = duration)
        private set
    private val undo = mutableListOf<VoiceEdit>()
    private val redo = mutableListOf<VoiceEdit>()
    private var liveStart: VoiceEdit? = null
    val canUndo get() = undo.isNotEmpty()
    val canRedo get() = redo.isNotEmpty()
    val words get() = transcript.words
    val longPauses: List<TimeRange> get() = words.zipWithNext().mapNotNull { (a, b) ->
        val start = (a.start + a.duration).coerceIn(0.0, duration)
        val end = b.start.coerceIn(start, duration)
        if (end - start > .7) TimeRange(start, end) else null
    }
    val fillers: List<Int> get() = words.indices.filter { index ->
        words[index].text.lowercase(Locale.ROOT).trim { !it.isLetter() } in FILLERS && !wordRemoved(index)
    }
    val removedRanges: List<TimeRange> get() {
        val all = mutableListOf<TimeRange>()
        if (edit.trimStart > 0) all += TimeRange(0.0, edit.trimStart)
        if (edit.trimEnd < duration) all += TimeRange(edit.trimEnd, duration)
        edit.removedWords.forEach { words.getOrNull(it)?.let { word -> all += bounded(word.start, word.start + word.duration) } }
        all += edit.cuts
        longPauses.forEachIndexed { index, range ->
            if (index in edit.shortenedPauses) all += TimeRange(range.start + .15, range.end - .15)
        }
        return merge(all)
    }
    val keptRanges: List<TimeRange> get() {
        var cursor = 0.0
        val kept = mutableListOf<TimeRange>()
        removedRanges.forEach { removed ->
            if (removed.start - cursor >= .05) kept += TimeRange(cursor, removed.start)
            cursor = max(cursor, removed.end)
        }
        if (duration - cursor >= .05) kept += TimeRange(cursor, duration)
        return kept
    }
    val keptDuration get() = keptRanges.sumOf { it.duration }
    val keptTranscript: String get() {
        if (words.isEmpty()) return if (removedRanges.isEmpty()) transcript.text else ""
        val survivors = words.indices.filterNot(::wordRemoved)
        return if (survivors.size == words.size) transcript.text else survivors.joinToString(" ") { words[it].text }
    }
    fun loadTranscript(value: VoiceTranscript) { transcript = value }
    fun isRemoved(time: Double) = removedRanges.any { time in it }
    fun wordRemoved(index: Int): Boolean = index in edit.removedWords || words.getOrNull(index)?.let { isRemoved(it.start + it.duration / 2) } == true
    fun apply(change: (VoiceEdit) -> VoiceEdit) {
        endLive()
        val next = change(edit)
        if (next == edit) return
        undo += edit
        redo.clear()
        edit = next
    }
    fun trim(start: Double, end: Double, live: Boolean = false) {
        val next = edit.copy(trimStart = start.coerceIn(0.0, duration - .3), trimEnd = end.coerceIn(start.coerceIn(0.0, duration - .3) + .3, duration))
        if (next == edit) return
        if (live) { if (liveStart == null) { liveStart = edit; undo += edit; redo.clear() }; edit = next }
        else apply { next }
    }
    fun endLive() { liveStart = null }
    fun undo() { endLive(); if (undo.isNotEmpty()) { redo += edit; edit = undo.removeAt(undo.lastIndex) } }
    fun redo() { endLive(); if (redo.isNotEmpty()) { undo += edit; edit = redo.removeAt(redo.lastIndex) } }
    fun cut(range: TimeRange) { apply { it.copy(cuts = it.cuts + bounded(range.start, range.end)) } }
    fun removeWords(indices: Set<Int>) { apply { it.copy(removedWords = it.removedWords + indices.filter { index -> index in words.indices }) } }
    fun shortenPauses(indices: Set<Int>) { apply { it.copy(shortenedPauses = it.shortenedPauses + indices.filter { index -> index in longPauses.indices }) } }
    fun restoreWords(indices: Set<Int>) {
        apply { original ->
            var next = original.copy(removedWords = original.removedWords - indices)
            for (index in indices) words.getOrNull(index)?.let { word ->
                val span = bounded(word.start, word.start + word.duration)
                next = next.copy(cuts = next.cuts.flatMap { subtract(span, it) },
                    trimStart = min(next.trimStart, max(0.0, span.start - .02)), trimEnd = max(next.trimEnd, min(duration, span.end + .02)),
                    shortenedPauses = next.shortenedPauses.filterNot { longPauses.getOrNull(it)?.overlaps(span) == true }.toSet())
            }
            next
        }
    }
    fun restoreRegion(time: Double) {
        val region = removedRanges.firstOrNull { time in it } ?: return
        apply { next -> next.copy(
            cuts = next.cuts.flatMap { subtract(region, it) },
            removedWords = next.removedWords.filterNot { index -> words.getOrNull(index)?.let { bounded(it.start, it.start + it.duration).overlaps(region) } == true }.toSet(),
            shortenedPauses = next.shortenedPauses.filterNot { longPauses.getOrNull(it)?.overlaps(region) == true }.toSet(),
            trimStart = if (region.start <= .001) 0.0 else next.trimStart,
            trimEnd = if (region.end >= duration - .001) duration else next.trimEnd,
        ) }
    }
    fun editedTime(original: Double): Double {
        var accumulated = 0.0
        for (range in keptRanges) {
            if (original <= range.start) return accumulated
            if (original < range.end) return accumulated + original - range.start
            accumulated += range.duration
        }
        return accumulated
    }
    fun originalTime(edited: Double): Double {
        var accumulated = 0.0
        for (range in keptRanges) {
            if (edited < accumulated + range.duration) return range.start + max(0.0, edited - accumulated)
            accumulated += range.duration
        }
        return keptRanges.lastOrNull()?.end ?: 0.0
    }
    private fun bounded(start: Double, end: Double) = TimeRange(start.coerceIn(0.0, duration), end.coerceIn(start.coerceIn(0.0, duration), duration))

    companion object {
        val FILLERS = setOf("uh", "um", "uhm", "umm", "er", "erm", "hmm", "hum", "ahn", "ã", "é", "eh", "tipo")
        fun merge(ranges: List<TimeRange>): List<TimeRange> {
            val out = mutableListOf<TimeRange>()
            ranges.filter { it.duration > 0 }.sortedBy { it.start }.forEach { range ->
                val last = out.lastOrNull()
                if (last != null && range.start <= last.end) out[out.lastIndex] = TimeRange(last.start, max(last.end, range.end))
                else out += range
            }
            return out
        }
        fun subtract(hole: TimeRange, range: TimeRange): List<TimeRange> = if (!range.overlaps(hole)) listOf(range) else buildList {
            if (hole.start - range.start > .02) add(TimeRange(range.start, min(hole.start, range.end)))
            if (range.end - hole.end > .02) add(TimeRange(max(hole.end, range.start), range.end))
        }
    }
}

data class VoiceNoteRef(val id: String, val ms: Long, val levels: List<Float>, val transcript: String) {
    val marker: String get() = "⟦voice:$id:$ms:${levels.joinToString("") { (it.coerceIn(0f, 1f) * 15).roundToInt().toString(16) }}⟧" + if (transcript.isEmpty()) "" else "\n$transcript"
    companion object {
        fun parse(text: String): VoiceNoteRef? {
            if (!text.startsWith("⟦voice:")) return null
            val end = text.indexOf('⟧')
            if (end < 0) return null
            val body = text.substring(7, end).split(':')
            if (body.size != 3 || !body[0].matches(Regex("[A-Za-z0-9_-]{1,160}"))) return null
            val ms = body[1].toLongOrNull()?.takeIf { it in 1..3_600_000 } ?: return null
            if (body[2].length !in 1..160 || body[2].any { it.digitToIntOrNull(16) == null }) return null
            return VoiceNoteRef(body[0], ms, body[2].map { it.digitToInt(16) / 15f }, text.substring(end + 1).trim())
        }
    }
}

fun voiceTime(seconds: Double): String {
    val total = max(0, seconds.toInt())
    return String.format(Locale.ROOT, "%d:%02d", total / 60, total % 60)
}
