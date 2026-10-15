package xyz.tironi.zoen

import android.app.Activity
import android.graphics.Bitmap
import android.os.Build
import android.os.Handler
import android.os.SystemClock
import android.view.FrameMetrics
import android.view.PixelCopy
import android.view.Window
import androidx.test.platform.app.InstrumentationRegistry
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

internal object CommittedWindowCapture {
    fun save(activity: Activity, directory: String, name: String) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val deadline = SystemClock.uptimeMillis() + 10_000
        val finished = CountDownLatch(1)
        val started = AtomicBoolean()
        val cancelled = AtomicBoolean()
        val watchdogCopy = AtomicBoolean()
        val status = AtomicInteger(-1)
        val attempts = mutableListOf<Int>()
        val failure = AtomicReference<Throwable?>()
        var bitmap: Bitmap? = null
        var commit: Runnable? = null
        var legacy: Window.OnFrameMetricsAvailableListener? = null
        val decor = activity.window.decorView
        try {
            instrumentation.runOnMainSync {
                check(decor.isHardwareAccelerated && decor.width > 0 && decor.height > 0)
                val pixels = Bitmap.createBitmap(decor.width, decor.height, Bitmap.Config.ARGB_8888)
                bitmap = pixels
                commit = Runnable {
                    if (cancelled.get() || !started.compareAndSet(false, true)) return@Runnable
                    try {
                        PixelCopy.request(activity.window, pixels, { result ->
                            status.set(result)
                            attempts.add(result)
                            if (cancelled.get()) {
                                pixels.recycle()
                                finished.countDown()
                            } else if (result in listOf(PixelCopy.ERROR_TIMEOUT, PixelCopy.ERROR_SOURCE_NO_DATA)
                                && attempts.size < 3 && SystemClock.uptimeMillis() < deadline) {
                                started.set(false)
                                if (Build.VERSION.SDK_INT >= 29) decor.viewTreeObserver.registerFrameCommitCallback(checkNotNull(commit))
                                decor.postInvalidateOnAnimation()
                            } else finished.countDown()
                        }, Handler(activity.mainLooper))
                    } catch (error: Exception) { failure.set(error); finished.countDown() }
                }
                if (Build.VERSION.SDK_INT >= 29) decor.viewTreeObserver.registerFrameCommitCallback(checkNotNull(commit))
                else {
                    val requestedAt = System.nanoTime()
                    legacy = Window.OnFrameMetricsAvailableListener { _, metrics, _ ->
                        // A frame already in flight when we asked does not count; ask for the next
                        // one instead of waiting for an unrelated redraw that may never come.
                        if (metrics.getMetric(FrameMetrics.INTENDED_VSYNC_TIMESTAMP) >= requestedAt) checkNotNull(commit).run()
                        else if (!started.get() && !cancelled.get()) decor.postInvalidateOnAnimation()
                    }
                    activity.window.addOnFrameMetricsAvailableListener(checkNotNull(legacy), Handler(activity.mainLooper))
                    // Frame metrics are only reported for frames the renderer actually draws; an idle
                    // window on an old emulator can skip the invalidation and never report one. The
                    // window still has a committed buffer, so copy it after a bounded wait.
                    val watchdog = Handler(activity.mainLooper)
                    fun nudge(left: Int) {
                        watchdog.postDelayed({
                            if (started.get() || cancelled.get()) return@postDelayed
                            if (left > 0) { decor.postInvalidateOnAnimation(); nudge(left - 1) } else { watchdogCopy.set(true); checkNotNull(commit).run() }
                        }, 750)
                    }
                    nudge(2)
                }
                decor.postInvalidateOnAnimation()
            }
            check(finished.await((deadline - SystemClock.uptimeMillis()).coerceAtLeast(0), TimeUnit.MILLISECONDS)) {
                "The foreground app frame did not commit and copy within the evidence deadline"
            }
            failure.get()?.let { throw it }
            Evidence.outputFile(directory, "$name-frame-copy.txt").writeText("PixelCopy statuses: $attempts; source: ${if (watchdogCopy.get()) "watchdog copy of last committed buffer (no new frame reported)" else "frame committed after request"}; deadline: 10000 ms\n")
            check(status.get() == PixelCopy.SUCCESS) { "Window PixelCopy failed: $attempts" }
            Evidence.outputFile(directory, "$name.png").outputStream().use {
                check(checkNotNull(bitmap).compress(Bitmap.CompressFormat.PNG, 100, it))
            }
        } finally {
            cancelled.set(true)
            instrumentation.runOnMainSync {
                if (Build.VERSION.SDK_INT >= 29 && decor.viewTreeObserver.isAlive) commit?.let(decor.viewTreeObserver::unregisterFrameCommitCallback)
                legacy?.let(activity.window::removeOnFrameMetricsAvailableListener)
                if (!started.get() || finished.count == 0L) bitmap?.takeUnless(Bitmap::isRecycled)?.recycle()
            }
        }
    }
}
