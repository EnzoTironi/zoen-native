package xyz.tironi.zoen.ui

import androidx.activity.ComponentActivity
import androidx.navigation3.runtime.NavKey
import androidx.compose.material3.Surface
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.ZoenApplication
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.miniapps.MiniAppTileStrip
import xyz.tironi.zoen.theme.ZoenTheme

@RunWith(AndroidJUnit4::class)
class HomeNotificationsCardTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun emptyPendingStillShowsFirstCardAndOpensActualActivity() {
        val app = ApplicationProvider.getApplicationContext<ZoenApplication>()
        runBlocking { app.repository.boot(true); app.repository.resetDemo() }
        val model = ViewModelProvider(compose.activity)[ZoenViewModel::class.java]
        val state = app.repository.state.value.copy(requests = emptyList())
        var destination by mutableStateOf<NavKey?>(null)
        compose.setContent {
            ZoenTheme { Surface {
                if (destination == Activity) ActivityScreen(model, state, {}, {})
                else MiniAppTileStrip(model, state, emptyList(), onOpenNotifications = { destination = Activity })
            } }
        }
        compose.onNodeWithTag("home-notifications").assertIsDisplayed()
        compose.onNodeWithTag("home-notifications").assert(hasText("0", substring = false))
        compose.onNodeWithTag("home-notifications").performClick()
        compose.runOnIdle { assertEquals(Activity, destination) }
        compose.onNodeWithTag("home-notifications").assertDoesNotExist()
    }
}
