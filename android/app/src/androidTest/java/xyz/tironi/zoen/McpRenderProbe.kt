package xyz.tironi.zoen

import android.app.KeyguardManager
import android.graphics.Bitmap
import android.graphics.Rect
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.PowerManager
import android.os.SystemClock
import android.util.Log
import android.view.FrameMetrics
import android.view.PixelCopy
import android.view.View
import android.view.ViewTreeObserver
import android.view.Window
import android.webkit.WebView
import androidx.activity.ComponentActivity
import androidx.test.platform.app.InstrumentationRegistry
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference
import org.json.JSONObject

/** Records native traversal and GPU submission separately from the page's DOM. */
internal class McpRenderProbe(private val activity: ComponentActivity, private val web: WebView) : AutoCloseable {
    private val thread = HandlerThread("McpRenderProbe").apply { start() }
    private val draws = AtomicInteger()
    private val frames = AtomicInteger()
    private val state = AtomicReference("{}")
    private val frame = AtomicReference("{}")
    private val capture = AtomicReference("{}")
    private val drawListener = ViewTreeObserver.OnDrawListener {
        if (draws.incrementAndGet() <= 3) Log.i(TAG, "Native draw: ${report()}")
    }
    private val layoutListener = ViewTreeObserver.OnGlobalLayoutListener { record("layout") }
    private val focusListener = ViewTreeObserver.OnWindowFocusChangeListener { record("focus=$it") }
    private val frameListener = Window.OnFrameMetricsAvailableListener { _, metrics, dropped ->
        val count = frames.incrementAndGet()
        val result = JSONObject().put("number", count).put("dropped", dropped)
        for ((name, id) in listOf("total" to FrameMetrics.TOTAL_DURATION, "draw" to FrameMetrics.DRAW_DURATION,
            "sync" to FrameMetrics.SYNC_DURATION, "issue" to FrameMetrics.COMMAND_ISSUE_DURATION,
            "swap" to FrameMetrics.SWAP_BUFFERS_DURATION)) result.put("${name}Ms", metrics.getMetric(id) / 1_000_000.0)
        frame.set(result.toString())
        if (count <= 3 || metrics.getMetric(FrameMetrics.TOTAL_DURATION) > 100_000_000) Log.i(TAG, "Native frame: $result")
    }

    init {
        web.viewTreeObserver.addOnDrawListener(drawListener)
        web.viewTreeObserver.addOnGlobalLayoutListener(layoutListener)
        web.viewTreeObserver.addOnWindowFocusChangeListener(focusListener)
        activity.window.addOnFrameMetricsAvailableListener(frameListener, Handler(thread.looper))
        record("attached")
    }

    /** Call on the UI thread; RESUMED alone does not establish a foreground surface. */
    fun nativeReady(): Boolean {
        record("readiness")
        val visible = Rect()
        return web.isAttachedToWindow && web.isShown && web.visibility == View.VISIBLE &&
            web.windowVisibility == View.VISIBLE && web.hasWindowFocus() && web.width > 0 && web.height > 0 &&
            web.getGlobalVisibleRect(visible) && visible.width() > 0 && visible.height() > 0 &&
            activity.getSystemService(PowerManager::class.java).isInteractive &&
            !activity.getSystemService(KeyguardManager::class.java).isKeyguardLocked
    }

    fun record(reason: String) {
        val visible = Rect()
        val globallyVisible = web.getGlobalVisibleRect(visible)
        val location = IntArray(2).also(web::getLocationOnScreen)
        val result = JSONObject().put("lifecycle", activity.lifecycle.currentState.name)
            .put("attached", web.isAttachedToWindow).put("shown", web.isShown)
            .put("visibility", web.visibility).put("windowVisibility", web.windowVisibility)
            .put("focus", web.hasWindowFocus()).put("hardware", web.isHardwareAccelerated)
            .put("width", web.width).put("height", web.height).put("screenX", location[0]).put("screenY", location[1])
            .put("visibleRect", visible.toShortString()).put("globallyVisible", globallyVisible)
            .put("density", web.resources.displayMetrics.density).put("textZoom", web.settings.textZoom)
            .put("windowFlags", activity.window.attributes.flags)
            .put("interactive", activity.getSystemService(PowerManager::class.java).isInteractive)
            .put("keyguardLocked", activity.getSystemService(KeyguardManager::class.java).isKeyguardLocked)
            .put("provider", WebView.getCurrentWebViewPackage()?.let { "${it.packageName}/${it.versionName}" })
        val serialized = result.toString()
        val previous = state.getAndSet(serialized)
        if (serialized != previous || reason != "readiness") Log.i(TAG, "Native $reason: $serialized")
    }

    fun report(): String = JSONObject().put("native", JSONObject(state.get())).put("draws", draws.get())
        .put("frames", frames.get()).put("lastFrame", JSONObject(frame.get())).put("windowCapture", JSONObject(capture.get())).toString()

    /** Captures the submitted app surface within the caller's existing visual/draw deadline. */
    fun captureCommittedWindow(name: String, deadline: Long) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val finished = CountDownLatch(1)
        val result = AtomicInteger(-1)
        val copiedAt = AtomicLong(Long.MAX_VALUE)
        val failure = AtomicReference<Throwable?>()
        val cancelled = AtomicBoolean()
        val started = AtomicBoolean()
        var bitmap: Bitmap? = null
        var commit: Runnable? = null
        var legacyFrame: Window.OnFrameMetricsAvailableListener? = null
        try {
            instrumentation.runOnMainSync {
                check(nativeReady() && web.isHardwareAccelerated) { "Committed evidence needs the foreground hardware-rendered window: ${report()}" }
                val decor = activity.window.decorView
                val pixels = Bitmap.createBitmap(decor.width, decor.height, Bitmap.Config.ARGB_8888)
                bitmap = pixels
                val info = JSONObject().put("deadlineUptimeMs", deadline).put("width", pixels.width).put("height", pixels.height)
                    .put("barrier", if (Build.VERSION.SDK_INT >= 29) "frame-commit" else "frame-metrics")
                fun mark(stage: String, timestampKey: String) {
                    info.put("stage", stage).put(timestampKey, SystemClock.uptimeMillis())
                    capture.set(info.toString())
                    Log.i(TAG, "Window capture: $info")
                }
                commit = Runnable {
                    if (cancelled.get() || !started.compareAndSet(false, true)) return@Runnable
                    mark("frame submitted", "submittedUptimeMs")
                    try {
                        PixelCopy.request(activity.window, pixels, { status ->
                            result.set(status)
                            copiedAt.set(SystemClock.uptimeMillis())
                            info.put("pixelCopyResult", status)
                            mark("pixels copied", "copiedUptimeMs")
                            if (cancelled.get()) pixels.recycle()
                            finished.countDown()
                        }, Handler(activity.mainLooper))
                    } catch (error: Exception) {
                        failure.set(error)
                        finished.countDown()
                    }
                }
                mark("frame requested", "requestedUptimeMs")
                if (Build.VERSION.SDK_INT >= 29) web.viewTreeObserver.registerFrameCommitCallback(checkNotNull(commit))
                else {
                    // API28 reports completed hardware frames but has no frame-commit callback.
                    val requestedAt = System.nanoTime()
                    legacyFrame = Window.OnFrameMetricsAvailableListener { _, metrics, _ ->
                        if (metrics.getMetric(FrameMetrics.INTENDED_VSYNC_TIMESTAMP) >= requestedAt) checkNotNull(commit).run()
                    }
                    activity.window.addOnFrameMetricsAvailableListener(checkNotNull(legacyFrame), Handler(activity.mainLooper))
                }
                web.postInvalidateOnAnimation()
            }
            val completed = finished.await((deadline - SystemClock.uptimeMillis()).coerceAtLeast(0), TimeUnit.MILLISECONDS)
            failure.get()?.let { throw it }
            check(completed && copiedAt.get() <= deadline) {
                "The updated app window did not commit and copy within the visual/draw deadline: ${report()}"
            }
            check(result.get() == PixelCopy.SUCCESS) { "Native Window PixelCopy failed with ${result.get()}: ${report()}" }
            Evidence.outputFile("mcp", name).outputStream().use { check(checkNotNull(bitmap).compress(Bitmap.CompressFormat.PNG, 100, it)) }
            Evidence.outputFile("mcp", name.removeSuffix(".png") + "-render.json").writeText(report())
        } finally {
            cancelled.set(true)
            instrumentation.runOnMainSync {
                if (Build.VERSION.SDK_INT >= 29 && web.viewTreeObserver.isAlive) commit?.let(web.viewTreeObserver::unregisterFrameCommitCallback)
                legacyFrame?.let(activity.window::removeOnFrameMetricsAvailableListener)
                // A timed-out PixelCopy still owns its destination until its callback completes.
                if (!started.get() || finished.count == 0L) bitmap?.takeUnless(Bitmap::isRecycled)?.recycle()
            }
        }
    }

    /** Uses the last observed UI state so diagnostics do not depend on an unblocked UI loop. */
    fun failureEvidence(name: String, error: Throwable) {
        Evidence.outputFile("mcp", "$name.json").writeText(JSONObject(report()).put("error", error.toString()).toString(2))
        Log.e(TAG, "Failure evidence $name: ${report()}")
        val screenshot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        if (screenshot != null) try {
            Evidence.outputFile("mcp", "$name.png").outputStream().use { screenshot.compress(Bitmap.CompressFormat.PNG, 100, it) }
        } finally { screenshot.recycle() }
    }

    override fun close() {
        if (web.viewTreeObserver.isAlive) {
            web.viewTreeObserver.removeOnDrawListener(drawListener)
            web.viewTreeObserver.removeOnGlobalLayoutListener(layoutListener)
            web.viewTreeObserver.removeOnWindowFocusChangeListener(focusListener)
        }
        activity.window.removeOnFrameMetricsAvailableListener(frameListener)
        thread.quitSafely()
    }

    private companion object { const val TAG = "McpWebViewTest" }
}
