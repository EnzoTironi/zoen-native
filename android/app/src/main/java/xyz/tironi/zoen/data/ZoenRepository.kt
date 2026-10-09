package xyz.tironi.zoen.data

import android.content.Context
import java.io.File
import java.util.Locale
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
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
    val revision: Long = 0,
) {
    val pending: Int get() = requests.count { it.status == RequestStatus.PENDING }
    val zoenChat: SpaceSummary? get() = spaces.firstOrNull { it.counterpart?.handle == "zoen" }
}

class ZoenRepository(private val context: Context) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val gate = Mutex()
    private val changes = Channel<Unit>(Channel.CONFLATED)
    private val errors = Channel<String>(Channel.BUFFERED)
    val errorEvents = errors.receiveAsFlow()
    private var engine: RodaEngine? = null
    private var openedLanguage: String? = null
    private val observed = mutableSetOf<String>()
    private val typingUpdates = mutableMapOf<String, Long>()
    private val mutableState = MutableStateFlow(AppState())
    val state = mutableState.asStateFlow()
    val vault = AndroidSecretVault(context)
    val preferences = context.getSharedPreferences("zoen", Context.MODE_PRIVATE)
    val locale: String get() = context.resources.configuration.locales[0].toLanguageTag()

    private val listener = object : CoreListener {
        override fun onChange(spaceIds: List<String>) { changes.trySend(Unit) }
        override fun onProfileChanged(identityId: String) { changes.trySend(Unit) }
        override fun onPresence(identityId: String, online: Boolean) { changes.trySend(Unit) }
        override fun onConnection(status: ConnectionDto) { changes.trySend(Unit) }
        override fun onError(message: String) { errors.trySend(message); changes.trySend(Unit) }
        override fun onEphemeral(spaceId: String, fromId: String, kind: String, detail: String) {
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
                engine?.destroy()
                engine = null
                observed.clear()
                mutableState.value = AppState()
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
                    if (core.unlock(vault)) core.startSync(listener)
                    else mutableState.value = mutableState.value.copy(keyMissing = true)
                }
                refreshLocked()
            } catch (e: Exception) {
                if (e is CancellationException) throw e
                engine?.stopSync()
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

    suspend fun createAccount(name: String, handle: String, relay: String) {
        change {
            it.createAccount(name.trim(), handle.trim().removePrefix("@"), relay.trim(), vault)
            it.startSync(listener)
        }
    }

    suspend fun useDemo() = withContext(Dispatchers.IO) {
        check(BuildConfig.DEBUG)
        gate.withLock {
            engine?.stopSync()
            engine?.destroy()
            engine = null
            observed.clear()
            preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
            mutableState.value = AppState()
        }
        boot(true)
    }

    suspend fun leaveDemo() = withContext(Dispatchers.IO) {
        gate.withLock {
            engine?.destroy()
            engine = null
            observed.clear()
            preferences.edit().putBoolean("demo", false).putBoolean("onboarded", false).commit()
            mutableState.value = AppState()
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
            refreshLocked()
        }
    }

    suspend fun signOut() = withContext(Dispatchers.IO) {
        gate.withLock {
            observed.clear()
            engine?.eraseDevice(vault)
            preferences.edit().putBoolean("onboarded", false).commit()
            refreshLocked()
        }
    }
}
