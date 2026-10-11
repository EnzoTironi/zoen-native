package xyz.tironi.zoen.media

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import androidx.core.content.ContextCompat
import java.io.File
import java.util.UUID
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.sqrt
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Clips stay in private cache until their edited render has been committed to the core. */
data class VoiceClip(val id: String, val file: File, val audio: PcmAudio) {
    val ms get() = (audio.duration * 1000).toLong()
    val levels get() = audio.waveform(40)
    val detail get() = audio.waveform(160)
    fun delete() { file.delete() }
}

enum class RecordingPhase { IDLE, HOLDING, LOCKED, FINISHING }
data class RecordingState(val phase: RecordingPhase = RecordingPhase.IDLE, val seconds: Double = 0.0, val levels: List<Float> = emptyList(), val error: String? = null)

class VoiceRecorder(private val context: Context, private val scope: CoroutineScope) {
    private val mutableState = MutableStateFlow(RecordingState())
    val state = mutableState.asStateFlow()
    private var recorder: AudioRecord? = null
    private var job: Job? = null
    private var source: File? = null
    private val reading = AtomicBoolean(false)
    private val rate = 44_100
    private var frames = 0
    private var id = ""

    fun start(): Boolean {
        if (mutableState.value.phase != RecordingPhase.IDLE) return false
        check(ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED)
        val size = max(AudioRecord.getMinBufferSize(rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT), 8192)
        return try {
            val input = AudioRecord.Builder().setAudioSource(MediaRecorder.AudioSource.VOICE_RECOGNITION)
                .setAudioFormat(AudioFormat.Builder().setSampleRate(rate).setChannelMask(AudioFormat.CHANNEL_IN_MONO).setEncoding(AudioFormat.ENCODING_PCM_16BIT).build())
                .setBufferSizeInBytes(size * 2).build()
            check(input.state == AudioRecord.STATE_INITIALIZED) { "The microphone could not start." }
            id = UUID.randomUUID().toString()
            source = File(directory(context), "$id.pcm")
            recorder = input
            frames = 0
            reading.set(true)
            input.startRecording()
            mutableState.value = RecordingState(RecordingPhase.HOLDING)
            job = scope.launch(Dispatchers.IO) {
                try {
                    checkNotNull(source).outputStream().buffered().use { output ->
                        val buffer = ByteArray(size)
                        while (reading.get()) {
                            val count = input.read(buffer, 0, buffer.size)
                            if (count < 0) { if (reading.get()) error("The microphone stopped unexpectedly."); break }
                            if (count == 0) continue
                            output.write(buffer, 0, count)
                            frames += count / 2
                            var peak = 0
                            for (i in 0 until count - 1 step 2) {
                                val sample = ((buffer[i].toInt() and 255) or (buffer[i + 1].toInt() shl 8)).toShort().toInt()
                                peak = max(peak, abs(sample))
                            }
                            val level = (.06f + sqrt(peak / 32768f) * .94f).coerceAtMost(1f)
                            mutableState.value = mutableState.value.copy(seconds = frames.toDouble() / rate, levels = (mutableState.value.levels + level).takeLast(64))
                            if (frames >= rate * 600) {
                                reading.set(false)
                                mutableState.value = mutableState.value.copy(phase = RecordingPhase.LOCKED)
                            }
                        }
                    }
                } catch (error: Exception) {
                    reading.set(false)
                    mutableState.value = mutableState.value.copy(phase = RecordingPhase.LOCKED, error = error.message)
                }
            }
            true
        } catch (error: Exception) {
            reading.set(false)
            runCatching { recorder?.release() }; recorder = null; source?.delete(); source = null
            mutableState.value = RecordingState(error = error.message)
            false
        }
    }
    fun lock() {
        if (mutableState.value.phase == RecordingPhase.HOLDING) mutableState.value = mutableState.value.copy(phase = RecordingPhase.LOCKED)
    }
    suspend fun finish(): VoiceClip? {
        if (mutableState.value.phase !in setOf(RecordingPhase.HOLDING, RecordingPhase.LOCKED)) return null
        mutableState.value = mutableState.value.copy(phase = RecordingPhase.FINISHING)
        stopInput()
        val input = source; source = null
        return try {
            if (input == null || frames < rate * .6) null
            else withContext(Dispatchers.IO) {
                val bytes = input.readBytes()
                val samples = ShortArray(bytes.size / 2)
                java.nio.ByteBuffer.wrap(bytes).order(java.nio.ByteOrder.LITTLE_ENDIAN).asShortBuffer().get(samples)
                val audio = PcmAudio(samples, rate, 1)
                val file = AudioFiles.encode(audio, File(directory(context), "$id.m4a"))
                VoiceClip(id, file, audio)
            }
        } finally {
            input?.delete()
            mutableState.value = RecordingState()
        }
    }
    suspend fun cancel() {
        stopInput()
        source?.delete(); source = null
        mutableState.value = RecordingState()
    }
    private suspend fun stopInput() {
        reading.set(false)
        runCatching { recorder?.stop() }
        job?.join(); job = null
        recorder?.release(); recorder = null
    }
    fun close() {
        reading.set(false)
        runCatching { recorder?.stop() }; runCatching { recorder?.release() }; recorder = null
        val abandoned = source; source = null
        job?.cancel(); job = null
        abandoned?.delete()
        mutableState.value = RecordingState()
    }
    companion object {
        fun directory(context: Context) = File(context.cacheDir, "voice").apply { mkdirs() }
    }
}
