package xyz.tironi.zoen.ui

import androidx.activity.ComponentActivity
import androidx.navigation3.runtime.NavKey
import androidx.compose.material3.Surface
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.ZoenApplication
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.PersonaKind
import xyz.tironi.zoen.theme.ZoenTheme

/** Production search UI and real offline core; no substituted search/DM callbacks. */
@RunWith(AndroidJUnit4::class)
class UnifiedSearchJourneyTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    private fun setup(): ZoenViewModel {
        val app = ApplicationProvider.getApplicationContext<ZoenApplication>()
        runBlocking { app.repository.boot(true); app.repository.resetDemo() }
        return ViewModelProvider(compose.activity)[ZoenViewModel::class.java]
    }

    private fun selectRecipient(personId: String, handle: String) {
        val tag = "search-recipient:$personId"
        try {
            // Lazy rows outside the viewport do not have semantics nodes yet.
            // Scroll the actual result list, without changing the query or person.
            compose.waitUntil(10_000) {
                try {
                    compose.onNodeWithTag("search-results").performScrollToNode(hasTestTag(tag))
                    compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()
                } catch (_: AssertionError) { false }
            }
        } catch (error: ComposeTimeoutException) {
            throw AssertionError("Missing actual recipient $personId (@$handle).\n" +
                compose.onRoot(useUnmergedTree = true).printToString(), error)
        }
        compose.onNodeWithTag(tag).assertIsDisplayed().performClick()
    }

    @Test fun selectingKnownPersonReusesTheirDirectConversation() {
        val model = setup()
        val state = model.repository.state.value
        val direct = state.spaces.first { it.counterpart?.kind == PersonaKind.PERSON }
        val person = requireNotNull(direct.counterpart)
        var destination: NavKey? = null
        compose.setContent { ZoenTheme { Surface { SearchScreen(model, state, { destination = it }, {}) } } }
        compose.onNodeWithTag("search").performTextInput(person.handle)
        selectRecipient(person.id, person.handle)
        compose.waitUntil(10_000) { destination != null }
        compose.runOnIdle { assertEquals(Chat(direct.id), destination) }
    }

    @Test fun selectingPersonWithoutDirectInDemoReportsAccountGateNotSharedGroup() {
        val model = setup()
        val state = model.repository.state.value
        val person = runBlocking { model.repository.query { it.personas() } }.first {
            !it.isMe && it.kind == PersonaKind.PERSON && state.spaces.none { chat -> chat.counterpart?.id == it.id }
        }
        var destination: NavKey? = null
        val errors = java.util.concurrent.CopyOnWriteArrayList<String>()
        compose.setContent { ZoenTheme { Surface {
            androidx.compose.runtime.LaunchedEffect(model) { model.notices.collect { errors.add(it.text) } }
            SearchScreen(model, state, { destination = it }, {})
        } } }
        compose.onNodeWithTag("search").performTextInput(person.handle)
        selectRecipient(person.id, person.handle)
        // Demo has no authenticated author. Exercise the real startDirect failure,
        // rather than mutating demo credentials or treating a shared group as a DM.
        compose.waitUntil(10_000) { errors.isNotEmpty() }
        assertTrue("Expected account gate, received $errors", errors.any {
            it.contains("Sign in first", ignoreCase = true) || it.contains("Entre na sua conta", ignoreCase = true)
        })
        compose.runOnIdle { assertNull(destination) }
        assertEquals(state.spaces.map { it.id }, model.repository.state.value.spaces.map { it.id })
    }

    @Test fun sameQueryFindsActualMessageAndOpensItsConversationAtMessage() {
        val model = setup()
        val chat = model.repository.state.value.spaces.first { it.counterpart != null }
        val query = "UnifiedSearchRegressionNeedle"
        val message = runBlocking { model.repository.change { it.sendMessage(chat.id, query) } }
        val state = model.repository.state.value
        var destination: NavKey? = null
        compose.setContent { ZoenTheme { Surface { SearchScreen(model, state, { destination = it }, {}) } } }
        compose.onNodeWithTag("search").performTextInput(query)
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("search-result:${message.id}").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("search-result:${message.id}").performScrollTo().performClick()
        compose.runOnIdle { assertEquals(Chat(chat.id, message.id), destination) }
    }
}
