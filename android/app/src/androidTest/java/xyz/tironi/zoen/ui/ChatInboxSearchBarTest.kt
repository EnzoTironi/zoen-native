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

/** Exercises the production compact menu used by the bottom-bar accessory. */
@RunWith(AndroidJUnit4::class)
class ChatInboxSearchBarTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun compactFilterSelectsAndRetainsSelectionWhenReopened() {
        var filter by mutableStateOf(ChatInboxFilter.All)
        val all = compose.activity.getString(R.string.chat_filter_all)
        val groups = compose.activity.getString(R.string.chat_filter_groups)
        compose.setContent {
            MaterialTheme {
                ChatFilterMenu(filter, compact = true) { filter = it }
            }
        }
        compose.onNodeWithText(all).assertDoesNotExist()
        compose.onNodeWithTag("chat-filter").assertIsDisplayed().assertHasClickAction().performClick()
        compose.onNodeWithTag("chat-filter-groups").performClick()
        compose.runOnIdle { assertEquals(ChatInboxFilter.Groups, filter) }
        compose.onNodeWithText(groups).assertDoesNotExist()
        compose.onNodeWithTag("chat-filter").assert(
            SemanticsMatcher.expectValue(androidx.compose.ui.semantics.SemanticsProperties.StateDescription, groups)
        ).performClick()
        compose.onNodeWithTag("chat-filter-groups").assertIsDisplayed()
        compose.onNodeWithTag("chat-filter-all").performClick()
        compose.runOnIdle { assertEquals(ChatInboxFilter.All, filter) }
        compose.onNodeWithTag("chat-filter").assertIsDisplayed()
    }
}
