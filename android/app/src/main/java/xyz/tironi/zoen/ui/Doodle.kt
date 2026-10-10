package xyz.tironi.zoen.ui

import android.content.Context
import android.graphics.Canvas
import android.graphics.Rect
import android.graphics.Paint
import android.os.SystemClock
import android.view.View
import android.view.ViewTreeObserver
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import xyz.tironi.zoen.ui.ink.DoodleArt
import xyz.tironi.zoen.ui.ink.InkCanvasRenderer
import kotlin.math.sin
import kotlin.math.abs
import kotlin.math.floor

@Composable
internal fun ZoenDoodle(art: DoodleArt, modifier: Modifier = Modifier, live: Boolean = true) {
    SnapshotArtView(art, modifier, live)
}

@Composable
internal fun ZoenPetSprite(modifier: Modifier, asleep: Boolean, faded: Boolean, live: Boolean) {
    SnapshotArtView(null, modifier.semantics { contentDescription = "Donkey" }, live, asleep, faded)
}

@Composable
private fun SnapshotArtView(art: DoodleArt?, modifier: Modifier, live: Boolean, asleep: Boolean = false, faded: Boolean = false) {
    val motion = rememberMotionEnabled()
    val owner = LocalLifecycleOwner.current
    var active by remember(owner) { mutableStateOf(owner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) }
    DisposableEffect(owner) {
        val observer = LifecycleEventObserver { _, _ -> active = owner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) }
        owner.lifecycle.addObserver(observer)
        onDispose { owner.lifecycle.removeObserver(observer) }
    }
    AndroidView(factory = { NativeArtCanvasView(it) }, modifier = modifier,
        update = { it.configure(art, live, motion, active, asleep, faded) }, onRelease = { it.pause() })
}

internal class NativeArtCanvasView(context: Context) : View(context) {
    private val renderer = InkCanvasRenderer()
    private val paint = Paint()
    private val visibleRect = Rect()
    private var art: DoodleArt? = DoodleArt.Notepad
    private var asleep = false
    private var faded = false
    private var live = false
    private var motion = false
    private var active = false
    private var elapsed = 0L
    private var lastTick = 0L
    private var scheduled = false
    internal val animationTimeSeconds get() = elapsed / 1000.0
    internal val hasScheduledFrame get() = scheduled
    private val scrollListener = ViewTreeObserver.OnScrollChangedListener { updateClock() }
    private val tick = Runnable {
        scheduled = false
        if (canAnimate()) {
            val now = SystemClock.uptimeMillis()
            if (lastTick != 0L) elapsed += now - lastTick
            lastTick = now
            invalidate()
            updateClock()
        }
    }

    init { importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO }
    fun configure(art: DoodleArt?, live: Boolean, motion: Boolean, active: Boolean, asleep: Boolean, faded: Boolean) {
        if (this.art != art) { elapsed = 0; lastTick = 0 }
        this.art = art; this.live = live; this.motion = motion; this.active = active
        this.asleep = asleep; this.faded = faded
        invalidate()
        updateClock()
    }
    private fun canAnimate() = live && motion && active && isAttachedToWindow && hasWindowFocus() && windowVisibility == VISIBLE && isShown && getGlobalVisibleRect(visibleRect)
    private fun updateClock() {
        if (!canAnimate()) { pause(); return }
        if (!scheduled) {
            if (lastTick == 0L) lastTick = SystemClock.uptimeMillis()
            scheduled = true
            postDelayed(tick, if (art == null) 50L else 100L)
        }
    }
    fun pause() { removeCallbacks(tick); scheduled = false; lastTick = 0 }
    override fun onAttachedToWindow() { super.onAttachedToWindow(); viewTreeObserver.addOnScrollChangedListener(scrollListener); updateClock() }
    override fun onDetachedFromWindow() { pause(); viewTreeObserver.removeOnScrollChangedListener(scrollListener); super.onDetachedFromWindow() }
    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) { super.onSizeChanged(w, h, oldw, oldh); updateClock() }
    override fun onWindowFocusChanged(hasWindowFocus: Boolean) { super.onWindowFocusChanged(hasWindowFocus); updateClock() }
    override fun onWindowVisibilityChanged(visibility: Int) { super.onWindowVisibilityChanged(visibility); updateClock() }
    override fun onVisibilityChanged(changedView: View, visibility: Int) { super.onVisibilityChanged(changedView, visibility); updateClock() }
    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val t = if (!live) 4.0 else if (motion) elapsed / 1000.0 else 0.0
        val frame = if (motion) (t * 10).toInt() % 4 else 0
        val progress = if (!live || !motion) 2.0 else t / 1.1
        val saved = canvas.save()
        val doodle = art
        if (doodle != null) {
            if (motion) { val scale = (1 + .014 * sin(t * 2.1)).toFloat(); canvas.scale(scale, scale, width / 2f, height / 2f) }
            renderer.render(doodle.strokes(t), canvas, width, height, doodle.seed, frame, progress, 1.0)
        } else {
            val scale = (floor(minOf(width / 30.0, height / 22.0) * 2) / 2).coerceAtLeast(1.0).toFloat()
            val left = (width - 26 * scale) / 2 - scale * 2
            var top = (height - 22 * scale) / 2
            val animated = live && motion
            if (animated) top += (if (asleep) sin(t * 1.4) * scale * .4 else -abs(sin(t * 3.2)) * scale * 1.5).toFloat()
            drawPixelDonkey(asleep, animated && t % 3.9 > 3.75) { x, y, w, h, color ->
                paint.color = color.toInt(); paint.alpha = if (faded) 89 else 255
                canvas.drawRect(left + x * scale, top + y * scale, left + (x + w) * scale + .3f, top + (y + h) * scale + .3f, paint)
            }
        }
        canvas.restoreToCount(saved)
        updateClock()
    }
}
