package xyz.tironi.zoen

import android.content.Intent
import android.graphics.Bitmap
import android.os.ParcelFileDescriptor
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.json.JSONObject
import xyz.tironi.zoen.core.MarkdownFileDto
import xyz.tironi.zoen.core.SpaceKindDto

/** Native production components, real local engine and synthetic demo data; not relay E2E. */
@RunWith(AndroidJUnit4::class)
class LivingSpaceJourneyTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val testName = org.junit.rules.TestName()
    private val fixtureSecrets = mutableMapOf<String, ByteArray>()
    private val fixtureVault = object : xyz.tironi.zoen.core.SecretVault {
        override fun load(key: String) = fixtureSecrets[key]?.copyOf()
        override fun save(key: String, value: ByteArray): Boolean { fixtureSecrets[key] = value.copyOf(); return true }
        override fun delete(key: String) { fixtureSecrets.remove(key) }
    }
    private val app get() = ApplicationProvider.getApplicationContext<ZoenApplication>()
    private lateinit var scenario: ActivityScenario<MainActivity>
    private var spaceId = ""
    private var theme = "light"
    @Before fun open() {
        if (android.os.Build.VERSION.SDK_INT >= 33) app.getSystemService(android.app.LocaleManager::class.java).applicationLocales = android.os.LocaleList.forLanguageTags("en-US")
        app.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).putBoolean("LivingSpacesEnabled", true).commit()
        scenario = ActivityScenario.launch(Intent(app, MainActivity::class.java).putExtra("demo", true))
        compose.waitUntil(30_000) { app.repository.state.value.ready && app.repository.state.value.me != null }
        runBlocking {
            app.repository.resetDemo()
            if (!testName.methodName.startsWith("lightHome") && !testName.methodName.startsWith("darkHome")) {
                // Separate demo SQLite file + in-memory vault: never touches the user's real account or vault.
                app.repository.change { it.createAccount("Alex", "livingfixture", "https://relay.test", fixtureVault) }
            }
            val created = app.repository.change { it.createCommunity("Living Space") }
            val space = app.repository.state.value.spaces.first { it.id == created }
            spaceId = space.id
            app.repository.change { core ->
                core.setLivingSpaceEnabled(true)
                val page = core.pagesImportMarkdown(space.id, listOf(MarkdownFileDto("Welcome to our Space.md", "# Welcome to our Space\n\nA shared place to plan, learn and stay connected.\n\n## This week\n\n- [ ] Share your ideas\n- [ ] Plan our next gathering\n\nEveryone can help keep this page up to date."))).single()
                core.livingSpaceCommand(space.id, JSONObject().put("action", "set_home").put("page_id", page.id).toString())
                core.livingSpaceCommand(space.id, JSONObject().put("action", "announce").put("item_id", page.id).toString())
                val bytes = InstrumentationRegistry.getInstrumentation().context.assets.open("living-space-cover.png").use { it.readBytes() }
                val image = core.fileAdd(space.id, "Space cover", "space-cover.png", "image/png", bytes, bytes)
                core.livingSpaceCommand(space.id, JSONObject().put("action", "set_cover").put("file_id", image.id).toString())
                if (testName.methodName.startsWith("lightHome") || testName.methodName.startsWith("darkHome")) core.livingSpaceCommand(space.id, "{\"action\":\"demo_join_request\"}")
            }
        }
        compose.waitForIdle()
    }
    @After fun close() {
        scenario.close()
        runBlocking { app.repository.resetDemo() }
        fixtureSecrets.clear()
        app.repository.preferences.edit().putBoolean("LivingSpacesEnabled", false).commit()
    }
    private fun scrollSection(title: String) {
        val section = compose.onNodeWithText(title, substring = false)
        section.performScrollTo()
        val container = compose.onNodeWithTag("living-space-hackathon")
        val delta = section.fetchSemanticsNode().boundsInRoot.top - container.fetchSemanticsNode().boundsInRoot.top - 20 * app.resources.displayMetrics.density
        container.performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.ScrollBy) { it(0f, delta) }
        compose.waitForIdle()
    }
    private fun shot(name: String) {
        val bitmap = checkNotNull(InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot())
        try { Evidence.outputFile("living-space", name.replace("android-", "android-$theme-")).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) } }
        finally { bitmap.recycle() }
    }
    @Test fun lightHomeUsesRealPageAndNotificationsTab() = journey("light")
    @Test fun darkHomeUsesRealPageAndNotificationsTab() = journey("dark")
    @Test fun lightHackathonTeamSubmissionAndResultsUseCore() = hackathonJourney("light")
    @Test fun darkHackathonTeamSubmissionAndResultsUseCore() = hackathonJourney("dark")
    private fun hackathonJourney(appearance: String) {
        theme = appearance
        compose.runOnIdle { xyz.tironi.zoen.theme.AppAppearance.set(app, appearance) }
        val deadline = System.currentTimeMillis() + 100_000
        runBlocking { app.repository.change { core ->
            core.livingSpaceCommand(spaceId, JSONObject().put("action", "hackathon_create").put("title", "Build together").put("deadline_ms", deadline).toString())
        } }
        val space = app.repository.state.value.spaces.first { it.id == spaceId }
        compose.onNodeWithText(space.title, substring = false).performClick()
        compose.onNodeWithTag("living-space-open-hackathon").performScrollTo().performClick()
        compose.onNodeWithTag("living-space-hackathon").assertExists()
        shot("android-hackathon.png")
        compose.onNodeWithText("Register", substring = false).performScrollTo().performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithText("Registered", substring = false).fetchSemanticsNodes().isNotEmpty() }
        compose.onNode(hasSetTextAction() and hasText("Team name")).performScrollTo().performTextInput("Garden builders")
        compose.onNodeWithText("Create team", substring = false).performScrollTo().performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithText("Open team conversation", substring = false).fetchSemanticsNodes().isNotEmpty() }
        scrollSection("Find teammates")
        shot("android-teams.png")
        compose.onNode(hasSetTextAction() and hasText("Project URL")).performScrollTo().performTextInput("https://example.com/garden")
        compose.onNodeWithText("Submit project", substring = false).performScrollTo().performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithText("Project submitted", substring = false).fetchSemanticsNodes().isNotEmpty() }
        scrollSection("Project")
        shot("android-submission.png")
        runBlocking { kotlinx.coroutines.delay((deadline - System.currentTimeMillis() + 1100).coerceAtLeast(0)) }
        compose.onNodeWithText("Publish winners announcement", substring = false).performScrollTo().performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithText("Winners", substring = false).fetchSemanticsNodes().isNotEmpty() }
        scrollSection("Winners")
        shot("android-results.png")
    }
    @Test fun authorizedHiddenConversationKeepsObservationAndRejectsUnjoinedLookup() {
        val owner = Any()
        runBlocking {
            val child = app.repository.change { core -> JSONObject(core.livingSpaceCommand(spaceId, "{\"action\":\"home\"}")).getString("conversation_space") }
            org.junit.Assert.assertFalse(app.repository.state.value.spaces.any { it.id == child })
            app.repository.observe(child, owner)
            org.junit.Assert.assertTrue(app.repository.state.value.timelines.containsKey(child))
            app.repository.change { it.sendMessage(child, "A real private conversation message") }
            org.junit.Assert.assertTrue(app.repository.state.value.timelines[child].orEmpty().any { (it.kind as? xyz.tironi.zoen.core.EntryKind.Message)?.text == "A real private conversation message" })
            val other = xyz.tironi.zoen.core.RodaEngine.open(":memory:", "en")
            try {
                val vault = object : xyz.tironi.zoen.core.SecretVault { override fun load(key: String): ByteArray? = null; override fun save(key: String, value: ByteArray) = true; override fun delete(key: String) {} }
                other.createAccount("Sam", "samfixture", "https://relay.test", vault)
                other.setLivingSpaceEnabled(true)
                val unrelated = other.createCommunity("Another private community")
                val unjoined = JSONObject(other.livingSpaceCommand(unrelated, "{\"action\":\"home\"}")).getString("conversation_space")
                org.junit.Assert.assertTrue(runCatching { app.repository.query { it.space(unjoined) } }.isFailure)
                app.repository.observe(unjoined, Any())
                org.junit.Assert.assertFalse(app.repository.state.value.timelines.containsKey(unjoined))
            } finally { other.destroy() }
            app.repository.unobserve(child, owner)
        }
    }
    @Test fun registrationFeedbackRespectsRemoveAnimations() {
        val resolver = app.contentResolver
        val original = android.provider.Settings.Global.getFloat(resolver, android.provider.Settings.Global.ANIMATOR_DURATION_SCALE, 1f)
        val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
        fun scale(value: Float) { ParcelFileDescriptor.AutoCloseInputStream(automation.executeShellCommand("settings put global animator_duration_scale $value")).use { it.readBytes() } }
        try {
            scale(0f)
            runBlocking { app.repository.change { core -> core.livingSpaceCommand(spaceId, JSONObject().put("action", "hackathon_create").put("title", "Build together").put("deadline_ms", System.currentTimeMillis() + 60_000).toString()) } }
            compose.onNodeWithText(app.repository.state.value.spaces.first { it.id == spaceId }.title, substring = false).performClick()
            compose.onNodeWithTag("living-space-open-hackathon").performScrollTo().performClick()
            compose.onNodeWithText("Register", substring = false).performScrollTo().performClick()
            compose.waitUntil(10_000) { compose.onAllNodesWithTag("living-space-confirmation").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText("Registered", substring = false).assertExists()
            scenario.onActivity { activity ->
                val image = activity.window.decorView.findViewWithTag<android.widget.ImageView>("living-space-feedback")
                org.junit.Assert.assertNotNull(image)
                org.junit.Assert.assertFalse(image.drawable is android.graphics.drawable.AnimatedVectorDrawable)
            }
        } finally { scale(original) }
    }
    @Test fun lightMembershipUsesAuthoritativeMembership() = membershipJourney("light")
    @Test fun darkMembershipUsesAuthoritativeMembership() = membershipJourney("dark")
    private fun membershipJourney(appearance: String) {
        theme = appearance
        compose.runOnIdle { xyz.tironi.zoen.theme.AppAppearance.set(app, appearance) }
        runBlocking { app.repository.change { core ->
            core.livingSpaceCommand(spaceId, "{\"action\":\"tier_create\",\"name\":\"Builder\"}")
            val tier = JSONObject(core.livingSpaceCommand(spaceId, "{\"action\":\"home\"}")).getJSONArray("tiers").getJSONObject(0)
            core.livingSpaceCommand(spaceId, JSONObject().put("action", "tier_assign").put("tier_id", tier.getString("id")).put("identity_id", app.repository.state.value.me!!.id).toString())
            core.livingSpaceCommand(spaceId, JSONObject().put("action", "tier_group_create").put("tier_id", tier.getString("id")).put("title", "Builder lounge").toString())
            val page = core.pagesImportMarkdown(tier.getString("space"), listOf(MarkdownFileDto("Builder notes.md", "# Builder notes\n\nA shared page for community builders."))).single()
            core.livingSpaceCommand(spaceId, JSONObject().put("action", "tier_bind").put("tier_id", tier.getString("id")).put("item_id", page.id).put("kind", "page").toString())
        } }
        val space = app.repository.state.value.spaces.first { it.id == spaceId }
        compose.onNodeWithText(space.title, substring = false).performClick()
        compose.onNodeWithTag("living-space-open-tiers").performScrollTo().performClick()
        compose.onNodeWithTag("living-space-tiers").assertExists()
        compose.onAllNodesWithText("Builder", substring = false).onFirst().assertExists()
        shot("android-tiers.png")
        compose.onNodeWithText("Create invitation", substring = false).performScrollTo().performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithText("Revoke invitation", substring = false).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithContentDescription(app.getString(R.string.back)).performClick()
        compose.onNodeWithTag("living-space-open-tiers").performScrollTo().performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithText("Revoke invitation", substring = false).fetchSemanticsNodes().isNotEmpty() }
        compose.onAllNodesWithText("Copy invitation", substring = false).assertCountEquals(0)
        compose.onNodeWithText("Revoke invitation", substring = false).performScrollTo().performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithText("Revoked", substring = false).fetchSemanticsNodes().isNotEmpty() }
    }
    private fun journey(appearance: String) {
        theme = appearance
        compose.runOnIdle { xyz.tironi.zoen.theme.AppAppearance.set(app, appearance) }
        val space = app.repository.state.value.spaces.first { it.id == spaceId }
        compose.onNodeWithText(space.title, substring = false).performClick()
        compose.onNodeWithTag("living-space-home").assertExists()
        compose.onNodeWithText("Pinned announcements").assertExists()
        shot("android-home.png")
        compose.onNodeWithTag("living-space-open-announcements").performScrollTo().performClick()
        compose.onNodeWithTag("living-space-announcements").assertExists()
        shot("android-announcements.png")
        compose.onNodeWithText("Done", substring = false).performClick()
        compose.onAllNodesWithText("Welcome to our Space", substring = false).onLast().performScrollTo().performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("living-space-home").fetchSemanticsNodes().isEmpty() }
        compose.waitUntil(10_000) { compose.onAllNodesWithTag("page-save").fetchSemanticsNodes().isNotEmpty() }
        shot("android-page.png")
        compose.onNodeWithText("Notifications", substring = false).performClick()
        compose.onNodeWithTag("living-space-notifications").assertExists()
        compose.waitUntil(10_000) { compose.onAllNodesWithText("Join request", substring = false).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Join request", substring = false).assertIsDisplayed()
        shot("android-notifications.png")
        compose.onNodeWithText("Approve", substring = false).performClick()
        compose.waitUntil(10_000) { app.repository.state.value.spaces.first { it.id == spaceId }.members.size == 2 }
        compose.onNodeWithTag("living-space-confirmation").assertExists()
    }
}
