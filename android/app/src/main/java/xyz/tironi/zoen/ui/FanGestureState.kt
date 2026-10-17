package xyz.tironi.zoen.ui

import kotlin.math.*

/** The same centre slop, angular wedges and sticky outer selection as iOS FanMenu. */
internal class FanGestureState(private val count: Int, private val radius: Float) {
    var dragging = false; private set
    var ownsVoice = false; private set
    var hovered: Int? = null; private set
    fun move(x: Float, y: Float) {
        if (ownsVoice) return
        val distance = hypot(x, y)
        if (distance > 14f) dragging = true
        hovered = if (!dragging || distance <= radius * .35f || count == 0) null else {
            var degrees = atan2(-y, x) * 180 / PI.toFloat()
            if (degrees < -90) degrees += 360
            val wedge = 90f / count
            if (distance < radius * 1.6f && (degrees < 90 - wedge || degrees > 180 + wedge)) null
            else ((degrees.coerceIn(90f, 180f) - 90) / wedge).toInt().coerceIn(0, count - 1)
        }
    }
    fun arm(): Boolean = (!dragging).also { if (it) ownsVoice = true }
    fun release(): FanRelease = when {
        ownsVoice -> { ownsVoice = false; FanRelease.Voice }
        dragging -> hovered?.let { FanRelease.Choose(it) } ?: FanRelease.Dismiss
        else -> FanRelease.Tap
    }
}
/** iOS PlusVoiceCapture's translation thresholds in logical points/dp. */
internal class VoiceHoldDragState {
    var settled = false; private set
    fun move(x: Float, y: Float): VoiceHoldDragResult? {
        if (settled) return null
        val result = when {
            x < -110f -> VoiceHoldDragResult.Cancel
            y < -80f -> VoiceHoldDragResult.Lock
            else -> return null
        }
        settled = true
        return result
    }
}
internal enum class VoiceHoldDragResult { Cancel, Lock }

internal sealed interface FanRelease {
    data object Voice : FanRelease
    data object Tap : FanRelease
    data object Dismiss : FanRelease
    data class Choose(val index: Int) : FanRelease
}
