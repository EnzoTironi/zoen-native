package xyz.tironi.zoen.core

import java.nio.file.Files
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ZoenDeviceErasureTest {
  @get:Rule val temporary = TemporaryFolder()
  private val scope = ZoenDeviceScope("a".repeat(64), "b".repeat(64))

  @Test fun interruptedErasureRetainsTheExactScopeAcrossRestart() {
    val root = temporary.newFolder()
    ZoenDeviceErasure(root).begin(scope)
    assertEquals(scope, ZoenDeviceErasure(root).pending())
    ZoenDeviceErasure(root).begin(scope)
    ZoenDeviceErasure(root).complete()
    assertNull(ZoenDeviceErasure(root).pending())
  }

  @Test fun anotherAccountCannotReplaceAnAcceptedErasure() {
    val root = temporary.newFolder()
    val erasure = ZoenDeviceErasure(root)
    erasure.begin(scope)
    assertThrows(IllegalStateException::class.java) {
      erasure.begin(ZoenDeviceScope("c".repeat(64), "d".repeat(64)))
    }
    assertEquals(scope, erasure.pending())
  }

  @Test fun malformedOrOversizedIntentsNeverBecomeAnAbsentIntent() {
    val root = temporary.newFolder()
    val marker = root.resolve("zoen-erase-device-v1.txt")
    for (value in listOf("", "2\n${scope.identityId}\n${scope.deviceId}\n", "x".repeat(257), "1\ninvalid\ninvalid\n")) {
      marker.writeText(value)
      assertThrows(RuntimeException::class.java) { ZoenDeviceErasure(root).pending() }
      assertTrue(marker.exists())
    }
  }

  @Test fun erasingOwnedFilesDoesNotFollowLinksToOtherFiles() {
    val root = temporary.newFolder()
    val outside = temporary.newFile().apply { writeText("Keep this file") }
    Files.createSymbolicLink(root.resolve("link").toPath(), outside.toPath())
    root.resolve("nested").mkdir()
    root.resolve("nested/private").writeText("Remove this file")
    ZoenDeviceErasure.eraseDirectory(root)
    assertFalse(root.exists())
    assertEquals("Keep this file", outside.readText())
    ZoenDeviceErasure.eraseDirectory(root)
  }

  @Test fun erasureFencesAdmissionAndWaitsForEveryAcceptedLease() {
    val lease = ZoenRequestLease()
    lease.admit()
    lease.admit()
    assertFalse(lease.close(erase = true))
    assertThrows(IllegalStateException::class.java) { lease.admit() }
    assertFalse(lease.retire())
    assertFalse(lease.drained.isCompleted)
    assertFalse(lease.retire())
    assertTrue(lease.drained.isCompleted)
    assertTrue(lease.erasing)
  }

  @Test fun ordinaryCloseDestroysOnlyAfterTheLastAcceptedLease() {
    val lease = ZoenRequestLease()
    lease.admit()
    assertFalse(lease.close())
    assertTrue(lease.retire())
    assertTrue(lease.drained.isCompleted)
    assertFalse(lease.retired.isCompleted)
    lease.destroyed()
    assertTrue(lease.retired.isCompleted)
  }
}
