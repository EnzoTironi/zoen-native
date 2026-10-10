package xyz.tironi.zoen.ui

import android.database.ContentObserver
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.view.HapticFeedbackConstants
import android.view.View
import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView

@Composable
internal fun rememberMotionEnabled(): Boolean {
    val resolver = LocalContext.current.contentResolver
    fun enabled() = Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) > 0f
    var value by remember(resolver) { mutableStateOf(enabled()) }
    DisposableEffect(resolver) {
        val observer = object : ContentObserver(Handler(Looper.getMainLooper())) {
            override fun onChange(selfChange: Boolean) { value = enabled() }
        }
        resolver.registerContentObserver(Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE), false, observer)
        onDispose { resolver.unregisterContentObserver(observer) }
    }
    return value
}

internal enum class ZoenFeedback { Tap, Selection, Open, Dismiss, PickUp, Drop, Send, Confirm, Reject, RecordStart, RecordLock }

internal class ZoenHaptics(private val view: View) {
    fun perform(feedback: ZoenFeedback): Boolean {
        val effect = feedbackConstant(feedback, Build.VERSION.SDK_INT)
        val accepted = view.performHapticFeedback(effect)
        if (xyz.tironi.zoen.BuildConfig.DEBUG) android.util.Log.d("ZoenFeedback", "$feedback effect=$effect accepted=$accepted")
        return accepted
    }
}

// Platform effects preserve the user's setting and the device's actuator tuning.
internal fun feedbackConstant(feedback: ZoenFeedback, api: Int): Int = when (feedback) {
    ZoenFeedback.Tap, ZoenFeedback.Send -> HapticFeedbackConstants.CONTEXT_CLICK
    ZoenFeedback.Selection -> if (api >= 34) HapticFeedbackConstants.SEGMENT_TICK else HapticFeedbackConstants.CLOCK_TICK
    ZoenFeedback.Open, ZoenFeedback.PickUp, ZoenFeedback.RecordStart, ZoenFeedback.RecordLock -> HapticFeedbackConstants.LONG_PRESS
    ZoenFeedback.Dismiss, ZoenFeedback.Drop -> HapticFeedbackConstants.VIRTUAL_KEY_RELEASE
    ZoenFeedback.Confirm -> if (api >= 30) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.CONTEXT_CLICK
    ZoenFeedback.Reject -> if (api >= 30) HapticFeedbackConstants.REJECT else HapticFeedbackConstants.LONG_PRESS
}

@Composable
internal fun rememberZoenHaptics(): ZoenHaptics {
    val view = LocalView.current
    return remember(view) { ZoenHaptics(view) }
}
