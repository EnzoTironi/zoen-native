package xyz.tironi.zoen

import android.content.Intent
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.core.RequestStatus

@RunWith(AndroidJUnit4::class)
class NativeJourneysTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val application get() = ApplicationProvider.getApplicationContext<ZoenApplication>()
    private lateinit var scenario: ActivityScenario<MainActivity>

    @Before fun openDemo() {
        application.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        scenario = ActivityScenario.launch(Intent(application, MainActivity::class.java).putExtra("demo", true))
        compose.waitUntil(30_000) { application.repository.state.value.ready && application.repository.state.value.me != null }
        runBlocking { application.repository.resetDemo() }
        compose.waitForIdle()
    }
    @After fun close() { scenario.close() }

    @Test fun createsAPlanThroughTheComposerAndKeepsItAfterRecreation() {
        compose.onNodeWithTag("chat:zoen").performClick()
        compose.onNodeWithTag("composer").performTextInput("Android weekend trip up to $1,500")
        compose.onNodeWithTag("send").performClick()
        compose.waitUntil(15_000) { application.repository.state.value.items.any { it.title == "Android weekend trip up to $1,500" } }
        val id = application.repository.state.value.items.first { it.title == "Android weekend trip up to $1,500" }.id
        compose.onNodeWithTag("item:$id").performScrollTo().performClick()
        compose.onAllNodes(isToggleable())[0].performScrollTo().performClick()
        compose.waitUntil(10_000) { application.repository.state.value.items.first { it.id == id }.plan!!.sections.first().lines.first().done }
        scenario.recreate()
        compose.onAllNodes(isToggleable())[0].assertIsOn()
        assertTrue(runBlocking { application.repository.query { it.verifyAll().all { report -> report.valid } } })
    }

    @Test fun approvalsAreReviewedAndRecordedByTheSharedEngine() {
        val request = application.repository.state.value.requests.first { it.status == RequestStatus.PENDING }
        compose.onNodeWithText(application.getString(R.string.activity)).performClick()
        compose.onNodeWithText(request.title).performScrollTo().performClick()
        compose.onNodeWithText(application.getString(R.string.approve), substring = false).performClick()
        compose.onNode(isDialog()).assertExists()
        compose.onAllNodesWithText(application.getString(R.string.approve), substring = false).onLast().performClick()
        compose.waitUntil(10_000) { application.repository.state.value.requests.first { it.id == request.id }.status == RequestStatus.APPROVED }
        compose.onNodeWithText(application.getString(R.string.status_approved)).assertExists()
    }

    @Test fun searchFindsThePersistedStory() {
        compose.onNodeWithContentDescription(application.getString(R.string.search)).performClick()
        compose.onNodeWithTag("search").performTextInput("Paraty")
        val result = hasText("Paraty", substring = true) and hasClickAction() and !hasSetTextAction()
        compose.waitUntil(10_000) { compose.onAllNodes(result).fetchSemanticsNodes().isNotEmpty() }
        compose.onAllNodes(result).onFirst().performClick()
        compose.onNodeWithTag("composer").assertExists()
    }
}
