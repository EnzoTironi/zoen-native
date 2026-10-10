package xyz.tironi.zoen.ui

import android.content.Context
import android.graphics.Canvas
import android.graphics.Rect
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
import xyz.tironi.zoen.ui.ink.InkCanvasRenderer
import xyz.tironi.zoen.ui.ink.MascotArt
import kotlin.math.*

enum class MascotPose { Wave, Phone, Map, Run, Juggle, Walk, Zen, Roar, Shield, Coin, Cheer }
enum class MascotMood { Pout, Smirk, Working }

@Composable
fun ZoenMascot(modifier: Modifier = Modifier, animated: Boolean = true, pose: MascotPose = MascotPose.Wave,
               headOnly: Boolean = false, mood: MascotMood = MascotMood.Pout) {
    val motion = rememberMotionEnabled()
    val owner = LocalLifecycleOwner.current
    var active by remember(owner) { mutableStateOf(owner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) }
    DisposableEffect(owner) {
        val observer = LifecycleEventObserver { _, _ -> active = owner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) }
        owner.lifecycle.addObserver(observer)
        onDispose { owner.lifecycle.removeObserver(observer) }
    }
    AndroidView(factory = { MascotCanvasView(it) }, modifier = modifier.semantics { contentDescription = "Zoen" },
        update = { it.configure(pose, headOnly, mood, animated && motion, active) }, onRelease = { it.pause() })
}

internal class MascotCanvasView(context: Context) : View(context) {
    private val renderer = InkCanvasRenderer()
    private val visibleRect = Rect()
    private var pose = MascotPose.Wave
    private var head = false
    private var mood = MascotMood.Pout
    private var moving = false
    private var active = false
    private var elapsed = 0L
    private var lastTick = 0L
    private var entrance = 0L
    internal var renderedFrames = 0L
        private set
    internal val animationTimeSeconds get() = elapsed / 1000.0
    internal val hasScheduledFrame get() = scheduled || framePending
    private var scheduled = false
    private var framePending = false
    private val frameFinished = Runnable { framePending = false; if (canAnimate()) schedule() else pause() }
    private val scrollListener = ViewTreeObserver.OnScrollChangedListener { if (canAnimate()) schedule() else pause() }
    private val tick = Runnable {
        scheduled = false
        if (canAnimate()) {
            framePending = true
            invalidate()
        } else pause()
    }

    init { importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO }

    fun configure(pose: MascotPose, head: Boolean, mood: MascotMood, moving: Boolean, active: Boolean) {
        val changedArt = this.pose != pose || this.head != head || this.mood != mood
        val changedMotion = this.moving != moving
        this.pose = pose; this.head = head; this.mood = mood; this.moving = moving; this.active = active
        if (changedArt) { elapsed = 0; entrance = 0 }
        if (changedArt || changedMotion) invalidate()
        if (canAnimate()) schedule() else pause()
    }

    private fun canAnimate() = moving && active && isAttachedToWindow && hasWindowFocus() && windowVisibility == VISIBLE && isShown && getGlobalVisibleRect(visibleRect)
    private fun schedule() {
        if (!scheduled && !framePending && canAnimate()) {
            if (lastTick == 0L) lastTick = SystemClock.uptimeMillis()
            scheduled = true
            // The rig and boiling lines use the original 12 fps body / 10 fps head cadence.
            postDelayed(tick, if (head) 100L else 83L)
        }
    }
    fun pause() { removeCallbacks(tick); removeCallbacks(frameFinished); scheduled = false; framePending = false; lastTick = 0 }
    override fun onAttachedToWindow() { super.onAttachedToWindow(); viewTreeObserver.addOnScrollChangedListener(scrollListener); schedule() }
    override fun onDetachedFromWindow() { pause(); viewTreeObserver.removeOnScrollChangedListener(scrollListener); super.onDetachedFromWindow() }
    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) { super.onSizeChanged(w, h, oldw, oldh); schedule() }
    override fun onWindowFocusChanged(hasWindowFocus: Boolean) { super.onWindowFocusChanged(hasWindowFocus); if (hasWindowFocus) schedule() else pause() }
    override fun onWindowVisibilityChanged(visibility: Int) {
        super.onWindowVisibilityChanged(visibility)
        if (visibility == VISIBLE) schedule() else pause()
    }
    override fun onVisibilityChanged(changedView: View, visibility: Int) {
        super.onVisibilityChanged(changedView, visibility)
        if (visibility == VISIBLE) schedule() else pause()
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        removeCallbacks(tick); removeCallbacks(frameFinished); scheduled = false
        framePending = canAnimate()
        if (framePending) {
            val now = SystemClock.uptimeMillis()
            if (lastTick != 0L) { val delta = now - lastTick; elapsed += delta; entrance += delta }
            lastTick = now
        } else pause()
        val t = if (moving) animationTimeSeconds else 0.0
        val strokes = if (head) MascotArt.head(t, mood) else MascotArt.strokes(pose, t)
        val progress = if (!moving || head) 2.0 else t / 0.9
        val saved = canvas.save()
        if (moving && !head && entrance < 1000L) {
            val e = entrance / 1000.0
            // Damped spring from 0.7 scale / -7 degrees / 16 dp, anchored at the feet.
            val settle = 1 - exp(-8 * e) * (cos(16 * e) + 0.5 * sin(16 * e))
            val scale = (0.7 + 0.3 * settle).toFloat()
            canvas.translate(0f, (16 * resources.displayMetrics.density * (1 - settle)).toFloat())
            canvas.rotate((-7 * (1 - settle)).toFloat(), width / 2f, height.toFloat())
            canvas.scale(scale, scale, width / 2f, height.toFloat())
        }
        renderer.render(strokes, canvas, width, height, if (head) 78uL else 77uL,
            if (moving) (t * if (head) 10 else 12).toInt() % 4 else 0,
            progress, if (head) 0.5 else 0.6)
        canvas.restoreToCount(saved)
        renderedFrames++
        if (framePending) post(frameFinished)
    }
}
