package xyz.tironi.zoen.miniapps

import android.graphics.Bitmap
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipe
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.layout.width
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import xyz.tironi.zoen.Evidence

class NativeGlobeJourneyTest {
    @get:Rule val compose = createComposeRule()

    @Test fun realLandAssetSupportsTapDragAccessibleSelectionAndAnswerReveal() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val polygons = context.assets.open("world/land110.json").bufferedReader().use { OrthographicGlobe.land(it.readText()) }
        assertTrue(polygons.size > 100)
        var guess by mutableStateOf<Offset?>(null)
        var actual by mutableStateOf<Offset?>(null)
        var selections = 0
        compose.setContent { NativeGlobe(guess, actual, Modifier.width(320.dp)) { lat, lon -> selections++; guess = Offset(lon, lat) } }
        val globe = compose.onNodeWithTag("native-globe")
        compose.waitUntil(10_000) { globe.fetchSemanticsNode().config.getOrElse(SemanticsActions.CustomActions) { emptyList() }.isNotEmpty() }
        globe.performTouchInput { click(center) }
        compose.runOnIdle { assertEquals(25f, guess!!.y, .001f); assertEquals(10f, guess!!.x, .001f) }
        globe.performTouchInput { swipe(start = center + Offset(70f, 0f), end = center - Offset(70f, 0f), durationMillis = 450) }
        globe.performTouchInput { click(center) }
        compose.runOnIdle { assertTrue(guess!!.x > 10f) }
        val before = selections
        globe.performTouchInput { click(Offset(1f, 1f)) }
        compose.runOnIdle { assertEquals(before, selections) }
        val actions = globe.fetchSemanticsNode().config[SemanticsActions.CustomActions]
        compose.runOnIdle { assertTrue(actions.last().action()) }
        compose.runOnIdle { assertEquals(before + 1, selections); actual = Offset(-74.0f, 40.7f) }
        compose.waitForIdle()
        compose.mainClock.advanceTimeBy(1800)
        compose.waitForIdle()
        val semantics = globe.fetchSemanticsNode().config
        assertTrue(semantics[SemanticsProperties.StateDescription].let { it.contains("40.7") || it.contains("40,7") })
        assertTrue(semantics[SemanticsActions.CustomActions].isEmpty())
        val beforeRevealTap = selections
        globe.performTouchInput { click(center) }
        compose.runOnIdle { assertEquals(beforeRevealTap, selections) }
        val image = globe.captureToImage().asAndroidBitmap()
        val file = Evidence.outputFile("globe", "native-globe-reveal.png")
        file.outputStream().use { image.compress(Bitmap.CompressFormat.PNG, 100, it) }
        assertTrue(file.length() > 10_000)
    }
}
