package xyz.tironi.zoen

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.util.UUID
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.core.RodaEngine
import xyz.tironi.zoen.core.TrustLevelDto
import xyz.tironi.zoen.data.AndroidSecretVault
import xyz.tironi.zoen.data.LocalPlanner

@RunWith(AndroidJUnit4::class)
class NativeCoreTest {
    @Test fun realAccountAndSignedFilesSurviveReopeningWithKeystoreKeys() {
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val namespace = "test-${UUID.randomUUID()}"
        val folder = File(context.noBackupFilesDir, namespace).apply { mkdirs() }
        val database = File(folder, "account.sqlite").absolutePath
        val vault = AndroidSecretVault(context, "$namespace-vault")
        var core = RodaEngine.open(database, "en")
        try {
            val account = core.createAccount("Android Test", "android_test", "http://10.0.2.2:18787", vault)
            assertNotNull(account)
            val chat = core.spaces().first { it.counterpart?.handle == "zoen" }
            val agent = core.agents().first { it.persona.handle == "zoen" }
            core.setTrust(agent.persona.id, chat.id, TrustLevelDto.ACT)
            val prompt = "A trip up to $1,500"
            val plan = checkNotNull(core.agentCreatePlan(chat.id, agent.persona.id, prompt, LocalPlanner.plan(prompt, "en"), "Local planner", 0).item)
            core.togglePlanLine(plan.id, plan.plan!!.sections.first().lines.first().id)
            val markdown = "# Android note\n\nThis page survives a restart.\n"
            val page = core.pageImportMarkdown(chat.id, "note.md", markdown)
            val bytes = "an offline attachment".toByteArray()
            val file = core.fileAdd(chat.id, "note.txt", "note.txt", "text/plain", bytes, null)
            core.destroy()

            core = RodaEngine.open(database, "en")
            assertTrue(core.unlock(AndroidSecretVault(context, "$namespace-vault")))
            assertTrue(core.items().first { it.id == plan.id }.plan!!.sections.first().lines.first().done)
            assertTrue(core.pageMarkdown(page.id).contains("This page survives a restart."))
            assertArrayEquals(bytes, core.fileBytes(file.id, null))
            assertTrue(core.verifyAll().isNotEmpty())
            assertTrue(core.verifyAll().all { it.valid })
            core.eraseDevice(vault)
            assertNull(core.account())
        } finally {
            core.destroy()
            folder.deleteRecursively()
            File(context.noBackupFilesDir, "$namespace-vault").deleteRecursively()
        }
    }
}
