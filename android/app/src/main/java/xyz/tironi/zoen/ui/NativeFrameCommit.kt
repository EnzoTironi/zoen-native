package xyz.tironi.zoen.ui

import android.os.Build
import android.view.View
import android.view.ViewTreeObserver

internal class NativeFrameCommit(
    private val view: View,
    private val needsCommit: () -> Boolean,
    private val complete: () -> Unit,
) : ViewTreeObserver.OnPreDrawListener {
    private var observer: ViewTreeObserver? = null
    private var callback: Runnable? = null

    fun attach() {
        observer = view.viewTreeObserver.also { it.addOnPreDrawListener(this) }
    }

    fun detach() {
        cancel()
        observer?.takeIf { it.isAlive }?.removeOnPreDrawListener(this)
        observer = null
    }

    fun cancel() {
        val pending = callback ?: return
        callback = null
        view.removeCallbacks(pending)
        if (Build.VERSION.SDK_INT >= 29) {
            observer?.takeIf { it.isAlive }?.unregisterFrameCommitCallback(pending)
        }
    }

    override fun onPreDraw(): Boolean {
        if (needsCommit()) {
            cancel()
            val pending = object : Runnable {
                override fun run() {
                    if (callback !== this) return
                    callback = null
                    complete()
                }
            }
            callback = pending
            if (Build.VERSION.SDK_INT >= 29 && view.isHardwareAccelerated) {
                view.viewTreeObserver.registerFrameCommitCallback(pending)
            } else view.post(pending)
        }
        return true
    }
}
