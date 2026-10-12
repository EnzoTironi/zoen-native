package xyz.tironi.zoen.pages

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.core.*

class PlanPagePresentationTest {
    private fun block(id: String, content: PlanPageContentDto, text: String = id, spans: List<TextSpanDto> = emptyList()) =
        PlanPageBlockDto(id, content, text, spans)
    private fun page(blocks: List<PlanPageBlockDto> = emptyList()) = PlanPageDto("plan", "space", PlanPageReadSourceDto.DOCUMENT_SESSION,
        2u, true, true, false, false, null, "observed-hash", -7, blocks)

    @Test fun orderedBlocksRetainProseAndUseBlockIdsWhenLegacyLineIdsRepeat() {
        val blocks = listOf(
            block("title", PlanPageContentDto.Title(1000)), block("summary", PlanPageContentDto.Summary),
            block("section-one", PlanPageContentDto.Section), block("first", PlanPageContentDto.Line("duplicate", -7, false)),
            block("between", PlanPageContentDto.Prose(PlanProseKindDto.Paragraph)), block("second", PlanPageContentDto.Line("duplicate", 0, true)),
            block("section-two", PlanPageContentDto.Section), block("third", PlanPageContentDto.Line("unique", 1, false)),
        )
        val rows = planPageRows(page(blocks))
        assertEquals(blocks, rows.map { it.block })
        assertEquals(listOf(null, null, 0u, 0u, 0u, 0u, 1u, 1u), rows.map { it.sectionIndex })
        assertFalse(rows[3].uniqueLine)
        assertFalse(rows[5].uniqueLine)
        assertTrue(rows[7].uniqueLine)
        assertEquals(-7L, (rows[3].block.content as PlanPageContentDto.Line).costCents)
    }

    @Test fun exactSignedCentsRoundTripWithoutFloatingPointOrRounding() {
        listOf(0L, 1L, -1L, -7L, 9_007_199_254_740_991L, Long.MAX_VALUE, Long.MIN_VALUE).forEach { cents ->
            val text = planCostText(cents)
            assertEquals(cents, parsePlanCents(text))
            assertEquals(cents, parsePlanCents(text.replace('.', ',')))
        }
        assertEquals(1L, parsePlanCents(".01"))
        listOf("", ".", "-", "0.001", "1e3", " 1.00 ", "NaN", "92233720368547758.08", "-92233720368547758.09").forEach {
            assertNull(it, parsePlanCents(it))
        }
    }

    @Test fun utf16SpansApplyToTheExactTextIncludingUnicodeAndLineSeparators() {
        val exact = "A😀e\u0301\r\nNEL\u0085line\u2028paragraph\u2029"
        val breakAt = exact.indexOf('\u2028').toUInt()
        val spans = listOf(TextSpanDto(1u, 3u, "b", ""), TextSpanDto(3u, 5u, "i", ""),
            TextSpanDto(3u, 5u, "a", "https://example.com/é"), TextSpanDto(breakAt, breakAt + 1u, "hb", ""))
        val rendered = styledPlanText(block("exact", PlanPageContentDto.Summary, exact, spans), Color.Blue, Color.Gray)
        assertEquals(exact, rendered.text)
        assertTrue(rendered.spanStyles.any { it.start == 1 && it.end == 3 && it.item.fontWeight == FontWeight.Bold })
        val link = rendered.getStringAnnotations("a", 0, exact.length).single()
        assertEquals(3, link.start)
        assertEquals(5, link.end)
        assertEquals("https://example.com/é", link.item)
        assertEquals(breakAt.toInt(), rendered.getStringAnnotations("hb", 0, exact.length).single().start)
    }

    @Test fun invalidWireSpansDoNotShiftMarksOntoAnotherScalar() {
        val spans = listOf(TextSpanDto(1u, 2u, "b", ""), TextSpanDto(0u, UInt.MAX_VALUE, "i", ""))
        val rendered = styledPlanText(block("bad-spans", PlanPageContentDto.Summary, "😀x", spans), Color.Blue, Color.Gray)
        assertEquals("😀x", rendered.text)
        assertTrue(rendered.spanStyles.isEmpty())
        assertTrue(rendered.getStringAnnotations(0, rendered.length).isEmpty())
    }

    @Test fun writeAdmissionAndObservedContentAreBothRequired() {
        val observed = page()
        requirePlanEdit(observed, observed, "read only", "changed")
        listOf(observed.copy(legacyEditAdmitted = false), observed.copy(ready = false), observed.copy(contentHash = "new"),
            observed.copy(itemId = "other")).forEach { current ->
            val failure = runCatching { requirePlanEdit(current, observed, "read only", "changed") }.exceptionOrNull()
            assertTrue(failure is IllegalStateException)
        }
        assertEquals("saved draft is read only", runCatching {
            requirePlanEdit(observed.copy(legacyEditAdmitted = false, saveError = "saved draft is read only"), observed, "read only", "changed")
        }.exceptionOrNull()?.message)
    }
}
