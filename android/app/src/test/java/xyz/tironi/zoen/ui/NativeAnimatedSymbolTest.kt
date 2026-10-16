package xyz.tironi.zoen.ui

import org.junit.Assert.assertEquals
import org.junit.Test

class NativeAnimatedSymbolTest {
    @Test fun toolbarAndSettingsUseNativeSilhouettes() {
        assertEquals("phone", nativeSymbolForMaterial("Rounded.Call"))
        assertEquals("video", nativeSymbolForMaterial("Rounded.VideoCall"))
        assertEquals("more", nativeSymbolForMaterial("Rounded.MoreVert"))
        assertEquals("language", nativeSymbolForMaterial("Rounded.Translate"))
        assertEquals("devices", nativeSymbolForMaterial("Rounded.Devices"))
    }
    @Test fun reducedMotionAlwaysUsesStaticVector() {
        assertEquals("", nativeSymbolEffect(false, true, true, true, true, true))
    }
    @Test fun restoredPendingDoesNotWiggle() {
        assertEquals("", nativeSymbolEffect(true, false, false, false, false, false))
    }
    @Test fun actualArrivalWigglesAndActualRepeatedTapBounces() {
        assertEquals("wiggle", nativeSymbolEffect(true, false, true, false, false, false))
        assertEquals("bounce", nativeSymbolEffect(true, true, false, false, false, false))
    }
    @Test fun appearanceAndReplacementDrawWhileOnlyLiveStateRepeats() {
        assertEquals("draw", nativeSymbolEffect(true, false, false, false, true, false))
        assertEquals("draw", nativeSymbolEffect(true, false, false, true, false, false))
        assertEquals("active", nativeSymbolEffect(true, false, false, false, false, true))
    }
}
