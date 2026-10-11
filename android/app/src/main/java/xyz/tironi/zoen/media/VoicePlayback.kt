package xyz.tironi.zoen.media

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaPlayer
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class VoicePlaybackState(val id: String? = null, val playing: Boolean = false, val position: Double = 0.0, val duration: Double = 0.0, val rate: Float = 1f, val error: String? = null)

/** There is one voice player so starting another message pauses the current one. */
object VoicePlayback {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val mutableState = MutableStateFlow(VoicePlaybackState())
    val state = mutableState.asStateFlow()
    private var player: MediaPlayer? = null
    private var tick: Job? = null
    private var audioManager: AudioManager? = null
    private var focus: AudioFocusRequest? = null
    private var file: File? = null
    private var prepared = false

    fun toggle(context: Context, id: String, file: File, deleteOnClose: Boolean = true) {
        if (mutableState.value.id == id && prepared) {
            if (mutableState.value.playing) pause() else start()
            return
        }
        stop()
        if (deleteOnClose) this.file = file
        audioManager = context.applicationContext.getSystemService(AudioManager::class.java)
        val attributes = AudioAttributes.Builder().setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).setUsage(AudioAttributes.USAGE_MEDIA).build()
        focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT).setAudioAttributes(attributes)
            .setOnAudioFocusChangeListener { change -> if (change <= AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) pause() }.build()
        mutableState.value = VoicePlaybackState(id = id)
        val media = MediaPlayer()
        player = media
        media.setAudioAttributes(attributes)
        media.setOnPreparedListener {
            if (player !== it) return@setOnPreparedListener
            prepared = true
            mutableState.value = mutableState.value.copy(duration = it.duration / 1000.0)
            start()
        }
        media.setOnCompletionListener {
            mutableState.value = mutableState.value.copy(playing = false, position = 0.0)
            it.seekTo(0); tick?.cancel(); audioManager?.abandonAudioFocusRequest(checkNotNull(focus))
        }
        media.setOnErrorListener { _, _, _ ->
            stop(); mutableState.value = VoicePlaybackState(id = id, error = "This recording could not be played."); true
        }
        try { media.setDataSource(file.absolutePath); media.prepareAsync() }
        catch (error: Exception) { stop(); mutableState.value = VoicePlaybackState(id = id, error = error.message) }
    }
    private fun start() {
        val media = player ?: return
        if (!prepared) return
        if (audioManager?.requestAudioFocus(checkNotNull(focus)) != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) return
        media.start()
        mutableState.value = mutableState.value.copy(playing = true)
        tick?.cancel()
        tick = scope.launch {
            while (mutableState.value.playing) {
                mutableState.value = mutableState.value.copy(position = media.currentPosition / 1000.0)
                delay(40)
            }
        }
    }
    fun seek(seconds: Double) {
        if (!prepared) return
        val target = seconds.coerceIn(0.0, mutableState.value.duration)
        player?.seekTo((target * 1000).toLong(), MediaPlayer.SEEK_CLOSEST)
        mutableState.value = mutableState.value.copy(position = target)
    }
    fun cycleRate() {
        if (!prepared) return
        val rate = when (mutableState.value.rate) { 1f -> 1.5f; 1.5f -> 2f; else -> 1f }
        player?.playbackParams = player!!.playbackParams.setSpeed(rate)
        if (!mutableState.value.playing) player?.pause()
        mutableState.value = mutableState.value.copy(rate = rate)
    }
    fun pause() {
        if (prepared) player?.pause()
        tick?.cancel(); mutableState.value = mutableState.value.copy(playing = false)
        focus?.let { audioManager?.abandonAudioFocusRequest(it) }
    }
    fun stop() {
        tick?.cancel(); player?.release(); player = null; prepared = false
        focus?.let { audioManager?.abandonAudioFocusRequest(it) }; focus = null
        file?.delete(); file = null
        mutableState.value = VoicePlaybackState()
    }
}
