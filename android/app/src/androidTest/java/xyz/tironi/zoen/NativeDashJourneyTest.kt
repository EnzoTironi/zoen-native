package xyz.tironi.zoen

import android.content.Intent
import android.net.Uri
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
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

    @Before fun open() {
        val preferences = app.repository.preferences
        hadHtmlPreference = preferences.contains("miniapps.html:pet")
        originalHtml = preferences.getBoolean("miniapps.html:pet", false)
        preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).putBoolean("miniapps.html:pet", false).commit()
        runBlocking { app.repository.boot(true); app.repository.resetDemo() }
        val pet = app.repository.state.value.items.first { it.app?.appId == "pet" }
        scenario = ActivityScenario.launch(Intent(app, MainActivity::class.java).putExtra("demo", true)
            .setAction(Intent.ACTION_VIEW).setData(Uri.parse("zoen://app/${pet.id}")))
    }
    @After fun close() {
        compose.mainClock.autoAdvance = true
        if (::scenario.isInitialized) scenario.close()
        app.repository.preferences.edit().apply {
            if (hadHtmlPreference) putBoolean("miniapps.html:pet", originalHtml) else remove("miniapps.html:pet")
        }.commit()
    }

    @Test fun realNativeRunJumpsPausesSurvivesRecreationAndStoresOneSignedScore() {
        val pet = app.repository.state.value.items.first { it.app?.appId == "pet" }
        val version = pet.version
        val logCount = runBlocking { app.repository.query { it.logEvents(pet.spaceId).size } }
        compose.onNodeWithText(app.getString(R.string.dash)).performScrollTo().performClick()
        compose.onNodeWithTag("dash-start").performScrollTo().assertIsDisplayed()
        compose.mainClock.autoAdvance = false
        compose.onNodeWithTag("dash-start").performClick()
        compose.mainClock.advanceTimeBy(160)
        compose.onNodeWithTag("dash-jump").performClick()
        compose.mainClock.advanceTimeBy(240)
        compose.onNodeWithTag("dash-canvas").performScrollTo()
        capture("native-dash-original-sprite-airborne")
        compose.onNodeWithTag("dash-pause").performScrollTo().performClick()
        compose.mainClock.advanceTimeByFrame()
        val distance = compose.onNodeWithTag("dash-distance").fetchSemanticsNode().config[SemanticsProperties.Text].single().text
        compose.mainClock.advanceTimeBy(5_000)
        compose.onNodeWithTag("dash-distance").assertTextEquals(distance)
        scenario.recreate()
        compose.mainClock.advanceTimeByFrame()
        compose.onNodeWithTag("dash-distance").assertTextEquals(distance)
        compose.onNodeWithTag("dash-continue").performScrollTo().performClick()
        compose.mainClock.advanceTimeBy(10_000)
        compose.waitUntil(10_000) { app.repository.state.value.items.first { it.id == pet.id }.version > version }
        val stored = app.repository.state.value.items.first { it.id == pet.id }
        val scores = JSONObject(stored.app!!.viewJson).getJSONObject("dash").getJSONObject("best")
        assertTrue(scores.keys().asSequence().any { scores.getJSONObject(it).getInt("meters") > 0 })
        assertEquals(logCount + 1, runBlocking { app.repository.query { it.logEvents(pet.spaceId).size } })
        assertTrue(runBlocking { app.repository.query { it.verifyLog(pet.spaceId).valid } })
        compose.onNodeWithTag("dash-start").performScrollTo().assertIsDisplayed()
        capture("native-dash-signed-score-after-recreation")
        Evidence.outputFile("experience", "dash-signed-score-receipt.txt").writeText(
            "PASS: real native game, grounded jump, paused distance, preserved run after Activity recreation, one signed pet_dash_score event and valid shared log.\n$scores\n")
    }

    private fun capture(name: String) {
        lateinit var activity: MainActivity
        scenario.onActivity { activity = it }
        CommittedWindowCapture.save(activity, "experience", name)
    }
}
