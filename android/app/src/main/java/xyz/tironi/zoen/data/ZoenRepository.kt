package xyz.tironi.zoen.data

import android.content.Context
import java.io.File
import java.util.Locale
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import xyz.tironi.zoen.BuildConfig
import xyz.tironi.zoen.core.*

data class RemoteActivity(val identity: String, val kind: String)

data class AppState(
    val ready: Boolean = false,
    val failure: String? = null,
    val demo: Boolean = false,
    val account: AccountDto? = null,
    val keyMissing: Boolean = false,
    val me: Persona? = null,
    val spaces: List<SpaceSummary> = emptyList(),
    val items: List<ItemDetail> = emptyList(),
    val agents: List<AgentProfile> = emptyList(),
    val requests: List<AgentRequestDto> = emptyList(),
    val timelines: Map<String, List<TimelineEntry>> = emptyMap(),
    val connection: ConnectionDto = ConnectionDto("offline", false, 0uL, null),
    val typing: Map<String, String> = emptyMap(),
    val online: Set<String> = emptySet(),
    val remoteActivity: Map<String, RemoteActivity> = emptyMap(),
    val revision: Long = 0,
) {
    val pending: Int get() = requests.count { it.status == RequestStatus.PENDING && it.agent.isMine }
    val zoenChat: SpaceSummary? get() = spaces.firstOrNull { it.counterpart?.handle == "zoen" }
}

class ZoenRepository(private val context: Context) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val gate = Mutex()
    private val changes = Channel<Unit>(Channel.CONFLATED)
    private val errors = Channel<String>(Channel.BUFFERED)
    val errorEvents = errors.receiveAsFlow()
    private var engine: RodaEngine? = null
    private var syncRunning = false
    private var backgroundOwner: Any? = null
    private var openedLanguage: String? = null
    private val observed = mutableSetOf<String>()
    private val typingUpdates = mutableMapOf<String, Long>()
    private val activityUpdates = mutableMapOf<String, Long>()
    private val mutableState = MutableStateFlow(AppState())
    val state = mutableState.asStateFlow()
    val vault = AndroidSecretVault(context)
    val preferences = context.getSharedPreferences("zoen", Context.MODE_PRIVATE)
    @Volatile var appVisible: Boolean = false
        private set
    private val visibleActivities = java.util.Collections.newSetFromMap(java.util.IdentityHashMap<Any, Boolean>())
    @Volatile var activeSpace: String? = null
    val locale: String get() = context.resources.configuration.locales[0].toLanguageTag()

    fun localKey(kind: String, id: String, owner: String = state.value.me?.id.orEmpty()) = "$kind:$owner:$id"

    fun setAppVisible(visible: Boolean, owner: Any) {
        synchronized(visibleActivities) {
            if (visible) visibleActivities.add(owner) else visibleActivities.remove(owner)
            appVisible = visibleActivities.isNotEmpty()
        }
        scope.launch { gate.withLock { updateSyncLocked(); refreshLocked() } }
    }

    suspend fun setBackgroundConnection(enabled: Boolean, owner: Any) = withContext(Dispatchers.IO) {
        gate.withLock {
            if (enabled) backgroundOwner = owner else if (backgroundOwner === owner) backgroundOwner = null
            updateSyncLocked(); refreshLocked()
        }
    }

    fun releaseBackgroundConnection(owner: Any) { scope.launch { setBackgroundConnection(false, owner) } }

    suspend fun close() {
        scope.coroutineContext[Job]?.cancelAndJoin()
        withContext(Dispatchers.IO) {
            gate.withLock {
                engine?.stopSync()
                engine?.destroy()
                engine = null
                syncRunning = false
                backgroundOwner = null
                changes.close()
                errors.close()
                synchronized(visibleActivities) { visibleActivities.clear(); appVisible = false }
                activeSpace = null
            }
        }
    }

    private fun updateSyncLocked() {
        val core = engine ?: return
        val shouldConnect = (appVisible || backgroundOwner != null) && !state.value.demo && !state.value.keyMissing && core.account() != null
        if (shouldConnect && !syncRunning) { core.startSync(listener); syncRunning = true }
        else if (!shouldConnect && syncRunning) {
            core.stopSync(); syncRunning = false
            mutableState.value = mutableState.value.copy(online = emptySet(), remoteActivity = emptyMap(), typing = emptyMap())
        }
    }

    private val listener = object : CoreListener {
        override fun onChange(spaceIds: List<String>) { changes.trySend(Unit) }
        override fun onProfileChanged(identityId: String) { changes.trySend(Unit) }
        override fun onPresence(identityId: String, online: Boolean) {
            scope.launch { gate.withLock {
                if (online && mutableState.value.spaces.none { it.members.any { person -> person.id == identityId } }) refreshLocked()
                val old = mutableState.value
                if (old.spaces.any { it.members.any { person -> person.id == identityId } }) mutableState.value = old.copy(online = if (online) old.online + identityId else old.online - identityId)
            } }
        }
        override fun onConnection(status: ConnectionDto) {
            scope.launch { gate.withLock {
                if (status.state != "online") mutableState.value = mutableState.value.copy(online = emptySet(), remoteActivity = emptyMap(), typing = emptyMap())
                refreshLocked()
            } }
        }
        override fun onError(message: String) { errors.trySend(message); changes.trySend(Unit) }
        override fun onEphemeral(spaceId: String, fromId: String, kind: String, detail: String) {
            if (kind == "read") { changes.trySend(Unit); return }
            if (kind == "status") {
                val status = if (detail == "in_call") "call" else detail
                scope.launch {
                    var update = 0L
                    gate.withLock {
                        val old = mutableState.value
                        if (fromId == old.me?.id || old.spaces.none { it.id == spaceId && it.members.any { person -> person.id == fromId } }) return@withLock
                        update = (activityUpdates[spaceId] ?: 0L) + 1
                        activityUpdates[spaceId] = update
                        mutableState.value = old.copy(remoteActivity = if (status in setOf("processing", "building", "call")) old.remoteActivity + (spaceId to RemoteActivity(fromId, status)) else old.remoteActivity - spaceId)
                    }
                    delay(when (status) { "processing" -> 60_000L; "building" -> 120_000L; "call" -> 3_600_000L; else -> 0L })
                    gate.withLock { if (activityUpdates[spaceId] == update) mutableState.value = mutableState.value.copy(remoteActivity = mutableState.value.remoteActivity - spaceId) }
                }
                return
            }
            if (kind != "typing" && kind != "stopped") return
            scope.launch {
                var update = 0L
                gate.withLock {
                    if (fromId == mutableState.value.me?.id) return@withLock
                    update = (typingUpdates[spaceId] ?: 0L) + 1
                    typingUpdates[spaceId] = update
                    val typing = mutableState.value.typing.toMutableMap()
                    if (kind == "typing") typing[spaceId] = fromId else typing.remove(spaceId)
                    mutableState.value = mutableState.value.copy(typing = typing)
                }
                if (kind == "typing") {
                    delay(5_000)
                    gate.withLock {
                        if (typingUpdates[spaceId] == update && mutableState.value.typing[spaceId] == fromId) {
                            mutableState.value = mutableState.value.copy(typing = mutableState.value.typing - spaceId)
                        }
                    }
                }
            }
        }
    }

    init {
        scope.launch {
            for (change in changes) {
                try { gate.withLock { refreshLocked() } }
                catch (e: Exception) {
                    if (e is CancellationException) throw e
                    errors.trySend(e.message ?: context.getString(xyz.tironi.zoen.R.string.something_wrong))
                }
            }
        }
    }

    suspend fun boot(requestDemo: Boolean = false) = withContext(Dispatchers.IO) {
        gate.withLock {
            val language = Locale.forLanguageTag(locale).language
            if (engine != null && openedLanguage == language) return@withLock
            if (engine != null) {
                engine?.stopSync()
                syncRunning = false
                engine?.destroy()
                engine = null
                observed.clear()
                mutableState.value = AppState()
                xyz.tironi.zoen.miniapps.MiniAppSnapshots.clear()
                xyz.tironi.zoen.miniapps.MiniAppModelContext.clear()
            }
            try {
                val demo = BuildConfig.DEBUG && (requestDemo || preferences.getBoolean("demo", false))
                val dir = File(context.noBackupFilesDir, "core").apply { mkdirs() }
                val name = if (demo) "demo-${Locale.forLanguageTag(locale).language}.sqlite" else "zoen.sqlite"
                val core = RodaEngine.open(File(dir, name).absolutePath, locale)
                engine = core
                openedLanguage = language
                mutableState.value = mutableState.value.copy(demo = demo)
                if (demo) core.seedDemoIfEmpty()
                else if (core.account() != null) {
                    if (!core.unlock(vault)) mutableState.value = mutableState.value.copy(keyMissing = true)
                }
                updateSyncLocked()
                refreshLocked()
            } catch (e: Exception) {
                if (e is CancellationException) throw e
                engine?.stopSync()
                syncRunning = false
                engine?.destroy()
                engine = null
                mutableState.value = mutableState.value.copy(ready = true, failure = e.message ?: "Couldn't open Zoen.")
            }
        }
    }

    private fun refreshLocked() {
        val core = engine ?: return
        val account = core.account()
        val old = mutableState.value
        val spaces = core.spaces()
        observed.retainAll(spaces.map { it.id }.toSet())
        mutableState.value = old.copy(
            ready = true, failure = null, account = account,
            me = if (old.demo || account != null) core.me() else null,
            spaces = spaces, items = core.items(), agents = core.agents(), requests = core.requests(),
            connection = core.connection(), revision = old.revision + 1,
            timelines = observed.associateWith { core.timeline(it) },
        )
    }

    suspend fun <T> query(block: (RodaEngine) -> T): T = withContext(Dispatchers.IO) {
        gate.withLock { block(checkNotNull(engine) { "Zoen is still opening." }) }
    }

    suspend fun <T> change(block: (RodaEngine) -> T): T = withContext(Dispatchers.IO) {
        gate.withLock {
            val result = block(checkNotNull(engine) { "Zoen is still opening." })
            refreshLocked()
            result
        }
    }

    suspend fun observe(space: String) = withContext(Dispatchers.IO) {
        gate.withLock {
            if (mutableState.value.spaces.any { it.id == space }) {
                observed.add(space)
                engine?.markRead(space)
            }
            refreshLocked()
        }
    }

    suspend fun unobserve(space: String) = withContext(Dispatchers.IO) {
        gate.withLock {
            observed.remove(space)
            engine?.setTyping(space, false)
            refreshLocked()
        }
    }

    suspend fun createAccount(name: String, handle: String, relay: String) {
        change {
            it.createAccount(name.trim(), handle.trim().removePrefix("@"), relay.trim(), vault)
            updateSyncLocked()
        }
    }

    suspend fun useDemo() = withContext(Dispatchers.IO) {
        check(BuildConfig.DEBUG)
        gate.withLock {
            engine?.stopSync()
            syncRunning = false
            engine?.destroy()
            engine = null
            observed.clear()
            preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
            mutableState.value = AppState()
            xyz.tironi.zoen.miniapps.MiniAppSnapshots.clear()
            xyz.tironi.zoen.miniapps.MiniAppModelContext.clear()
        }
        boot(true)
    }

    suspend fun leaveDemo() = withContext(Dispatchers.IO) {
        gate.withLock {
            engine?.stopSync()
            syncRunning = false
            engine?.destroy()
            engine = null
            observed.clear()
            preferences.edit().putBoolean("demo", false).putBoolean("onboarded", false).commit()
            mutableState.value = AppState()
            xyz.tironi.zoen.miniapps.MiniAppSnapshots.clear()
            xyz.tironi.zoen.miniapps.MiniAppModelContext.clear()
        }
        boot()
    }

    suspend fun refresh() = withContext(Dispatchers.IO) { gate.withLock { refreshLocked() } }

    suspend fun <T> network(block: suspend (RodaEngine) -> T): T {
        val core = query { it }
        val result = block(core)
        refresh()
        return result
    }

    suspend fun resetDemo() = withContext(Dispatchers.IO) {
        gate.withLock {
            check(mutableState.value.demo)
            observed.clear()
            engine?.resetDemo()
            xyz.tironi.zoen.miniapps.MiniAppSnapshots.clear()
            xyz.tironi.zoen.miniapps.MiniAppModelContext.clear()
            refreshLocked()
        }
    }

    suspend fun signOut() = withContext(Dispatchers.IO) {
        gate.withLock {
            observed.clear()
            syncRunning = false
            backgroundOwner = null
            activeSpace = null
            engine?.stopSync()
            engine?.eraseDevice(vault)
            xyz.tironi.zoen.miniapps.MiniAppSnapshots.clear()
            xyz.tironi.zoen.miniapps.MiniAppModelContext.clear()
            preferences.edit().clear().putBoolean("onboarded", false).putBoolean("demo", false).commit()
            File(context.noBackupFilesDir, "onboarding").deleteRecursively()
            refreshLocked()
        }
    }
}
