package xyz.tironi.zoen.ui

import androidx.activity.ComponentActivity
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.R

/** Exercises the production input and menu, not a replacement search widget. */
@RunWith(AndroidJUnit4::class)
class ChatInboxSearchBarTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun queryFocusAndClearKeepTheFilterAvailable() {
        var query by mutableStateOf("")
        var focused by mutableStateOf(false)
        var filter by mutableStateOf(ChatInboxFilter.All)
        val all = compose.activity.getString(R.string.chat_filter_all)
        compose.setContent {
            MaterialTheme {
                ChatInboxSearchBar(query, { query = it }, focused, { focused = it }, filter, { filter = it })
            }
        }
        compose.onNodeWithText(all).assertIsDisplayed()
        compose.onNodeWithTag("chat-search-clear").assertDoesNotExist()
        compose.onNodeWithTag("chat-search").performClick()
        compose.onNodeWithText(all).assertDoesNotExist()
        compose.onNodeWithTag("chat-search").performTextInput("Alice")
        compose.onNodeWithTag("chat-search").assertTextEquals("Alice")
        compose.runOnIdle { assertEquals("Alice", query) }
        compose.onNodeWithTag("chat-filter").assertIsDisplayed().performClick()
        compose.onNodeWithTag("chat-filter-groups").performClick()
        compose.runOnIdle { assertEquals(ChatInboxFilter.Groups, filter) }
        compose.onNodeWithTag("chat-search").assertTextEquals("Alice")
        compose.onNodeWithTag("chat-search-clear").performClick()
        compose.onNodeWithTag("chat-search").assertTextEquals("")
        compose.onNodeWithTag("chat-search-clear").assertDoesNotExist()
        compose.onNodeWithTag("chat-filter").assertIsDisplayed()
        compose.runOnIdle { assertEquals("", query) }
    }
}
