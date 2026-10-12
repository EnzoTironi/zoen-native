package xyz.tironi.zoen.media

import android.content.Context
import java.io.File
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import xyz.tironi.zoen.core.TimelineEntry
import xyz.tironi.zoen.data.ZoenRepository

object VoiceTransport {
    /** Sending a file Item lets the existing core seal, queue and fetch the AAC bytes. */
    suspend fun send(repository: ZoenRepository, clip: VoiceClip, transcript: String, space: String, reply: String? = null, thread: Boolean = false): TimelineEntry {
        val bytes = withContext(Dispatchers.IO) { clip.file.readBytes() }
        val pending = "voicePending:${clip.id}:$space"
        val attachmentId = repository.preferences.getString(pending, null) ?: repository.change { core ->
            core.fileAdd(space, "VoiceNotes/${clip.id}.m4a", "${clip.id}.m4a", "audio/mp4", bytes, null).id
        }.also { repository.preferences.edit().putString(pending, it).commit() }
        val marker = VoiceNoteRef(attachmentId, clip.ms, clip.levels, transcript).marker
        val entry = repository.change { core ->
            if (reply == null) core.sendMessage(space, marker) else core.sendReply(space, marker, reply, thread)
        }
        repository.preferences.edit().remove(pending).apply()
        withContext(Dispatchers.IO) { clip.delete() }
        return entry
    }
    suspend fun localFile(context: Context, repository: ZoenRepository, reference: VoiceNoteRef): File? = withContext(Dispatchers.IO) {
        val bytes = repository.query { core ->
            runCatching { core.fileBytes(reference.id, 1u) }.getOrNull()
                ?: runCatching { core.media(reference.id) }.getOrNull()
                ?: core.items().firstOrNull { it.file?.path == "VoiceNotes/${reference.id}.m4a" || it.file?.name == "${reference.id}.m4a" }?.let { core.fileBytes(it.id, 1u) }
        }
        if (bytes == null) return@withContext null
        val file = File(VoiceRecorder.directory(context), "play-${UUID.randomUUID()}.m4a")
        file.writeBytes(bytes)
        file
    }
}
