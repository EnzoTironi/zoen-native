package xyz.tironi.zoen.media

import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.media.MediaMetadataRetriever
import android.net.Uri
import android.util.AtomicFile
import androidx.core.content.FileProvider
import java.io.ByteArrayOutputStream
import java.io.File
import java.security.MessageDigest
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import xyz.tironi.zoen.R

object MediaFiles {
    suspend fun local(context: Context, item: String, version: UInt, name: String, bytes: ByteArray): File = withContext(Dispatchers.IO) {
        val directory = File(context.cacheDir, "media-cache/${sha(item.toByteArray())}/v$version").apply { mkdirs() }
        val output = File(directory, File(name).name.ifBlank { "file" })
        if (!output.exists() || sha(output.readBytes()) != sha(bytes)) {
            val atomic = AtomicFile(output)
            val stream = atomic.startWrite()
            try { stream.write(bytes); atomic.finishWrite(stream) } catch (error: Exception) { atomic.failWrite(stream); throw error }
        }
        output
    }
    fun videoUri(context: Context): Uri {
        val file = File(File(context.cacheDir, "exports").apply { mkdirs() }, "video-${UUID.randomUUID()}.mp4")
        return FileProvider.getUriForFile(context, "${context.packageName}.files", file)
    }
    suspend fun open(context: Context, name: String, mime: String, bytes: ByteArray) {
        val uri = withContext(Dispatchers.IO) {
            val directory = File(context.cacheDir, "exports/${UUID.randomUUID()}").apply { mkdirs() }
            val file = File(directory, File(name).name.ifBlank { "file" }); file.writeBytes(bytes)
            FileProvider.getUriForFile(context, "${context.packageName}.files", file)
        }
        context.startActivity(Intent.createChooser(Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, mime)
            clipData = ClipData.newUri(context.contentResolver, name, uri)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }, context.getString(R.string.media_open_with)))
    }
    suspend fun thumbnail(file: File, mime: String): ByteArray? = withContext(Dispatchers.IO) {
        runCatching {
            val bitmap = when {
                mime.startsWith("image/") -> DocumentMarkup.decodeImage(file.readBytes(), 480)
                mime == "application/pdf" -> NativePdf(file).use { it.render(0, 480) }
                mime.startsWith("video/") -> { val retriever = MediaMetadataRetriever(); try { retriever.setDataSource(file.absolutePath); retriever.getFrameAtTime(0) } finally { retriever.release() } }
                else -> null
            } ?: return@runCatching null
            try {
                val scale = minOf(1f, 480f / maxOf(bitmap.width, bitmap.height))
                val scaled = Bitmap.createScaledBitmap(bitmap, maxOf(1, (bitmap.width * scale).toInt()), maxOf(1, (bitmap.height * scale).toInt()), true)
                try { ByteArrayOutputStream().use { out -> scaled.compress(Bitmap.CompressFormat.PNG, 100, out); out.toByteArray() } }
                finally { if (scaled !== bitmap) scaled.recycle() }
            } finally { bitmap.recycle() }
        }.getOrNull()
    }
    private fun sha(bytes: ByteArray) = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
}
