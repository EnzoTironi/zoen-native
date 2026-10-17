package xyz.tironi.zoen.ui

import android.content.res.Configuration
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.CommittedWindowCapture
import xyz.tironi.zoen.core.AgentProfile
import xyz.tironi.zoen.core.AgentSpaceTrust
import xyz.tironi.zoen.core.AgentStatus
import xyz.tironi.zoen.core.Persona
import xyz.tironi.zoen.core.PersonaKind
import xyz.tironi.zoen.core.TrustLevelDto
import xyz.tironi.zoen.theme.AppAppearance
import xyz.tironi.zoen.theme.ZoenTheme
import java.util.Locale

/** Seus agentes and an agent's profile, in plain Portuguese, light and dark. */
@RunWith(AndroidJUnit4::class)
class AgentCitizenTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    private fun persona(id: String, name: String, mine: Boolean, owner: String? = if (mine) null else "Bruno") =
        Persona(id, PersonaKind.AGENT, name, id, name.take(1), "#19B5A5", null, "", owner?.lowercase(), owner, null,
            photoSha256 = null, photoReady = false, photoMime = null, isMe = false, isMine = mine)

    private fun profile(p: Persona, pending: UInt = 0u, chats: Int = 1) = AgentProfile(
        p, null, null, false,
        List(chats) { AgentSpaceTrust("s$it", "Conversa $it", TrustLevelDto.SUGGEST) },
        0u, 0u, "", pending,
    )

    private val agenda = profile(persona("agenda", "Agenda do Enzo", mine = true), pending = 2u, chats = 3)
    private val corte = profile(persona("corte", "Corte do Bruno", mine = false))

    @After fun resetTheme() { AppAppearance.set(compose.activity, "auto") }

    /** setContent runs once per test; the theme flips live through AppAppearance. */
    private fun show(content: @androidx.compose.runtime.Composable () -> Unit) {
        AppAppearance.set(compose.activity, "light")
        compose.setContent {
            val conf = Configuration(LocalConfiguration.current).apply { setLocale(Locale.forLanguageTag("pt-BR")) }
            val ctx = LocalContext.current.createConfigurationContext(conf)
            CompositionLocalProvider(LocalContext provides ctx, LocalConfiguration provides conf) {
                ZoenTheme { Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) { content() } }
            }
        }
        compose.waitForIdle()
    }

    @Test fun listShowsStatusPendingNotesAndOwner() {
        val opened = mutableListOf<String>()
        show {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    AgentCitizenRow(agenda, AgentStatus.ACTIVE) { opened += "agenda" }
                    AgentCitizenRow(profile(persona("loja", "Loja", mine = true)), AgentStatus.PAUSED) { opened += "loja" }
                    AgentCitizenRow(corte, null) { opened += "corte" }
                }
        }
        for (dark in listOf(false, true)) {
            AppAppearance.set(compose.activity, if (dark) "dark" else "light")
            compose.waitForIdle()
            compose.onNodeWithText("Ativo").assertIsDisplayed()
            compose.onNodeWithText("Pausado").assertIsDisplayed()
            compose.onNodeWithText("2 esperando você").assertIsDisplayed()
            compose.onNodeWithText("Em 3 conversas").assertIsDisplayed()
            compose.onNodeWithText("De Bruno · Em 1 conversa").assertIsDisplayed()
            CommittedWindowCapture.save(compose.activity, "agents", "agents-list-${if (dark) "dark" else "light"}")
            compose.onNodeWithTag("agent-row-corte").performClick()
            compose.runOnIdle { assertEquals("corte", opened.last()) }
        }
    }

    @Test fun profileTalksSharesPausesAndAsksBeforeTurningOff() {
        var status by mutableStateOf(AgentStatus.ACTIVE)
        val did = mutableListOf<String>()
        show {
                Column(Modifier.padding(16.dp)) {
                    AgentCitizenActions(
                        agenda, status, "https://tryzoen.com/a/abc", busy = false,
                        onChat = { did += "chat" }, onShare = { did += "share:$it" },
                        onPauseToggle = { pause -> did += if (pause) "pause" else "resume"; status = if (pause) AgentStatus.PAUSED else AgentStatus.ACTIVE },
                        onRevoke = { did += "revoke"; status = AgentStatus.REVOKED },
                    )
                }
        }
        for (dark in listOf(false, true)) {
            status = AgentStatus.ACTIVE
            did.clear()
            AppAppearance.set(compose.activity, if (dark) "dark" else "light")
            compose.waitForIdle()
            compose.onNodeWithText("Conversar").assertIsDisplayed()
            compose.onNodeWithText("Compartilhar link").assertIsDisplayed()
            CommittedWindowCapture.save(compose.activity, "agents", "agent-profile-${if (dark) "dark" else "light"}")
            compose.onNodeWithTag("agent-chat").performClick()
            compose.onNodeWithTag("agent-share").performClick()
            compose.onNodeWithTag("agent-pause").performClick()
            compose.onNodeWithText("Pausado").assertIsDisplayed()
            compose.onNodeWithText("Retomar").assertIsDisplayed()
            compose.onNodeWithTag("agent-pause").performClick()
            // Turning off asks first, in plain words; cancelling does nothing.
            compose.onNodeWithTag("agent-revoke").performClick()
            compose.onNodeWithText("Desligar Agenda do Enzo?").assertIsDisplayed()
            CommittedWindowCapture.save(compose.activity, "agents", "agent-revoke-confirm-${if (dark) "dark" else "light"}")
            compose.onNodeWithText("Cancelar").performClick()
            compose.runOnIdle { assertEquals(listOf("chat", "share:https://tryzoen.com/a/abc", "pause", "resume"), did) }
            compose.onNodeWithTag("agent-revoke").performClick()
            compose.onNodeWithTag("agent-revoke-confirm").performClick()
            compose.onNodeWithText("Desligado").assertIsDisplayed()
            compose.onNodeWithTag("agent-revoke").assertDoesNotExist()
            compose.onNodeWithTag("agent-share").assertDoesNotExist()
            compose.runOnIdle { assertEquals("revoke", did.last()) }
        }
    }

    @Test fun pauseWaitsForTheRelayToTellTheStatus() {
        show {
            AgentCitizenActions(agenda, null, null, busy = false, onChat = {}, onShare = {}, onPauseToggle = {}, onRevoke = {})
        }
        compose.onNodeWithTag("agent-pause").assertIsNotEnabled()
        compose.onNodeWithTag("agent-share").assertIsNotEnabled()
        compose.onNodeWithTag("agent-revoke").assertDoesNotExist()
    }
}
