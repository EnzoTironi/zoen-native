package xyz.tironi.zoen

import android.graphics.drawable.AnimatedImageDrawable
import android.os.ParcelFileDescriptor
import android.os.SystemClock
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
            shell("settings put global animator_duration_scale 1")
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
            shell("settings put global animator_duration_scale 0")
            awaitNative { !animation.isRunning }
            awaitNative { findImage(compose.activity.window.decorView) == null }
            CommittedWindowCapture.save(compose.activity, "experience", "native-avatar-live-reduced-motion")
            shell("settings put global animator_duration_scale 1")
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

    private fun awaitNative(condition: () -> Boolean) {
        val deadline = SystemClock.elapsedRealtime() + 3_000
        while (true) {
            var reached = false
            instrumentation.runOnMainSync { reached = condition() }
            if (reached) return
            assertTrue("Native avatar did not reach its required state within 3000 ms", SystemClock.elapsedRealtime() < deadline)
            compose.mainClock.advanceTimeByFrame()
            Thread.sleep(10)
        }
    }

    private fun findImage(view: View): ImageView? {
        if (view is ImageView && view.drawable is AnimatedImageDrawable) return view
        if (view is ViewGroup) for (i in 0 until view.childCount) findImage(view.getChildAt(i))?.let { return it }
        return null
    }

    private fun shell(command: String): String = ParcelFileDescriptor.AutoCloseInputStream(
        instrumentation.uiAutomation.executeShellCommand(command)).use { it.readBytes().toString(Charsets.UTF_8) }
}
