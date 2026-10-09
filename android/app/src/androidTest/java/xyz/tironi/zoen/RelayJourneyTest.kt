package xyz.tironi.zoen

import android.graphics.Bitmap
import android.graphics.Color
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.UUID
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AndroidSecretVault

@RunWith(AndroidJUnit4::class)
class RelayJourneyTest {
    @Test fun independentAndroidIdentitiesExchangeEncryptedMediaAndRecoverOfflineOutbox() = runBlocking {
        val relay = InstrumentationRegistry.getArguments().getString("zoenRelay")
        assumeTrue("Run with zoenRelay pointing to the isolated test relay", !relay.isNullOrBlank())
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val suffix = UUID.randomUUID().toString().replace("-", "").take(10)
        val folder = File(context.noBackupFilesDir, "relay-test-$suffix").apply { mkdirs() }
        val vaultA = AndroidSecretVault(context, "relay-vault-a-$suffix")
        val vaultB = AndroidSecretVault(context, "relay-vault-b-$suffix")
        val pathA = File(folder, "a.sqlite").absolutePath
        var a = RodaEngine.open(pathA, "en")
        val b = RodaEngine.open(File(folder, "b.sqlite").absolutePath, "en")
        try {
            val accountA = a.createAccount("Android Alice", "aa_$suffix", relay!!, vaultA)
            val accountB = b.createAccount("Android Bob", "ab_$suffix", relay, vaultB)
            assertNotEquals(accountA.identityId, accountB.identityId)
            assertNotEquals(accountA.deviceId, accountB.deviceId)
            a.startSync(null); b.startSync(null)
            await { a.account()?.registered == true && b.account()?.registered == true && a.connection().synced && b.connection().synced }
            val found = a.findPeople(accountB.handle).single { it.id == accountB.identityId }
            val chat = a.startDirect(found.id)
            await { b.spaces().any { it.id == chat } && a.groupKeys(chat) != null && b.groupKeys(chat) != null }
            a.sendMessage(chat, "Encrypted hello from Android Alice")
            await { b.timeline(chat).any { (it.kind as? EntryKind.Message)?.text == "Encrypted hello from Android Alice" } }
            b.sendMessage(chat, "Encrypted reply from Android Bob")
            await { a.timeline(chat).any { (it.kind as? EntryKind.Message)?.text == "Encrypted reply from Android Bob" } }

            val payload = ByteArray(70_000) { (it * 31 % 251).toByte() }
            val attachment = a.fileAdd(chat, "", "android-private.bin", "application/octet-stream", payload, null)
            await { b.items().any { it.id == attachment.id && it.file?.ready == true } }
            assertArrayEquals(payload, b.fileBytes(attachment.id, null))
            a.fileNewVersion(attachment.id, "second encrypted version".toByteArray(), null, "Updated on Android")
            await { b.items().any { it.id == attachment.id && it.version == 2u && it.file?.ready == true } }
            assertEquals("second encrypted version", b.fileBytes(attachment.id, null)!!.toString(Charsets.UTF_8))

            val image = Bitmap.createBitmap(32, 32, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.rgb(76, 108, 67)) }
            val photo = ByteArrayOutputStream().use { image.compress(Bitmap.CompressFormat.JPEG, 80, it); it.toByteArray() }
            image.recycle()
            val media = a.putMedia(photo, "image/jpeg", 32u, 32u)
            a.setBackground(chat, BackgroundDto("photo", media, 1500u, 200, -150, 200u, 0u, "auto"))
            await { b.background(chat)?.media?.sha256 == media.sha256 }
            assertArrayEquals(photo, b.waitForBackgroundMedia(chat, 30_000uL))

            a.stopSync()
            a.sendMessage(chat, "Queued while Android is offline")
            assertTrue(a.connection().pending > 0uL)
            a.destroy()
            a = RodaEngine.open(pathA, "en")
            assertTrue(a.unlock(vaultA))
            assertTrue(a.connection().pending > 0uL)
            a.startSync(null)
            await { b.timeline(chat).any { (it.kind as? EntryKind.Message)?.text == "Queued while Android is offline" } }
            assertEquals(0uL, a.waitUntilIdle(30_000uL).pending)
            assertArrayEquals(photo, a.media(media.sha256))
            assertEquals(1500u, a.background(chat)!!.zoomPm)
            assertTrue(a.verifyAll().all { it.valid })
            assertTrue(b.verifyAll().all { it.valid })
        } finally {
            a.stopSync(); b.stopSync()
            a.eraseDevice(vaultA); b.eraseDevice(vaultB)
            a.destroy(); b.destroy()
            folder.deleteRecursively()
            File(context.noBackupFilesDir, "relay-vault-a-$suffix").deleteRecursively()
            File(context.noBackupFilesDir, "relay-vault-b-$suffix").deleteRecursively()
        }
    }

    private suspend fun await(predicate: () -> Boolean) {
        withTimeout(45_000) { while (!predicate()) delay(100) }
    }
}
