package xyz.tironi.zoen.ui

import androidx.activity.ComponentActivity
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.CommittedWindowCapture

@RunWith(AndroidJUnit4::class)
class NativeMaterialComponentsTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun buttonsAndCardsKeepTheirClickAndDisabledContracts() {
        var clicks = 0
        compose.setContent {
            MaterialTheme {
                Column {
                    GlassButton(
                        onClick = { clicks++ },
                        modifier = Modifier.testTag("filled"),
                        shape = RoundedCornerShape(8.dp),
                        colors = ButtonDefaults.buttonColors(containerColor = Color.Blue),
                    ) { Text("Filled") }
                    GlassOutlinedButton(
                        onClick = { clicks++ },
                        modifier = Modifier.testTag("outlined"),
                    ) { Text("Outlined") }
                    GlassButton(
                        onClick = { clicks++ },
                        modifier = Modifier.testTag("disabled-button"),
                        enabled = false,
                    ) { Text("Disabled button") }
                    GlassCard(
                        onClick = { clicks++ },
                        modifier = Modifier.testTag("clickable-card"),
                        shape = RoundedCornerShape(8.dp),
                        colors = CardDefaults.cardColors(containerColor = Color.Cyan),
                        elevation = CardDefaults.cardElevation(defaultElevation = 4.dp),
                        border = BorderStroke(1.dp, Color.Blue),
                    ) { Text("Clickable card") }
                    GlassCard(
                        onClick = { clicks++ },
                        modifier = Modifier.testTag("disabled-card"),
                        enabled = false,
                    ) { Text("Disabled card") }
                    GlassCard(modifier = Modifier.testTag("static-card")) { Text("Static card") }
                }
            }
        }
        compose.waitForIdle()
        CommittedWindowCapture.save(compose.activity, "native-material", "buttons-and-cards")
        listOf("filled", "outlined", "clickable-card").forEach {
            compose.onNodeWithTag(it).assertIsDisplayed().assertHasClickAction().performClick()
        }
        listOf("disabled-button", "disabled-card").forEach {
            compose.onNodeWithTag(it).assertIsNotEnabled().performClick()
        }
        compose.onNodeWithTag("static-card").assertIsDisplayed()
        compose.runOnIdle { assertEquals(3, clicks) }
    }
}
