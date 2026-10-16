package xyz.tironi.zoen.ui

import org.junit.Assert.*
import org.junit.Test

class FanGestureStateTest {
    @Test fun centreHoldOwnsReleaseEvenAfterMorphAndMovement() {
        val state = FanGestureState(4, 107f)
        state.move(10f, 0f)
        assertTrue(state.arm())
        state.move(-100f, -100f)
        assertEquals(FanRelease.Voice, state.release())
        assertFalse(state.ownsVoice)
    }
    @Test fun dragCancelsArmAndContinuouslyChangesWedge() {
        val state = FanGestureState(4, 107f)
        state.move(0f, -107f)
        assertFalse(state.arm())
        assertEquals(0, state.hovered)
        state.move(-107f, 0f)
        assertEquals(3, state.hovered)
        assertEquals(FanRelease.Choose(3), state.release())
    }
    @Test fun deadZoneAndOutsideArcDismissButOuterZoneSticks() {
        val state = FanGestureState(4, 107f)
        state.move(20f, 0f)
        assertEquals(FanRelease.Dismiss, state.release())
        state.move(100f, 0f)
        assertNull(state.hovered)
        state.move(200f, 0f)
        assertEquals(0, state.hovered)
    }
}
