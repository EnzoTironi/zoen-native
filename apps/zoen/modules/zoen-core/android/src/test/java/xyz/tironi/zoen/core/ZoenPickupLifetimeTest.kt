package xyz.tironi.zoen.core

import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.yield
import org.junit.Assert.*
import org.junit.Test

class ZoenPickupLifetimeTest {
  private fun assertOriginalFailure(expected: Throwable, observed: Throwable?) {
    val actual = requireNotNull(observed) { "The native failure must propagate" }
    assertEquals(expected.javaClass, actual.javaClass)
    assertEquals(expected.message, actual.message)
    // Coroutine stack recovery may copy the exception while retaining its cause.
    assertTrue("The original failure must remain in the causal chain",
      generateSequence(actual) { it.cause }.any { it === expected })
  }

  @Test fun destructionSynchronouslyRevokesTheExactClientAndRejectsALateOpen() {
    val lifetime = ZoenModulePickupLifetime()
    val first = AtomicInteger()
    assertNotNull(lifetime.bind { first.incrementAndGet() })
    assertEquals(0, first.get())
    lifetime.retire()
    assertEquals(1, first.get())
    lifetime.retire()
    assertEquals(1, first.get())

    val late = AtomicInteger()
    assertNull(lifetime.bind { late.incrementAndGet() })
    assertEquals("A refused open is revoked before bind returns", 1, late.get())
  }

  @Test fun oldCompletionCannotClearTheReplacementAndDuplicateCannotDisplaceIt() {
    val lifetime = ZoenModulePickupLifetime()
    val first = AtomicInteger()
    val firstToken = requireNotNull(lifetime.bind { first.incrementAndGet() })
    lifetime.clear(firstToken)
    val second = AtomicInteger()
    val secondToken = requireNotNull(lifetime.bind { second.incrementAndGet() })
    assertNotSame(firstToken, secondToken)
    val rejected = AtomicInteger()
    assertNull(lifetime.bind { rejected.incrementAndGet() })
    assertEquals(1, rejected.get())
    assertEquals(0, second.get())
    lifetime.clear(firstToken)
    lifetime.retire()
    assertEquals(0, first.get())
    assertEquals(1, second.get())
  }

  @Test(timeout = 10000) fun nativeCallbacksRunOutsideTheBindingLockIncludingRejectedOpen() {
    val lifetime = ZoenModulePickupLifetime()
    val errors = ConcurrentLinkedQueue<Throwable>()
    val completed = CountDownLatch(1)
    assertNotNull(lifetime.bind {
      val cleared = CountDownLatch(1)
      val otherThread = Thread { lifetime.clear(Any()); cleared.countDown() }
      otherThread.isDaemon = true
      otherThread.start()
      assertTrue("Native callback cannot hold the binding lock", cleared.await(2, TimeUnit.SECONDS))
      otherThread.join(2000)
    })
    val destroying = Thread {
      try { lifetime.retire() } catch (error: Throwable) { errors.add(error) }
      finally { completed.countDown() }
    }
    destroying.isDaemon = true
    destroying.start()
    assertTrue(completed.await(3, TimeUnit.SECONDS))
    destroying.join(2000)
    errors.firstOrNull()?.let { throw it }

    assertNull(lifetime.bind {
      val cleared = CountDownLatch(1)
      val otherThread = Thread { lifetime.clear(Any()); cleared.countDown() }
      otherThread.isDaemon = true
      otherThread.start()
      assertTrue("Rejected native callback must also run outside the lock", cleared.await(2, TimeUnit.SECONDS))
      otherThread.join(2000)
    })
  }

  @Test(timeout = 10000) fun concurrentDestroyWaitsForTheSameSynchronousRevocation() {
    val lifetime = ZoenModulePickupLifetime()
    val entered = CountDownLatch(1)
    val release = CountDownLatch(1)
    val secondStarted = CountDownLatch(1)
    val firstReturned = CountDownLatch(1)
    val secondReturned = CountDownLatch(1)
    val calls = AtomicInteger()
    val errors = ConcurrentLinkedQueue<Throwable>()
    assertNotNull(lifetime.bind {
      calls.incrementAndGet()
      entered.countDown()
      assertTrue(release.await(4, TimeUnit.SECONDS))
    })
    val first = Thread {
      try { lifetime.retire() } catch (error: Throwable) { errors.add(error) }
      finally { firstReturned.countDown() }
    }.apply { isDaemon = true }
    val second = Thread {
      secondStarted.countDown()
      try { lifetime.retire() } catch (error: Throwable) { errors.add(error) }
      finally { secondReturned.countDown() }
    }.apply { isDaemon = true }
    try {
      first.start()
      assertTrue(entered.await(2, TimeUnit.SECONDS))
      second.start()
      assertTrue(secondStarted.await(2, TimeUnit.SECONDS))
      assertFalse("Concurrent teardown cannot return before native revocation", secondReturned.await(50, TimeUnit.MILLISECONDS))
    } finally { release.countDown() }
    assertTrue(firstReturned.await(2, TimeUnit.SECONDS))
    assertTrue(secondReturned.await(2, TimeUnit.SECONDS))
    first.join(2000)
    second.join(2000)
    errors.firstOrNull()?.let { throw it }
    assertEquals(1, calls.get())
  }

  @Test(timeout = 10000) fun exactBindingClearWaitsBeforeExplicitClientDestruction() {
    val lifetime = ZoenModulePickupLifetime()
    val entered = CountDownLatch(1)
    val release = CountDownLatch(1)
    val clearStarted = CountDownLatch(1)
    val cleared = CountDownLatch(1)
    val destroyed = AtomicBoolean(false)
    val errors = ConcurrentLinkedQueue<Throwable>()
    val token = requireNotNull(lifetime.bind {
      entered.countDown()
      assertTrue(release.await(4, TimeUnit.SECONDS))
      assertFalse("The bound Client must remain callable until native retirement returns", destroyed.get())
    })
    val destroyingModule = Thread {
      try { lifetime.retire() } catch (error: Throwable) { errors.add(error) }
    }.apply { isDaemon = true }
    val destroyingClient = Thread {
      clearStarted.countDown()
      try {
        lifetime.clear(token)
        destroyed.set(true)
        cleared.countDown()
      } catch (error: Throwable) { errors.add(error) }
    }.apply { isDaemon = true }
    try {
      destroyingModule.start()
      assertTrue(entered.await(2, TimeUnit.SECONDS))
      // A stale completion must not wait on or alter this exact native callback.
      lifetime.clear(Any())
      destroyingClient.start()
      assertTrue(clearStarted.await(2, TimeUnit.SECONDS))
      assertFalse(cleared.await(50, TimeUnit.MILLISECONDS))
      assertFalse(destroyed.get())
    } finally { release.countDown() }
    assertTrue(cleared.await(2, TimeUnit.SECONDS))
    destroyingModule.join(2000)
    destroyingClient.join(2000)
    errors.firstOrNull()?.let { throw it }
    assertTrue(destroyed.get())
  }

  @Test fun callbackFailureDoesNotStrandAConcurrentOrRepeatedDestroy() {
    val lifetime = ZoenModulePickupLifetime()
    val failure = IllegalStateException("native failure")
    assertNotNull(lifetime.bind { throw failure })
    assertSame(failure, assertThrows(IllegalStateException::class.java) { lifetime.retire() })
    lifetime.retire()
    val late = AtomicInteger()
    assertNull(lifetime.bind { late.incrementAndGet() })
    assertEquals(1, late.get())
  }

  @Test fun retirementFencesNewRequestsBeforeEnteringTheNativeGate() {
    val lease = ZoenRequestLease()
    lease.admit()
    val calls = AtomicInteger()
    val pickup = ZoenSessionPickup(lease, {
      assertTrue(lease.closing)
      assertThrows(IllegalStateException::class.java) { lease.admit() }
      calls.incrementAndGet()
    }, {})
    pickup.retire()
    assertEquals(1, calls.get())
    assertFalse(lease.drained.isCompleted)
    assertFalse(lease.retired.isCompleted)
    assertTrue(lease.retire())
  }

  @Test fun nativeWorkerAndAcceptedRequestsMustBothActuallyDrain() = runBlocking<Unit> {
    withTimeout(4000) {
      for (requestFirst in listOf(true, false)) {
        val lease = ZoenRequestLease()
        lease.admit()
        val worker = CompletableDeferred<Unit>()
        var nativeCompleted = false
        val pickup = ZoenSessionPickup(lease, {}, {
          worker.await()
          nativeCompleted = true
        })
        pickup.retire()
        val draining = async(start = CoroutineStart.UNDISPATCHED) { pickup.drain() }
        try {
          if (requestFirst) {
            assertTrue(lease.retire())
            assertTrue(lease.drained.isCompleted)
            assertFalse(nativeCompleted)
          } else {
            worker.complete(Unit)
            yield()
            assertTrue(nativeCompleted)
            assertFalse(lease.drained.isCompleted)
          }
          assertFalse("One acknowledgement cannot stand in for the other", draining.isCompleted)
          assertFalse(lease.retired.isCompleted)
          if (requestFirst) worker.complete(Unit) else assertTrue(lease.retire())
          draining.await()
          assertTrue(nativeCompleted)
          assertFalse("Drain is not handle destruction", lease.retired.isCompleted)
          lease.destroyed()
          assertTrue(lease.retired.isCompleted)
        } finally {
          worker.complete(Unit)
          if (!lease.drained.isCompleted) lease.retire()
          draining.join()
        }
      }
    }
  }

  @Test fun cancellingTheCallerCannotFreeTheNativeDrainBeforeWorkerCompletion() = runBlocking<Unit> {
    withTimeout(4000) {
      val lease = ZoenRequestLease()
      val worker = CompletableDeferred<Unit>()
      var nativeFutureFreed = false
      var nativeCompleted = false
      val pickup = ZoenSessionPickup(lease, {}, {
        // Models the generated Kotlin future's finally/free boundary, not a
        // substitute for native worker retirement or a raw FFI cancellation.
        try { worker.await(); nativeCompleted = true }
        finally { nativeFutureFreed = true }
      })
      pickup.retire()
      val draining = async(start = CoroutineStart.UNDISPATCHED) { pickup.drain() }
      try {
        draining.cancel()
        yield()
        assertFalse(nativeFutureFreed)
        assertFalse(nativeCompleted)
        assertFalse(draining.isCompleted)
        worker.complete(Unit)
        draining.join()
        assertTrue(nativeCompleted)
        assertTrue(nativeFutureFreed)
        assertFalse("Cancellation is not successful destruction", lease.retired.isCompleted)
      } finally { worker.complete(Unit); draining.join() }
    }
  }

  @Test fun destroyedModuleCannotPublishAnOpenSuspendedUnderTheProcessMutex() = runBlocking<Unit> {
    withTimeout(4000) {
      val process = ZoenProcessLifecycle<Any>()
      val lifetime = ZoenModulePickupLifetime()
      val owner = Any()
      val ticket = process.beginOpen(owner)
      val snapshotDone = CompletableDeferred<Unit>()
      val revoked = AtomicBoolean(false)
      val opening = async(start = CoroutineStart.UNDISPATCHED) {
        process.mutex.withLock {
          snapshotDone.await()
          val binding = lifetime.bind { revoked.set(true) }
          if (binding != null) process.install(owner, ticket, Any())
          binding
        }
      }
      try {
        assertTrue(process.mutex.isLocked)
        // This must return synchronously without needing the process mutex.
        lifetime.retire()
        snapshotDone.complete(Unit)
        assertNull(opening.await())
        assertTrue(revoked.get())
        assertNull(process.current(owner))
      } finally { snapshotDone.complete(Unit); opening.join() }
    }
  }

  @Test fun moduleCancellationCannotReopenFilesBeforeWorkerAndHandleRetirement() = runBlocking<Unit> {
    withTimeout(5000) {
      val process = ZoenProcessLifecycle<Any>()
      val oldOwner = Any()
      val newOwner = Any()
      val oldSession = Any()
      val replacement = Any()
      val lease = ZoenRequestLease()
      val lifetime = ZoenModulePickupLifetime()
      val revocations = AtomicInteger()
      val binding = requireNotNull(lifetime.bind { revocations.incrementAndGet() })
      val worker = CompletableDeferred<Unit>()
      val destruction = CompletableDeferred<Unit>()
      val nativeDrained = CompletableDeferred<Unit>()
      val moduleJob = SupervisorJob()
      val moduleScope = CoroutineScope(coroutineContext.minusKey(Job) + moduleJob)
      val processJob = SupervisorJob()
      val processScope = CoroutineScope(coroutineContext.minusKey(Job) + processJob)
      val pickup = ZoenSessionPickup(lease, { revocations.incrementAndGet() }, {
        worker.await()
        nativeDrained.complete(Unit)
      })
      process.mutex.withLock {
        process.install(oldOwner, process.beginOpen(oldOwner), oldSession)
        lease.admit()
      }
      val moduleCallback = moduleScope.launch(start = CoroutineStart.UNDISPATCHED) {
        CompletableDeferred<Unit>().await()
      }
      val cleanup = processScope.launch(start = CoroutineStart.LAZY) {
        try {
          pickup.drain()
          destruction.await()
          process.mutex.withLock {
            lifetime.clear(binding)
            lease.destroyed()
            process.retired.remove(oldSession)
          }
        } catch (error: Throwable) { lease.destructionFailed(error) }
      }
      val nextTicket = process.mutex.withLock {
        assertSame(oldSession, process.retire(oldOwner))
        assertTrue("Production retirement must retain the exact old owner", process.retired.contains(oldSession))
        pickup.retire()
        process.beginOpen(newOwner)
      }
      cleanup.start()
      val reopening = async(start = CoroutineStart.UNDISPATCHED) {
        lease.retired.await()
        process.mutex.withLock {
          assertTrue(process.isOpening(newOwner, nextTicket))
          process.install(newOwner, nextTicket, replacement)
        }
      }
      try {
        moduleJob.cancel()
        moduleCallback.join()
        lifetime.retire()
        process.mutex.withLock {
          process.cancelOpening(oldOwner)
          assertTrue(lease.retire())
        }
        yield()
        assertFalse(cleanup.isCompleted)
        assertFalse(reopening.isCompleted)
        assertNull(process.current(newOwner))
        worker.complete(Unit)
        nativeDrained.await()
        yield()
        assertFalse("Worker completion is not handle destruction", reopening.isCompleted)
        assertFalse(lease.retired.isCompleted)
        destruction.complete(Unit)
        cleanup.join()
        reopening.await()
        assertSame(replacement, process.current(newOwner))
        assertTrue(process.retired.isEmpty())
        assertEquals(2, revocations.get())
        lifetime.retire()
        assertEquals("Old module cannot revoke the replacement", 2, revocations.get())
      } finally {
        worker.complete(Unit)
        destruction.complete(Unit)
        if (!lease.drained.isCompleted) lease.retire()
        cleanup.join()
        reopening.join()
        moduleScope.cancel()
        processScope.cancel()
      }
    }
  }

  @Test fun eraseKeepsItsAdmissionFenceUntilNativeAndRequestDrainComplete() = runBlocking<Unit> {
    withTimeout(4000) {
      val lease = ZoenRequestLease()
      val worker = CompletableDeferred<Unit>()
      val pickup = ZoenSessionPickup(lease, {}, { worker.await() })
      lease.admit()
      pickup.retire(erase = true)
      assertTrue(lease.erasing)
      assertThrows(IllegalStateException::class.java) { lease.admit() }
      var erased = false
      val erasure = async(start = CoroutineStart.UNDISPATCHED) {
        pickup.drain()
        erased = true
      }
      try {
        assertFalse(lease.retire())
        assertFalse(erased)
        worker.complete(Unit)
        erasure.await()
        assertTrue(erased)
        assertFalse("Erase owns destruction rather than request finally", lease.retired.isCompleted)
        lease.destroyed()
      } finally { worker.complete(Unit); erasure.join() }
    }
  }

  @Test fun failedErasureDrainRetainsTheOwnerAndBlocksReopeningItsFiles() = runBlocking<Unit> {
    val process = ZoenProcessLifecycle<ZoenRequestLease>()
    val owner = Any()
    val scope = ZoenDeviceScope("a".repeat(64), "b".repeat(64))
    val lease = ZoenRequestLease()
    val failure = IllegalStateException("native drain failed")
    val pickup = ZoenSessionPickup(lease, {}, { throw failure })
    process.mutex.withLock {
      process.install(owner, process.beginOpen(owner), lease)
      assertSame(lease, process.retire(owner))
      assertTrue("Production retirement must retain the erasing owner", process.retired.contains(lease))
      pickup.retire(erase = true)
    }
    // An erase never waits on its own destruction acknowledgement.
    assertTrue(process.retired.filter { it !== lease }.isEmpty())
    val firstTask = CompletableDeferred<Unit>()
    process.mutex.withLock {
      process.acceptErasure(owner, scope, firstTask, preservingOpening = false)
    }
    val observed = runCatching { pickup.drain() }.exceptionOrNull()
    assertOriginalFailure(failure, observed)
    process.mutex.withLock {
      lease.destructionFailed(requireNotNull(observed))
      process.finishErasure(scope, firstTask, completed = false)
    }
    firstTask.complete(Unit)
    assertTrue(process.retired.contains(lease))
    assertNull(process.current(owner))
    var reopened = false
    val retry = CompletableDeferred<Unit>()
    val blockers = process.mutex.withLock {
      process.acceptErasure(owner, scope, retry, preservingOpening = false)
      process.retired.map { it.retired }
    }
    val retryFailure = runCatching {
      process.withErasureAfterRetirements(scope, retry, blockers) { reopened = true }
    }.exceptionOrNull()
    assertOriginalFailure(failure, retryFailure)
    assertFalse("A failed drain cannot make the same files available to another Client", reopened)
    assertTrue(process.retired.contains(lease))
    process.mutex.withLock { process.finishErasure(scope, retry, completed = false) }
    retry.complete(Unit)
  }

  @Test fun failedNativeDrainCannotAcknowledgeHandleDestruction() = runBlocking<Unit> {
    val lease = ZoenRequestLease()
    val failure = IllegalStateException("native drain failed")
    val pickup = ZoenSessionPickup(lease, {}, { throw failure })
    pickup.retire()
    val observed = runCatching { pickup.drain() }.exceptionOrNull()
    assertOriginalFailure(failure, observed)
    assertFalse(lease.retired.isCompleted)
    lease.destructionFailed(requireNotNull(observed))
    assertOriginalFailure(failure, runCatching { lease.retired.await() }.exceptionOrNull())
  }
}
