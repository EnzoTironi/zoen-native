package xyz.tironi.zoen.data

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import java.io.File
import java.security.KeyStore
import java.security.MessageDigest
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import xyz.tironi.zoen.core.SecretVault

/** The Keystore holds the wrapping key; only authenticated ciphertext reaches disk. */
class AndroidSecretVault(context: Context, namespace: String = "keys") : SecretVault {
    private val directory = File(context.noBackupFilesDir, namespace).apply { mkdirs() }
    private val alias = "xyz.tironi.zoen.vault.v1"

    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(alias, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256).build())
            generateKey()
        }
    }

    private fun file(name: String): AtomicFile {
        val hash = MessageDigest.getInstance("SHA-256").digest(name.toByteArray())
            .joinToString("") { "%02x".format(it) }
        return AtomicFile(File(directory, hash))
    }

    @Synchronized
    override fun load(key: String): ByteArray? {
        return try {
            val bytes = file(key).readFully()
            require(bytes.size >= 29 && bytes[0] == 1.toByte())
            Cipher.getInstance("AES/GCM/NoPadding").run {
                init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes.copyOfRange(1, 13)))
                updateAAD(key.toByteArray())
                doFinal(bytes.copyOfRange(13, bytes.size))
            }
        } catch (_: Exception) { null }
    }

    @Synchronized
    fun loadStored(key: String): ByteArray? {
        val target = file(key)
        if (!target.baseFile.exists() && !File(target.baseFile.path + ".bak").exists()) return null
        return load(key) ?: error("The saved draft is locked; it has not been deleted")
    }

    @Synchronized
    fun storedCiphertext(key: String): ByteArray = file(key).readFully()

    @Synchronized
    override fun save(key: String, value: ByteArray): Boolean {
        val target = file(key)
        var stream: java.io.FileOutputStream? = null
        return try {
            val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply {
                init(Cipher.ENCRYPT_MODE, key())
                updateAAD(key.toByteArray())
            }
            val bytes = byteArrayOf(1) + cipher.iv + cipher.doFinal(value)
            stream = target.startWrite()
            stream.write(bytes)
            target.finishWrite(stream)
            true
        } catch (_: Exception) {
            stream?.let(target::failWrite)
            false
        }
    }

    @Synchronized
    override fun delete(key: String) { file(key).delete() }
}
