package xyz.tironi.zoen.ui.ink

import java.util.zip.GZIPInputStream
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.ui.MascotMood
import xyz.tironi.zoen.ui.MascotPose

class OriginalInkParityTest {
    private val reference by lazy {
        val stream = requireNotNull(javaClass.getResourceAsStream("/ink-swift-reference.json.gz"))
        Json.parseToJsonElement(GZIPInputStream(stream).bufferedReader().use { it.readText() }).jsonObject
    }

    @Test fun allPosesAndHeadsMatchOriginalSwiftAtEarlyBlinkAndLongRunningTimes() {
        val frames = reference.getValue("frames").jsonArray
        assertEquals(98, frames.size)
        for (frame in frames) {
            val f = frame.jsonObject; val name = f.getValue("name").jsonPrimitive.content
            val t = f.getValue("time").jsonPrimitive.double
            val actual = when (name) {
                "head" -> MascotArt.head(t, MascotMood.Pout)
                "head_smirk" -> MascotArt.head(t, MascotMood.Smirk)
                "head_working" -> MascotArt.head(t, MascotMood.Working)
                else -> MascotArt.strokes(MascotPose.entries.single { it.name.equals(name, true) }, t)
            }
            val expected = f.getValue("strokes").jsonArray
            assertEquals("$name at $t stroke count", expected.size, actual.size)
            actual.forEachIndexed { i, a ->
                val e = expected[i].jsonObject; val label = "$name at $t stroke $i"
                assertPoints(label, e.getValue("points").jsonArray, a.points)
                assertEquals(label, e.getValue("closed").jsonPrimitive.boolean, a.closed)
                assertEquals(label, e.getValue("smooth").jsonPrimitive.boolean, a.smooth)
                assertEquals(label, e.getValue("ghost").jsonPrimitive.boolean, a.ghost)
                for ((key, value) in listOf("width" to a.width, "start" to a.start, "span" to a.span,
                    "wobble" to a.wobble, "opacity" to a.opacity, "misregister" to a.misregister)) {
                    assertEquals("$label $key", e.getValue(key).jsonPrimitive.double, value, 1e-12)
                }
                assertColor(label, e.getValue("color"), a.color)
                assertColor(label, e.getValue("fill"), a.fill)
            }
        }
    }

    @Test fun inkCurvesTrimAndVariablePressureRibbonMatchSwift() {
        val g = reference.getValue("geometry").jsonObject
        val p = g.getValue("points").jsonArray.map { InkPoint(it.jsonArray[0].jsonPrimitive.double, it.jsonArray[1].jsonPrimitive.double) }
        val curve = InkGeometry.catmull(p, false)
        assertPoints("catmull", g.getValue("catmull").jsonArray, curve)
        assertPoints("closed catmull", g.getValue("closed_catmull").jsonArray, InkGeometry.catmull(p, true))
        assertPoints("linear", g.getValue("linear").jsonArray, InkGeometry.linear(p, true))
        val trimmed = InkGeometry.trim(curve, 0.37)
        assertPoints("trimmed", g.getValue("trimmed").jsonArray, trimmed)
        assertPoints("ribbon", g.getValue("ribbon").jsonArray, InkGeometry.ribbon(trimmed, 0.016, true, InkRng(78uL)))
    }

    @Test fun overflowingSeedMatchesOriginalUnsignedRandomSequence() {
        val rng = InkRng(ULong.MAX_VALUE)
        reference.getValue("rng").jsonArray.forEach { assertEquals(it.jsonPrimitive.double, rng.unit(), 0.0) }
    }

    private fun assertPoints(label: String, expected: JsonArray, actual: List<InkPoint>) {
        assertEquals("$label point count", expected.size, actual.size)
        actual.forEachIndexed { i, p ->
            assertEquals("$label point $i x", expected[i].jsonArray[0].jsonPrimitive.double, p.x, 1e-12)
            assertEquals("$label point $i y", expected[i].jsonArray[1].jsonPrimitive.double, p.y, 1e-12)
        }
    }
    private fun assertColor(label: String, expected: JsonElement, actual: InkColor?) {
        if (expected == JsonNull) { assertNull(label, actual); return }
        val value = requireNotNull(actual)
        assertEquals(label, expected.jsonArray[0].jsonPrimitive.int, value.rgb)
        assertEquals(label, expected.jsonArray[1].jsonPrimitive.double, value.alpha, 1e-12)
    }
}
