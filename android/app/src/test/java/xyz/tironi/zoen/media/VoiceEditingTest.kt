package xyz.tironi.zoen.media

import org.junit.Assert.*
import org.junit.Test

class VoiceEditingTest {
    private fun editor(): VoiceEditor = VoiceEditor(7.0, VoiceTranscript("So um we leave at eight uh and bring snacks and the thermos", listOf(
        TranscriptWord("So", .2, .2), TranscriptWord("um", .6, .2), TranscriptWord("we", 1.0, .2), TranscriptWord("leave", 1.4, .2),
        TranscriptWord("at", 1.8, .2), TranscriptWord("eight", 2.2, .2), TranscriptWord("uh", 3.8, .2), TranscriptWord("and", 4.2, .2),
        TranscriptWord("bring", 4.6, .2), TranscriptWord("snacks", 5.0, .2), TranscriptWord("and", 5.4, .2), TranscriptWord("the", 5.8, .2), TranscriptWord("thermos", 6.2, .3),
    )))
    @Test fun multipleWordAndWaveformCutsRestoreIndividuallyAndRenderOnlySurvivors() {
        val edit = editor()
        edit.removeWords(setOf(0, 9))
        edit.cut(TimeRange(6.6, 6.9))
        edit.removeWords(edit.fillers.toSet())
        assertEquals(5, edit.removedRanges.size)
        edit.restoreWords(setOf(9))
        assertEquals(4, edit.removedRanges.size)
        assertTrue(edit.keptTranscript.contains("snacks"))
        assertFalse(edit.keptTranscript.contains("um"))
        assertFalse(edit.keptTranscript.contains("uh"))
        assertFalse(edit.keptTranscript.startsWith("So"))
        edit.undo(); assertEquals(5, edit.removedRanges.size)
        edit.redo(); assertEquals(4, edit.removedRanges.size)
        assertEquals(6.1, edit.keptDuration, .001)
    }
    @Test fun restoringWordCarvesOnlyItsSpanFromWaveformCuts() {
        val edit = editor()
        edit.cut(TimeRange(4.8, 5.8))
        edit.restoreWords(setOf(9))
        assertEquals(listOf(TimeRange(4.8, 5.0), TimeRange(5.2, 5.8)), edit.edit.cuts)
        assertFalse(edit.wordRemoved(9))
        assertTrue(edit.wordRemoved(10))
    }
    @Test fun longPauseShorteningKeepsPointThreeSecondsAndCanBeUndone() {
        val edit = editor()
        assertEquals(1, edit.longPauses.size)
        edit.shortenPauses(setOf(0))
        assertEquals(5.9, edit.keptDuration, .001)
        edit.restoreRegion(3.0)
        assertEquals(7.0, edit.keptDuration, .001)
        edit.undo(); assertEquals(5.9, edit.keptDuration, .001)
    }
    @Test fun trimDragIsOneUndoAndTimeMappingSkipsCuts() {
        val edit = editor()
        edit.trim(.2, 6.8, live = true); edit.trim(.4, 6.7, live = true); edit.endLive()
        edit.undo(); assertEquals(0.0, edit.edit.trimStart, .001)
        assertFalse(edit.canUndo)
        edit.cut(TimeRange(1.0, 2.0)); edit.cut(TimeRange(3.0, 4.0))
        assertEquals(1.5, edit.editedTime(2.5), .001)
        assertEquals(4.5, edit.originalTime(2.5), .001)
        assertEquals(1.0, edit.editedTime(1.5), .001)
    }
    @Test fun untimedTranscriptNeverClaimsRemovedAudioRemainsInTheTranscript() {
        val edit = VoiceEditor(3.0, VoiceTranscript("All of these words"))
        assertEquals("All of these words", edit.keptTranscript)
        edit.cut(TimeRange(.4, .8)); assertEquals("", edit.keptTranscript)
        edit.undo(); assertEquals("All of these words", edit.keptTranscript)
    }
    @Test fun markerRetainsAppleGrammarAndRejectsMalformedOrUnsafeIds() {
        val reference = VoiceNoteRef("item_voice-123", 7012, listOf(0f, .5f, 1f), "We leave at eight.")
        assertEquals("⟦voice:item_voice-123:7012:08f⟧\nWe leave at eight.", reference.marker)
        val parsed = checkNotNull(VoiceNoteRef.parse(reference.marker))
        assertEquals(reference.id, parsed.id); assertEquals(reference.ms, parsed.ms); assertEquals(reference.transcript, parsed.transcript)
        assertNull(VoiceNoteRef.parse("⟦voice:../../secret:1:1⟧"))
        assertNull(VoiceNoteRef.parse("⟦voice:ok:-2:fff⟧"))
        assertNull(VoiceNoteRef.parse("⟦voice:ok:3:gh⟧"))
    }
    @Test fun pcmCutsPreserveOnlySelectedSamplesAndFadeEveryJoin() {
        val samples = ShortArray(1000) { (it + 100).toShort() }
        val source = PcmAudio(samples, 1000, 1)
        val result = source.edit(listOf(TimeRange(.1, .2), TimeRange(.8, .9)), 0.0)
        assertEquals(200, result.frames)
        assertEquals(200, result.samples[0].toInt()); assertEquals(999, result.samples.last().toInt())
        assertFalse(result.samples.any { it.toInt() in 300..899 })
        val faded = source.edit(listOf(TimeRange(.1, .2), TimeRange(.8, .9)))
        assertEquals(0, faded.samples[0].toInt()); assertEquals(0, faded.samples[99].toInt()); assertEquals(0, faded.samples[100].toInt()); assertEquals(0, faded.samples.last().toInt())
    }
    @Test fun inkUndoRedoDoesNotMutateEarlierSnapshots() {
        val history = InkHistory()
        val first = InkStroke(0, 0, .01f, listOf(InkPoint(.1f, .1f), InkPoint(.9f, .9f)))
        val second = first.copy(page = 1)
        history.add(first); val snapshot = history.strokes
        history.add(second); history.undo(); assertEquals(snapshot, history.strokes)
        history.redo(); assertEquals(listOf(first, second), history.strokes)
        assertEquals(listOf(first), snapshot)
    }
}
