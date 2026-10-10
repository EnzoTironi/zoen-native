package xyz.tironi.zoen

import android.graphics.Bitmap
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.runtime.getValue
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Before
import org.junit.After
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.theme.ZoenTheme
import xyz.tironi.zoen.ui.ChatScreen
import xyz.tironi.zoen.ui.ChatAppearanceStore
import xyz.tironi.zoen.ui.ConversationsScreen

@RunWith(AndroidJUnit4::class)
class ChatReadingJourneyTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private val repository get() = ApplicationProvider.getApplicationContext<ZoenApplication>().repository
    private lateinit var model: ZoenViewModel
    private val visibilityOwner = Any()
    @Before fun prepare() {
        repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        runBlocking { repository.boot(true); repository.resetDemo() }
        repository.setAppVisible(true, visibilityOwner)
        model = ViewModelProvider(compose.activity)[ZoenViewModel::class.java]
    }
    @After fun stopObserving() { repository.setAppVisible(false, visibilityOwner) }

    @Test fun incomingMessagesKeepReadingPositionAndCountOnlyNewRootMessages() {
        val space = repository.state.value.spaces.first { it.kind == SpaceKindDto.GROUP }
        val friend = space.members.first { !it.isMe && it.kind == PersonaKind.PERSON }
        val ids = runBlocking { repository.change { core ->
            core.markRead(space.id)
            (1..16).map { core.sendMessage(space.id, "Reading history $it: " + "We keep our place in the conversation. ".repeat(4)).id }
        } }
        openChat(space.id)
        await(ids.last())
        compose.onNodeWithTag("chat-timeline").performScrollToNode(hasTestTag("timeline:${ids.first()}"))
        compose.onNodeWithTag("timeline:${ids.first()}").assertIsDisplayed()
        runBlocking { repository.change { core ->
            core.demoMemberSay(space.id, friend.handle, "New arrival one")
            core.demoMemberSay(space.id, friend.handle, "New arrival two")
            core.sendReply(space.id, "A reply inside the thread", ids.first(), true)
        } }
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("chat-new-messages").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("timeline:${ids.first()}").assertIsDisplayed()
        compose.onNodeWithText(compose.activity.resources.getQuantityString(R.plurals.chat_new_messages, 2, 2)).assertIsDisplayed()
        capture("incoming-keeps-reading")
        compose.onNodeWithTag("chat-new-messages").performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithText("New arrival two").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("New arrival two").assertIsDisplayed()
        compose.onNodeWithTag("chat-new-messages").assertDoesNotExist()
        runBlocking { repository.change { it.demoMemberSay(space.id, friend.handle, "Following while at the bottom") } }
        compose.waitUntil(10_000) { compose.onAllNodesWithText("Following while at the bottom").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Following while at the bottom").assertIsDisplayed()
        assertTrue(runBlocking { repository.query { it.verifyLog(space.id).valid } })
    }

    @Test fun unreadStartsAtFirstIncomingRootDespiteOwnMessagesRepliesAndSystemEvents() {
        val friend = repository.state.value.spaces.first { it.kind == SpaceKindDto.GROUP }.members.first { !it.isMe && it.kind == PersonaKind.PERSON }
        val pair = runBlocking { repository.change { core ->
            val chat = core.createCommunity("First Android reading")
            core.addMember(chat, friend.id)
            core.markRead(chat)
            var first = ""
            for (i in 1..12) {
                val entry = core.demoMemberSay(chat, friend.handle, "Unread root $i: " + "Meet us in the park before the walk. ".repeat(4))
                if (i == 1) first = entry.id
                core.sendMessage(chat, "Own message $i")
                if (i == 1) core.sendReply(chat, "Own threaded reply", first, true)
                core.setBackground(chat, ChatAppearanceStore.default())
            }
            chat to first
        } }
        openChat(pair.first)
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("unread-divider").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("unread-divider").assertIsDisplayed()
        compose.onNodeWithTag("timeline:${pair.second}").assertIsDisplayed()
        capture("mixed-first-unread")
        assertEquals(0u, repository.state.value.spaces.single { it.id == pair.first }.unread)
    }

    @Test fun inboxFiltersUseRealKindsAndSurviveRestorationWithSearch() {
        val restoration = StateRestorationTester(compose)
        restoration.setContent {
            val state by model.state.collectAsStateWithLifecycle()
            ZoenTheme { ConversationsScreen(model, state, {}, false) }
        }
        val all = repository.state.value.spaces
        for ((filter, kind) in listOf("direct" to SpaceKindDto.DIRECT, "groups" to SpaceKindDto.GROUP, "communities" to SpaceKindDto.COMMUNITY)) {
            compose.onNodeWithTag("chat-filter").performScrollTo().performClick()
            compose.onNodeWithTag("chat-filter-$filter").performClick()
            val included = all.filter { it.kind == kind }
            for (space in included) compose.onNodeWithTag("conversation-list").performScrollToNode(hasTestTag("chat:${space.counterpart?.handle ?: space.id}"))
                .also { compose.onNodeWithTag("chat:${space.counterpart?.handle ?: space.id}").assertIsDisplayed() }
            for (space in all.filter { it.kind != kind }) compose.onNodeWithTag("chat:${space.counterpart?.handle ?: space.id}").assertDoesNotExist()
            compose.onNodeWithTag("chat-filter").performScrollTo()
            capture("inbox-$filter")
        }
        restoration.emulateSavedInstanceStateRestore()
        compose.onNodeWithTag("chat-filter").assertIsDisplayed()
        compose.onNodeWithText(compose.activity.getString(R.string.chat_filter_communities)).assertIsDisplayed()
        compose.onNodeWithContentDescription(compose.activity.getString(R.string.search)).performClick()
        val community = all.first { it.kind == SpaceKindDto.COMMUNITY }
        compose.onNode(hasSetTextAction()).performTextInput(community.title)
        compose.onNodeWithTag("chat:${community.id}").assertIsDisplayed()
        for (space in all.filter { it.kind != SpaceKindDto.COMMUNITY }) compose.onNodeWithTag("chat:${space.counterpart?.handle ?: space.id}").assertDoesNotExist()
    }

    private fun openChat(id: String) { compose.setContent {
        val state by model.state.collectAsStateWithLifecycle()
        ZoenTheme { ChatScreen(model, state, id, {}, {}) }
    } }
    private fun await(id: String) {
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("timeline:$id").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("timeline:$id").assertIsDisplayed()
    }
    private fun capture(name: String) {
        compose.waitForIdle()
        val image = checkNotNull(InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot())
        Evidence.outputFile("experience", "$name.png").outputStream().use { assertTrue(image.compress(Bitmap.CompressFormat.PNG, 100, it)) }
        image.recycle()
    }
}
