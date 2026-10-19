package xyz.tironi.zoen.core

import java.io.File
import java.io.FileOutputStream
import java.nio.channels.FileChannel
import java.nio.file.FileVisitResult
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.NoSuchFileException
import java.nio.file.Path
import java.nio.file.SimpleFileVisitor
import java.nio.file.StandardCopyOption
import java.nio.file.StandardOpenOption
import java.nio.file.attribute.BasicFileAttributes
import kotlinx.coroutines.CompletableDeferred

internal data class ZoenDeviceScope(val identityId: String, val deviceId: String) {
  init {
    require(listOf(identityId, deviceId).all { it.matches(Regex("[0-9a-f]{64}")) }) {
      "The account changed. Open the confirmation again."
    }
  }
}

/** A durable operation intent outside the account directory, containing no keys or content. */
internal class ZoenDeviceErasure(private val root: File) {
  private val marker = File(root, "zoen-erase-device-v1.txt").toPath()

  fun pending(): ZoenDeviceScope? {
    val attributes = try {
      Files.readAttributes(marker, BasicFileAttributes::class.java, LinkOption.NOFOLLOW_LINKS)
    } catch (_: NoSuchFileException) { return null }
    check(attributes.isRegularFile && attributes.size() <= 256) { "Device sign-out needs a retry." }
    val bytes = Files.newInputStream(marker).use { input ->
      val result = java.io.ByteArrayOutputStream()
      val buffer = ByteArray(257)
      while (true) {
        val count = input.read(buffer)
        if (count < 0) break
        result.write(buffer, 0, count)
        check(result.size() <= 256) { "Device sign-out needs a retry." }
      }
      result.toByteArray()
    }
    val lines = bytes.toString(Charsets.US_ASCII).split('\n')
    check(lines.size == 4 && lines[0] == "1" && lines[3].isEmpty()) { "Device sign-out needs a retry." }
    return ZoenDeviceScope(lines[1], lines[2])
  }

  fun begin(scope: ZoenDeviceScope) {
    val pending = pending()
    check(pending == null || pending == scope) { "The account changed. Open the confirmation again." }
    Files.createDirectories(root.toPath())
    val temporary = File(root, "zoen-erase-device-v1.pending").toPath()
    FileOutputStream(temporary.toFile()).use {
      it.write("1\n${scope.identityId}\n${scope.deviceId}\n".toByteArray(Charsets.US_ASCII))
      it.fd.sync()
    }
    Files.move(temporary, marker, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
    syncRoot()
  }

  fun complete() {
    Files.deleteIfExists(marker)
    syncRoot()
    check(pending() == null) { "Device sign-out needs a retry." }
  }

  private fun syncRoot() {
    FileChannel.open(root.toPath(), StandardOpenOption.READ).use { it.force(true) }
  }

  companion object {
    /** Does not follow symlinks; only directories owned by this app are passed here. */
    fun eraseDirectory(directory: File) {
      try {
        Files.walkFileTree(directory.toPath(), object : SimpleFileVisitor<Path>() {
          override fun visitFile(file: Path, attributes: BasicFileAttributes): FileVisitResult {
            Files.delete(file)
            return FileVisitResult.CONTINUE
          }
          override fun postVisitDirectory(directory: Path, error: java.io.IOException?): FileVisitResult {
            if (error != null) throw error
            Files.delete(directory)
            return FileVisitResult.CONTINUE
          }
        })
      } catch (_: NoSuchFileException) { /* Already removed by an earlier completed stage. */ }
      check(!Files.exists(directory.toPath(), LinkOption.NOFOLLOW_LINKS)) { "Device sign-out needs a retry." }
    }
  }
}

/** Calls are serialized by the module's lifecycle mutex. */
internal class ZoenRequestLease {
  private var requests = 0
  var closing = false
    private set
  var erasing = false
    private set
  val drained = CompletableDeferred<Unit>()
  val retired = CompletableDeferred<Unit>()

  fun admit() {
    check(!closing) { "The app session has closed" }
    requests += 1
  }

  fun close(erase: Boolean = false): Boolean {
    closing = true
    erasing = erasing || erase
    if (requests == 0) drained.complete(Unit)
    return requests == 0 && !erasing
  }

  fun retire(): Boolean {
    check(requests > 0)
    requests -= 1
    if (closing && requests == 0) drained.complete(Unit)
    return closing && requests == 0 && !erasing
  }

  fun destroyed() {
    check(closing && drained.isCompleted)
    retired.complete(Unit)
  }

  fun destructionFailed(error: Throwable) {
    retired.completeExceptionally(error)
  }
}
