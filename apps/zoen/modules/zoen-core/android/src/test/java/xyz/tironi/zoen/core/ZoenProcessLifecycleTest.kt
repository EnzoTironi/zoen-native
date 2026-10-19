package xyz.tironi.zoen.core

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ZoenProcessLifecycleTest {
  @get:Rule val temporary = TemporaryFolder()
  private val scope = ZoenDeviceScope("a".repeat(64), "b".repeat(64))

  @Test fun obsoleteModuleCannotCloseOrAdmitWorkIntoTheReplacementSession() {
    val process = ZoenProcessLifecycle<Any>()
    val oldOwner = Any()
    val newOwner = Any()
    process.install(oldOwner, process.beginOpen(oldOwner), Any())
    val newTicket = process.beginOpen(newOwner)
    assertNotNull(process.retire(newOwner, allOwners = true))
    process.cancelOpening(oldOwner)
    assertTrue(process.isOpening(newOwner, newTicket))
    val replacement = Any()
    process.install(newOwner, newTicket, replacement)
    assertNull(process.retire(oldOwner))
    assertThrows(IllegalArgumentException::class.java) { process.admit(oldOwner) }
    assertThrows(IllegalStateException::class.java) { process.assertOwnerOrEmpty(oldOwner) }
    assertSame(replacement, process.admit(newOwner))
  }

  @Test fun anOlderSuspendedOpenCannotPublishAfterTheReplacementStartsOpening() {
    val process = ZoenProcessLifecycle<Any>()
    val oldOwner = Any()
    val newOwner = Any()
    val oldTicket = process.beginOpen(oldOwner)
    val newTicket = process.beginOpen(newOwner)
    assertThrows(IllegalStateException::class.java) { process.install(oldOwner, oldTicket, Any()) }
    process.cancelOpening(oldOwner)
    assertTrue(process.isOpening(newOwner, newTicket))
  }

  @Test fun anotherModuleCannotResumeTheMarkerWhileTheAcceptedEraserStillOwnsIt() {
    val root = temporary.newFolder()
    val vault = root.resolve("keys").apply { mkdirs() }
    val encrypted = byteArrayOf(17, 23, 41, 61)
    vault.resolve("ciphertext").writeBytes(encrypted)
    val erasure = ZoenDeviceErasure(root)
    val process = ZoenProcessLifecycle<Any>()
    val oldOwner = Any()
    val newOwner = Any()
    process.install(oldOwner, process.beginOpen(oldOwner), Any())
    erasure.begin(scope)
    process.retire(oldOwner)
    val oldTask = CompletableDeferred<Unit>()
    process.acceptErasure(oldOwner, scope, oldTask, preservingOpening = false)

    // This is the overlap between old OnDestroy and a replacement React context.
    process.cancelOpening(oldOwner)
    assertEquals(scope, ZoenDeviceErasure(root).pending())
    assertThrows(IllegalStateException::class.java) { process.beginOpen(newOwner) }
    assertArrayEquals(encrypted, vault.resolve("ciphertext").readBytes())
    assertSame(oldTask, process.erasureTask)

    ZoenDeviceErasure.eraseDirectory(vault)
    erasure.complete()
    process.finishErasure(scope, oldTask, completed = true)
    oldTask.complete(Unit)
    val ticket = process.beginOpen(newOwner)
    vault.mkdirs()
    val replacementCiphertext = byteArrayOf(3, 5, 7, 11)
    vault.resolve("ciphertext").writeBytes(replacementCiphertext)
    val replacement = Any()
    process.install(newOwner, ticket, replacement)

    assertThrows(IllegalStateException::class.java) {
      process.finishErasure(scope, oldTask, completed = true)
    }
    assertNull(process.retire(oldOwner))
    assertSame(replacement, process.current(newOwner))
    assertArrayEquals(replacementCiphertext, vault.resolve("ciphertext").readBytes())
  }

  @Test fun restartRecoveryPreservesOnlyItsOwnOpeningTicket() {
    val process = ZoenProcessLifecycle<Any>()
    val recoveringOwner = Any()
    val oldOwner = Any()
    val ticket = process.beginOpen(recoveringOwner)
    val task = CompletableDeferred<Unit>()
    process.acceptErasure(recoveringOwner, scope, task, preservingOpening = true)
    process.cancelOpening(oldOwner)
    assertThrows(IllegalStateException::class.java) { process.beginOpen(oldOwner) }
    process.finishErasure(scope, task, completed = true)
    task.complete(Unit)
    assertTrue(process.isOpening(recoveringOwner, ticket))
    process.install(recoveringOwner, ticket, Any())
    assertNull(process.completedErasure)
  }

  @Test fun aCompletedCloseStillAllowsOnlyTheExactPersistedAccountToBeErased() {
    val root = temporary.newFolder()
    val vault = root.resolve("keys").apply { mkdirs() }
    val bytes = byteArrayOf(13, 19, 29)
    vault.resolve("ciphertext").writeBytes(bytes)
    val erasure = ZoenDeviceErasure(root)
    val process = ZoenProcessLifecycle<Any>()
    val owner = Any()
    process.install(owner, process.beginOpen(owner), Any())
    process.retire(owner)
    assertNull(erasure.pending())

    val other = ZoenDeviceScope("c".repeat(64), "d".repeat(64))
    assertThrows(IllegalStateException::class.java) { requireErasureScope(other, scope) }
    assertThrows(IllegalStateException::class.java) { requireErasureScope(null, scope) }
    assertNull(erasure.pending())
    assertArrayEquals(bytes, vault.resolve("ciphertext").readBytes())

    requireErasureScope(scope, scope)
    erasure.begin(scope)
    ZoenDeviceErasure.eraseDirectory(vault)
    // A retry after canonical erasure may see an empty account only with this exact intent.
    requireErasureScope(null, scope, allowEmpty = erasure.pending() == scope)
    erasure.complete()
    assertNull(erasure.pending())
    assertFalse(vault.exists())
  }

  @Test fun aStaleEraserCompletionCannotClearAnotherAcceptedTask() {
    val process = ZoenProcessLifecycle<Any>()
    val owner = Any()
    val first = CompletableDeferred<Unit>()
    process.acceptErasure(owner, scope, first, preservingOpening = false)
    process.finishErasure(scope, first, completed = false)
    val second = CompletableDeferred<Unit>()
    process.acceptErasure(owner, scope, second, preservingOpening = false)
    assertThrows(IllegalStateException::class.java) { process.finishErasure(scope, first, completed = true) }
    assertSame(second, process.erasureTask)
    process.finishErasure(scope, second, completed = false)
    first.complete(Unit)
    second.complete(Unit)
  }

  @Test fun closedSessionErasureDoesNotReopenFilesUntilTheOldHandleIsDestroyed() = runBlocking<Unit> {
    val root = temporary.newFolder()
    val ciphertext = root.resolve("ciphertext").apply { writeBytes(byteArrayOf(2, 7, 19)) }
    val original = ciphertext.readBytes()
    val process = ZoenProcessLifecycle<Any>()
    val owner = Any()
    val lease = ZoenRequestLease()
    lease.admit()
    assertFalse(lease.close())
    val erasure = CompletableDeferred<Unit>()
    process.acceptErasure(owner, scope, erasure, preservingOpening = false)
    var opened = false
    val opening = async(start = CoroutineStart.UNDISPATCHED) {
      process.withErasureAfterRetirements(scope, erasure, listOf(lease.retired)) {
        opened = true
        ciphertext.readBytes()
      }
    }
    assertFalse(opened)
    assertFalse(opening.isCompleted)
    assertTrue(lease.retire())
    assertTrue(lease.drained.isCompleted)
    assertFalse(lease.retired.isCompleted)
    assertFalse(opened)
    lease.destroyed()
    assertArrayEquals(original, opening.await())
    assertTrue(opened)
    process.finishErasure(scope, erasure, completed = false)
    erasure.complete(Unit)
  }

  @Test fun staleOwnerCannotCommandeerAnotherModulesPendingOpen() {
    val process = ZoenProcessLifecycle<Any>()
    val owner = Any()
    val replacement = Any()
    val ticket = process.beginOpen(replacement)
    assertThrows(IllegalStateException::class.java) { process.assertOwnerOrEmpty(owner) }
    assertTrue(process.isOpening(replacement, ticket))
  }
}
