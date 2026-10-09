package xyz.tironi.zoen

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.json.*
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.agent.*

data class Notice(val text: String, val undo: UndoToken? = null)

class ZoenViewModel(application: Application) : AndroidViewModel(application) {
    val repository = (application as ZoenApplication).repository
    val state = repository.state
    val planner = AgentPlanner()
    val pageSaves = xyz.tironi.zoen.pages.PageSaveCoordinator()
    val browser = AgentBrowser(application, viewModelScope, repository.vault)
    private val mutableWorking = MutableStateFlow<Map<String, String>>(emptyMap())
    val working = mutableWorking.asStateFlow()
    private val workCounts = mutableMapOf<String, Int>()
    private var openItemId: String? = null
    fun viewingItem(id: String?) { openItemId = id }
    private val messages = Channel<Notice>(Channel.BUFFERED)
    val notices = messages.receiveAsFlow()

    init { viewModelScope.launch { repository.errorEvents.collect { messages.send(Notice(it)) } } }

    fun boot(demo: Boolean) = viewModelScope.launch { repository.boot(demo) }
    fun reportGrowthAfterOnboarding() {
        val owner = state.value.account?.copy() ?: return
        viewModelScope.launch {
            xyz.tironi.zoen.growth.reportGrowthWhenRegistered(state, owner) {
                repository.network { it.growthSync(owner.relayUrl, false, 0u, 0u) }
            }
        }
    }
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
        val mine = space.members.filter { it.kind == PersonaKind.AGENT && it.isMine }
        val pet = state.value.items.firstOrNull { it.spaceId == spaceId && it.app?.appId == "pet" &&
            !runCatching { Json.parseToJsonElement(it.app!!.viewJson).jsonObject["released"]?.jsonPrimitive?.booleanOrNull == true }.getOrDefault(false) }
        val agent = space.counterpart?.takeIf { it.kind == PersonaKind.AGENT && it.isMine }
            ?: mine.firstOrNull { clean.contains("@${it.handle}", true) || clean.contains(it.name, true) }
            ?: mine.firstOrNull { it.handle == "zoen" && (AppChooser.kind(clean) != null || (pet != null && AppChooser.renameTarget(clean) != null)) }
        if (agent == null) { planReadyHikes(spaceId); return }
        val locale = repository.locale
        val owner = state.value.me?.id
        val context = plannerContext(spaceId)
        workCounts[spaceId] = (workCounts[spaceId] ?: 0) + 1
        mutableWorking.value = mutableWorking.value + (spaceId to agent.name)
        try {
            val renamed = AppChooser.renameTarget(clean)
            if (pet != null && renamed != null) {
                val args = buildJsonObject { put("name", renamed) }.toString()
                val result = repository.change { it.appCallTool(pet.id, "pet_rename", args, false) }
                if (result.message.isNotBlank()) notify(result.message)
            } else if (AppChooser.kind(clean) != null) {
                val app = planner.chooseApp(clean, locale, context)
                if (owner != state.value.me?.id || locale != repository.locale) return
                if (app != null) {
                    val outcome = repository.change { it.agentCreateApp(spaceId, agent.id, app.tool, app.args, app.engineLabel, clean) }
                    if (outcome.item == null && outcome.message.isNotBlank()) notify(outcome.message)
                } else notify(getApplication<Application>().getString(R.string.agent_countdown_needs_date))
            } else if (AgentPlanner.looksLikePlanRequest(clean)) {
                val draft = planner.makePlan(clean, space.members.filter { it.kind == PersonaKind.PERSON }.map { it.name }, locale, context)
                if (owner != state.value.me?.id || locale != repository.locale) return
                val outcome = repository.change { it.agentCreatePlan(spaceId, agent.id, clean, draft.plan, draft.engineLabel, 0) }
                if (outcome.item == null && outcome.message.isNotBlank()) notify(outcome.message)
            } else {
                val response = planner.reply(clean, agent.name, locale, context)
                if (owner != state.value.me?.id || locale != repository.locale) return
                repository.change { it.agentSay(spaceId, agent.id, response, 0) }
            }
            planReadyHikes(spaceId)
        } catch (e: Exception) {
            if (e is CancellationException) throw e
            messages.send(Notice(e.message ?: getApplication<Application>().getString(R.string.something_wrong)))
        } finally {
            val remaining = (workCounts[spaceId] ?: 1) - 1
            if (remaining <= 0) { workCounts.remove(spaceId); mutableWorking.value = mutableWorking.value - spaceId }
            else workCounts[spaceId] = remaining
        }
    }

    suspend fun plannerContext(spaceId: String): PlannerContext = repository.query { core ->
        val space = state.value.spaces.firstOrNull { it.id == spaceId }
        val openCard = openItemId?.let { id -> state.value.items.firstOrNull { it.id == id && it.spaceId == spaceId }?.title }
            ?: state.value.items.firstOrNull { it.spaceId == spaceId && it.plan != null }?.title
        PlannerContext(space?.title.orEmpty(), space?.members.orEmpty().filter { !it.isMe }.map { it.name }, openCard,
            core.timeline(spaceId).takeLast(6).mapNotNull { entry -> (entry.kind as? EntryKind.Message)?.let { "${entry.author.name}: ${it.text}" } })
    }

    fun planHikeIfReady(itemId: String) = launch { planHike(itemId) }

    private suspend fun planReadyHikes(spaceId: String) {
        state.value.items.filter { it.spaceId == spaceId && it.app?.appId == "hike" }.forEach { planHike(it.id) }
    }

    private suspend fun planHike(itemId: String) {
        val item = repository.query { it.item(itemId) }
        val app = item.app?.takeIf { it.appId == "hike" } ?: return
        val view = Json.parseToJsonElement(app.viewJson).jsonObject
        if (view["decided"] == null || view["decided"] == JsonNull || view["itinerary"] is JsonArray) return
        val voters = (view["trails"] as? JsonArray).orEmpty().flatMap { trail -> (trail.jsonObject["votes"] as? JsonArray).orEmpty().map { it.jsonPrimitive.content } }
        val entries = repository.query { it.timeline(item.spaceId) }
        val args = HikePlanner.plan(entries, item.versions.minOfOrNull { it.atMs } ?: 0, voters, state.value.demo) ?: return
        val result = repository.change { it.appCallTool(itemId, "hike_set_itinerary", args, false) }
        if (result.status == AppCallStatus.DENIED) notify(result.message)
    }

    override fun onCleared() { browser.close(); planner.close(); super.onCleared() }
}
