package xyz.tironi.zoen

import android.graphics.Bitmap
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Rule
import org.junit.Test
import xyz.tironi.zoen.miniapps.MiniAppWebProvider
import xyz.tironi.zoen.miniapps.MiniAppWebProviderGate

class McpWebProviderTest {
    @get:Rule val compose = createComposeRule()

    @Test fun installedProviderEitherGetsIsolatedHtmlOrAnExplicitUpdateScreen() {
        val supported = MiniAppWebProvider.supported()
        var hostEntered = false
        compose.setContent {
            MaterialTheme {
                if (MiniAppWebProviderGate()) {
                    hostEntered = true
                    Text("Secure HTML host available", Modifier.testTag("mcp-web-supported"))
                }
            }
        }
        compose.waitForIdle()
        assertEquals(supported, hostEntered)
        if (supported) {
            compose.onNodeWithTag("mcp-web-supported").assertIsDisplayed()
            compose.onNodeWithTag("mcp-web-unsupported").assertDoesNotExist()
        } else {
            compose.onNodeWithTag("mcp-web-supported").assertDoesNotExist()
            compose.onNodeWithTag("mcp-web-unsupported").assertIsDisplayed()
            compose.onNodeWithTag("mcp-web-update").assertIsDisplayed()
            assertThrows(IllegalStateException::class.java) { MiniAppWebProvider.requireSupported() }
            val file = Evidence.outputFile("mcp", "mcp-unsupported-webview.png")
            val bitmap = compose.onNodeWithTag("mcp-web-unsupported").captureToImage().asAndroidBitmap()
            try { file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) } } finally { bitmap.recycle() }
        }
    }
}
