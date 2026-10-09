package xyz.tironi.zoen

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.io.IOException
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.core.ConnectionDto
import xyz.tironi.zoen.core.CoreListener
import xyz.tironi.zoen.core.RodaEngine
import xyz.tironi.zoen.data.AndroidSecretVault

@RunWith(AndroidJUnit4::class)
class NativeCallbackThreadTest {
    @Test fun inlineFutureResumptionCanPollRustAgainOnTheNativeCallbackThread() = runBlocking {
        DelayedConfigServer().use { server ->
            val core = RodaEngine.open(":memory:", "en")
            try {
                withTimeout(20_000) {
                    // Unconfined forces the same inline native-thread resumption
                    // observed with Compose's continuation interceptor in CI.
                    withContext(Dispatchers.Unconfined) {
                        repeat(8) {
                            val result = core.growthSync(server.url, false, 0u, 0u)
                            assertEquals(7uL, result.configVersion)
                            assertFalse(result.reported)
                            assertTrue(Thread.currentThread().isDaemon)
                            assertEquals("zoen-native-callback", Thread.currentThread().name)
                        }
                    }
                }
                assertEquals(8, server.requests.get())
            } finally { core.destroy() }
        }
    }

    @Test fun listenerCanSynchronouslyReenterRustAndItsKeystoreCallback() {
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val namespace = "native-callback-${UUID.randomUUID()}"
        val folder = File(context.noBackupFilesDir, namespace).apply { mkdirs() }
        val vault = AndroidSecretVault(context, "$namespace-vault")
        DelayedConfigServer().use { server ->
            val core = RodaEngine.open(File(folder, "core.sqlite").absolutePath, "en")
            val callback = CountDownLatch(1)
            val closing = AtomicBoolean(false)
            val failure = AtomicReference<Throwable?>()
            val callbackThread = AtomicReference<Thread?>()
            val listener = object : CoreListener {
                override fun onChange(spaceIds: List<String>) = Unit
                override fun onEphemeral(spaceId: String, fromId: String, kind: String, detail: String) = Unit
                override fun onPresence(identityId: String, online: Boolean) = Unit
                override fun onError(message: String) = Unit
                override fun onProfileChanged(identityId: String) = Unit
                override fun onConnection(status: ConnectionDto) {
                    if (closing.get()) return
                    try {
                        // This is Rust -> listener -> Rust -> SecretVault -> Java,
                        // with the outer listener's Java frame still on the stack.
                        check(core.unlock(vault))
                        callbackThread.set(Thread.currentThread())
                    } catch (error: Throwable) { failure.compareAndSet(null, error) }
                    finally { callback.countDown() }
                }
            }
            try {
                val owner = core.createAccount("Callback owner", "callback_owner", server.url, vault)
                core.startSync(listener)
                assertTrue("Native CoreListener callback did not complete", callback.await(15, TimeUnit.SECONDS))
                failure.get()?.let { throw AssertionError("Nested listener/vault callback failed", it) }
                assertEquals(owner.identityId, core.me().id)
                assertTrue(callbackThread.get()!!.isDaemon)
                assertEquals("zoen-native-callback", callbackThread.get()!!.name)
            } finally {
                closing.set(true)
                core.stopSync()
                core.eraseDevice(vault)
                core.destroy()
                folder.deleteRecursively()
                File(context.noBackupFilesDir, "$namespace-vault").deleteRecursively()
            }
        }
    }

    private class DelayedConfigServer : AutoCloseable {
        private val server = ServerSocket(0, 8, InetAddress.getByName("127.0.0.1"))
        private val running = AtomicBoolean(true)
        private val active = AtomicReference<Socket?>()
        val url = "http://127.0.0.1:${server.localPort}"
        val requests = AtomicInteger()
        private val worker = Thread({
            while (running.get()) {
                try {
                    server.accept().use { socket ->
                        active.set(socket)
                        socket.soTimeout = 5_000
                        val input = socket.getInputStream().bufferedReader()
                        while (!input.readLine().isNullOrEmpty()) Unit
                        requests.incrementAndGet()
                        Thread.sleep(75)
                        val body = "{\"version\":7}"
                        val response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: ${body.length}\r\nConnection: close\r\n\r\n$body"
                        socket.getOutputStream().write(response.toByteArray(Charsets.UTF_8))
                        socket.getOutputStream().flush()
                    }
                } catch (_: IOException) {
                    if (!running.get()) break
                } catch (_: InterruptedException) { break }
                finally { active.set(null) }
            }
        }, "native-callback-http-test").apply { isDaemon = true; start() }

        override fun close() {
            running.set(false)
            server.close()
            active.get()?.close()
            worker.interrupt()
            worker.join(3_000)
            check(!worker.isAlive) { "Native callback test server did not stop" }
        }
    }
}
