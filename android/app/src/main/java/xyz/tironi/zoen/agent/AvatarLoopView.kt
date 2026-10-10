package xyz.tironi.zoen.agent

import android.content.Context
import android.graphics.Rect
import android.graphics.drawable.AnimatedImageDrawable
import android.graphics.drawable.Drawable
import android.view.View
import android.view.ViewTreeObserver
import androidx.appcompat.widget.AppCompatImageView

internal class AvatarLoopView(context: Context) : AppCompatImageView(context) {
    private var playbackEnabled = false
    private val visibleRect = Rect()
    private val scrollListener = ViewTreeObserver.OnScrollChangedListener { reconcile() }

    fun updatePlayback(image: Drawable, enabled: Boolean) {
        if (drawable !== image) {
            (drawable as? AnimatedImageDrawable)?.stop()
            setImageDrawable(image)
        }
        playbackEnabled = enabled
        reconcile()
    }

    private fun reconcile() {
        val animation = drawable as? AnimatedImageDrawable ?: return
        if (playbackEnabled && isAttachedToWindow && hasWindowFocus() &&
            windowVisibility == VISIBLE && isShown && getGlobalVisibleRect(visibleRect)
        ) animation.start() else animation.stop()
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        viewTreeObserver.addOnScrollChangedListener(scrollListener)
        reconcile()
    }

    override fun onDetachedFromWindow() {
        (drawable as? AnimatedImageDrawable)?.stop()
        viewTreeObserver.removeOnScrollChangedListener(scrollListener)
        super.onDetachedFromWindow()
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        reconcile()
    }

    override fun onWindowFocusChanged(hasWindowFocus: Boolean) {
        super.onWindowFocusChanged(hasWindowFocus)
        reconcile()
    }

    override fun onWindowVisibilityChanged(visibility: Int) {
        super.onWindowVisibilityChanged(visibility)
        reconcile()
    }

    override fun onVisibilityChanged(changedView: View, visibility: Int) {
        super.onVisibilityChanged(changedView, visibility)
        reconcile()
    }
}
