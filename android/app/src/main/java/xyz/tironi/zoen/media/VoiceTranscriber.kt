package xyz.tironi.zoen.media

import android.content.Context
import android.content.Intent
import android.media.AudioFormat
import android.os.Build
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.speech.ModelDownloadListener
import android.speech.RecognitionListener
import android.speech.RecognitionPart
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import androidx.annotation.RequiresApi
import java.io.File
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlin.coroutines.resume

object VoiceTranscriber {
    fun supported(context: Context): Boolean = Build.VERSION.SDK_INT >= 33 && SpeechRecognizer.isOnDeviceRecognitionAvailable(context)

    /** createOnDeviceSpeechRecognizer prevents cloud fallback, even when a model is missing. */
    suspend fun transcribe(context: Context, audio: PcmAudio, locale: String): VoiceTranscript? {
        if (Build.VERSION.SDK_INT < 33 || !supported(context)) return null
        return withPcmSource(context, audio) { source ->
            withContext(Dispatchers.Main) {
                withTimeoutOrNull((audio.duration * 1000).toLong().coerceIn(15_000, 120_000)) {
                    recognize(context, source, audio, locale)
                }
            }
        }
    }

    internal suspend fun withPcmSource(context: Context, audio: PcmAudio, useSource: suspend (File) -> VoiceTranscript?): VoiceTranscript? = withContext(Dispatchers.IO) {
        val source = File(VoiceRecorder.directory(context), "transcribe-${UUID.randomUUID()}.pcm")
        try {
            source.writeBytes(audio.monoAt(16_000).bytes())
            useSource(source)
        } finally { source.delete() }
    }

    @RequiresApi(33)
    private suspend fun recognize(context: Context, source: File, audio: PcmAudio, locale: String): VoiceTranscript? = suspendCancellableCoroutine { continuation ->
        val descriptor = ParcelFileDescriptor.open(source, ParcelFileDescriptor.MODE_READ_ONLY)
        val recognizer = SpeechRecognizer.createOnDeviceSpeechRecognizer(context)
        val segments = mutableListOf<VoiceTranscript>()
        var finished = false
        fun finish(result: VoiceTranscript?) {
            if (finished) return
            finished = true
            recognizer.cancel(); recognizer.destroy(); descriptor.close()
            if (continuation.isActive) continuation.resume(result)
        }
        fun result(bundle: Bundle): VoiceTranscript {
            val text = bundle.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull().orEmpty()
            val words = if (Build.VERSION.SDK_INT >= 34) timedWords(bundle, audio) else emptyList()
            return VoiceTranscript(text, words)
        }
        recognizer.setRecognitionListener(object : RecognitionListener {
            override fun onReadyForSpeech(params: Bundle?) = Unit
            override fun onBeginningOfSpeech() = Unit
            override fun onRmsChanged(rmsdB: Float) = Unit
            override fun onBufferReceived(buffer: ByteArray?) = Unit
            override fun onEndOfSpeech() = Unit
            override fun onError(error: Int) { finish(null) }
            override fun onResults(results: Bundle) { finish(result(results)) }
            override fun onPartialResults(partialResults: Bundle?) = Unit
            override fun onEvent(eventType: Int, params: Bundle?) = Unit
            override fun onSegmentResults(segmentResults: Bundle) { segments += result(segmentResults) }
            override fun onEndOfSegmentedSession() {
                finish(VoiceTranscript(segments.joinToString(" ") { it.text }, segments.flatMap { it.words }))
            }
        })
        continuation.invokeOnCancellation {
            android.os.Handler(android.os.Looper.getMainLooper()).post { finish(null) }
        }
        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            putExtra(RecognizerIntent.EXTRA_LANGUAGE, locale)
            putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE, descriptor)
            putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_CHANNEL_COUNT, 1)
            putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_ENCODING, AudioFormat.ENCODING_PCM_16BIT)
            putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_SAMPLING_RATE, 16_000)
            putExtra(RecognizerIntent.EXTRA_SEGMENTED_SESSION, RecognizerIntent.EXTRA_AUDIO_SOURCE)
            putExtra(RecognizerIntent.EXTRA_ENABLE_FORMATTING, RecognizerIntent.FORMATTING_OPTIMIZE_QUALITY)
            if (Build.VERSION.SDK_INT >= 34) putExtra(RecognizerIntent.EXTRA_REQUEST_WORD_TIMING, true)
        }
        try { recognizer.startListening(intent) } catch (_: Exception) { finish(null) }
    }

    @RequiresApi(34)
    private fun timedWords(bundle: Bundle, audio: PcmAudio): List<TranscriptWord> {
        val parts = bundle.getParcelableArrayList(SpeechRecognizer.RECOGNITION_PARTS, RecognitionPart::class.java).orEmpty()
        val timed = parts.takeIf { it.size < 2 || it.any { part -> part.timestampMillis > 0 } }.orEmpty()
        return timed.mapIndexedNotNull { index, part ->
            val start = (part.timestampMillis / 1000.0).coerceIn(0.0, audio.duration)
            val next = timed.getOrNull(index + 1)?.timestampMillis?.div(1000.0)?.coerceIn(start, audio.duration) ?: audio.duration
            val end = audio.wordEnd(start, next)
            if (end > start) TranscriptWord(part.formattedText ?: part.rawText, start, end - start) else null
        }
    }

    fun downloadModel(context: Context, locale: String) {
        if (Build.VERSION.SDK_INT < 33 || !supported(context)) return
        val recognizer = SpeechRecognizer.createOnDeviceSpeechRecognizer(context)
        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
            putExtra(RecognizerIntent.EXTRA_LANGUAGE, locale)
            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
        }
        val handler = android.os.Handler(android.os.Looper.getMainLooper())
        var closed = false
        val close = Runnable { if (!closed) { closed = true; recognizer.destroy() } }
        // Binding and model requests are asynchronous; keep the recognizer until acknowledgement.
        handler.postDelayed(close, 120_000)
        try {
            if (Build.VERSION.SDK_INT >= 34) downloadWithCallback(context, recognizer, intent) { handler.removeCallbacks(close); close.run() }
            else recognizer.triggerModelDownload(intent)
        } catch (_: Exception) { handler.removeCallbacks(close); close.run() }
    }

    @RequiresApi(34)
    private fun downloadWithCallback(context: Context, recognizer: SpeechRecognizer, intent: Intent, close: () -> Unit) {
        recognizer.triggerModelDownload(intent, context.mainExecutor, object : ModelDownloadListener {
            override fun onProgress(completedPercent: Int) = Unit
            override fun onSuccess() = close()
            override fun onScheduled() = close()
            override fun onError(error: Int) = close()
        })
    }
}
