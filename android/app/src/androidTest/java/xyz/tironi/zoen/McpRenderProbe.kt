package xyz.tironi.zoen

import android.app.KeyguardManager
import android.graphics.Bitmap
import android.graphics.Rect
import android.os.Handler
import android.os.HandlerThread
import android.os.PowerManager
import android.util.Log
import android.view.FrameMetrics
import android.view.View
import android.view.ViewTreeObserver
import android.view.Window
import android.webkit.WebView
import androidx.activity.ComponentActivity
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import org.json.JSONObject

/** Records native traversal and GPU submission separately from the page's DOM. */
internal class McpRenderProbe(private val activity: ComponentActivity, private val web: WebView) : AutoCloseable {
    private val thread = HandlerThread("McpRenderProbe").apply { start() }
    private val draws = AtomicInteger()
    private val frames = AtomicInteger()
    private val state = AtomicReference("{}")
    private val frame = AtomicReference("{}")
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
        .put("frames", frames.get()).put("lastFrame", JSONObject(frame.get())).toString()

    /** Uses the last observed UI state so diagnostics do not depend on an unblocked UI loop. */
    fun failureEvidence(name: String, error: Throwable) {
        val directory = File(activity.getExternalFilesDir(null), "evidence").apply { mkdirs() }
        File(directory, "$name.json").writeText(JSONObject(report()).put("error", error.toString()).toString(2))
        Log.e(TAG, "Failure evidence $name: ${report()}")
        val screenshot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        if (screenshot != null) try {
            File(directory, "$name.png").outputStream().use { screenshot.compress(Bitmap.CompressFormat.PNG, 100, it) }
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
