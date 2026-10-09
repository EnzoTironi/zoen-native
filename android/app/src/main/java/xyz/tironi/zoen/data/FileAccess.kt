package xyz.tironi.zoen.data

import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.provider.OpenableColumns
import androidx.core.content.FileProvider
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import xyz.tironi.zoen.R
import xyz.tironi.zoen.core.ItemDetail

object FileAccess {
    const val MAX_BYTES = 20 * 1024 * 1024

    suspend fun import(context: Context, repository: ZoenRepository, space: String, uri: Uri): ItemDetail = withContext(Dispatchers.IO) {
        val resolver = context.contentResolver
        var name = "attachment"
        resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) name = cursor.getString(0) ?: name
        }
        name = File(name).name.take(160)
        val mime = resolver.getType(uri) ?: "application/octet-stream"
        val bytes = resolver.openInputStream(uri)?.use { input ->
            val output = ByteArrayOutputStream()
            val buffer = ByteArray(8192)
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                require(output.size() + count <= MAX_BYTES) { context.getString(R.string.file_too_large) }
                output.write(buffer, 0, count)
            }
            output.toByteArray()
        } ?: error(context.getString(R.string.something_wrong))
        if (name.endsWith(".md", true) || mime == "text/markdown") repository.change { it.pageImportMarkdown(space, name, bytes.toString(Charsets.UTF_8)) }
        else {
            var thumbnail: ByteArray? = null
            if (mime.startsWith("image/")) {
                val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
                var sample = 1
                while (bounds.outWidth / sample > 512 || bounds.outHeight / sample > 512) sample *= 2
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })?.let { bitmap ->
                    thumbnail = ByteArrayOutputStream().use { out -> bitmap.compress(Bitmap.CompressFormat.PNG, 100, out); out.toByteArray() }
                    bitmap.recycle()
                }
            }
            repository.change { it.fileAdd(space, "", name, mime, bytes, thumbnail) }
        }
    }

    fun cameraUri(context: Context): Uri {
        val file = File(File(context.cacheDir, "exports").apply { mkdirs() }, "photo-${UUID.randomUUID()}.jpg")
        return FileProvider.getUriForFile(context, "${context.packageName}.files", file)
    }

    fun shareText(context: Context, text: String) {
        context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply { type = "text/plain"; putExtra(Intent.EXTRA_TEXT, text) }, context.getString(R.string.share)))
    }

    suspend fun shareFile(context: Context, name: String, mime: String, bytes: ByteArray) {
        val uri = withContext(Dispatchers.IO) {
            val directory = File(File(context.cacheDir, "exports"), UUID.randomUUID().toString()).apply { mkdirs() }
            val file = File(directory, File(name).name.ifBlank { "export" })
            file.writeBytes(bytes)
            FileProvider.getUriForFile(context, "${context.packageName}.files", file)
        }
        context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply {
            type = mime
            putExtra(Intent.EXTRA_STREAM, uri)
            clipData = ClipData.newUri(context.contentResolver, name, uri)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }, context.getString(R.string.share)))
    }
}
