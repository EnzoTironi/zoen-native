package xyz.tironi.zoen.ui

import android.database.ContentObserver
import android.graphics.ImageDecoder
import android.graphics.drawable.AnimatedImageDrawable
import android.graphics.drawable.Drawable
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.widget.ImageView
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import xyz.tironi.zoen.R

enum class MascotPose { Wave, Phone, Map, Run, Juggle, Walk, Cheer }
enum class MascotMood { Pout, Smirk, Working }

@Composable
fun ZoenMascot(modifier: Modifier = Modifier, animated: Boolean = false, pose: MascotPose = MascotPose.Wave, headOnly: Boolean = false, mood: MascotMood = MascotMood.Pout) {
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val resolver = context.contentResolver
    fun motionDisabled() = Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    var reduced by remember(context) { mutableStateOf(motionDisabled()) }
    DisposableEffect(resolver) {
        val observer = object : ContentObserver(Handler(Looper.getMainLooper())) {
            override fun onChange(selfChange: Boolean) { reduced = motionDisabled() }
        }
        resolver.registerContentObserver(Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE), false, observer)
        onDispose { resolver.unregisterContentObserver(observer) }
    }
    val moving = animated && !reduced
    val resource = if (headOnly) {
        when (mood) {
            MascotMood.Pout -> if (moving) R.drawable.zoen_mascot_head_motion else R.drawable.zoen_mascot_head
            MascotMood.Smirk -> if (moving) R.drawable.zoen_mascot_head_smirk_motion else R.drawable.zoen_mascot_head_smirk
            MascotMood.Working -> if (moving) R.drawable.zoen_mascot_head_working_motion else R.drawable.zoen_mascot_head_working
        }
    } else when (pose) {
        MascotPose.Wave -> if (moving) R.drawable.zoen_mascot_wave_motion else R.drawable.zoen_mascot_wave
        MascotPose.Phone -> if (moving) R.drawable.zoen_mascot_phone_motion else R.drawable.zoen_mascot_phone
        MascotPose.Map -> if (moving) R.drawable.zoen_mascot_map_motion else R.drawable.zoen_mascot_map
        MascotPose.Run -> if (moving) R.drawable.zoen_mascot_run_motion else R.drawable.zoen_mascot_run
        MascotPose.Juggle -> if (moving) R.drawable.zoen_mascot_juggle_motion else R.drawable.zoen_mascot_juggle
        MascotPose.Walk -> if (moving) R.drawable.zoen_mascot_walk_motion else R.drawable.zoen_mascot_walk
        MascotPose.Cheer -> if (moving) R.drawable.zoen_mascot_cheer_motion else R.drawable.zoen_mascot_cheer
    }
    val drawable by produceState<Drawable?>(null, resource, context) {
        value = withContext(Dispatchers.IO) {
            ImageDecoder.decodeDrawable(ImageDecoder.createSource(context.resources, resource)).apply {
                if (this is AnimatedImageDrawable) repeatCount = AnimatedImageDrawable.REPEAT_INFINITE
            }
        }
    }
    DisposableEffect(drawable, lifecycle) {
        val animation = drawable as? AnimatedImageDrawable
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_START -> animation?.start()
                Lifecycle.Event.ON_STOP -> animation?.stop()
                else -> Unit
            }
        }
        lifecycle.addObserver(observer)
        if (lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) animation?.start()
        onDispose { lifecycle.removeObserver(observer); animation?.stop() }
    }
    AndroidView(
        factory = { ImageView(it).apply { scaleType = ImageView.ScaleType.FIT_CENTER; importantForAccessibility = ImageView.IMPORTANT_FOR_ACCESSIBILITY_NO } },
        modifier = modifier.semantics { contentDescription = "Zoen" },
        update = { view -> if (view.drawable !== drawable) view.setImageDrawable(drawable) },
        onRelease = { view -> (view.drawable as? AnimatedImageDrawable)?.stop(); view.setImageDrawable(null) },
    )
}
