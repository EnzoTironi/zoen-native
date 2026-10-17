package xyz.tironi.zoen.ui

import org.junit.Assert.assertEquals
import org.junit.Test

class ControlIconMotionTest {
    @Test fun bitmapPressUsesNativeSourceScale() {
        assertEquals(.96f, iconPressScale(true, true), 0f)
        assertEquals(1f, iconPressScale(false, true), 0f)
    }
    @Test fun reducedMotionOrPausedAlwaysRestoresScale() {
        assertEquals(1f, iconPressScale(true, false), 0f)
        assertEquals(1f, iconPressScale(false, false), 0f)
    }
    @Test fun sourceDurationsRemainExplicitNotMeasuredParity() {
        assertEquals(200, ICON_REPLACEMENT_MS)
        assertEquals(150, ICON_PRESS_MS)
    }
}
