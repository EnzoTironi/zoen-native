package xyz.tironi.zoen

import androidx.activity.ComponentActivity
import androidx.compose.runtime.*
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation3.runtime.NavKey
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.theme.AppAppearance
import xyz.tironi.zoen.theme.ZoenTheme
import xyz.tironi.zoen.ui.*

/** Ajustes as a person uses it: Tema, back one screen at a time, search, and the X from deep inside. */
@RunWith(AndroidJUnit4::class)
class SettingsJourneyTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    private fun open(): SnapshotStateList<NavKey> {
        val app = ApplicationProvider.getApplicationContext<ZoenApplication>()
        app.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        runBlocking { app.repository.boot(true); app.repository.resetDemo() }
        val model = ViewModelProvider(compose.activity)[ZoenViewModel::class.java]
        val stack = mutableStateListOf<NavKey>(Home, SettingsRoute())
        compose.setContent {
            val state by model.state.collectAsStateWithLifecycle()
            ZoenTheme {
                when (val top = stack.last()) {
                    is SettingsRoute -> SettingsScreen(model, state, top.page, { stack.add(it) }, { stack.removeAt(stack.lastIndex) }) {
                        while (stack.last() is SettingsRoute) stack.removeAt(stack.lastIndex)
                    }
                    else -> {}
                }
            }
        }
        return stack
    }

    @Test fun pickDarkThemeBackPopsOneScreenAndCloseLeavesAjustes() {
        val stack = open()
        val context = compose.activity
        AppAppearance.set(context, "auto")
        compose.onNodeWithTag("settings:Preferences").performClick()
        compose.onNodeWithTag("settings:Theme").performClick()
        compose.onNodeWithTag("theme:dark").assertIsNotSelected().performClick()
        compose.onNodeWithTag("theme:dark").assertIsSelected()
        assertEquals("dark", AppAppearance.mode)

        compose.onNodeWithTag("settings:back").performClick()
        compose.onNodeWithTag("settings:Theme").assertIsDisplayed()  // one pop: Preferências
        compose.onNodeWithTag("settings:back").performClick()
        compose.onNodeWithTag("settings:search").assertIsDisplayed() // Ajustes home

        compose.onNodeWithTag("settings:search").performTextInput("xyzzy")
        compose.onNodeWithTag("settings:noResults").assertExists()
        compose.onNodeWithTag("settings:search").performTextReplacement("senha")
        compose.onNodeWithTag("settings:Security").performClick()

        compose.onNodeWithTag("settings:close").performClick()
        compose.runOnIdle { assertTrue(stack.none { it is SettingsRoute }) }
        AppAppearance.set(context, "auto")
    }

    @Test fun restoreIsDisabledWithoutABackupOnThisDevice() {
        open()
        compose.onNodeWithTag("settings:Backup").performClick()
        compose.onNodeWithTag("settings:restore").assertHasNoClickAction()
    }
}
