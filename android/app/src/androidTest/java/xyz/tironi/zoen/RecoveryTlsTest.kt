package xyz.tironi.zoen

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.io.IOException
import java.net.InetAddress
import java.security.KeyFactory
import java.security.KeyStore
import java.security.cert.CertificateFactory
import java.security.spec.PKCS8EncodedKeySpec
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import javax.net.ssl.KeyManagerFactory
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLServerSocket
import javax.net.ssl.SSLSocket
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.core.CoreException
import xyz.tironi.zoen.core.RodaEngine
import xyz.tironi.zoen.data.AndroidSecretVault

@RunWith(AndroidJUnit4::class)
class RecoveryTlsTest {
    @Test fun restoreBeforeSyncRejectsAnUntrustedTlsServerWithoutPanicOrAccountMutation() = runBlocking {
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val assets = InstrumentationRegistry.getInstrumentation().context.assets
        // This public test-only key has no role in an account or production service.
        val certificate = assets.open("untrusted-loopback-test-cert.der").use {
            CertificateFactory.getInstance("X.509").generateCertificate(it)
        }
        val privateKey = assets.open("untrusted-loopback-test-key.pk8").use {
            KeyFactory.getInstance("RSA").generatePrivate(PKCS8EncodedKeySpec(it.readBytes()))
        }
        val password = "loopback-test".toCharArray()
        val store = KeyStore.getInstance(KeyStore.getDefaultType()).apply {
            load(null)
            setKeyEntry("loopback", privateKey, password, arrayOf(certificate))
        }
        val managers = KeyManagerFactory.getInstance(KeyManagerFactory.getDefaultAlgorithm()).apply { init(store, password) }
        val tls = SSLContext.getInstance("TLS").apply { init(managers.keyManagers, null, null) }
        val server = tls.serverSocketFactory.createServerSocket(0, 1, InetAddress.getByName("127.0.0.1")) as SSLServerSocket
        server.soTimeout = 10_000
        val executor = Executors.newSingleThreadExecutor()
        val handshake = executor.submit<Boolean> {
            (server.accept() as SSLSocket).use { socket ->
                socket.soTimeout = 5_000
                try { socket.startHandshake(); false }
                catch (_: IOException) { true }
            }
        }
        val namespace = "tls-recovery-${UUID.randomUUID()}"
        val folder = File(context.noBackupFilesDir, namespace).apply { mkdirs() }
        val vault = AndroidSecretVault(context, "$namespace-vault")
        val core = RodaEngine.open(File(folder, "account.sqlite").absolutePath, "en")
        try {
            assertNull(core.account())
            try {
                withTimeout(15_000) {
                    core.restoreBackup("https://127.0.0.1:${server.localPort}", "tls_recovery_probe", "not a backup key", vault)
                }
                fail("An untrusted certificate must not be accepted")
            } catch (refused: CoreException.Invalid) {
                assertTrue(refused.reason, refused.reason.contains("Can't reach the server right now."))
            }
            assertTrue("The actual TLS handshake must be rejected", handshake.get(10, TimeUnit.SECONDS))
            assertNull(core.account())
            Evidence.outputFile("recovery", "untrusted-tls-receipt.txt").writeText(
                "PASS: restore before sync reached a real loopback TLS server, rejected its untrusted certificate, " +
                    "returned the typed error and left the device without an account.\n"
            )
        } finally {
            server.close()
            executor.shutdownNow()
            core.eraseDevice(vault)
            core.destroy()
            folder.deleteRecursively()
            File(context.noBackupFilesDir, "$namespace-vault").deleteRecursively()
        }
    }
}
