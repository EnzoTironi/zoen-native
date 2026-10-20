package xyz.tironi.zoen.core

import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext

/** Revokes only the exact Client bound to this module, before asynchronous teardown. */
internal class ZoenModulePickupLifetime {
  private data class Binding(val token: Any, val retire: () -> Unit)
  private val lock = ReentrantLock()
  private val completed = lock.newCondition()
  private var terminal = false
  private var retiring: Binding? = null
  private var binding: Binding? = null

  fun bind(retire: () -> Unit): Any? {
    val token = lock.withLock {
      if (terminal || binding != null) null
      else Any().also { binding = Binding(it, retire) }
    }
    // Rust may acquire the install/signing gate. Never call it under this lock.
    if (token == null) retire()
    return token
  }

  /** Clear before explicit Client.destroy; a captured callback must finish first. */
  fun clear(token: Any) {
    lock.withLock {
      while (retiring?.token === token) completed.awaitUninterruptibly()
      if (binding?.token === token) binding = null
    }
  }

  /** The native callback is synchronous and must not reenter this same retire(). */
  fun retire() {
    val action = lock.withLock {
      terminal = true
      while (retiring != null) completed.awaitUninterruptibly()
      val action = binding
      binding = null
      retiring = action
      action
    }
    try { action?.retire?.invoke() }
    finally {
      lock.withLock {
        retiring = null
        completed.signalAll()
      }
    }
  }
}

/** Shared close/erase ordering; native completion and bridge-request retirement are distinct. */
internal class ZoenSessionPickup(
  private val lease: ZoenRequestLease,
  private val retireNative: () -> Unit,
  private val drainNative: suspend () -> Unit,
) {
  fun retire(erase: Boolean = false) {
    lease.close(erase)
    retireNative()
  }

  suspend fun drain() = withContext(NonCancellable) {
    // Kotlin cancellation frees a UniFFI future; it does not acknowledge Rust
    // worker Drop. Keep this real drain alive until the native future completes.
    drainNative()
    lease.drained.await()
  }
}
