package xyz.tironi.zoen.media

import android.content.Context
import android.media.MediaMetadataRetriever
import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.util.UnstableApi
import androidx.media3.transformer.Composition
import androidx.media3.transformer.Codec
import androidx.media3.transformer.DefaultEncoderFactory
import androidx.media3.transformer.EditedMediaItem
import androidx.media3.transformer.ExportException
import androidx.media3.transformer.ExportResult
import androidx.media3.transformer.Transformer
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

object MediaExport {
    suspend fun duration(file: File): Double = withContext(Dispatchers.IO) {
        val retriever = MediaMetadataRetriever()
        try { retriever.setDataSource(file.absolutePath); (retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull() ?: 0) / 1000.0 }
        finally { retriever.release() }
    }
    suspend fun trimAudio(input: File, range: TimeRange, output: File): File {
        val audio = AudioFiles.decode(input)
        require(range.end <= audio.duration + .2)
        return AudioFiles.encode(audio.edit(listOf(range)), output)
    }
    /** Force both tracks through decoders so excluded frames and pre-roll are absent. */
    @androidx.annotation.OptIn(UnstableApi::class)
    suspend fun trimVideo(context: Context, input: File, range: TimeRange, output: File): File = withContext(Dispatchers.Main) {
        suspendCancellableCoroutine { continuation ->
            output.parentFile?.mkdirs()
            val item = EditedMediaItem.Builder(MediaItem.Builder().setUri(Uri.fromFile(input))
                .setClippingConfiguration(MediaItem.ClippingConfiguration.Builder().setStartPositionMs((range.start * 1000).toLong()).setEndPositionMs((range.end * 1000).toLong()).build()).build())
                .build()
            val encoder = DefaultEncoderFactory.Builder(context.applicationContext).setEnableFallback(true).build()
            val forceEncoding = object : Codec.EncoderFactory by encoder {
                override fun videoNeedsEncoding() = true
                override fun audioNeedsEncoding() = true
            }
            val transformer = Transformer.Builder(context.applicationContext).setVideoMimeType(MimeTypes.VIDEO_H264).setAudioMimeType(MimeTypes.AUDIO_AAC)
                .setEncoderFactory(forceEncoding)
                .addListener(object : Transformer.Listener {
                    override fun onCompleted(composition: Composition, exportResult: ExportResult) { if (continuation.isActive) continuation.resume(output) }
                    override fun onError(composition: Composition, exportResult: ExportResult, exportException: ExportException) {
                        output.delete(); if (continuation.isActive) continuation.resumeWithException(exportException)
                    }
                }).build()
            continuation.invokeOnCancellation { android.os.Handler(android.os.Looper.getMainLooper()).post { transformer.cancel(); output.delete() } }
            try { transformer.start(item, output.absolutePath) }
            catch (error: Exception) { output.delete(); if (continuation.isActive) continuation.resumeWithException(error) }
        }
    }
}
