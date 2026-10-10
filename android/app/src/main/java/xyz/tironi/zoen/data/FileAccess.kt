package xyz.tironi.zoen.data

import android.content.ClipData
import android.content.Context
import android.content.Intent
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
import xyz.tironi.zoen.core.MarkdownFileDto
import xyz.tironi.zoen.media.MediaFiles

object FileAccess {
    const val MAX_BYTES = 20 * 1024 * 1024

    suspend fun import(context: Context, repository: ZoenRepository, space: String, uri: Uri): ItemDetail = importMany(context, repository, space, listOf(uri)).single()

    suspend fun importMany(context: Context, repository: ZoenRepository, space: String, uris: List<Uri>): List<ItemDetail> = withContext(Dispatchers.IO) {
        val pages = mutableListOf<MarkdownFileDto>()
        val items = mutableListOf<ItemDetail>()
        var markdownBytes = 0
        for (uri in uris) {
            val selected = read(context, uri)
            val markdown = if (selected.name.substringAfterLast('.').lowercase() in listOf("md", "markdown") || selected.mime == "text/markdown")
                runCatching { Charsets.UTF_8.newDecoder().decode(java.nio.ByteBuffer.wrap(selected.bytes)).toString() }.getOrNull() else null
            if (markdown != null) {
                markdownBytes += selected.bytes.size
                require(markdownBytes <= MAX_BYTES) { context.getString(R.string.file_too_large) }
                pages.add(MarkdownFileDto(selected.name, markdown))
            } else {
                val directory = File(context.cacheDir, "import-${UUID.randomUUID()}").apply { mkdirs() }
                val local = File(directory, selected.name)
                try {
                    local.writeBytes(selected.bytes)
                    val thumbnail = MediaFiles.thumbnail(local, selected.mime)
                    items.add(repository.change { it.fileAdd(space, selected.name, selected.name, selected.mime, selected.bytes, thumbnail) })
                } finally { directory.deleteRecursively() }
            }
        }
        if (pages.isNotEmpty()) items.addAll(repository.change { it.pagesImportMarkdown(space, pages) })
        items
    }

    private data class SelectedFile(val name: String, val mime: String, val bytes: ByteArray)

    private fun read(context: Context, uri: Uri): SelectedFile {
        val resolver = context.contentResolver
        var name = "attachment"
        resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) name = cursor.getString(0) ?: name
        }
        name = File(name).name.take(160).takeUnless { it == "." || it == ".." || it.isBlank() } ?: "attachment"
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
        return SelectedFile(name, mime, bytes)
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
