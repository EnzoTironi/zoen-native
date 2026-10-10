package xyz.tironi.zoen.ui.ink

import java.util.zip.GZIPInputStream
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class OriginalIconParityTest {
    @Test fun everyOriginalGlyphHasIdenticalBrushRibbonsCapsBlotsAndWash() {
        val source = checkNotNull(javaClass.getResourceAsStream("/ink-icons-swift-reference.json.gz"))
        val icons = Json.parseToJsonElement(GZIPInputStream(source).bufferedReader().use { it.readText() }).jsonObject.getValue("icons").jsonArray
        assertEquals(35, icons.size)
        for (element in icons) {
            val icon = element.jsonObject
            val name = icon.getValue("name").jsonPrimitive.content
            val art = ZoenIconArt.all.getValue(ZoenGlyph.entries.single { it.name.equals(name, true) })
            val samples = listOf(Triple(icon.getValue("paths").jsonArray, 0, 1.0),
                Triple(icon.getValue("animatedPaths").jsonArray[0].jsonArray, 2, .37),
                Triple(icon.getValue("animatedPaths").jsonArray[1].jsonArray, 4, 1.0))
            for ((paths, frame, progress) in samples) {
            for ((index, layer) in IconLayer.entries.withIndex()) {
                val actual = InkIconGeometry.paths(art, layer, progress = progress, frame = frame)
                val expected = paths[index].jsonArray
                assertEquals("$name $layer polygon count", expected.size, actual.size)
                for (polygon in actual.indices) {
                    val points = expected[polygon].jsonArray
                    assertEquals("$name $layer polygon $polygon point count", points.size, actual[polygon].size)
                    for ((point, value) in actual[polygon].withIndex()) {
                        val reference = points[point].jsonArray
                        assertEquals("$name $layer polygon $polygon point $point x", reference[0].jsonPrimitive.double, value.x, 1e-11)
                        assertEquals("$name $layer polygon $polygon point $point y", reference[1].jsonPrimitive.double, value.y, 1e-11)
                    }
                }
            }
            }
        }
    }
}
