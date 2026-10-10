package xyz.tironi.zoen

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.security.MessageDigest
import java.util.UUID
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.data.AndroidSecretVault

@RunWith(AndroidJUnit4::class)
class SecretVaultTest {
    @Test fun encryptsAuthenticatesAndDeletesSecretsUsingAndroidKeystore() {
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val name = "test:${UUID.randomUUID()}"
        val vault = AndroidSecretVault(context)
        val secret = "a private key must never be written in plaintext".toByteArray()
        try {
            assertTrue(vault.save(name, secret))
            assertArrayEquals(secret, AndroidSecretVault(context).load(name))
            val hash = MessageDigest.getInstance("SHA-256").digest(name.toByteArray()).joinToString("") { "%02x".format(it) }
            val file = File(File(context.noBackupFilesDir, "keys"), hash)
            val ciphertext = file.readBytes()
            assertFalse(ciphertext.toString(Charsets.UTF_8).contains(secret.toString(Charsets.UTF_8)))
            ciphertext[ciphertext.lastIndex] = (ciphertext.last().toInt() xor 1).toByte()
            file.writeBytes(ciphertext)
            assertNull(vault.load(name))
        } finally { vault.delete(name) }
        assertNull(vault.load(name))
    }
}
