package xyz.tironi.zoen.core

import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import expo.modules.kotlin.functions.Coroutine
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.cancel
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean

class ZoenCoreModule : Module() {
  private companion object {
    private val process = ZoenProcessLifecycle<Session>()
    private val retirementWork = CoroutineScope(SupervisorJob() + Dispatchers.IO)
  }
  private val owner = Any()
  private val mutex get() = process.mutex
  private val cleanup = CoroutineScope(SupervisorJob() + Dispatchers.IO)
  private val destroyed = AtomicBoolean(false)
  private val pickupLifetime = ZoenModulePickupLifetime()
  private class Session(
    val owner: Any,
    val engine: Client,
    val vault: ZoenVault,
    val listener: CoreListener,
    val mode: String,
    val eventToken: Any = Any(),
    val lease: ZoenRequestLease = ZoenRequestLease(),
    var syncStarted: Boolean = false,
    val releasePickupBinding: () -> Unit = {},
  ) {
    val pickup = ZoenSessionPickup(lease, engine::retireLinkPickup, engine::drainLinkPickup)
  }
  private val session get() = process.current(owner)
  private val retired get() = process.retired
  private var sessionLocale = "en"
  private val erasureTask get() = process.erasureTask
  private val erasureScope get() = process.erasureScope
  private val completedErasure get() = process.completedErasure

  override fun definition() = ModuleDefinition {
    Name("ZoenCore")
    Events("onChange")
    AsyncFunction("open") Coroutine { selectedMode: String, locale: String ->
      withContext(Dispatchers.IO) {
        val opening = mutex.withLock {
          check(!destroyed.get()) { "The app session has closed" }
          require(selectedMode in setOf("account", "demo")) { "Invalid account mode" }
          require(locale in setOf("en", "pt-BR")) { "Invalid session language" }
          val ticket = process.beginOpen(owner)
          sessionLocale = locale
          closeCore(allOwners = true)
          ticket to retired.map { it.lease.retired }
        }
        for (drain in opening.second) drain.await()
        val recovery = mutex.withLock {
          check(!destroyed.get() && process.isOpening(owner, opening.first)) { "The app session has closed" }
          val context = requireNotNull(appContext.reactContext)
          val erasure = ZoenDeviceErasure(context.noBackupFilesDir)
          val pending = erasure.pending()
          if (pending != null) {
            startErasure(pending, null, erasure, preservingOpening = true)
          } else null
        }
        recovery?.await()
        mutex.withLock {
          check(!destroyed.get()) { "The app session has closed" }
          check(process.isOpening(owner, opening.first)) { "The app session has closed" }
          closeCore(allOwners = true)
          val context = requireNotNull(appContext.reactContext)
          val root = File(context.noBackupFilesDir, selectedMode).apply { mkdirs() }
          val core = Client.open(File(root, "client.sqlite").path, locale)
          val keys = ZoenVault(context)
          val eventToken = Any()
          val bridge = object : CoreListener {
            override fun onChange(spaceIds: List<String>) { emit(eventToken, mapOf("kind" to "change", "spaceIds" to spaceIds)) }
            override fun onEphemeral(spaceId: String, fromId: String, kind: String, detail: String) {
              emit(eventToken, mapOf("kind" to "ephemeral", "spaceId" to spaceId, "fromId" to fromId, "type" to kind, "detail" to detail))
            }
            override fun onPresence(identityId: String, online: Boolean) { emit(eventToken, mapOf("kind" to "presence", "identityId" to identityId, "online" to online)) }
            override fun onConnection(status: ConnectionDto) { emit(eventToken, mapOf("kind" to "connection")) }
            override fun onError(message: String) { emit(eventToken, mapOf("kind" to "error", "message" to message)) }
            override fun onProfileChanged(identityId: String) { emit(eventToken, mapOf("kind" to "profile", "identityId" to identityId)) }
          }
          var pickupBinding: Any? = null
          try {
            if (selectedMode == "demo") core.seedDemoIfEmpty()
            else if (core.account() != null) {
              check(core.unlock(keys)) { "Não foi possível desbloquear esta conta. Use a recuperação da conta." }
              if (core.account()?.setupPending != true) core.startSync(bridge)
            } else check(!keys.hasAccountMaterial()) { "Há chaves de uma conta neste aparelho. Recupere a conta para preservá-las." }
            val snapshot = core.clientRequest("{\"op\":\"snapshot\"}")
            // Publish only after unlock, vault consistency and snapshot validation.
            val binding = pickupLifetime.bind { core.retireLinkPickup() }
            check(binding != null) { "The app session has closed" }
            pickupBinding = binding
            process.install(owner, opening.first, Session(owner, core, keys, bridge, selectedMode, eventToken = eventToken,
              syncStarted = core.account()?.let { it.unlocked && !it.setupPending } == true,
              releasePickupBinding = { pickupLifetime.clear(binding) }))
            // Cover changes received between the initial snapshot and listener admission.
            emit(eventToken, mapOf("kind" to "change", "spaceIds" to emptyList<String>()))
            snapshot
          } catch (error: Throwable) {
            // This unpublished Client has admitted no vault/network request.
            core.retireLinkPickup()
            pickupBinding?.let { pickupLifetime.clear(it) }
            core.stopSync()
            core.destroy()
            throw error
          }
        }
      }
    }
    AsyncFunction("request") Coroutine { request: String ->
      withContext(Dispatchers.IO) {
        val active = mutex.withLock {
          check(!destroyed.get()) { "The app session has closed" }
          process.admit(owner).also { it.lease.admit() }
        }
        try {
          // Rust serializes its own domain mutations. Do not hold the lifecycle
          // mutex across network awaits: a slow lookup must not block the chat.
          val result = if (active.mode == "demo") active.engine.clientRequest(request)
            else active.engine.clientVaultRequest(request, active.vault)
          mutex.withLock {
            check(session === active && !active.lease.closing) { "The app session has closed" }
            if (!active.syncStarted && active.engine.account()?.let { it.unlocked && !it.setupPending } == true) {
              active.engine.startSync(active.listener)
              active.syncStarted = true
            }
          }
          result
        } finally {
          withContext(NonCancellable + Dispatchers.IO) {
            mutex.withLock {
              // A request finishing is not native worker completion. The one
              // process-owned close task acknowledges both before destruction.
              active.lease.retire()
            }
          }
        }
      }
    }
    AsyncFunction("createAccount") Coroutine { name: String, handle: String, relay: String ->
      withContext(Dispatchers.IO) { mutex.withLock {
        check(!destroyed.get()) { "The app session has closed" }
        val active = process.admit(owner)
        val core = active.engine
        check(active.mode == "account" && core.account() == null) { "An account already exists" }
        check(!active.vault.hasAccountMaterial()) { "Há chaves de uma conta neste aparelho. Recupere a conta para preservá-las." }
        core.createAccount(name, handle, relay, active.vault)
        core.startSync(active.listener)
        active.syncStarted = true
        core.clientRequest("{\"op\":\"snapshot\"}")
      } }
    }
    // File-message ingestion remains iOS-only until Android URI ownership and
    // lifecycle draining are implemented and validated. Match the JSON contract
    // while refusing before any canonical mutation.
    AsyncFunction("sendFile").Coroutine<String, String> { _ ->
      error("Sending attachments is not available on Android yet.")
    }
    AsyncFunction("eraseDevice") Coroutine { identityId: String, deviceId: String ->
      withContext(Dispatchers.IO) {
        val expected = ZoenDeviceScope(identityId, deviceId)
        val task = mutex.withLock {
          check(!destroyed.get()) { "The app session has closed" }
          process.assertOwnerOrEmpty(owner)
          erasureTask?.let {
            check(erasureScope == expected) { "The account changed. Open the confirmation again." }
            return@withLock it
          }
          if (session == null && completedErasure == expected) return@withLock null
          val context = requireNotNull(appContext.reactContext)
          val erasure = ZoenDeviceErasure(context.noBackupFilesDir)
          val active = session
          if (active != null) {
            check(active.mode == "account") { "The account changed. Open the confirmation again." }
            verifyErasureScope(active.engine, expected, allowEmpty = erasure.pending() == expected)
          }
          startErasure(expected, active, erasure)
        }
        task?.await()
        Unit
      }
    }
    AsyncFunction("close") Coroutine { ->
      withContext(Dispatchers.IO) {
        val work = mutex.withLock {
          process.cancelOpening(owner)
          closeCore()
          retired.filter { it.owner === owner }.map { it.lease.retired } to erasureTask
        }
        for (acknowledgement in work.first) acknowledgement.await()
        work.second?.await()
      }
    }
    OnDestroy {
      destroyed.set(true)
      // Revoke synchronously, even if another bridge coroutine holds the mutex.
      // Binding rejects an open suspended before this module was destroyed.
      try { pickupLifetime.retire() }
      finally {
        cleanup.launch {
          try {
            val work = mutex.withLock {
              process.cancelOpening(owner)
              closeCore()
              retired.filter { it.owner === owner }.map { it.lease.retired } to erasureTask
            }
            for (acknowledgement in work.first) acknowledgement.await()
            work.second?.await()
          }
          finally { cleanup.cancel() }
        }
      }
    }
  }

  private fun emit(token: Any, event: Map<String, Any>) {
    cleanup.launch {
      mutex.withLock {
        if (!destroyed.get() && session?.eventToken === token) sendEvent("onChange", event)
      }
    }
  }

  private fun closeCore(allOwners: Boolean = false) {
    val active = process.retire(owner, allOwners) ?: return
    try {
      active.pickup.retire()
      active.engine.stopSync()
    } catch (error: Throwable) {
      active.lease.destructionFailed(error)
      throw error
    }
    // One close job owns the exact retired Session. Module destruction must not
    // cancel this scope, and the process mutex must remain available to requests.
    retirementWork.launch {
      try {
        active.pickup.drain()
        mutex.withLock { destroyRetired(active) }
      } catch (error: Throwable) {
        mutex.withLock { active.lease.destructionFailed(error) }
      }
    }
  }

  private fun destroyRetired(active: Session) {
    try {
      active.releasePickupBinding()
      active.engine.destroy()
      active.lease.destroyed()
      retired.remove(active)
    } catch (error: Throwable) {
      active.lease.destructionFailed(error)
      retired.add(active)
      throw error
    }
  }

  private fun verifyErasureScope(engine: Client, expected: ZoenDeviceScope, allowEmpty: Boolean = false) {
    val account = engine.account()
    requireErasureScope(account?.let { ZoenDeviceScope(it.identityId, it.deviceId) }, expected, allowEmpty)
  }

  /** Called under mutex. Admission closes before waiting on already accepted work. */
  private fun startErasure(expected: ZoenDeviceScope, active: Session?, erasure: ZoenDeviceErasure,
    preservingOpening: Boolean = false): Deferred<Unit> {
    val context = requireNotNull(appContext.reactContext)
    val locale = sessionLocale
    if (active != null) {
      erasure.begin(expected)
      if (session === active) process.retire(owner)
      // Process retirement retains this owner even if native retirement fails.
      try { active.pickup.retire(erase = true) }
      catch (error: Throwable) {
        active.lease.destructionFailed(error)
        throw error
      }
    }
    val earlierRequests = retired.filter { it !== active }.map { it.lease.retired }
    // Independent of the bridge caller's cancellation. The durable intent is resumed on open after process death.
    lateinit var task: Deferred<Unit>
    task = retirementWork.async(start = CoroutineStart.LAZY) {
      var engineDestroyed = false
      var completed = false
      var working = active
      try {
        working?.engine?.stopSync()
        if (working == null) working = process.withErasureAfterRetirements(expected, task, earlierRequests) {
          val root = File(context.noBackupFilesDir, "account").apply { mkdirs() }
          val engine = Client.open(File(root, "client.sqlite").path, locale)
          val candidate = Session(owner, engine, ZoenVault(context), emptyListener(), "account")
          try {
            candidate.pickup.retire(erase = true)
            verifyErasureScope(engine, expected, allowEmpty = erasure.pending() == expected)
            erasure.begin(expected)
            retired.add(candidate)
            candidate
          } catch (error: Throwable) {
            destroyRetired(candidate)
            throw error
          }
        }
        else for (drain in earlierRequests) drain.await()
        val current = requireNotNull(working)
        current.pickup.drain()
        verifyErasureScope(current.engine, expected, allowEmpty = erasure.pending() == expected)
        current.engine.eraseDevice(current.vault)
        current.vault.eraseAllVerified()
        mutex.withLock {
          destroyRetired(current)
          engineDestroyed = true
        }
        ZoenDeviceErasure.eraseDirectory(File(context.noBackupFilesDir, "account"))
        for (name in listOf("ZoenMedia", "ZoenVoiceEditor", "ImagePicker", "ImageManipulator", "DocumentPicker", "ExpoAudio", "zoen-exports")) {
          ZoenDeviceErasure.eraseDirectory(File(context.cacheDir, name))
        }
        erasure.complete()
        completed = true
      } finally {
        withContext(NonCancellable + Dispatchers.IO) {
          for (drain in earlierRequests) runCatching { drain.await() }
          val current = working
          try {
            if (!engineDestroyed && current != null) {
              current.pickup.drain()
              mutex.withLock { destroyRetired(current) }
            }
          } catch (error: Throwable) {
            mutex.withLock { current?.lease?.destructionFailed(error) }
            throw error
          } finally {
            mutex.withLock { process.finishErasure(expected, task, completed) }
          }
        }
      }
    }
    process.acceptErasure(owner, expected, task, preservingOpening)
    task.start()
    return task
  }

  private fun emptyListener(): CoreListener = object : CoreListener {
    override fun onChange(spaceIds: List<String>) = Unit
    override fun onEphemeral(spaceId: String, fromId: String, kind: String, detail: String) = Unit
    override fun onPresence(identityId: String, online: Boolean) = Unit
    override fun onConnection(status: ConnectionDto) = Unit
    override fun onError(message: String) = Unit
    override fun onProfileChanged(identityId: String) = Unit
  }
}
