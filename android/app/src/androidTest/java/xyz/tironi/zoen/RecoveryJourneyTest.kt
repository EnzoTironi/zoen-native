package xyz.tironi.zoen

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.TimeoutCancellationException
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
            await("Initial account registration and sync", engines) { a.account()?.registered == true && b.account()?.registered == true && a.connection().synced && b.connection().synced }
            val chat = b.startDirect(b.findPeople(account.handle).single { it.id == account.identityId }.id)
            await("Initial encrypted group", engines) { a.groupKeys(chat) != null && b.groupKeys(chat) != null }
            b.sendMessage(chat, "Encrypted history before losing the phone")
            await("Initial encrypted message", engines) { a.hasMessage(chat, "Encrypted history before losing the phone") }
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
            await("Recovered device sync and group keys", engines) { recovered.connection().synced && recovered.groupKeys(chat) != null }
            b.sendMessage(chat, "Encrypted message after recovery")
            await("Peer message after recovery", engines) { recovered.hasMessage(chat, "Encrypted message after recovery") }
            recovered.sendMessage(chat, "The recovered Android device replies")
            await("Recovered device reply", engines) { b.hasMessage(chat, "The recovered Android device replies") }
            close("recovered")
            recovered = open("recovered")
            assertTrue(recovered.unlock(vaults.getValue("recovered")))
            assertTrue(recovered.hasMessage(chat, "Encrypted message after recovery"))
            recovered.startSync(null)
            await("Recovered device sync after Keystore reopening", engines) { recovered.connection().synced }

            val linked = open("linked")
            val request = linked.linkRequest(relay, vaults.getValue("linked"))
            val accepted = recovered.linkDevice(request.code, vaults.getValue("recovered"))
            assertEquals(request.check, accepted.check)
            val linkedAccount = linked.linkWait(vaults.getValue("linked"), 30_000uL)
            assertEquals(account.identityId, linkedAccount.identityId)
            assertNotEquals(restored.deviceId, linkedAccount.deviceId)
            linked.startSync(null)
            await("Linked device joins recovered groups", engines) {
                val progress = recovered.linkProgress(accepted.deviceId)
                progress.size == 2 && progress[1] > 0u && progress[0] == progress[1]
            }
            assertTrue(recovered.sendHistory(accepted.deviceId, null) > 0u)
            assertTrue(linked.receiveHistory(30_000uL, null).messages > 0u)
            assertTrue(linked.hasMessage(chat, "Encrypted history before losing the phone"))
            assertTrue(linked.hasMessage(chat, "Encrypted message after recovery"))
            linked.sendMessage(chat, "The linked Android device replies")
            await("Linked device reply", engines) { b.hasMessage(chat, "The linked Android device replies") }
            close("linked")
            val reopened = open("linked")
            assertTrue(reopened.unlock(vaults.getValue("linked")))
            assertTrue(reopened.hasMessage(chat, "Encrypted history before losing the phone"))
            assertTrue(reopened.verifyAll().all { it.valid })
            assertTrue(recovered.verifyAll().all { it.valid })
            assertTrue(b.verifyAll().all { it.valid })
            recovered.unlinkDevice(account.deviceId)
            val lost = open("a")
            assertTrue(lost.unlock(vaults.getValue("a")))
            lost.startSync(null)
            await("Lost device revocation", engines) { lost.connection().error?.contains("unlinked") == true }
            assertFalse(lost.connection().synced)
            close("a")
            recovered.backupTurnOff(vaults.getValue("recovered"))
            assertFalse(recovered.backupStatus().enabled)
            val deleted = open("deleted")
            try {
                deleted.restoreBackup(relay, account.handle, secret, vaults.getValue("deleted"))
                fail("The deleted backup must not restore on a fresh device")
            } catch (missing: CoreException.Invalid) {
                // A relay with password recovery disabled uses a refusal instead of a
                // password decoy for a missing vault, so it does not disclose accounts.
                assertTrue(missing.reason, missing.reason.contains("We couldn't find a backup") ||
                    missing.reason == "Password backup isn't available on this server yet.")
            }
            assertNull(deleted.account())
            Evidence.outputFile("recovery", "recovery-link-receipt.txt").writeText(
                "PASS: fresh recovery device, encrypted history, bidirectional peer messages, " +
                    "Keystore reopen, linked history, linked peer messages, old device unlink and backup deletion.\n"
            )
        } catch (caught: Throwable) {
            failure = caught
            runCatching { Evidence.outputFile("recovery", "failure-diagnostics.txt").writeText(caught.stackTraceToString()) }
                .exceptionOrNull()?.let(caught::addSuppressed)
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

    private suspend fun await(stage: String, engines: Map<String, RodaEngine>, condition: () -> Boolean) {
        try {
            withTimeout(30_000) { while (!condition()) delay(50) }
        } catch (timeout: TimeoutCancellationException) {
            val states = engines.entries.joinToString("\n") { (name, engine) ->
                "$name: " + runCatching { engine.connection().toString() }.getOrElse { it.toString() }
            }
            throw AssertionError("$stage did not finish within 30 seconds.\n$states", timeout)
        }
    }
}
