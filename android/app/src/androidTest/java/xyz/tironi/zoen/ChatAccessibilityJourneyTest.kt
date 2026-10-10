package xyz.tironi.zoen

import android.content.Intent
import android.graphics.BitmapFactory
import android.graphics.Color
import android.net.Uri
import android.os.ParcelFileDescriptor
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.Timeout
import org.junit.runner.RunWith
import kotlin.math.ceil
import kotlin.math.floor
import xyz.tironi.zoen.core.SpaceKindDto

@RunWith(AndroidJUnit4::class)
class ChatAccessibilityJourneyTest {
    @get:Rule(order = 0) val deadline = Timeout.seconds(120)
    @get:Rule(order = 1) val compose = createEmptyComposeRule()
    private val instrumentation get() = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation()

    @Test fun fullChatTitleAndParticipantsRemainReachableAtSystem200PercentText() {
        val originalFont = shell("settings get system font_scale").trim()
        val originalMotion = shell("settings get global animator_duration_scale").trim()
        val app = ApplicationProvider.getApplicationContext<ZoenApplication>()
        var scenario: ActivityScenario<MainActivity>? = null
        try {
            shell("settings put system font_scale 2.0")
            shell("settings put global animator_duration_scale 0")
            app.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
            scenario = ActivityScenario.launch(Intent(app, MainActivity::class.java).putExtra("demo", true))
            compose.waitUntil(30_000) { app.repository.state.value.ready && app.repository.state.value.me != null }
            runBlocking { app.repository.resetDemo() }
            val space = app.repository.state.value.spaces.first { it.kind == SpaceKindDto.GROUP }
            val before = runBlocking { app.repository.query { it.logEvents(space.id).size } }
            scenario.onActivity {
                assertEquals(2f, it.resources.configuration.fontScale, .01f)
                it.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("zoen://chat/${space.id}"), it, MainActivity::class.java)
                    .putExtra("demo", true).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP))
            }
            compose.waitUntil(10_000) { compose.onAllNodesWithTag("composer").fetchSemanticsNodes().isNotEmpty() }
            val title = compose.onNodeWithText(space.title, useUnmergedTree = true)
            title.assertIsDisplayed()
            lateinit var activity: MainActivity
            scenario.onActivity { activity = it }
            CommittedWindowCapture.save(activity, "experience", "native-chat-system-font200")
            assertFullTitle(space.title)
            assertRenderedTitle(space.title, "native-chat-system-font200")
            compose.onNodeWithContentDescription(app.getString(R.string.back)).assertIsDisplayed()
            compose.onNodeWithContentDescription(app.getString(R.string.more)).assertIsDisplayed()
            title.performClick()
            compose.onNodeWithText(app.getString(R.string.participants)).assertIsDisplayed()
            assertEquals(before, runBlocking { app.repository.query { it.logEvents(space.id).size } })
            assertTrue(runBlocking { app.repository.query { it.verifyLog(space.id).valid } })
            val longTitle = "Paraty with Marina: travel plans and weekend reservations"
            val community = runBlocking { app.repository.change { it.createCommunity(longTitle) } }
            scenario.onActivity {
                it.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("zoen://chat/$community"), it, MainActivity::class.java)
                    .putExtra("demo", true).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP))
            }
            compose.waitUntil(10_000) { compose.onAllNodesWithText(longTitle, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
            assertTrue("Long native titles must wrap at system 200% text", assertFullTitle(longTitle).lineCount > 1)
            compose.onNodeWithTag("composer").assertIsDisplayed()
            compose.onNodeWithContentDescription(app.getString(R.string.back)).assertIsDisplayed()
            compose.onNodeWithContentDescription(app.getString(R.string.more)).assertIsDisplayed()
            compose.onNodeWithText(app.resources.getQuantityString(R.plurals.chat_people_count, 1, 1)).assertIsDisplayed()
            compose.waitForIdle()
            CommittedWindowCapture.save(activity, "experience", "native-chat-system-font200-long-title")
            assertRenderedTitle(longTitle, "native-chat-system-font200-long-title")
            assertTrue(runBlocking { app.repository.query { it.verifyLog(community).valid } })
            Evidence.outputFile("experience", "native-chat-system-font200-receipt.txt").writeText(
                "PASS: actual system font_scale=2.0; original and newly signed long chat titles show every character without ellipsis or clipped glyph bounds; the long title wraps; native Back, More and composer remain visible; tapping the original title opens Participants without changing the signed log.\n")
        } finally {
            try { scenario?.close() } finally {
                if (originalFont == "null") shell("settings delete system font_scale") else shell("settings put system font_scale $originalFont")
                if (originalMotion == "null") shell("settings delete global animator_duration_scale") else shell("settings put global animator_duration_scale $originalMotion")
            }
        }
    }

    private fun assertFullTitle(text: String): TextLayoutResult {
        val title = compose.onNodeWithText(text, useUnmergedTree = true)
        title.assertIsDisplayed()
        val layouts = mutableListOf<TextLayoutResult>()
        title.performSemanticsAction(SemanticsActions.GetTextLayoutResult) { it(layouts) }
        assertEquals(1, layouts.size)
        val layout = layouts.single()
        val bounds = title.fetchSemanticsNode().boundsInRoot
        assertEquals("The complete native title must fit the visible viewport width", layout.size.width.toFloat(), bounds.width, 1f)
        assertEquals("The complete native title must fit the visible viewport height", layout.size.height.toFloat(), bounds.height, 1f)
        val ellipsized = (0 until layout.lineCount).map(layout::isLineEllipsized)
        Evidence.outputFile("experience", "native-chat-system-font200-layout.txt").appendText(
            "text=${layout.layoutInput.text} size=${layout.size} constraints=${layout.layoutInput.constraints} " +
                "paragraph=${layout.multiParagraph.width}x${layout.multiParagraph.height} lines=${layout.lineCount} " +
                "widthOverflow=${layout.didOverflowWidth} heightOverflow=${layout.didOverflowHeight} ellipsized=$ellipsized\n")
        assertFalse("The full chat title must remain readable at system 200% text", ellipsized.any { it })
        assertEquals(text.length, layout.getLineEnd(layout.lineCount - 1, visibleEnd = true))
        text.indices.filter { !text[it].isWhitespace() }.forEach { offset ->
            val glyph = layout.getBoundingBox(offset)
            assertTrue("Title character $offset must fit its native text bounds: $glyph in ${layout.size}",
                glyph.left >= -1f && glyph.right <= layout.size.width + 1f &&
                    glyph.top >= -1f && glyph.bottom <= layout.size.height + 1f)
        }
        return layout
    }

    private fun assertRenderedTitle(text: String, capture: String) {
        val title = compose.onNodeWithText(text, useUnmergedTree = true)
        val layouts = mutableListOf<TextLayoutResult>()
        title.performSemanticsAction(SemanticsActions.GetTextLayoutResult) { it(layouts) }
        val layout = layouts.single()
        val bounds = title.fetchSemanticsNode().boundsInWindow
        val bitmap = checkNotNull(BitmapFactory.decodeFile(Evidence.outputFile("experience", "$capture.png").path))
        try {
            text.indices.filter { !text[it].isWhitespace() }.forEach { offset ->
                val glyph = layout.getBoundingBox(offset)
                val left = floor(bounds.left + glyph.left).toInt().coerceIn(0, bitmap.width - 1)
                val right = ceil(bounds.left + glyph.right).toInt().coerceIn(left + 1, bitmap.width)
                val top = floor(bounds.top + glyph.top).toInt().coerceIn(0, bitmap.height - 1)
                val bottom = ceil(bounds.top + glyph.bottom).toInt().coerceIn(top + 1, bitmap.height)
                val visibleInk = (top until bottom).any { y -> (left until right).any { x ->
                    val pixel = bitmap.getPixel(x, y)
                    Color.red(pixel) < 160 && Color.green(pixel) < 160 && Color.blue(pixel) < 160
                } }
                assertTrue("Actual committed frame must render title character $offset (${text[offset]}) in $capture", visibleInk)
            }
        } finally { bitmap.recycle() }
    }

    private fun shell(command: String): String = ParcelFileDescriptor.AutoCloseInputStream(
        instrumentation.uiAutomation.executeShellCommand(command)).use { it.readBytes().toString(Charsets.UTF_8) }
}
