package xyz.tironi.zoen.core

import kotlinx.coroutines.Deferred
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** One account namespace per Android process, even while React contexts overlap. */
internal class ZoenProcessLifecycle<T : Any> {
  val mutex = Mutex()
  val retired = mutableSetOf<T>()
  private data class Owned<T>(val owner: Any, val value: T)
  @Volatile private var active: Owned<T>? = null
  private var openingOwner: Any? = null
  private var generation = 0L
  var erasureTask: Deferred<Unit>? = null
    private set
  var erasureScope: ZoenDeviceScope? = null
    private set
  var completedErasure: ZoenDeviceScope? = null
    private set

  fun current(owner: Any): T? = active?.takeIf { it.owner === owner }?.value

  /** All methods except current are called with mutex held. */
  fun beginOpen(owner: Any): Long {
    check(erasureTask == null) { "Device sign-out needs a retry." }
    generation += 1
    openingOwner = owner
    return generation
  }

  fun isOpening(owner: Any, ticket: Long): Boolean =
    openingOwner === owner && generation == ticket && erasureTask == null

  fun install(owner: Any, ticket: Long, value: T) {
    check(isOpening(owner, ticket)) { "The app session has closed" }
    active = Owned(owner, value)
    openingOwner = null
    completedErasure = null
  }

  fun admit(owner: Any): T {
    check(erasureTask == null) { "Device sign-out needs a retry." }
    return requireNotNull(current(owner)) { "The app session has closed" }
  }

  fun retire(owner: Any, allOwners: Boolean = false): T? {
    val previous = active ?: return null
    if (!allOwners && previous.owner !== owner) return null
    // Ownership survives removal from admission, including failed close/erase.
    // Reopening callers wait this exact value's destruction acknowledgement.
    retired.add(previous.value)
    active = null
    return previous.value
  }

  fun assertOwnerOrEmpty(owner: Any) {
    check(active?.owner?.let { it === owner } != false && (openingOwner == null || openingOwner === owner)) {
      "The account changed. Open the confirmation again."
    }
  }

  fun cancelOpening(owner: Any) {
    if (openingOwner === owner) {
      generation += 1
      openingOwner = null
    }
  }

  fun acceptErasure(owner: Any, scope: ZoenDeviceScope, task: Deferred<Unit>, preservingOpening: Boolean) {
    check(erasureTask == null) { "Device sign-out needs a retry." }
    assertOwnerOrEmpty(owner)
    if (preservingOpening) check(openingOwner === owner) { "The app session has closed" }
    else {
      generation += 1
      openingOwner = null
    }
    erasureScope = scope
    erasureTask = task
    completedErasure = null
  }

  fun finishErasure(scope: ZoenDeviceScope, task: Deferred<Unit>, completed: Boolean) {
    check(erasureTask === task && erasureScope == scope) { "The app session has closed" }
    if (completed) completedErasure = scope
    erasureTask = null
    erasureScope = null
  }

  /** Reopening the shared files is admitted only after every old handle acknowledged destruction. */
  suspend fun <R> withErasureAfterRetirements(scope: ZoenDeviceScope, task: Deferred<Unit>,
    acknowledgements: List<Deferred<Unit>>, open: () -> R): R {
    for (acknowledgement in acknowledgements) acknowledgement.await()
    return mutex.withLock {
      check(erasureTask === task && erasureScope == scope) { "The app session has closed" }
      open()
    }
  }
}

internal fun requireErasureScope(current: ZoenDeviceScope?, expected: ZoenDeviceScope, allowEmpty: Boolean = false) {
  check(if (current == null) allowEmpty else current == expected) {
    "The account changed. Open the confirmation again."
  }
}
