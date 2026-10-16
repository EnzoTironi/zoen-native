package xyz.tironi.zoen

import android.content.Intent
import android.net.Uri
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.core.EntryKind

/** Real Compose controls + core storage; networking/other people are covered by core journeys. */
@RunWith(AndroidJUnit4::class)
class ReactionJourneyTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val application get() = ApplicationProvider.getApplicationContext<ZoenApplication>()
    private lateinit var scenario: ActivityScenario<MainActivity>

    @Before fun openDemo() {
        application.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        scenario = ActivityScenario.launch(Intent(application, MainActivity::class.java).putExtra("demo", true))
        compose.waitUntil(30_000) { application.repository.state.value.ready && application.repository.state.value.me != null }
        runBlocking { application.repository.resetDemo() }
    }

    @After fun close() { if (::scenario.isInitialized) scenario.close() }

    @Test fun chooseAnyEmojiReplaceViewPeopleAndRemove() {
        val space = application.repository.state.value.zoenChat!!.id
        val message = "Mensagem para testar reações"
        val id = runBlocking {
            application.repository.change { it.sendMessage(space, message) }
            application.repository.query { it.timeline(space).first { row -> (row.kind as? EntryKind.Message)?.text == message }.id }
        }
        scenario.onActivity { activity ->
            activity.startActivity(Intent(activity, MainActivity::class.java).setAction(Intent.ACTION_VIEW)
                .setData(Uri.parse("zoen://chat/$space")).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP))
        }
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("chat-timeline").fetchSemanticsNodes().isNotEmpty() }
        fun openPicker() {
            compose.onNodeWithTag("chat-timeline").performScrollToNode(hasTestTag("timeline:$id"))
            compose.onNodeWithTag("message-actions:$id").performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.OnLongClick) { it() }
            compose.onNodeWithText("Mais reações").performClick()
            compose.onNodeWithTag("reaction-emoji").assertIsDisplayed()
        }
        fun mine(): String? = runBlocking {
            application.repository.query { it.timeline(space).first { row -> row.id == id }.reactions.firstOrNull { it.mine }?.emoji }
        }
        openPicker()
        compose.onNodeWithTag("reaction-emoji").performTextInput("🦋")
        compose.onNodeWithText("Reagir", substring = false).performClick()
        compose.waitUntil(10_000) { mine() == "🦋" }
        openPicker()
        compose.onNodeWithTag("reaction-emoji").performTextInput("🌻")
        compose.onNodeWithText("Reagir", substring = false).performClick()
        compose.waitUntil(10_000) { mine() == "🌻" }
        assertEquals(1, runBlocking { application.repository.query { it.timeline(space).first { row -> row.id == id }.reactions.count { it.mine } } })
        compose.onNodeWithTag("chat-timeline").performScrollToNode(hasTestTag("timeline:$id"))
        compose.onNodeWithTag("reaction-details:$id").performClick()
        compose.onNodeWithText("Reações", substring = false).assertIsDisplayed()
        compose.onNodeWithText(application.repository.state.value.me!!.name).assertIsDisplayed()
        scenario.onActivity { CommittedWindowCapture.save(it, "reactions", "participants-and-replace") }
        compose.onNodeWithText("Remover minha reação").performClick()
        compose.waitUntil(10_000) { mine() == null }
        scenario.recreate()
        compose.waitUntil(10_000) { application.repository.state.value.ready }
        assertNull(mine())
    }
}
