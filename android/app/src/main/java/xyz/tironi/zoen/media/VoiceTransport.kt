package xyz.tironi.zoen.media

import android.content.Context
import java.io.File
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import xyz.tironi.zoen.core.TimelineEntry
import xyz.tironi.zoen.data.ZoenRepository

object VoiceTransport {
    /**
     * A voice note is an end-to-end encrypted audio file (the core seals it in chunks and
     * queues the pieces for the relay like any file) tagged with its length, waveform and
     * transcript, so the other person's phone, Mac and web can play it. One core call: the
     * file and, for a reply, the message pointing at it.
     */
    suspend fun send(repository: ZoenRepository, clip: VoiceClip, transcript: String, space: String, reply: String? = null, thread: Boolean = false): TimelineEntry {
        val bytes = withContext(Dispatchers.IO) { clip.file.readBytes() }
        val entry = repository.change { core ->
            core.sendVoiceNote(space, bytes, "audio/mp4", clip.ms.coerceAtLeast(0).toULong(), clip.detail, transcript, reply, thread)
        }
        // Keep my copy under the note's id so it plays at once, before any download.
        withContext(Dispatchers.IO) {
            val id = entry.voice?.itemId
            if (id == null || !clip.file.renameTo(File(clip.file.parentFile, "$id.m4a"))) clip.delete()
        }
        return entry
    }

    /** The audio: my own copy, else the encrypted file from the core (`null` until it arrives). */
    suspend fun localFile(context: Context, repository: ZoenRepository, reference: VoiceNoteRef): File? = withContext(Dispatchers.IO) {
        File(VoiceRecorder.directory(context), "${reference.id}.m4a").takeIf { it.isFile }?.let { return@withContext it }
        val bytes = repository.query { core ->
            runCatching { core.fileBytes(reference.id, null) }.getOrNull()
                ?: runCatching { core.media(reference.id) }.getOrNull()
                ?: core.items().firstOrNull { it.file?.path == "VoiceNotes/${reference.id}.m4a" || it.file?.name == "${reference.id}.m4a" }?.let { core.fileBytes(it.id, 1u) }
        }
        if (bytes == null) return@withContext null
        val file = File(VoiceRecorder.directory(context), "play-${UUID.randomUUID()}.audio")
        file.writeBytes(bytes)
        file
    }
}
