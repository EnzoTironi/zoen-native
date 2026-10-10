package xyz.tironi.zoen

import androidx.activity.ComponentActivity
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.core.EntryKind
import xyz.tironi.zoen.theme.ZoenTheme
import xyz.tironi.zoen.ui.ChatScreen

@RunWith(AndroidJUnit4::class)
class ChatViewportLoadingTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun restoredHistoryAndLatestPositionSurviveAnUnloadedTimeline() {
        val application = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val repository = application.repository
        application.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        runBlocking { repository.boot(true); repository.resetDemo() }
        val space = checkNotNull(repository.state.value.zoenChat).id
        val texts = (1..8).map { index ->
            "Loading viewport message $index: " + "Keep the reading position through a subscription handover. ".repeat(3).trim()
        }
        val ids = runBlocking { repository.change { core ->
            texts.forEach { core.sendMessage(space, it) }
            val entries = core.timeline(space)
            texts.map { text -> entries.single { (it.kind as? EntryKind.Message)?.text == text }.id }
        } }
        val model = ViewModelProvider(compose.activity)[ZoenViewModel::class.java]
        var timelineAvailable by mutableStateOf(true)
        val restoration = StateRestorationTester(compose)
        restoration.setContent {
            val state by model.state.collectAsStateWithLifecycle()
            val rendered = if (timelineAvailable) state else state.copy(timelines = state.timelines - space)
            ZoenTheme { ChatScreen(model, rendered, space, {}, {}) }
        }
        fun awaitEntry(id: String) {
            compose.waitUntil(10_000) { compose.onAllNodesWithTag("timeline:$id").fetchSemanticsNodes().size == 1 }
            compose.onNodeWithTag("timeline:$id").assertIsDisplayed()
        }
        fun restoreThenHandover(id: String) {
            restoration.emulateSavedInstanceStateRestore()
            awaitEntry(id)
            compose.runOnIdle { timelineAvailable = false }
            compose.waitForIdle()
            compose.runOnIdle { timelineAvailable = true }
            awaitEntry(id)
        }
        awaitEntry(ids.last())
        compose.onNodeWithTag("chat-timeline").performScrollToNode(hasTestTag("timeline:${ids.first()}"))
        compose.onNodeWithTag("timeline:${ids.first()}").assertIsDisplayed()
        restoreThenHandover(ids.first())
        val latest = "Own message after the restored history and subscription handover"
        val latestId = runBlocking { repository.change { core ->
            core.sendMessage(space, latest)
            core.timeline(space).single { (it.kind as? EntryKind.Message)?.text == latest }.id
        } }
        awaitEntry(latestId)
        restoreThenHandover(latestId)
        assertTrue(runBlocking { repository.query { it.verifyAll().all { report -> report.valid } } })
    }
}
