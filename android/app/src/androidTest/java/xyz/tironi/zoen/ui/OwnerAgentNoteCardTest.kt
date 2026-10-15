package xyz.tironi.zoen.ui

import android.content.res.Configuration
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.CommittedWindowCapture
import xyz.tironi.zoen.core.Persona
import xyz.tironi.zoen.core.PersonaKind
import xyz.tironi.zoen.theme.ZoenTheme
import java.util.Locale

@RunWith(AndroidJUnit4::class)
class OwnerAgentNoteCardTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    private val agent = Persona("agenda", PersonaKind.AGENT, "Agenda do Bruno", "agenda_bruno", "A", "#19B5A5", null, "", "bruno", "Bruno", null, false, true)

    @Test fun parsesOnlyTheKnownNoteShapes() {
        assertEquals(OwnerAgentNote.Ask("s1", "carla", "Oi"), OwnerAgentNote.parse("""{"ask_owner":{"space":"s1","from":"carla","draft":"Oi","rule":"r"}}"""))
        assertEquals(OwnerAgentNote.Proposal("s1", "Agenda", "agenda"), OwnerAgentNote.parse("""{"agent_proposal":{"space":"s1","source":"m","spec":{"name":"Agenda","handle":"agenda"}}}"""))
        assertEquals(OwnerAgentNote.Booking("s1", false, "Carla", "Corte", "seg 10:00"), OwnerAgentNote.parse("""{"booking":{"space":"s1","kind":"new","client":"Carla","service":"Corte","when":"seg 10:00","source":"m"}}"""))
        assertEquals(true, (OwnerAgentNote.parse("""{"booking":{"kind":"cancelled"}}""") as OwnerAgentNote.Booking).cancelled)
        assertNull(OwnerAgentNote.parse("""{"grant_admin":{}}"""))
        assertNull(OwnerAgentNote.parse("not json"))
    }

    @Test fun cardsReadInPlainPortugueseAndOpenTheConversation() {
        val opened = mutableListOf<String>()
        val notes = listOf(
            OwnerAgentNote.Ask("barbearia", "carla", "Oi Carla! Recebi o comprovante, já confirmo seu horário de sábado às 10h."),
            OwnerAgentNote.Proposal("dm", "Agenda do Salão", "agenda_salao"),
            OwnerAgentNote.Booking("barbearia", false, "Carla", "Corte", "sáb 10:00"),
        )
        compose.setContent {
            val base = LocalContext.current
            val conf = Configuration(LocalConfiguration.current).apply { setLocale(Locale.forLanguageTag("pt-BR")) }
            val ctx = base.createConfigurationContext(conf)
            CompositionLocalProvider(LocalContext provides ctx, LocalConfiguration provides conf) {
                ZoenTheme {
                    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
                        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                            notes.forEach { note ->
                                Box { OwnerAgentNoteCard(agent, note, personName = { if (it == "carla") "Carla" else null }, fresh = true, open = { opened += it }) }
                            }
                        }
                    }
                }
            }
        }
        compose.waitForIdle()
        compose.onNodeWithText("Seu agente precisa de você").assertIsDisplayed()
        compose.onNodeWithText("Antes de responder a Carla, ele quer que você veja a resposta.").assertIsDisplayed()
        compose.onNodeWithText("Seu agente quer criar um agente").assertIsDisplayed()
        compose.onNodeWithText("Novo agendamento").assertIsDisplayed()
        compose.onNodeWithText("Carla marcou Corte, sáb 10:00.").assertIsDisplayed()
        assertEquals(3, compose.onAllNodesWithTag("owner-note").fetchSemanticsNodes().size)
        CommittedWindowCapture.save(compose.activity, "owner-notes", "owner-notes-pt")
        compose.onAllNodesWithTag("owner-note-open")[0].performClick()
        compose.runOnIdle { assertEquals(listOf("barbearia"), opened) }
    }
}
