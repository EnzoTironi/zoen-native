package xyz.tironi.zoen

import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class NativeDashJourneyTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val app get() = ApplicationProvider.getApplicationContext<ZoenApplication>()
    private lateinit var scenario: ActivityScenario<MainActivity>
    private var hadHtmlPreference = false
    private var originalHtml = false
    private var originalScale = "0"

    @Before fun open() {
        originalScale = shell("settings get global animator_duration_scale").trim()
        val preferences = app.repository.preferences
        hadHtmlPreference = preferences.contains("miniapps.html:pet")
        originalHtml = preferences.getBoolean("miniapps.html:pet", false)
        preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).putBoolean("miniapps.html:pet", false).commit()
        runBlocking { app.repository.boot(true); app.repository.resetDemo() }
        val pet = app.repository.state.value.items.first { it.app?.appId == "pet" }
        scenario = ActivityScenario.launch(Intent(app, MainActivity::class.java).putExtra("demo", true)
            .setAction(Intent.ACTION_VIEW).setData(Uri.parse("zoen://app/${pet.id}")))
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("pet-app-content").fetchSemanticsNodes().isNotEmpty() }
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("pet-unboxing").fetchSemanticsNodes().isEmpty() }
    }

    @Test fun firstAdoptionRevealIsSkippableSavedPerPetAndTabsUseTheSharedLeaderboard() {
        val pet = app.repository.state.value.items.first { it.app?.appId == "pet" }
        val version = pet.version
        app.repository.preferences.edit().remove("RodaUnboxed.${pet.id}").commit()
        scenario.close()
        shell("settings put global animator_duration_scale 1")
        compose.mainClock.autoAdvance = false
        scenario = ActivityScenario.launch(Intent(app, MainActivity::class.java).putExtra("demo", true)
            .setAction(Intent.ACTION_VIEW).setData(Uri.parse("zoen://app/${pet.id}")))
        compose.waitUntil(10_000) {
            compose.mainClock.advanceTimeByFrame()
            compose.onAllNodesWithTag("pet-unboxing").fetchSemanticsNodes().isNotEmpty()
        }
        compose.mainClock.advanceTimeBy(2_600)
        compose.onNodeWithTag("pet-unboxing").assertIsDisplayed()
        capture("native-pet-first-adoption-reveal")
        compose.onNodeWithTag("pet-unboxing").performClick()
        compose.mainClock.autoAdvance = true
        compose.onNodeWithTag("pet-unboxing").assertDoesNotExist()
        assertTrue(app.repository.preferences.getBoolean("RodaUnboxed.${pet.id}", false))
        scenario.recreate()
        compose.onNodeWithTag("pet-unboxing").assertDoesNotExist()
        compose.onNodeWithTag("pet-tab-0").assertIsSelected()
        compose.onNodeWithTag("pet-tab-2").performClick().assertIsSelected()
        compose.onNodeWithText(app.getString(R.string.pet_group_leaderboard)).assertIsDisplayed()
        capture("native-pet-group-leaderboard")
        compose.onNodeWithTag("pet-tab-1").performClick().assertIsSelected()
        compose.onNodeWithTag("dash-start").performScrollTo().assertIsDisplayed()
        compose.onNodeWithTag("pet-tab-0").performClick().assertIsSelected()
        compose.onNodeWithTag("pet-app-content").assertExists()
        assertEquals(version, app.repository.state.value.items.first { it.id == pet.id }.version)
        compose.onNodeWithTag("pet-app-content").performScrollToNode(hasText(app.getString(R.string.nap)))
        compose.onNodeWithText(app.getString(R.string.nap)).performClick()
        compose.waitUntil(10_000) { org.json.JSONObject(app.repository.state.value.items.first { it.id == pet.id }.app!!.viewJson).getBoolean("asleep") }
        compose.onNodeWithTag("pet-tab-1").assertIsNotEnabled()
        capture("native-pet-sleeping-care")
        compose.onNodeWithTag("pet-tab-2").performClick().assertIsSelected()
        compose.onNodeWithText(app.getString(R.string.pet_group_leaderboard)).assertIsDisplayed()
        capture("native-pet-sleeping-leaderboard")
        compose.onNodeWithTag("pet-tab-0").performClick()
        compose.onNodeWithTag("pet-app-content").performScrollToNode(hasText(app.getString(R.string.wake)))
        compose.onNodeWithText(app.getString(R.string.wake)).performClick()
        compose.waitUntil(10_000) { !org.json.JSONObject(app.repository.state.value.items.first { it.id == pet.id }.app!!.viewJson).getBoolean("asleep") }
        compose.onNodeWithTag("pet-tab-1").assertIsEnabled()
        assertEquals(version + 2u, app.repository.state.value.items.first { it.id == pet.id }.version)
        assertTrue(runBlocking { app.repository.query { it.verifyLog(pet.spaceId).valid } })
    }
    @After fun close() {
        compose.mainClock.autoAdvance = true
        if (::scenario.isInitialized) scenario.close()
        app.repository.preferences.edit().apply {
            if (hadHtmlPreference) putBoolean("miniapps.html:pet", originalHtml) else remove("miniapps.html:pet")
        }.commit()
        if (originalScale == "null") shell("settings delete global animator_duration_scale") else shell("settings put global animator_duration_scale $originalScale")
    }

    @OptIn(ExperimentalTestApi::class)
    @Test fun realNativeRunJumpsPausesSurvivesRecreationAndStoresOneSignedScore() {
        val pet = app.repository.state.value.items.first { it.app?.appId == "pet" }
        val version = pet.version
        val logCount = runBlocking { app.repository.query { it.logEvents(pet.spaceId).size } }
        compose.onNodeWithTag("pet-app-content").performScrollToNode(hasTestTag("pet-emotes-toggle"))
        compose.onNodeWithTag("pet-emotes-toggle").performClick()
        compose.onNodeWithTag("pet-app-content").performScrollToNode(hasText("😴"))
        compose.onNodeWithText("😴").assertIsDisplayed()
        compose.onNodeWithText("😴").performClick()
        compose.waitUntil(3_000) { compose.onAllNodesWithText("😴").fetchSemanticsNodes().size == 1 }
        compose.onNodeWithTag("pet-app-content").performScrollToNode(hasText(app.getString(R.string.dash)))
        compose.onNodeWithText(app.getString(R.string.dash)).assertIsDisplayed().performClick()
        compose.onNodeWithTag("dash-start").performScrollTo().assertIsDisplayed()
        compose.mainClock.autoAdvance = false
        compose.onNodeWithTag("dash-start").performClick()
        compose.mainClock.advanceTimeBy(160)
        compose.onNodeWithTag("dash-jump").performClick()
        compose.mainClock.advanceTimeBy(240)
        compose.onNodeWithTag("dash-canvas").performCustomAccessibilityActionWithLabel(app.getString(R.string.miniapp_pause))
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.autoAdvance = true
        compose.onNodeWithTag("dash-canvas").performScrollTo()
        capture("native-dash-original-sprite-airborne")
        val distance = compose.onNodeWithTag("dash-distance").fetchSemanticsNode().config[SemanticsProperties.Text].single().text
        compose.mainClock.advanceTimeBy(5_000)
        compose.onNodeWithTag("dash-distance").assertTextEquals(distance)
        scenario.recreate()
        compose.onNodeWithTag("dash-distance").assertTextEquals(distance)
        compose.onNodeWithTag("dash-continue").performScrollTo()
        compose.mainClock.autoAdvance = false
        compose.onNodeWithTag("dash-continue").performClick()
        compose.mainClock.advanceTimeBy(10_000)
        compose.waitUntil(10_000) { app.repository.state.value.items.first { it.id == pet.id }.version > version }
        val stored = app.repository.state.value.items.first { it.id == pet.id }
        val scores = JSONObject(stored.app!!.viewJson).getJSONObject("dash").getJSONObject("best")
        assertTrue(scores.keys().asSequence().any { scores.getJSONObject(it).getInt("meters") > 0 })
        assertEquals(logCount + 1, runBlocking { app.repository.query { it.logEvents(pet.spaceId).size } })
        assertTrue(runBlocking { app.repository.query { it.verifyLog(pet.spaceId).valid } })
        compose.mainClock.autoAdvance = true
        compose.onNodeWithTag("dash-start").performScrollTo().assertIsDisplayed()
        capture("native-dash-signed-score-after-recreation")
        compose.onNodeWithTag("pet-tab-2").performClick().assertIsSelected()
        val person = checkNotNull(app.repository.state.value.me).name
        val result = scores.getJSONObject(person)
        compose.onNodeWithTag("pet-leaderboard").performScrollToNode(hasText(person))
        compose.onNodeWithText(app.resources.getQuantityString(R.plurals.dash_distance, result.getInt("meters"), result.getInt("meters"))).assertIsDisplayed()
        compose.onNodeWithText(app.resources.getQuantityString(R.plurals.dash_carrot_count, result.getInt("carrots"), result.getInt("carrots"))).assertIsDisplayed()
        capture("native-pet-leaderboard-signed-score")
        Evidence.outputFile("experience", "dash-signed-score-receipt.txt").writeText(
            "PASS: real native game, grounded jump, paused distance, preserved run after Activity recreation, one signed pet_dash_score event and valid shared log.\nitem=${pet.id}; version=${stored.version}\n$scores\n")
    }

    private fun capture(name: String) {
        compose.waitForIdle()
        val image = checkNotNull(InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot())
        Evidence.outputFile("experience", "$name.png").outputStream().use {
            assertTrue(image.compress(Bitmap.CompressFormat.PNG, 100, it))
        }
        image.recycle()
    }
    private fun shell(command: String) = android.os.ParcelFileDescriptor.AutoCloseInputStream(
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)
    ).use { it.readBytes().toString(Charsets.UTF_8) }
}
