package xyz.tironi.zoen

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.LocalPlanner

data class Notice(val text: String, val undo: UndoToken? = null)

class ZoenViewModel(application: Application) : AndroidViewModel(application) {
    val repository = (application as ZoenApplication).repository
    val state = repository.state
    private val messages = Channel<Notice>(Channel.BUFFERED)
    val notices = messages.receiveAsFlow()

    init { viewModelScope.launch { repository.errorEvents.collect { messages.send(Notice(it)) } } }

    fun boot(demo: Boolean) = viewModelScope.launch { repository.boot(demo) }
    fun launch(block: suspend () -> Unit) = viewModelScope.launch {
        try { block() }
        catch (e: Exception) {
            if (e is CancellationException) throw e
            messages.send(Notice(e.message ?: getApplication<Application>().getString(R.string.something_wrong)))
        }
    }
    fun notify(text: String) { messages.trySend(Notice(text)) }
    fun edit(block: (RodaEngine) -> EditOutcome) = launch {
        val result = repository.change(block)
        messages.send(Notice(result.reaction ?: getApplication<Application>().getString(R.string.saved), result.undo))
    }
    fun undo(token: UndoToken) = launch { repository.change { it.undo(token) } }

    suspend fun send(spaceId: String, text: String, reply: String? = null, thread: Boolean = false) {
        val clean = text.trim()
        if (clean.isEmpty()) return
        repository.change { core ->
            if (reply == null) core.sendMessage(spaceId, clean)
            else core.sendReply(spaceId, clean, reply, thread)
        }
        if (reply != null) return
        val space = state.value.spaces.firstOrNull { it.id == spaceId } ?: return
        val agent = space.members.firstOrNull { it.handle == "zoen" } ?: return
        if (space.counterpart?.handle != "zoen" && !clean.contains("@zoen", true)) return
        val label = getApplication<Application>().getString(R.string.local_planner)
        try {
            val outcome = repository.change { core ->
                val app = LocalPlanner.app(clean, repository.locale)
                if (app != null) core.agentCreateApp(spaceId, agent.id, app.first, app.second, label, clean)
                else core.agentCreatePlan(spaceId, agent.id, clean, LocalPlanner.plan(clean, repository.locale), label, 0)
            }
            if (outcome.item == null && outcome.message.isNotBlank()) notify(outcome.message)
        } catch (e: Exception) {
            if (e is CancellationException) throw e
            messages.send(Notice(e.message ?: getApplication<Application>().getString(R.string.something_wrong)))
        }
    }
}
