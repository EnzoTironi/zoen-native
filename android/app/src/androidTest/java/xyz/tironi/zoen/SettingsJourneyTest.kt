package xyz.tironi.zoen

import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.activity.ComponentActivity
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.SnapshotStateList
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
import org.junit.After
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

    /** A real local account (no demo data); `account = false` signs out first. */
    private fun open(account: Boolean = true): SnapshotStateList<NavKey> {
        val app = ApplicationProvider.getApplicationContext<ZoenApplication>()
        runBlocking {
            // Close whatever engine is open (often the demo one) and reopen the real database,
            // so signing out never erases the demo identity other journeys rely on.
            app.repository.leaveDemo()
            if (app.repository.state.value.account != null) {
                app.repository.signOut()
                app.repository.leaveDemo()
            }
            if (account) app.repository.createAccount("Teste Ajustes", "teste_ajustes", "http://10.0.2.2:18787")
        }
        app.repository.preferences.edit().putBoolean("onboarded", true).commit()
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

    /** Hand the next journey back the demo world it expects. */
    @After fun restoreDemo() = runBlocking {
        ApplicationProvider.getApplicationContext<ZoenApplication>().repository.useDemo()
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

    @Test fun recoveryRequiresTheSavedKeyAndDoesNotOfferPassword() {
        open()
        compose.onNodeWithTag("settings:Backup").performClick()
        compose.waitUntil(15_000) { compose.onAllNodesWithTag("settings:backupTurnOn").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("settings:restore").assertDoesNotExist()
        compose.onNodeWithTag("settings:backupTurnOn").performClick()
        var readinessObserved = false
        var dialogPresent = false
        var keyNodePresent = false
        var savedNodeCount = 0
        var savedEnabledCount = 0
        try {
            compose.waitUntil(5_000) {
                val saved = compose.onAllNodesWithTag("recovery-saved").fetchSemanticsNodes()
                val currentDialogPresent = compose.onAllNodesWithText("Chave de recuperação").fetchSemanticsNodes().isNotEmpty()
                val currentKeyNodePresent = compose.onAllNodesWithTag("recovery-key").fetchSemanticsNodes().isNotEmpty()
                // Cache only nonsecret readiness metadata; never retain or log key text or nodes.
                dialogPresent = currentDialogPresent
                keyNodePresent = currentKeyNodePresent
                savedNodeCount = saved.size
                savedEnabledCount = saved.count { SemanticsProperties.Disabled !in it.config }
                readinessObserved = true
                saved.any { SemanticsProperties.Disabled !in it.config }
            }
        } catch (error: ComposeTimeoutException) {
            // No new UI query or key generation after timeout; keep the original failure.
            android.util.Log.e("RecoveryReadiness", "observed=$readinessObserved dialogPresent=$dialogPresent keyNodePresent=$keyNodePresent savedNodeCount=$savedNodeCount savedEnabledCount=$savedEnabledCount")
            throw error
        }
        compose.onNodeWithTag("recovery-key").assertExists()
        compose.onNodeWithTag("recovery-saved").performClick()
        // Synthetic incorrect input only: never read/type/log the generated secret.
        compose.onNodeWithTag("recovery-confirmation").performTextInput("1".repeat(63))
        compose.onNodeWithTag("recovery-activate").assertIsNotEnabled()
        compose.onNodeWithTag("recovery-confirmation").performTextInput("1")
        compose.onNodeWithTag("recovery-activate").assertIsNotEnabled()
        compose.onNodeWithTag("settings:backupPassword").assertDoesNotExist()
        compose.onNodeWithText("Fechar").performClick()
        val app = ApplicationProvider.getApplicationContext<ZoenApplication>()
        assertTrue(!runBlocking { app.repository.query { it.backupStatus() } }.enabled)
    }

    @Test fun invitesShareTheLinkTheCoreMakes() {
        open()
        val app = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val link = runBlocking { app.repository.query { it.friendInviteLink() } }
        assertTrue("core link: $link", link?.contains("teste_ajustes") == true)
        compose.onNodeWithTag("settings:Invites").performScrollTo().performClick()
        compose.waitUntil(15_000) { compose.onAllNodesWithTag("settings:inviteShare").fetchSemanticsNodes().any { SemanticsProperties.Disabled !in it.config } }
        compose.onNodeWithTag("settings:inviteShare").assertIsDisplayed().assertIsEnabled()
    }

    @Test fun disappearingMessagesShowsEveryOptionDisabled() {
        open()
        compose.onNodeWithTag("settings:Disappearing").performScrollTo().performClick()
        listOf("off", "24h", "7d", "90d").forEach { compose.onNodeWithTag("disappear:$it").assertIsNotEnabled().assertIsNotSelected() }
    }

    @Test fun withoutAnAccountNothingShowsSomeonesIdentity() {
        open(account = false)
        compose.onNodeWithText("@", substring = true).assertDoesNotExist()
        compose.onNodeWithTag("settings:Plans").performScrollTo().performClick()
        compose.onNodeWithTag("settings:back").assertExists()
        compose.onNodeWithText("R$", substring = true).assertDoesNotExist()
    }
}
