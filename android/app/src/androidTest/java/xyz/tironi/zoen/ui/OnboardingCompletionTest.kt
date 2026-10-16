package xyz.tironi.zoen.ui

import androidx.activity.ComponentActivity
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class OnboardingCompletionTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun recoveryCompletionSurvivesAccountIdentityChangeBeforeCallback() {
        val owner = Any()
        var accountId by mutableStateOf("temporary-account")
        var persistedCompletion = false
        var finishRecovery: (() -> Unit)? = null
        var originalCompletion: OnboardingCompletion? = null
        compose.setContent {
            val completion = rememberOnboardingCompletion(owner, false) { persistedCompletion }
            Text("Account: $accountId")
            if (!completion.completed) Text("Recovering") else Text("Account home")
            SideEffect {
                if (finishRecovery == null) {
                    originalCompletion = completion
                    // Simulate the callback captured by an already-running recovery.
                    finishRecovery = { completion.finish(null) {} }
                }
                assertSame(originalCompletion, completion)
            }
        }
        compose.runOnIdle { accountId = "restored-account" }
        compose.onNodeWithText("Account: restored-account").assertIsDisplayed()
        compose.onNodeWithText("Recovering").assertIsDisplayed()
        compose.runOnIdle {
            persistedCompletion = true
            finishRecovery!!()
        }
        compose.onNodeWithText("Account home").assertIsDisplayed()
        compose.onNodeWithText("Recovering").assertDoesNotExist()
    }

    @Test fun recoveryCompletionDismissesOnboardingWithNullLanding() {
        val preferences = compose.activity.getSharedPreferences("onboarding-completion-test", 0)
        preferences.edit().clear().commit()
        var landing by mutableStateOf<String?>(null)
        compose.setContent {
            val completion = rememberOnboardingCompletion(preferences, false) { preferences.getBoolean("onboarded", false) }
            if (!completion.completed) {
                Button(onClick = {
                    // Recovery already refreshed the account; only this preference and
                    // the null landing callback change at the end of recovery.
                    preferences.edit().putBoolean("onboarded", true).commit()
                    completion.finish(null) { landing = it }
                }) { Text("Finish recovery") }
            } else {
                Text("Account home")
            }
        }
        compose.onNodeWithText("Finish recovery").performClick()
        compose.onNodeWithText("Account home").assertIsDisplayed()
        compose.onNodeWithText("Finish recovery").assertDoesNotExist()
        compose.runOnIdle {
            assertTrue(preferences.getBoolean("onboarded", false))
            assertNull(landing)
            preferences.edit().clear().commit()
        }
    }
}
