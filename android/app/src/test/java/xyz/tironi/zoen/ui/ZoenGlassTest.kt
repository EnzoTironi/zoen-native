package xyz.tironi.zoen.ui

import org.junit.Assert.*
import org.junit.Test

class ZoenGlassTest {
    @Test fun api28Through30UseUnblurredMaterial() {
        for (sdk in 28..30) assertFalse(supportsGlassBackdrop(sdk))
        for (sdk in 31..36) assertTrue(supportsGlassBackdrop(sdk))
    }

    @Test fun fallbackRemainsTranslucentButMoreLegibleWithoutDiffusion() {
        for (dark in listOf(false, true)) {
            val blurred = glassAppearance(dark, true)
            val fallback = glassAppearance(dark, false)
            assertTrue(fallback.tintAlpha > blurred.tintAlpha)
            assertTrue(fallback.tintAlpha < 1f)
            assertTrue(fallback.tintAlpha > 0f)
        }
    }

    @Test fun edgeAndDepthFollowIOSBarEdge() {
        val dark = glassAppearance(true, true)
        val light = glassAppearance(false, true)
        assertEquals(0.62f, dark.tintAlpha)
        assertEquals(0.28f, dark.edgeTopAlpha)
        assertEquals(0.12f, dark.edgeBottomAlpha)
        assertTrue(dark.shadowAlpha > light.shadowAlpha)
        for (style in listOf(dark, light)) assertTrue(style.edgeTopAlpha > style.edgeBottomAlpha)
    }
}
