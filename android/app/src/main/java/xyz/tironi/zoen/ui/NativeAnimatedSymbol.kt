package xyz.tironi.zoen.ui

import android.graphics.drawable.Animatable2
import android.graphics.drawable.AnimatedVectorDrawable
import android.graphics.drawable.Drawable
import android.widget.ImageView
import androidx.compose.foundation.layout.size
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.currentStateAsState

internal fun nativeSymbolEffect(running: Boolean, tapped: Boolean, arrived: Boolean,
                                replaced: Boolean, appearing: Boolean, active: Boolean): String = when {
    !running -> ""
    arrived -> "wiggle"
    tapped -> "bounce"
    appearing || replaced -> "draw"
    active -> "active"
    else -> ""
}

/** Native drawable motion, driven solely by real input and application state.
 * Appearance draws once; initial/restored requests never masquerade as new arrivals.
 * Android animator scale and the app preference both gate every effect. */
@Composable
internal fun NativeAnimatedSymbol(
    name: String,
    description: String?,
    modifier: Modifier = Modifier,
    press: Int = 0,
    pendingIds: Set<String> = emptySet(),
    active: Boolean = false,
) {
    val signals = LocalNativeSymbolSignals.current
    val allPending = pendingIds + signals.pendingIds
    val liveActive = active || signals.active
    val context = LocalContext.current
    val resources = LocalResources.current
    val view = remember(context) { ImageView(context).apply { importantForAccessibility = ImageView.IMPORTANT_FOR_ACCESSIBILITY_NO } }
    val enabled = rememberMotionEnabled()
    val lifecycle by LocalLifecycleOwner.current.lifecycle.currentStateAsState()
    val running = enabled && lifecycle.isAtLeast(Lifecycle.State.RESUMED)
    val tint = LocalContentColor.current.toArgb()
    var previousPress by remember { mutableIntStateOf(press) }
    var previousPending by remember { mutableStateOf(allPending) }
    var previousSelection by remember { mutableStateOf(signals.selection) }
    var previousName by remember { mutableStateOf(name) }
    var appeared by remember { mutableStateOf(false) }
    LaunchedEffect(name, press, allPending, liveActive, signals.selection, running, resources) {
        val tapped = previousPress != press || previousSelection != signals.selection
        val arrived = hasNewPendingRequest(previousPending, allPending)
        val replaced = previousName != name
        previousPress = press
        previousPending = allPending
        previousSelection = signals.selection
        previousName = name
        val effect = nativeSymbolEffect(running, tapped, arrived, replaced, !appeared, liveActive)
        appeared = true
        (view.drawable as? AnimatedVectorDrawable)?.apply { clearAnimationCallbacks(); stop() }
        val suffix = if (effect.isEmpty()) "" else "_$effect"
        val resource = resources.getIdentifier("native_$name$suffix", "drawable", context.packageName)
        check(resource != 0) { "Missing native symbol: $name$suffix" }
        val drawable = resources.getDrawable(resource, context.theme).mutate()
        drawable.setTint(tint)
        view.setImageDrawable(drawable)
        if (drawable is AnimatedVectorDrawable && running) {
            // Only a live connecting/typing state repeats, never an idle timer.
            if (liveActive) drawable.registerAnimationCallback(object : Animatable2.AnimationCallback() {
                override fun onAnimationEnd(ended: Drawable?) {
                    view.post {
                        if (view.drawable !== drawable || !view.isAttachedToWindow) return@post
                        if (effect == "active") {
                            drawable.start()
                        } else {
                            // Finish tap/arrival feedback once, then show the live state.
                            val id = resources.getIdentifier("native_${name}_active", "drawable", context.packageName)
                            val live = resources.getDrawable(id, context.theme).mutate() as AnimatedVectorDrawable
                            live.setTint(tint)
                            live.registerAnimationCallback(object : Animatable2.AnimationCallback() {
                                override fun onAnimationEnd(ended: Drawable?) {
                                    view.post { if (view.drawable === live && view.isAttachedToWindow) live.start() }
                                }
                            })
                            view.setImageDrawable(live)
                            live.start()
                        }
                    }
                }
            })
            drawable.start()
        }
    }
    DisposableEffect(view) {
        onDispose { (view.drawable as? AnimatedVectorDrawable)?.apply { clearAnimationCallbacks(); stop() } }
    }
    AndroidView(factory = { view }, modifier = modifier.size(26.dp).then(
        if (description == null) Modifier else Modifier.semantics { contentDescription = description }
    ), update = { it.drawable?.setTint(tint) })
}
