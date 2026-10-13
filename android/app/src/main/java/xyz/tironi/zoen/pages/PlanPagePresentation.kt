package xyz.tironi.zoen.pages

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import java.math.BigDecimal
import xyz.tironi.zoen.core.PlanPageBlockDto
import xyz.tironi.zoen.core.PlanPageContentDto
import xyz.tironi.zoen.core.PlanPageDto
import xyz.tironi.zoen.core.formatMoney

data class PlanPageRow(val block: PlanPageBlockDto, val sectionIndex: UInt?, val uniqueLine: Boolean)

fun planPageRows(page: PlanPageDto): List<PlanPageRow> {
    val lineCounts = page.blocks.mapNotNull { (it.content as? PlanPageContentDto.Line)?.legacyId }
        .groupingBy { it }.eachCount()
    var section: UInt? = null
    return page.blocks.map { block ->
        if (block.content is PlanPageContentDto.Section) section = section?.plus(1u) ?: 0u
        val line = block.content as? PlanPageContentDto.Line
        PlanPageRow(block, section, line != null && lineCounts[line.legacyId] == 1)
    }
}

fun planCostText(cents: Long): String = BigDecimal.valueOf(cents, 2).toPlainString()

fun parsePlanCents(text: String): Long? {
    if (!Regex("-?[0-9]*[.,]?[0-9]{0,2}").matches(text)) return null
    return try { text.replace(',', '.').toBigDecimalOrNull()?.movePointRight(2)?.longValueExact() }
    catch (_: ArithmeticException) { null }
}

fun planPageText(page: PlanPageDto, locale: String): String = page.blocks.joinToString("\n") { block ->
    val line = block.content as? PlanPageContentDto.Line
    if (line == null) block.text else "- [${if (line.done) "x" else " "}] ${block.text} · ${formatMoney(line.costCents, locale)}"
}

fun styledPlanText(block: PlanPageBlockDto, linkColor: Color, codeColor: Color): AnnotatedString = buildAnnotatedString {
    append(block.text)
    block.spans.forEach { span ->
        if (span.start >= span.end || span.end.toLong() > block.text.length) return@forEach
        val start = span.start.toInt()
        val end = span.end.toInt()
        if (PageTextOffsets.boundary(block.text, start) != start || PageTextOffsets.boundary(block.text, end) != end) return@forEach
        addStringAnnotation(span.key, span.value, start, end)
        addStyle(when (span.key) {
            "b" -> SpanStyle(fontWeight = FontWeight.Bold)
            "i" -> SpanStyle(fontStyle = FontStyle.Italic)
            "s" -> SpanStyle(textDecoration = TextDecoration.LineThrough)
            "c" -> SpanStyle(fontFamily = FontFamily.Monospace, background = codeColor)
            "a" -> SpanStyle(color = linkColor, textDecoration = TextDecoration.Underline)
            else -> SpanStyle()
        }, start, end)
    }
}
