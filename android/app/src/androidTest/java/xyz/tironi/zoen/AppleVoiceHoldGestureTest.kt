package xyz.tironi.zoen

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import xyz.tironi.zoen.media.detectVoiceHoldGestures

class AppleVoiceHoldGestureTest {
    @get:Rule val compose = createComposeRule()
    private val events = mutableListOf<String>()
    private fun subject() {
        compose.setContent { Box(Modifier.size(100.dp).testTag("hold").pointerInput(Unit) {
            detectVoiceHoldGestures(110f, 80f, { events += "start" }, { events += "release" }, { events += "lock" }, { events += "cancel" }, holdMillis = 700, armSlop = 14f)
        }) }
        compose.mainClock.autoAdvance = false
    }
    @Test fun plusDoesNotArmBefore700ms() {
        subject()
        compose.onNodeWithTag("hold").performTouchInput { down(center) }
        compose.mainClock.advanceTimeBy(650)
        compose.onNodeWithTag("hold").performTouchInput { up() }
        compose.runOnIdle { assertEquals(emptyList<String>(), events) }
    }
    @Test fun stationaryHoldArmsAndReleaseFinishesExactlyOnce() {
        subject()
        compose.onNodeWithTag("hold").performTouchInput { down(center) }
        compose.mainClock.advanceTimeBy(750)
        compose.onNodeWithTag("hold").performTouchInput { up() }
        compose.runOnIdle { assertEquals(listOf("start", "release"), events) }
    }
    @Test fun leaving14PointSlopCancelsVoiceArm() {
        subject()
        compose.onNodeWithTag("hold").performTouchInput { down(center); moveBy(Offset(15f, 0f)) }
        compose.mainClock.advanceTimeBy(750)
        compose.onNodeWithTag("hold").performTouchInput { up() }
        compose.runOnIdle { assertEquals(emptyList<String>(), events) }
    }
}
