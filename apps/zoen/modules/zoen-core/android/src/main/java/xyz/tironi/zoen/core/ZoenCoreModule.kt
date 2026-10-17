package xyz.tironi.zoen.core

import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import expo.modules.kotlin.functions.Coroutine
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.io.File
import java.util.Locale
import java.util.concurrent.atomic.AtomicBoolean

class ZoenCoreModule : Module() {
  private val mutex = Mutex()
  private val cleanup = CoroutineScope(SupervisorJob() + Dispatchers.IO)
  private val destroyed = AtomicBoolean(false)
  private class Session(
    val engine: RodaEngine,
    val vault: ZoenVault,
    val listener: CoreListener,
    val mode: String,
    var requests: Int = 0,
    var closing: Boolean = false,
    var syncStarted: Boolean = false,
  )
  private var session: Session? = null

  override fun definition() = ModuleDefinition {
    Name("ZoenCore")
    Events("onChange")
    AsyncFunction("open") Coroutine { selectedMode: String ->
      withContext(Dispatchers.IO) { mutex.withLock {
        check(!destroyed.get()) { "The app session has closed" }
        require(selectedMode in setOf("account", "demo")) { "Invalid account mode" }
        closeCore()
        val context = requireNotNull(appContext.reactContext)
        val root = File(context.noBackupFilesDir, if (selectedMode == "account") "core" else "zoen/demo").apply { mkdirs() }
        val core = RodaEngine.open(File(root, "zoen.sqlite").path, Locale.getDefault().toLanguageTag())
        val keys = ZoenVault(context)
        val bridge = object : CoreListener {
          override fun onChange(spaceIds: List<String>) { emit(mapOf("kind" to "change", "spaceIds" to spaceIds)) }
          override fun onEphemeral(spaceId: String, fromId: String, kind: String, detail: String) {
            emit(mapOf("kind" to "ephemeral", "spaceId" to spaceId, "fromId" to fromId, "type" to kind, "detail" to detail))
          }
          override fun onPresence(identityId: String, online: Boolean) { emit(mapOf("kind" to "presence", "identityId" to identityId, "online" to online)) }
          override fun onConnection(status: ConnectionDto) { emit(mapOf("kind" to "connection")) }
          override fun onError(message: String) { emit(mapOf("kind" to "error", "message" to message)) }
          override fun onProfileChanged(identityId: String) { emit(mapOf("kind" to "profile", "identityId" to identityId)) }
        }
        try {
          if (selectedMode == "demo") core.seedDemoIfEmpty()
          else if (core.account() != null) {
            check(core.unlock(keys)) { "Não foi possível desbloquear esta conta. Use a recuperação da conta." }
            if (core.account()?.setupPending != true) core.startSync(bridge)
          } else check(!keys.hasAccountMaterial()) { "Há chaves de uma conta neste aparelho. Recupere a conta para preservá-las." }
          val snapshot = core.clientRequest("{\"op\":\"snapshot\"}")
          // Publish only after unlock, vault consistency and snapshot validation.
          session = Session(core, keys, bridge, selectedMode, syncStarted = core.account()?.let { it.unlocked && !it.setupPending } == true)
          snapshot
        } catch (error: Throwable) {
          core.stopSync()
          core.destroy()
          throw error
        }
      } }
    }
    AsyncFunction("request") Coroutine { request: String ->
      withContext(Dispatchers.IO) {
        val active = mutex.withLock {
          check(!destroyed.get()) { "The app session has closed" }
          requireNotNull(session).also { it.requests += 1 }
        }
        try {
          // Rust serializes its own domain mutations. Do not hold the lifecycle
          // mutex across network awaits: a slow lookup must not block the chat.
          val result = if (active.mode == "demo") active.engine.clientRequest(request)
            else active.engine.clientVaultRequest(request, active.vault)
          mutex.withLock {
            check(session === active && !active.closing) { "The app session has closed" }
            if (!active.syncStarted && active.engine.account()?.let { it.unlocked && !it.setupPending } == true) {
              active.engine.startSync(active.listener)
              active.syncStarted = true
            }
          }
          result
        } finally {
          withContext(NonCancellable + Dispatchers.IO) {
            mutex.withLock {
              active.requests -= 1
              if (active.closing && active.requests == 0) active.engine.destroy()
            }
          }
        }
      }
    }
    AsyncFunction("createAccount") Coroutine { name: String, handle: String, relay: String ->
      withContext(Dispatchers.IO) { mutex.withLock {
        check(!destroyed.get()) { "The app session has closed" }
        val active = requireNotNull(session)
        val core = active.engine
        check(active.mode == "account" && core.account() == null) { "An account already exists" }
        check(!active.vault.hasAccountMaterial()) { "Há chaves de uma conta neste aparelho. Recupere a conta para preservá-las." }
        core.createAccount(name, handle, relay, active.vault)
        core.startSync(active.listener)
        active.syncStarted = true
        core.clientRequest("{\"op\":\"snapshot\"}")
      } }
    }
    AsyncFunction("close") Coroutine { withContext(Dispatchers.IO) { mutex.withLock { closeCore() } } }
    OnDestroy {
      destroyed.set(true)
      // Retire the session now; the final lease destroys its UniFFI handle.
      cleanup.launch {
        try { mutex.withLock { closeCore() } }
        finally { cleanup.cancel() }
      }
    }
  }

  private fun emit(event: Map<String, Any>) {
    if (!destroyed.get()) sendEvent("onChange", event)
  }

  private fun closeCore() {
    val active = session ?: return
    session = null
    active.closing = true
    active.engine.stopSync()
    if (active.requests == 0) active.engine.destroy()
  }
}
