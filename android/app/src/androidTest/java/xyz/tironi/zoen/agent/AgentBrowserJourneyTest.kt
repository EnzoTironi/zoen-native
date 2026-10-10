package xyz.tironi.zoen.agent

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.ZoenApplication
import xyz.tironi.zoen.core.Persona
import xyz.tironi.zoen.core.PersonaKind
import xyz.tironi.zoen.data.AndroidSecretVault
import xyz.tironi.zoen.theme.ZoenTheme

@RunWith(AndroidJUnit4::class)
class AgentBrowserJourneyTest {
    @get:Rule val compose = createComposeRule()

    @Test fun ownerTakesOverTypesPrivatelyAndOnlyDoneHandsTheSessionBack() {
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val namespace = "browser-${UUID.randomUUID()}"
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        val browser = AgentBrowser(context, scope, AndroidSecretVault(context, namespace))
        val agent = Persona("test-agent", PersonaKind.AGENT, "Zoen", "zoen", "Z", "#6B8F71", null, "", "test-owner", "Owner", null, false, true)
        var closed = 0
        try {
            compose.setContent { ZoenTheme { AgentBrowserScreen(browser) { closed++ } } }
            compose.runOnIdle { browser.startDemo("test-space", agent, "en") }
            compose.waitUntil(10_000) { browser.state.value.phase == BrowserPhase.NeedsYou }
            compose.onNodeWithTag("browser-takeover-screen").performScrollTo().performClick()
            compose.onNodeWithTag("browser-password").performScrollTo().performTextInput("demo-password")
            compose.runOnIdle { assertEquals(BrowserPhase.Driving, browser.state.value.phase); assertEquals(13, browser.state.value.ownership.typed) }
            compose.onNodeWithContentDescription("Back").performClick()
            compose.runOnIdle { assertEquals(1, closed); assertEquals(BrowserPhase.Driving, browser.state.value.phase); assertEquals(0, browser.state.value.ownership.typed) }
            compose.onNodeWithTag("browser-password").performScrollTo().performTextInput("owner-demo")
            compose.onNodeWithTag("browser-done").performScrollTo().performClick()
            compose.runOnIdle { assertEquals(BrowserPhase.Finished, browser.state.value.phase); assertEquals(0, browser.state.value.ownership.typed) }
            compose.onNodeWithTag("browser-back-to-chat").assertExists()
        } finally {
            compose.runOnIdle { browser.close() }
            scope.cancel()
            File(context.noBackupFilesDir, namespace).deleteRecursively()
        }
    }
}
