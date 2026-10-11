package xyz.tironi.zoen.media

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.*
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.ui.PlusVoiceButton

@RunWith(AndroidJUnit4::class)
class VoiceHoldGestureTest {
    @get:Rule val compose = createComposeRule()
    private var actions = 0
    private var starts = 0
    private var releases = 0
    private var locks = 0
    private var cancels = 0
    private val density get() = ApplicationProvider.getApplicationContext<android.content.Context>().resources.displayMetrics.density

    private fun button() {
        compose.setContent {
            MaterialTheme {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    PlusVoiceButton({ actions++ }, { starts++ }, { releases++ }, { locks++ }, { cancels++ })
                }
            }
        }
    }

    private fun hold() {
        compose.onNodeWithTag("home-plus").performTouchInput { down(center) }
        compose.mainClock.advanceTimeBy(800)
        compose.runOnIdle { assertEquals(1, starts); assertEquals(0, actions) }
    }

    @Test fun stationaryHoldReleasesOnceAndNeverClicks() {
        button()
        hold()
        compose.onNodeWithTag("home-plus").performTouchInput { up() }
        compose.runOnIdle { assertEquals(1, releases); assertEquals(0, locks); assertEquals(0, cancels); assertEquals(0, actions) }
    }

    @Test fun leftSlideCancelsOnceAndReleaseDoesNotSendOrClick() {
        button()
        hold()
        val left = -140f * density
        compose.onNodeWithTag("home-plus").performTouchInput { moveBy(Offset(left, 0f)); up() }
        compose.runOnIdle { assertEquals(1, cancels); assertEquals(0, releases); assertEquals(0, locks); assertEquals(0, actions) }
    }

    @Test fun upwardSlideLocksOnceAndReleaseDoesNotSendOrClick() {
        button()
        hold()
        val distance = -100f * density
        compose.onNodeWithTag("home-plus").performTouchInput { moveBy(Offset(0f, distance)); up() }
        compose.runOnIdle { assertEquals(1, locks); assertEquals(0, releases); assertEquals(0, cancels); assertEquals(0, actions) }
    }

    @Test fun externalCancellationCancelsOnceAndDoesNotClick() {
        button()
        hold()
        compose.onNodeWithTag("home-plus").performTouchInput { cancel() }
        compose.runOnIdle { assertEquals(1, cancels); assertEquals(0, releases); assertEquals(0, locks); assertEquals(0, actions) }
    }

    @Test fun quickTapAndNativeAccessibilityClickKeepOpeningActions() {
        button()
        compose.onNodeWithTag("home-plus").performTouchInput { click() }
        compose.onNodeWithTag("home-plus").performClick()
        compose.runOnIdle { assertEquals(2, actions); assertEquals(0, starts); assertEquals(0, releases); assertEquals(0, locks); assertEquals(0, cancels) }
    }

    @Test fun accessibilityRecordingActionStartsLockedWithoutClicking() {
        button()
        val action = compose.onNodeWithTag("home-plus").fetchSemanticsNode().config[SemanticsActions.CustomActions].single()
        compose.runOnIdle { assertTrue(action.action()); assertEquals(1, starts); assertEquals(1, locks); assertEquals(0, releases); assertEquals(0, cancels); assertEquals(0, actions) }
    }
}
