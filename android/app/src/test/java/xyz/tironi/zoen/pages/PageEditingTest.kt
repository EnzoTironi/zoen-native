package xyz.tironi.zoen.pages

import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.core.TextSpanDto

class PageEditingTest {
    @Test fun separateRemoteChangesPreserveUtf16SelectionPositions() {
        assertEquals(5, PageTextOffsets.remap(4, "abcdefghi", "XabcdefghiY"))
        assertEquals(3, PageTextOffsets.remap(4, "XabcdefghiY", "abcdefghi"))
        assertEquals(6, PageTextOffsets.remap(4, "abcdefghi", "XXabcdeZghi"))
        assertEquals(5, PageTextOffsets.remap(4, "🌿abcdef", "X🌿abcdefY"))
        assertEquals(0, PageTextOffsets.boundary("🌿", 1))
        assertEquals(2, PageTextOffsets.remap(2, "🌿", "🌱"))
        assertEquals(0, PageTextOffsets.remap(4, "gone", ""))
    }
    @Test fun multipleInsertionsAndRemovalsKeepUntouchedCaretAnchors() {
        val old = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
        val random = kotlin.random.Random(7)
        repeat(200) {
            val anchor = random.nextInt(2, 24)
            val next = buildString {
                old.forEachIndexed { index, char ->
                    if (index != anchor + 1 && random.nextBoolean()) append("xy")
                    if (index == anchor || index == anchor + 1 || random.nextBoolean()) append(char)
                }
                append("zz")
            }
            assertEquals(next.indexOf(old[anchor]) + 1, PageTextOffsets.remap(anchor + 1, old, next))
        }
    }
    @Test fun replacingEmojiCannotLeaveFormattingInsideItsSurrogatePair() {
        val first = PageEditing.blank().copy(text = "🌿", spans = listOf(TextSpanDto(0u, 2u, "b", "")))
        val replaced = PageEditing.replaceText(first, "🌱", mapOf("b" to null, "i" to ""))
        assertEquals(listOf(TextSpanDto(0u, 2u, "i", "")), replaced.spans)
        val (left, right) = PageEditing.split(first, 1)
        assertEquals("", left.text)
        assertEquals(first.text, right.text)
    }
    @Test fun olderDraftRecoveryKeepsUnseenBlocksAndRefusesOverlappingEdits() {
        val a = PageEditing.blank().copy(text = "A")
        val b = PageEditing.blank().copy(text = "B")
        val extra = PageEditing.blank().copy(text = "New remote block")
        val local = listOf(a.copy(text = "Local A"), b)
        assertEquals(listOf(local[0], extra, b.copy(text = "Remote B")), PageEditing.recoverLegacy(local, listOf(a, b), listOf(a, extra, b.copy(text = "Remote B"))))
        assertThrows(IllegalStateException::class.java) { PageEditing.recoverLegacy(local, listOf(a, b), listOf(a.copy(text = "Remote A"), b)) }
        assertEquals(listOf(b), PageEditing.recoverLegacy(listOf(a, b), listOf(a, b), listOf(b)))
        assertThrows(IllegalArgumentException::class.java) { PageEditing.recoverLegacy(local, listOf(a, b), listOf(b, a)) }
    }
    @Test fun savedDraftPreservesEveryImportedBlockFieldAndUtf16Span() {
        val blocks = listOf(
            PageEditing.blank("numbered").copy(indent = 2u, number = 7u, text = "Hello 🌿 world", spans = listOf(TextSpanDto(6u, 8u, "b", ""), TextSpanDto(9u, 14u, "a", "https://example.com"))),
            PageEditing.blank("code").copy(lang = "kotlin", text = "val x = 1"),
            PageEditing.blank("image").copy(url = "https://example.com/forest.png", alt = "Forest"),
            PageEditing.blank("heading").copy(level = 3u),
        )
        assertEquals(blocks, PageEditing.decode(PageEditing.encode(blocks)))
        assertNull(PageEditing.decode("not a draft"))
    }
    @Test fun typingBeforeAndInsideAFormattedRangeKeepsItsOffsets() {
        val block = PageEditing.blank().copy(text = "one bold end", spans = listOf(TextSpanDto(4u, 8u, "b", "")))
        val before = PageEditing.replaceText(block, "new one bold end")
        assertEquals(TextSpanDto(8u, 12u, "b", ""), before.spans.single())
        val inside = PageEditing.replaceText(block, "one boXXld end")
        assertEquals(TextSpanDto(4u, 10u, "b", ""), inside.spans.single())
        val delete = PageEditing.replaceText(block, "one end")
        assertTrue(delete.spans.isEmpty())
    }
    @Test fun togglingOnlyPartOfBoldTextDoesNotStripAdjacentFormatting() {
        val block = PageEditing.blank().copy(text = "abcdefghij", spans = listOf(TextSpanDto(1u, 9u, "b", "")))
        val split = PageEditing.toggle(block, "b", "", 3, 6)
        assertEquals(listOf(TextSpanDto(1u, 3u, "b", ""), TextSpanDto(6u, 9u, "b", "")), split.spans)
        val restored = PageEditing.toggle(split, "b", "", 3, 6)
        assertEquals(3, restored.spans.size)
    }
    @Test fun caretFormattingAffectsOnlyFutureTypingAndCanTurnOffInheritedMarks() {
        val block = PageEditing.blank().copy(text = "🌿 bold", spans = listOf(TextSpanDto(3u, 7u, "b", "")))
        assertEquals(block, PageEditing.toggle(block, "i", "", 3, 3))
        val inserted = PageEditing.replaceText(block, "🌿 new bold", mapOf("i" to "", "b" to null))
        assertEquals(TextSpanDto(3u, 7u, "i", ""), inserted.spans.first { it.key == "i" })
        assertEquals(TextSpanDto(7u, 11u, "b", ""), inserted.spans.first { it.key == "b" })
        val appended = PageEditing.replaceText(block, "🌿 bold!", PageEditing.marksAt(block, 7))
        assertEquals(TextSpanDto(3u, 8u, "b", ""), appended.spans.single())
        val plain = PageEditing.replaceText(block, "🌿 bold!", PageEditing.marksAt(block, 7) + ("b" to null))
        assertEquals(block.spans, plain.spans)
    }
    @Test fun linkEditsPreserveUnselectedTextAndCaretInsertsTheAddress() {
        val block = PageEditing.blank().copy(text = "abcde", spans = listOf(TextSpanDto(0u, 5u, "a", "https://old.example")))
        val removed = PageEditing.setLink(block, "", 1, 4)
        assertEquals(listOf(TextSpanDto(0u, 1u, "a", "https://old.example"), TextSpanDto(4u, 5u, "a", "https://old.example")), removed.spans)
        val updated = PageEditing.setLink(block, "new.example", 1, 4)
        assertEquals("https://new.example", updated.spans.first { it.start == 1u }.value)
        val inserted = PageEditing.setLink(PageEditing.blank().copy(text = "Before after"), "example.com", 7, 7)
        assertEquals("Before example.comafter", inserted.text)
        assertEquals(TextSpanDto(7u, 18u, "a", "https://example.com"), inserted.spans.single())
        assertNull(PageEditing.linkTarget("javascript://alert(1)"))
    }
    @Test fun softLineBreakKeepsTheBlockAndSurroundingUtf16Formatting() {
        val block = PageEditing.blank("quote").copy(text = "🌿 bold", spans = listOf(TextSpanDto(3u, 7u, "b", "")))
        val next = PageEditing.hardBreak(block, 3, 3)
        assertEquals(block.id, next.id)
        assertEquals("quote", next.kind)
        assertEquals("🌿 \u2028bold", next.text)
        assertTrue(next.spans.contains(TextSpanDto(3u, 4u, "hb", "")))
        assertTrue(next.spans.contains(TextSpanDto(4u, 8u, "b", "")))
    }
    @Test fun enteringAListContinuesItsNumberAndSplitsRichTextAtUtf16Boundary() {
        val block = PageEditing.blank("numbered").copy(number = 4u, indent = 2u, text = "🌿 abcdef", spans = listOf(TextSpanDto(3u, 9u, "i", "")))
        val (left, right) = PageEditing.split(block, 6)
        assertEquals("🌿 abc", left.text)
        assertEquals("def", right.text)
        assertEquals(5u, right.number)
        assertEquals(2u, right.indent)
        assertEquals(TextSpanDto(0u, 3u, "i", ""), right.spans.single())
        assertNotEquals(left.id, right.id)
    }
    @Test fun undoCoalescesTypingButKeepsStructuralChangesAndRedo() {
        val first = listOf(PageEditing.blank().copy(text = "a"))
        val second = listOf(first[0].copy(text = "ab"))
        val third = listOf(first[0].copy(text = "abc"))
        val history = PageEditHistory()
        history.record(first, "block", 1000); history.record(second, "block", 1200)
        assertEquals(first, history.undo(third))
        assertEquals(third, history.redo(first))
        history.record(third, null, 1300)
        assertEquals(third, history.undo(third + PageEditing.blank()))
    }
    @Test fun markdownShortcutsCreateRealTypedBlocks() {
        assertEquals("heading", PageEditing.shortcut(PageEditing.blank().copy(text = "### ")).kind)
        assertEquals(3u, PageEditing.shortcut(PageEditing.blank().copy(text = "### ")).level)
        assertEquals("task", PageEditing.shortcut(PageEditing.blank().copy(text = "- [ ] ")).kind)
        assertEquals(7u, PageEditing.shortcut(PageEditing.blank().copy(text = "7. ")).number)
        assertEquals("quote", PageEditing.shortcut(PageEditing.blank().copy(text = "> ")).kind)
        assertEquals("code", PageEditing.shortcut(PageEditing.blank().copy(text = "```")).kind)
    }
}
