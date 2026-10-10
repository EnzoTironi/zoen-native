package xyz.tironi.zoen

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
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
class RecoveryJourneyTest {
    @Test fun recoveryEnrollsANewDeviceThenLinksHistoryAndSurvivesKeystoreReopening() = runBlocking {
        val relay = InstrumentationRegistry.getArguments().getString("zoenRelay")
        assumeTrue("Requires the isolated test relay", !relay.isNullOrBlank())
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val suffix = UUID.randomUUID().toString().replace("-", "").take(10)
        val folder = File(context.noBackupFilesDir, "recovery-$suffix").apply { mkdirs() }
        val engines = mutableMapOf<String, RodaEngine>()
        val vaults = mutableMapOf<String, AndroidSecretVault>()
        fun open(name: String): RodaEngine {
            vaults.getOrPut(name) { AndroidSecretVault(context, "recovery-$suffix-$name-vault") }
            return RodaEngine.open(File(folder, "$name.sqlite").absolutePath, "en").also { engines[name] = it }
        }
        fun close(name: String) {
            engines.remove(name)?.let { it.stopSync(); it.destroy() }
        }
        var failure: Throwable? = null
        try {
            val a = open("a")
            val b = open("b")
            val account = a.createAccount("Recovery Alice", "ra_$suffix", relay!!, vaults.getValue("a"))
            b.createAccount("Recovery Bob", "rb_$suffix", relay, vaults.getValue("b"))
            a.startSync(null); b.startSync(null)
            await { a.account()?.registered == true && b.account()?.registered == true && a.connection().synced && b.connection().synced }
            val chat = b.startDirect(b.findPeople(account.handle).single { it.id == account.identityId }.id)
            await { a.groupKeys(chat) != null && b.groupKeys(chat) != null }
            b.sendMessage(chat, "Encrypted history before losing the phone")
            await { a.hasMessage(chat, "Encrypted history before losing the phone") }
            val secret = a.backupTurnOnRecoveryKey(vaults.getValue("a"))
            assertTrue(a.backupStatus().enabled)
            assertTrue(a.backupStatus().lastBackupMs > 0)
            close("a")

            var recovered = open("recovered")
            val restored = recovered.restoreBackup(relay, account.handle, secret, vaults.getValue("recovered"))
            assertEquals(account.identityId, restored.identityId)
            assertNotEquals(account.deviceId, restored.deviceId)
            assertTrue(recovered.hasMessage(chat, "Encrypted history before losing the phone"))
            recovered.startSync(null)
            await { recovered.connection().synced && recovered.groupKeys(chat) != null }
            b.sendMessage(chat, "Encrypted message after recovery")
            await { recovered.hasMessage(chat, "Encrypted message after recovery") }
            recovered.sendMessage(chat, "The recovered Android device replies")
            await { b.hasMessage(chat, "The recovered Android device replies") }
            close("recovered")
            recovered = open("recovered")
            assertTrue(recovered.unlock(vaults.getValue("recovered")))
            assertTrue(recovered.hasMessage(chat, "Encrypted message after recovery"))
            recovered.startSync(null)
            await { recovered.connection().synced }

            val linked = open("linked")
            val request = linked.linkRequest(relay, vaults.getValue("linked"))
            val accepted = recovered.linkDevice(request.code, vaults.getValue("recovered"))
            assertEquals(request.check, accepted.check)
            val linkedAccount = linked.linkWait(vaults.getValue("linked"), 30_000uL)
            assertEquals(account.identityId, linkedAccount.identityId)
            assertNotEquals(restored.deviceId, linkedAccount.deviceId)
            linked.startSync(null)
            withTimeout(30_000) {
                while (true) {
                    val progress = recovered.linkProgress(accepted.deviceId)
                    if (progress.size == 2 && progress[1] > 0u && progress[0] == progress[1]) break
                    delay(50)
                }
            }
            assertTrue(recovered.sendHistory(accepted.deviceId, null) > 0u)
            assertTrue(linked.receiveHistory(30_000uL, null).messages > 0u)
            assertTrue(linked.hasMessage(chat, "Encrypted history before losing the phone"))
            assertTrue(linked.hasMessage(chat, "Encrypted message after recovery"))
            linked.sendMessage(chat, "The linked Android device replies")
            await { b.hasMessage(chat, "The linked Android device replies") }
            close("linked")
            val reopened = open("linked")
            assertTrue(reopened.unlock(vaults.getValue("linked")))
            assertTrue(reopened.hasMessage(chat, "Encrypted history before losing the phone"))
            assertTrue(reopened.verifyAll().all { it.valid })
            assertTrue(recovered.verifyAll().all { it.valid })
            assertTrue(b.verifyAll().all { it.valid })
            recovered.unlinkDevice(account.deviceId)
            recovered.backupTurnOff(vaults.getValue("recovered"))
            assertFalse(recovered.backupStatus().enabled)
            Evidence.outputFile("recovery", "recovery-link-receipt.txt").writeText(
                "PASS: fresh recovery device, encrypted history, bidirectional peer messages, " +
                    "Keystore reopen, linked history, linked peer messages, old device unlink and backup deletion.\n"
            )
        } catch (caught: Throwable) {
            failure = caught
            throw caught
        } finally {
            var cleanupFailure: Throwable? = null
            engines.forEach { (name, engine) ->
                try { engine.stopSync(); engine.eraseDevice(vaults.getValue(name)) }
                catch (caught: Throwable) { cleanupFailure?.addSuppressed(caught) ?: run { cleanupFailure = caught } }
                finally { engine.destroy() }
            }
            folder.deleteRecursively()
            vaults.keys.forEach { File(context.noBackupFilesDir, "recovery-$suffix-$it-vault").deleteRecursively() }
            cleanupFailure?.let { caught -> failure?.addSuppressed(caught) ?: throw caught }
        }
    }

    private fun RodaEngine.hasMessage(chat: String, text: String): Boolean =
        timeline(chat).any { (it.kind as? EntryKind.Message)?.text == text }

    private suspend fun await(condition: () -> Boolean) = withTimeout(30_000) {
        while (!condition()) delay(50)
    }
}
