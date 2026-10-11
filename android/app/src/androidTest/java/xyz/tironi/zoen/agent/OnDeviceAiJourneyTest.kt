package xyz.tironi.zoen.agent

import android.graphics.Bitmap
import android.os.Build
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModelProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.Timeout
import org.junit.runner.RunWith
import xyz.tironi.zoen.Evidence
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.media.VoiceTranscriber
import xyz.tironi.zoen.theme.ZoenTheme

@RunWith(AndroidJUnit4::class)
class OnDeviceAiJourneyTest {
    @get:Rule(order = 0) val deadline = Timeout.seconds(120)
    @get:Rule(order = 1) val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun settingsReflectTheActualProviderAndUnavailableChatNeverClaimsInference() {
        val model = ViewModelProvider(compose.activity)[ZoenViewModel::class.java]
        compose.setContent {
            ZoenTheme {
                Surface(Modifier.fillMaxSize()) {
                    Column(Modifier.safeDrawingPadding().padding(24.dp)) { OnDeviceAiSettings(model) }
                }
            }
        }
        compose.waitUntil(15_000) { model.planner.availability.value.status != ModelStatus.Checking }
        val availability = model.planner.availability.value
        val label = when (availability.status) {
            ModelStatus.Ready -> R.string.agent_ai_ready
            ModelStatus.Downloadable -> R.string.agent_ai_downloadable
            ModelStatus.Downloading -> R.string.agent_ai_downloading
            ModelStatus.Unavailable -> R.string.agent_ai_unavailable
            ModelStatus.Failed -> R.string.agent_ai_failed
            ModelStatus.Checking -> error("Provider did not finish its actual capability check")
        }
        compose.onNodeWithText(compose.activity.getString(label, availability.downloadedBytes / (1024 * 1024))).assertIsDisplayed()
        compose.onNodeWithText(compose.activity.getString(R.string.agent_ai_privacy)).assertIsDisplayed()
        if (availability.status == ModelStatus.Downloadable) {
            compose.onNodeWithText(compose.activity.getString(R.string.agent_ai_download)).assertIsDisplayed()
        }
        val screenshot = checkNotNull(InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot())
        try {
            Evidence.outputFile("agent", "actual-on-device-provider-settings.png").outputStream().use {
                assertTrue(screenshot.compress(Bitmap.CompressFormat.PNG, 100, it))
            }
        } finally { screenshot.recycle() }

        val reply = runBlocking { model.planner.reply("Say a short greeting.", "Zoen", "en-US", null) }
        assertTrue(reply.isNotBlank())
        val inferenceAvailable = model.planner.availability.value.status == ModelStatus.Ready
        if (inferenceAvailable) assertFalse("An available provider must complete real inference", reply.startsWith("Free chat needs"))
        else assertTrue("An unavailable provider must explain the local rules without simulating AI", reply.startsWith("Free chat needs the on-device model"))
        Evidence.outputFile("agent", "actual-on-device-provider-receipt.json").writeText(JSONObject()
            .put("sdk", Build.VERSION.SDK_INT).put("deviceModel", Build.MODEL)
            .put("providerStatus", availability.status.name)
            .put("providerStatusAfterReply", model.planner.availability.value.status.name)
            .put("realInferenceCompleted", inferenceAvailable)
            .put("onDeviceSpeechServiceAvailable", VoiceTranscriber.supported(compose.activity))
            .put("reply", reply)
            .put("scope", "Actual GeminiNanoModel provider and settings; no injected model, download or cloud fallback. Speech field probes service availability, not a downloaded language or transcription quality.")
            .toString(2))
    }
}
