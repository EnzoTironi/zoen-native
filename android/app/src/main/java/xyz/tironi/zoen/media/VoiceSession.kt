package xyz.tironi.zoen.media

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import xyz.tironi.zoen.data.ZoenRepository

data class VoiceSessionState(val clip: VoiceClip? = null, val editor: VoiceEditor? = null, val editing: Boolean = false, val transcribing: Boolean = false, val busy: Boolean = false, val revision: Int = 0, val error: String? = null)

class VoiceSession(private val context: Context) : ViewModel() {
    val recorder = VoiceRecorder(context, viewModelScope)
    private val mutableState = MutableStateFlow(VoiceSessionState())
    val state = mutableState.asStateFlow()
    private var preview: File? = null
    private var render: VoiceClip? = null
    private var transcription: Job? = null
    private var generation = 0

    fun review() = viewModelScope.launch {
        val currentGeneration = generation
        try {
            val clip = recorder.finish() ?: return@launch
            if (currentGeneration != generation) { clip.delete(); return@launch }
            mutableState.value = VoiceSessionState(clip, VoiceEditor(clip.audio.duration))
        } catch (error: Exception) { fail(error) }
    }
    fun release(repository: ZoenRepository, space: String, reply: String?, thread: Boolean, onSent: () -> Unit) = viewModelScope.launch {
        val currentGeneration = generation
        try {
            val clip = recorder.finish() ?: return@launch
            if (currentGeneration != generation) { clip.delete(); return@launch }
            mutableState.value = VoiceSessionState(clip, VoiceEditor(clip.audio.duration))
            transcribe(repository.locale)
            send(repository, space, reply, thread, onSent)
        } catch (error: Exception) { fail(error) }
    }
    suspend fun transcribe(locale: String) {
        val state = mutableState.value
        val clip = state.clip ?: return
        if (state.editor?.transcript?.text?.isNotEmpty() == true || state.transcribing) return
        mutableState.value = state.copy(transcribing = true)
        try {
            val result = VoiceTranscriber.transcribe(context, clip.audio, locale)
            if (mutableState.value.clip?.id == clip.id) result?.let { mutableState.value.editor?.loadTranscript(it) }
        } finally {
            if (mutableState.value.clip?.id == clip.id) mutableState.value = mutableState.value.copy(transcribing = false, revision = mutableState.value.revision + 1)
        }
    }
    fun requestTranscript(locale: String) {
        transcription?.cancel()
        transcription = viewModelScope.launch { try { transcribe(locale) } catch (error: Exception) { fail(error) } }
    }
    fun edit() { mutableState.value = mutableState.value.copy(editing = true) }
    fun change(action: (VoiceEditor) -> Unit) {
        val editor = mutableState.value.editor ?: return
        if (mutableState.value.busy) return
        VoicePlayback.stop(); preview?.delete(); preview = null; render?.delete(); render = null
        action(editor)
        mutableState.value = mutableState.value.copy(revision = mutableState.value.revision + 1)
    }
    fun preview() = viewModelScope.launch {
        val state = mutableState.value
        val clip = state.clip ?: return@launch
        val editor = state.editor ?: return@launch
        if (state.busy || editor.keptDuration < .3) return@launch
        if (VoicePlayback.state.value.id == "preview:${clip.id}") {
            VoicePlayback.toggle(context, "preview:${clip.id}", preview ?: clip.file, false)
            return@launch
        }
        mutableState.value = state.copy(busy = true)
        try {
            val rendered = preview ?: withContext(Dispatchers.IO) { AudioFiles.encode(clip.audio.edit(editor.keptRanges), File(VoiceRecorder.directory(context), "preview-${UUID.randomUUID()}.m4a")) }.also { preview = it }
            VoicePlayback.toggle(context, "preview:${clip.id}", rendered, false)
        } catch (error: Exception) { fail(error) }
        finally { mutableState.value = mutableState.value.copy(busy = false) }
    }
    fun sendNow(repository: ZoenRepository, space: String, reply: String?, thread: Boolean, onSent: () -> Unit) = viewModelScope.launch {
        try { send(repository, space, reply, thread, onSent) } catch (error: Exception) { fail(error) }
    }
    private suspend fun send(repository: ZoenRepository, space: String, reply: String?, thread: Boolean, onSent: () -> Unit) {
        val current = mutableState.value
        val clip = current.clip ?: return
        val editor = current.editor ?: return
        if (current.busy || editor.keptDuration < .3) return
        transcription?.cancel(); transcription = null
        mutableState.value = current.copy(busy = true, transcribing = false, error = null)
        VoicePlayback.stop()
        try {
            val sentClip = render ?: withContext(Dispatchers.IO) {
                val edited = clip.audio.edit(editor.keptRanges)
                val id = UUID.randomUUID().toString()
                VoiceClip(id, AudioFiles.encode(edited, File(VoiceRecorder.directory(context), "$id.m4a")), edited)
            }.also { render = it }
            VoiceTransport.send(repository, sentClip, editor.keptTranscript, space, reply, thread)
            withContext(Dispatchers.IO) { clip.delete(); preview?.delete() }
            preview = null; render = null
            mutableState.value = VoiceSessionState()
            onSent()
        } finally { mutableState.value = mutableState.value.copy(busy = false) }
    }
    fun cancel() = viewModelScope.launch {
        if (mutableState.value.busy) return@launch
        generation++
        transcription?.cancel(); transcription = null
        recorder.cancel(); VoicePlayback.stop()
        withContext(Dispatchers.IO) { mutableState.value.clip?.delete(); preview?.delete(); render?.delete() }
        preview = null; render = null
        mutableState.value = VoiceSessionState()
    }
    fun clearError() { mutableState.value = mutableState.value.copy(error = null) }
    private fun fail(error: Exception) {
        if (error is CancellationException) throw error
        mutableState.value = mutableState.value.copy(error = error.message ?: "The recording could not be saved.")
    }
    override fun onCleared() {
        recorder.close(); VoicePlayback.stop(); mutableState.value.clip?.delete(); preview?.delete(); render?.delete()
    }
}
