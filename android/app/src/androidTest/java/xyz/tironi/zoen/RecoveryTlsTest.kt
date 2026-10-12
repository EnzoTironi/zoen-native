package xyz.tironi.zoen

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.net.InetAddress
import java.security.KeyFactory
import java.security.KeyStore
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.security.spec.PKCS8EncodedKeySpec
import java.util.Locale
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import javax.net.ssl.KeyManagerFactory
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLHandshakeException
import javax.net.ssl.SSLServerSocket
import javax.net.ssl.SSLSocket
import javax.net.ssl.X509TrustManager
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
        server.enabledProtocols = arrayOf("TLSv1.2")
        server.enabledCipherSuites = arrayOf("TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256")
        val executor = Executors.newSingleThreadExecutor()
        val handshake = executor.submit<SSLHandshakeException?> {
            (server.accept() as SSLSocket).use { socket ->
                socket.soTimeout = 5_000
                socket.startHandshake()
            }
            (server.accept() as SSLSocket).use { socket ->
                socket.soTimeout = 5_000
                try { socket.startHandshake(); null }
                catch (refused: SSLHandshakeException) { refused }
            }
        }
        val namespace = "tls-recovery-${UUID.randomUUID()}"
        val folder = File(context.noBackupFilesDir, namespace).apply { mkdirs() }
        val vault = AndroidSecretVault(context, "$namespace-vault")
        val core = RodaEngine.open(File(folder, "account.sqlite").absolutePath, "en")
        try {
            val fixtureTrust = object : X509TrustManager {
                override fun getAcceptedIssuers(): Array<X509Certificate> = arrayOf(certificate as X509Certificate)
                override fun checkClientTrusted(chain: Array<out X509Certificate>, authType: String) = error("No client certificate")
                override fun checkServerTrusted(chain: Array<out X509Certificate>, authType: String) {
                    require(chain.size == 1 && chain[0].encoded.contentEquals(certificate.encoded))
                    chain[0].checkValidity()
                }
            }
            val control = SSLContext.getInstance("TLS").apply { init(null, arrayOf(fixtureTrust), null) }
            (control.socketFactory.createSocket("127.0.0.1", server.localPort) as SSLSocket).use { socket ->
                socket.soTimeout = 5_000
                socket.enabledProtocols = arrayOf("TLSv1.2")
                socket.sslParameters = socket.sslParameters.apply { endpointIdentificationAlgorithm = "HTTPS" }
                socket.startHandshake()
            }
            assertNull(core.account())
            try {
                withTimeout(15_000) {
                    core.restoreBackup("https://127.0.0.1:${server.localPort}", "tls_recovery_probe", "not a backup key", vault)
                }
                fail("An untrusted certificate must not be accepted")
            } catch (refused: CoreException.Invalid) {
                assertTrue(refused.reason, refused.reason.contains("Can't reach the server right now."))
            }
            // Android 9 wraps the native alert in a generic "Handshake failed" exception.
            val causes = generateSequence<Throwable>(handshake.get(10, TimeUnit.SECONDS)) { it.cause }
                .take(8).toList()
            val diagnostics = causes.joinToString("\n") {
                "${it.javaClass.name}: ${it.message.orEmpty().take(2_048)}"
            }.ifEmpty { "TLS handshake succeeded" }
            val certificateAlert = causes.any {
                val message = it.message.orEmpty().take(2_048).lowercase(Locale.ROOT).replace('_', ' ')
                "alert" in message && (
                    "unknown ca" in message || "bad certificate" in message ||
                        "certificate unknown" in message || "unsupported certificate" in message
                    )
            }
            assertTrue("The tested TLS fixture must receive a certificate-rejection alert:\n$diagnostics", certificateAlert)
            assertNull(core.account())
            Evidence.outputFile("recovery", "untrusted-tls-receipt.txt").writeText(
                "PASS: restore before sync reached a real loopback TLS server, rejected its untrusted certificate, " +
                    "returned the typed error and left the device without an account.\n" +
                    "Fixture handshake:\n$diagnostics\n"
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
