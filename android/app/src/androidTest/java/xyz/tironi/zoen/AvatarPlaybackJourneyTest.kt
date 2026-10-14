package xyz.tironi.zoen

import android.graphics.drawable.AnimatedImageDrawable
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.provider.Settings
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.Lifecycle
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.agent.AvatarArt
import xyz.tironi.zoen.agent.AvatarDrawing
import xyz.tironi.zoen.theme.ZoenTheme

@RunWith(AndroidJUnit4::class)
class AvatarPlaybackJourneyTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    @Test fun nativeAvatarStopsForLiveReducedMotionLifecycleVisibilityAndRemoval() {
        val originalScale = shell("settings get global animator_duration_scale").trim()
        val show = mutableStateOf(true)
        try {
            setMotion(1f)
            awaitWindowFocus()
            val asset = AvatarArt.assets(compose.activity).first()
            compose.setContent {
                ZoenTheme { Box(Modifier.fillMaxSize()) { if (show.value) AvatarDrawing(asset, 96) } }
            }
            lateinit var image: ImageView
            lateinit var animation: AnimatedImageDrawable
            awaitNative {
                findImage(compose.activity.window.decorView)?.let {
                    image = it
                    animation = it.drawable as AnimatedImageDrawable
                    animation.isRunning
                } == true
            }
            CommittedWindowCapture.save(compose.activity, "experience", "native-avatar-live")
            setMotion(0f)
            awaitNative { !animation.isRunning }
            awaitNative { findImage(compose.activity.window.decorView) == null }
            CommittedWindowCapture.save(compose.activity, "experience", "native-avatar-live-reduced-motion")
            setMotion(1f)
            awaitNative {
                findImage(compose.activity.window.decorView)?.let {
                    image = it
                    animation = it.drawable as AnimatedImageDrawable
                    animation.isRunning
                } == true
            }
            compose.activityRule.scenario.moveToState(Lifecycle.State.STARTED)
            awaitNative { !animation.isRunning }
            compose.activityRule.scenario.moveToState(Lifecycle.State.RESUMED)
            awaitWindowFocus()
            awaitNative { animation.isRunning }
            instrumentation.runOnMainSync { image.visibility = View.INVISIBLE }
            awaitNative { !animation.isRunning }
            instrumentation.runOnMainSync { image.visibility = View.VISIBLE }
            awaitNative { animation.isRunning }
            instrumentation.runOnMainSync { show.value = false }
            awaitNative { !animation.isRunning && findImage(compose.activity.window.decorView) == null }
            Evidence.outputFile("experience", "native-avatar-playback-receipt.txt").writeText(
                "PASS: original ${asset.name} animated WebP plays through AnimatedImageDrawable; live reduced motion stops it and displays the original still without another user action; background, invisibility and removal stop playback; foreground and visibility resume it.\n")
        } finally {
            instrumentation.runOnMainSync { show.value = false }
            if (originalScale == "null") shell("settings delete global animator_duration_scale")
            else shell("settings put global animator_duration_scale $originalScale")
        }
    }

    // Playback depends on state the system delivers asynchronously (window focus, the animator
    // setting and its observer). Each wait polls that state on the main thread frame by frame and
    // fails only when the state never arrives within a safety bound.
    private fun awaitNative(condition: () -> Boolean) = awaitState("Native avatar did not reach its required state", condition)

    private fun awaitWindowFocus() = awaitState("The test window never received focus") { compose.activity.hasWindowFocus() }

    private fun setMotion(scale: Float) {
        shell("settings put global animator_duration_scale ${if (scale == 0f) "0" else "1"}")
        awaitState("The animator setting was not applied") {
            Settings.Global.getFloat(compose.activity.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == scale
        }
    }

    private fun awaitState(message: String, condition: () -> Boolean) {
        val bound = SystemClock.elapsedRealtime() + SAFETY_BOUND_MS
        while (true) {
            var reached = false
            instrumentation.runOnMainSync { reached = condition() }
            if (reached) return
            assertTrue(message, SystemClock.elapsedRealtime() < bound)
            compose.mainClock.advanceTimeByFrame()
            Thread.sleep(10)
        }
    }

    private companion object { const val SAFETY_BOUND_MS = 20_000L }

    private fun findImage(view: View): ImageView? {
        if (view is ImageView && view.drawable is AnimatedImageDrawable) return view
        if (view is ViewGroup) for (i in 0 until view.childCount) findImage(view.getChildAt(i))?.let { return it }
        return null
    }

    private fun shell(command: String): String = ParcelFileDescriptor.AutoCloseInputStream(
        instrumentation.uiAutomation.executeShellCommand(command)).use { it.readBytes().toString(Charsets.UTF_8) }
}
