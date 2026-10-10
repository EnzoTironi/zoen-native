package xyz.tironi.zoen.media

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.awaitLongPressOrCancellation
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerInputScope
import androidx.compose.ui.input.pointer.changedToUpIgnoreConsumed
import androidx.compose.ui.input.pointer.positionChangeIgnoreConsumed
import kotlinx.coroutines.CancellationException

internal suspend fun PointerInputScope.detectVoiceHoldGestures(
    cancelDistance: Float,
    lockDistance: Float,
    onStart: () -> Unit,
    onRelease: () -> Unit,
    onLock: () -> Unit,
    onCancel: () -> Unit,
) {
    awaitEachGesture {
        var started = false
        var finished = false
        try {
            val down = awaitFirstDown(requireUnconsumed = false)
            val hold = awaitLongPressOrCancellation(down.id) ?: return@awaitEachGesture
            started = true
            onStart()
            var pointer = hold.id
            var x = 0f
            var y = 0f
            while (true) {
                // Claim the release before the button's native clickable sees it on Main.
                val event = awaitPointerEvent(PointerEventPass.Initial)
                val change = event.changes.firstOrNull { it.id == pointer }
                if (change == null || event.changes.any { it.isConsumed }) {
                    if (!finished) { finished = true; onCancel() }
                    break
                }
                val amount = change.positionChangeIgnoreConsumed()
                event.changes.forEach { it.consume() }
                if (change.changedToUpIgnoreConsumed()) {
                    val next = event.changes.firstOrNull { it.pressed }
                    if (next == null) {
                        if (!finished) { finished = true; onRelease() }
                        break
                    }
                    pointer = next.id
                } else if (!finished) {
                    x += amount.x
                    y += amount.y
                    when {
                        x < -cancelDistance -> { finished = true; onCancel() }
                        y < -lockDistance -> { finished = true; onLock() }
                    }
                }
            }
        } catch (cancelled: CancellationException) {
            if (started && !finished) onCancel()
            throw cancelled
        }
    }
}
