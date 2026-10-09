package xyz.tironi.zoen

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.AccessibilityServiceInfo
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProviderInfo
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.Rect
import android.os.Build
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.util.Log
import android.view.InputDevice
import android.view.MotionEvent
import android.view.ViewConfiguration
import android.view.WindowManager
import android.view.accessibility.AccessibilityNodeInfo
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.runner.lifecycle.ActivityLifecycleMonitorRegistry
import androidx.test.runner.lifecycle.Stage
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.miniapps.WidgetSnapshot
import xyz.tironi.zoen.widgets.WidgetConfigurationActivity
import xyz.tironi.zoen.widgets.ZoenWidgetProvider
import java.util.Locale
import java.util.UUID

/** The launcher owns allocation, pin acceptance, configuration, and every RemoteViews interaction. */
@RunWith(AndroidJUnit4::class)
class LauncherWidgetJourneyTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val application get() = ApplicationProvider.getApplicationContext<ZoenApplication>()
    private val automation get() = instrumentation.uiAutomation
    private val trace = JSONArray()
    private var stage = "setup"
    private var launcher = ""
    private var launcherLocale = Locale.getDefault()

    @Test fun launcherPinsOpensReconfiguresAndFeedsTheSameSignedMiniApp() {
        check(BuildConfig.DEBUG) { "This journey requires the separate debug demo database" }
        val repository = application.repository
        val manager = application.getSystemService(AppWidgetManager::class.java)
        val provider = ComponentName(application, ZoenWidgetProvider::class.java)
        val initialIds = manager.getAppWidgetIds(provider).toSet()
        val savedPreferences = listOf("demo", "onboarded", "miniapps.html:pet", "miniapps.html:list")
            .associateWith { key -> repository.preferences.all[key] as Boolean? }
        val serviceFlags = automation.serviceInfo.flags
        val suffix = UUID.randomUUID().toString().take(6)
        val listTitle = "Widget list $suffix"
        val petTitle = "Widget Pet $suffix"
        val listRow = "Opened from my launcher $suffix"
        var scenario: ActivityScenario<MainActivity>? = null
        var createdId: Int? = null
        var configuredTitle: String? = null
        var touchedDemo = false
        var failure: Throwable? = null
        try {
            runBlocking {
                repository.boot()
                check(repository.state.value.ready && repository.state.value.failure == null) { "Account ownership must be readable before entering the test demo" }
                check(repository.state.value.demo || repository.state.value.account == null) {
                    "Never switch or modify a pre-existing real account for a widget test"
                }
                if (!repository.state.value.demo) repository.useDemo()
            }
            check(repository.state.value.demo && repository.state.value.me != null && !repository.state.value.keyMissing)
            touchedDemo = true
            val owner = checkNotNull(repository.state.value.me).id
            val space = checkNotNull(repository.state.value.zoenChat).id
            // New, uniquely named demo Items avoid resetting any existing demo or launcher widget.
            val list = runBlocking { repository.change { it.installApp(space, "list", JSONObject().put("title", listTitle).put("items", JSONArray().put(listRow)).toString()) } }
            val pet = runBlocking { repository.change { it.installApp(space, "pet", JSONObject().put("name", petTitle).toString()) } }
            assertEquals(owner, list.createdBy.id)
            assertEquals(owner, pet.createdBy.id)
            assertTrue(manager.isRequestPinAppWidgetSupported)
            automation.serviceInfo = automation.serviceInfo.apply {
                flags = flags or AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS or AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS
            }
            wake()
            step("Discover the default launcher through system HOME")
            assertTrue("System HOME action must succeed", automation.performGlobalAction(AccessibilityService.GLOBAL_ACTION_HOME))
            val homeIntent = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME)
            val homeActivity = checkNotNull(application.packageManager.resolveActivity(homeIntent, PackageManager.MATCH_DEFAULT_ONLY)).activityInfo
            await("The resolved HOME activity is actually foreground") { readUi()?.packageName == homeActivity.packageName }
            launcher = checkNotNull(readUi()?.packageName)
            launcherLocale = application.packageManager.getResourcesForApplication(launcher).configuration.locales[0]
            assertEquals(homeActivity.packageName, launcher)
            val homePackages = application.packageManager.queryIntentActivities(homeIntent, PackageManager.MATCH_DEFAULT_ONLY).map { it.activityInfo.packageName }.toSet()
            assertTrue("The observed launcher must advertise a HOME activity", launcher in homePackages)
            trace.put(JSONObject().put("observedLauncher", launcher).put("resolvedHome", ComponentName(homeActivity.packageName, homeActivity.name).flattenToString()).put("homePackages", JSONArray(homePackages.toList())))
            repository.preferences.edit().putBoolean("onboarded", true).putBoolean("miniapps.html:pet", false).putBoolean("miniapps.html:list", false).commit()
            scenario = ActivityScenario.launch(Intent(application, MainActivity::class.java).putExtra("demo", true))
            scenario.onActivity { it.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) }
            compose.waitUntil(20_000) { compose.onAllNodesWithTag("home-plus").fetchSemanticsNodes().size == 1 }

            step("Reach Home widgets through the native action sheet")
            compose.onNodeWithTag("home-plus").performTouchInput { click() }
            compose.waitUntil(10_000) { compose.onAllNodesWithTag("quickActions").fetchSemanticsNodes().size == 1 }
            // Expand the actual Material3 handle before scrolling the bounded action body.
            compose.onNode(SemanticsMatcher.keyIsDefined(SemanticsActions.Expand))
                .performSemanticsAction(SemanticsActions.Expand) { assertTrue(it()) }
            compose.onNodeWithTag("quickActions").assert(hasScrollAction())
            assertTrue(compose.onNodeWithTag("quickActions").fetchSemanticsNode().config.contains(SemanticsProperties.VerticalScrollAxisRange))
            compose.onNodeWithText(application.getString(R.string.widgets)).performScrollTo().assertIsDisplayed().performClick()

            val reconfigurable = Build.VERSION.SDK_INT >= 31
            val initial = if (reconfigurable) list else pet
            configuredTitle = if (reconfigurable) listTitle else petTitle
            step("Request and accept the launcher's real widget pin")
            compose.onNodeWithTag("widget-pin:${initial.id}").performScrollTo().assertIsDisplayed().performTouchInput { click() }
            val addLabels = launcherStrings("add_to_home_screen", "place_automatically")
            val accept = awaitNode("Launcher pin acceptance button") { node ->
                node.packageName == launcher && node.className == "android.widget.Button" && node.enabled && node.clickable && matchesLabel(node.text, addLabels)
            }
            assertEquals("The launcher must own the pin confirmation window", launcher, readUi()?.packageName)
            if (Build.VERSION.SDK_INT >= 31) {
                assertTrue(awaitNode("Pin preview for the selected Item") { it.id == "${application.packageName}:id/widget_title" && it.text == configuredTitle }.visible)
            } else {
                // Older Launcher3 renders RemoteViews into a bitmap, without exposing provider text nodes.
                val cell = awaitNode("Launcher-owned raster widget preview cell") {
                    it.packageName == launcher && it.id == "$launcher:id/widget_cell" && it.className == "com.android.launcher3.widget.WidgetCell"
                }
                val children = cell.descendants().filter { it.visible && it.packageName == launcher }
                val providerLabel = manager.installedProviders.single { it.provider == provider }.loadLabel(application.packageManager)
                assertEquals(providerLabel, children.single { it.id == "$launcher:id/widget_name" }.text)
                val raster = children.single { it.id == "$launcher:id/widget_preview" }
                assertTrue(raster.className == "android.view.View" || raster.className == "android.widget.ImageView")
                assertFalse(raster.bounds.isEmpty)
                assertTrue(cell.bounds.contains(raster.bounds))
                trace.put(JSONObject().put("pinPreview", "launcher raster").put("provider", provider.flattenToString()).put("cell", cell.toJson()).put("raster", raster.toJson()))
            }
            capture("01-launcher-pin-dialog")
            // Re-read the exposed target after the preview capture so popup animation cannot stale its bounds.
            touch(awaitNode("Launcher pin button ready for touch") { it.packageName == launcher && it.enabled && it.clickable && it.text == accept.text && it.className == accept.className })
            await("A single launcher-allocated widget ID") { (manager.getAppWidgetIds(provider).toSet() - initialIds).size == 1 }
            createdId = (manager.getAppWidgetIds(provider).toSet() - initialIds).single()
            val widgetId = createdId
            assertNotEquals(AppWidgetManager.INVALID_APPWIDGET_ID, widgetId)
            assertEquals(provider, manager.getAppWidgetInfo(widgetId)?.provider)
            await("Pin callback binds the real ID to the selected Item and owner") {
                repository.preferences.getString("widget:$widgetId", null) == initial.id && repository.preferences.getString("widget-owner:$widgetId", null) == owner
            }
            trace.put(JSONObject().put("widgetId", widgetId).put("owner", owner).put("initialItem", initial.id))

            step("Touch the pinned widget on the actual launcher")
            home()
            val openDeadline = SystemClock.uptimeMillis() + 10_000
            awaitWidget(checkNotNull(configuredTitle), openDeadline)
            capture("02-launcher-pinned-widget")
            // A capture can outlive a launcher transition. Never reuse pin-preview coordinates.
            touch(awaitWidget(checkNotNull(configuredTitle), openDeadline).title)
            val openedText = if (reconfigurable) listRow else petTitle
            compose.waitUntil(10_000) {
                readUi()?.packageName == application.packageName &&
                    compose.onAllNodesWithText(openedText).fetchSemanticsNodes(atLeastOneRootRequired = false).isNotEmpty()
            }
            compose.onAllNodesWithText(openedText).onFirst().assertIsDisplayed()
            assertEquals(application.packageName, readUi()?.packageName)
            capture("03-widget-opens-native-miniapp")

            if (reconfigurable) {
                step("Long-press and edit the same widget through the launcher")
                assertTrue(manager.getAppWidgetInfo(widgetId).widgetFeatures and AppWidgetProviderInfo.WIDGET_FEATURE_RECONFIGURABLE != 0)
                home()
                longPress(awaitWidget(listTitle).title)
                val editId = "$launcher:id/widget_reconfigure_button"
                val instructionLabels = launcherStrings("reconfigurable_widget_education_tip")
                var editOrInstruction: UiNode? = null
                await("Launcher reconfigure control or its first-use instruction") {
                    val nodes = readUi()?.descendants().orEmpty().filter { it.visible && it.enabled }
                    editOrInstruction = nodes.singleOrNull { it.clickable && it.id == editId }
                        ?: nodes.singleOrNull { it.text in instructionLabels }
                    editOrInstruction != null
                }
                if (checkNotNull(editOrInstruction).id != editId) {
                    // Launcher3's instruction temporarily hides the resize frame from accessibility.
                    // Its real touch handler closes the instruction without selecting the widget behind it.
                    capture("04-launcher-first-use-edit-instruction")
                    touch(checkNotNull(editOrInstruction))
                }
                val edit = awaitNode("Launcher widget reconfigure control") { it.enabled && it.clickable && it.id == editId }
                capture("04-launcher-widget-edit-control")
                touch(edit)
                compose.waitUntil(10_000) {
                    readUi()?.packageName == application.packageName &&
                        compose.onAllNodesWithText(application.getString(R.string.widget_choose)).fetchSemanticsNodes(atLeastOneRootRequired = false).isNotEmpty()
                }
                instrumentation.runOnMainSync {
                    val configuration = ActivityLifecycleMonitorRegistry.getInstance().getActivitiesInStage(Stage.RESUMED).filterIsInstance<WidgetConfigurationActivity>().single()
                    assertEquals(widgetId, configuration.intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID))
                    configuration.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                }
                compose.onNodeWithText(petTitle).performScrollTo().assertIsDisplayed()
                capture("05-real-widget-configuration-pet")
                compose.onNodeWithText(petTitle).performTouchInput { click() }
                await("Configuration saves Pet on the existing launcher widget ID") { repository.preferences.getString("widget:$widgetId", null) == pet.id }
                configuredTitle = petTitle
                assertEquals(setOf(widgetId), manager.getAppWidgetIds(provider).toSet() - initialIds)
                assertEquals(owner, repository.preferences.getString("widget-owner:$widgetId", null))
            } else {
                trace.put(JSONObject().put("stage", "Reconfigure").put("status", "unsupported").put("reason", "Android ${Build.VERSION.SDK_INT} ignores widgetFeatures=reconfigurable; Android 12/API31 is required. Pin, open, Feed, and signed state are still exercised."))
                Log.i(TAG, "API ${Build.VERSION.SDK_INT}: only launcher reconfiguration is unsupported; pin/open/Feed continue")
            }

            step("Press Feed on the live launcher RemoteViews")
            home()
            val before = runBlocking { repository.query { it.item(pet.id) } }
            val beforeFullness = JSONObject(before.app!!.viewJson).getDouble("fullness")
            val beforeSnapshot = checkNotNull(WidgetSnapshot.from(before))
            val feed = beforeSnapshot.actions.single { it.tool == "pet_feed" }
            val feedDeadline = SystemClock.uptimeMillis() + 10_000
            val petWidget = awaitWidget(petTitle, feedDeadline)
            assertEquals(bars(beforeSnapshot), petWidget.bars?.text)
            val remoteFeed = checkNotNull(petWidget.actionOne)
            assertTrue("The native Feed label must match the signed snapshot", matchesLabel(remoteFeed.text, setOf(feed.label)))
            assertEquals(feed.label, remoteFeed.description)
            assertTrue(remoteFeed.enabled && remoteFeed.clickable)
            capture("06-launcher-pet-before-feed")
            val currentFeed = checkNotNull(awaitWidget(petTitle, feedDeadline).actionOne)
            assertTrue(matchesLabel(currentFeed.text, setOf(feed.label)))
            assertEquals(feed.label, currentFeed.description)
            assertTrue(currentFeed.enabled && currentFeed.clickable)
            touch(currentFeed)
            await("Feed creates exactly one signed version in the shared Rust engine") { repository.state.value.items.firstOrNull { it.id == pet.id }?.version == before.version + 1u }
            val after = runBlocking { repository.query { it.item(pet.id) } }
            val afterFullness = JSONObject(after.app!!.viewJson).getDouble("fullness")
            assertEquals(before.version + 1u, after.version)
            assertTrue(afterFullness > beforeFullness)
            assertEquals(owner, after.versions.single { it.number == after.version }.author.id)
            assertTrue(runBlocking { repository.query { core -> val reports = core.verifyAll(); reports.isNotEmpty() && reports.all { it.valid } } })
            val afterSnapshot = checkNotNull(WidgetSnapshot.from(after))
            await("Launcher RemoteViews displays the signed food update") { findWidget(petTitle)?.bars?.text == bars(afterSnapshot) }
            assertNotEquals(bars(beforeSnapshot), bars(afterSnapshot))
            trace.put(JSONObject().put("petItem", pet.id).put("beforeVersion", before.version.toLong()).put("afterVersion", after.version.toLong()).put("beforeFullness", beforeFullness).put("afterFullness", afterFullness).put("signedAuthor", owner).put("widgetBars", bars(afterSnapshot)))
            capture("07-launcher-pet-after-signed-feed")

            step("Open Pet from the launcher and see the same updated native state")
            touch(awaitWidget(petTitle).title)
            compose.waitUntil(10_000) {
                readUi()?.packageName == application.packageName &&
                    compose.onAllNodesWithText(petTitle).fetchSemanticsNodes(atLeastOneRootRequired = false).isNotEmpty() &&
                    compose.onAllNodesWithText("${afterFullness.toInt()}%").fetchSemanticsNodes(atLeastOneRootRequired = false).isNotEmpty()
            }
            compose.onAllNodesWithText(petTitle).onFirst().assertIsDisplayed()
            compose.onNodeWithText("${afterFullness.toInt()}%").assertIsDisplayed()
            capture("08-native-pet-matches-launcher-update")
        } catch (error: Throwable) {
            failure = error
            trace.put(JSONObject().put("failedStage", stage).put("error", error.toString()))
            if (touchedDemo) runCatching { capture("failure") }.onFailure(error::addSuppressed)
            throw error
        } finally {
            try {
                val allocated = manager.getAppWidgetIds(provider).toSet() - initialIds
                val ownedId = createdId ?: allocated.singleOrNull()
                if (ownedId != null && touchedDemo && repository.state.value.demo) {
                    check(allocated == setOf(ownedId)) { "Another widget was allocated during this test; refusing ambiguous cleanup" }
                    step("Remove only the widget allocated by this test through the launcher")
                    home()
                    awaitWidget(checkNotNull(configuredTitle))
                    removeFromLauncher(checkNotNull(configuredTitle))
                    await("Launcher removes only the widget allocated by this test") { manager.getAppWidgetIds(provider).toSet() == initialIds }
                    await("Provider removes this deleted widget's selection") { !repository.preferences.contains("widget:$ownedId") && !repository.preferences.contains("widget-owner:$ownedId") }
                    trace.put(JSONObject().put("cleanup", "removed through the launcher's exposed Remove control").put("widgetId", ownedId))
                    capture("09-launcher-only-test-widget-removed")
                }
            } catch (cleanup: Throwable) {
                trace.put(JSONObject().put("cleanupError", cleanup.toString()))
                runCatching { capture("cleanup-failure") }.onFailure(cleanup::addSuppressed)
                if (failure == null) { failure = cleanup; throw cleanup } else failure.addSuppressed(cleanup)
            } finally {
                scenario?.close()
                automation.serviceInfo = automation.serviceInfo.apply { flags = serviceFlags }
                if (touchedDemo) {
                    val editor = repository.preferences.edit()
                    savedPreferences.forEach { (key, value) -> if (value == null) editor.remove(key) else editor.putBoolean(key, value) }
                    editor.commit()
                }
                Evidence.outputFile("widgets", "launcher-widget-journey.json").writeText(JSONObject().put("status", if (failure == null) "passed" else "failed").put("sdk", Build.VERSION.SDK_INT).put("launcher", launcher).put("initialWidgetIds", JSONArray(initialIds.toList())).put("steps", trace).toString(2))
            }
        }
    }

    private fun step(description: String) {
        stage = description
        trace.put(JSONObject().put("stage", description).put("uptimeMs", SystemClock.uptimeMillis()))
        Log.i(TAG, description)
    }

    private fun wake() {
        for (command in listOf("input keyevent KEYCODE_WAKEUP", "wm dismiss-keyguard")) {
            ParcelFileDescriptor.AutoCloseInputStream(automation.executeShellCommand(command)).use { it.readBytes() }
        }
    }

    private fun home() {
        assertTrue("System HOME action must succeed", automation.performGlobalAction(AccessibilityService.GLOBAL_ACTION_HOME))
        await("The default launcher's home workspace is foreground") { readUi()?.let(::homeWorkspace) != null }
    }

    private fun launcherStrings(vararg names: String): Set<String> {
        val resources = application.packageManager.getResourcesForApplication(launcher)
        return names.mapNotNull { name ->
            val id = resources.getIdentifier(name, "string", launcher)
            val value = id.takeIf { it != 0 }?.let(resources::getString)
            trace.put(JSONObject().put("launcherString", name).put("resourceId", id).put("value", value ?: JSONObject.NULL))
            value
        }.toSet()
    }

    private fun matchesLabel(text: String?, labels: Set<String>) = text != null && labels.any { text.uppercase(launcherLocale) == it.uppercase(launcherLocale) }

    private fun await(description: String, until: Long = SystemClock.uptimeMillis() + 10_000, ready: () -> Boolean) {
        do {
            if (SystemClock.uptimeMillis() >= until) break
            if (ready() && SystemClock.uptimeMillis() <= until) return
            SystemClock.sleep(50)
        } while (SystemClock.uptimeMillis() < until)
        throw AssertionError("$stage: $description did not become ready within 10 seconds")
    }

    private fun awaitNode(description: String, matches: (UiNode) -> Boolean): UiNode {
        var result: UiNode? = null
        await(description) {
            val matchesNow = readUi()?.descendants()?.filter { it.visible && matches(it) }.orEmpty()
            check(matchesNow.size <= 1) { "$description is ambiguous: ${matchesNow.map { it.toJson() }}" }
            result = matchesNow.singleOrNull()
            result != null
        }
        return checkNotNull(result)
    }

    private data class WidgetUi(val workspace: UiNode, val host: UiNode, val root: UiNode, val title: UiNode, val bars: UiNode?, val actionOne: UiNode?) {
        fun geometry() = listOf(workspace.bounds, host.bounds, root.bounds, title.bounds, bars?.bounds, actionOne?.bounds)
        fun toJson() = JSONObject().put("workspace", workspace.toJson()).put("host", host.toJson()).put("root", root.toJson()).put("title", title.toJson())
    }

    private fun homeWorkspace(root: UiNode): UiNode? {
        if (root.packageName != launcher) return null
        return root.descendants().singleOrNull {
            it.id == "$launcher:id/workspace" && it.packageName == launcher && it.visible && it.enabled && !it.bounds.isEmpty
        }
    }

    private fun UiNode.inside(parent: UiNode) = visible && enabled && !bounds.isEmpty && parent.bounds.contains(bounds)

    private fun findWidget(title: String): WidgetUi? {
        val root = readUi() ?: return null
        val workspace = homeWorkspace(root) ?: return null
        // The pin dialog has an AppWidgetHostView too, but it is never a workspace child.
        val hosts = workspace.descendants().filter {
            it.packageName == launcher && it.className?.endsWith("AppWidgetHostView") == true && it.inside(workspace)
        }
        val widgets = hosts.flatMap { host ->
            host.descendants().filter { it.id == "${application.packageName}:id/widget_root" && it.clickable && it.inside(host) }.mapNotNull { widget ->
                val children = widget.descendants()
                val heading = children.singleOrNull { it.id == "${application.packageName}:id/widget_title" && it.text == title && it.inside(widget) } ?: return@mapNotNull null
                WidgetUi(workspace, host, widget, heading, children.singleOrNull { it.id == "${application.packageName}:id/widget_bars" && it.inside(widget) }, children.singleOrNull { it.id == "${application.packageName}:id/widget_action_one" && it.inside(widget) })
            }
        }
        check(widgets.size <= 1) { "The test Item appears in more than one launcher widget" }
        return widgets.singleOrNull()
    }

    private fun awaitWidget(title: String, until: Long = SystemClock.uptimeMillis() + 10_000): WidgetUi {
        var result: WidgetUi? = null
        var previous: WidgetUi? = null
        await("Launcher renders widget $title at stable workspace bounds", until) {
            result = findWidget(title)
            val settled = result != null && result?.geometry() == previous?.geometry()
            previous = result
            settled
        }
        trace.put(JSONObject().put("settledWidget", checkNotNull(result).toJson()).put("uptimeMs", SystemClock.uptimeMillis()))
        return checkNotNull(result)
    }

    private fun touch(node: UiNode) = gesture(node, 80)
    private fun longPress(node: UiNode) = gesture(node, ViewConfiguration.getLongPressTimeout().toLong() + 200)

    private fun gesture(node: UiNode, holdMs: Long) {
        check(node.visible && node.enabled && !node.bounds.isEmpty) { "Cannot touch a hidden or disabled node" }
        val now = SystemClock.uptimeMillis()
        trace.put(JSONObject().put("touch", node.toJson()).put("holdMs", holdMs).put("uptimeMs", now))
        fun inject(action: Int) {
            val event = MotionEvent.obtain(now, SystemClock.uptimeMillis(), action, node.bounds.exactCenterX(), node.bounds.exactCenterY(), 0).apply { source = InputDevice.SOURCE_TOUCHSCREEN }
            try { assertTrue("Native touchscreen injection must succeed", automation.injectInputEvent(event, true)) }
            finally { event.recycle() }
        }
        inject(MotionEvent.ACTION_DOWN)
        try { SystemClock.sleep(holdMs) } finally { inject(MotionEvent.ACTION_UP) }
    }

    @Suppress("DEPRECATION")
    private fun removeFromLauncher(title: String) {
        val removeLabel = launcherStrings("remove_drop_target_label").single()
        val moveLabels = launcherStrings("action_move")
        val root = checkNotNull(automation.rootInActiveWindow)
        var node: AccessibilityNodeInfo? = null
        var moveNode: AccessibilityNodeInfo? = null
        var moveAction: AccessibilityNodeInfo.AccessibilityAction? = null
        try {
            check(root.packageName?.toString() == launcher) { "Only the actual launcher can remove the test widget" }
            var remaining = 512
            fun visit(candidate: AccessibilityNodeInfo) {
                check(remaining-- > 0) { "Accessibility tree exceeds the bounded widget probe" }
                if (candidate.isVisibleToUser && candidate.viewIdResourceName == "${application.packageName}:id/widget_title" && candidate.text?.toString() == title) {
                    check(node == null) { "The test Item appears in more than one visible launcher widget" }
                    node = AccessibilityNodeInfo.obtain(candidate)
                }
                for (index in 0 until candidate.childCount) {
                    candidate.getChild(index)?.let { child -> try { visit(child) } finally { child.recycle() } }
                }
            }
            // RemoteViews IDs belong to the provider, so the launcher's ID lookup can miss them.
            visit(root)
            check(node != null) { "Launcher exposes no visible title for the widget created by this test" }
            for (depth in 0 until 12) {
                val current = checkNotNull(node)
                trace.put(JSONObject().put("cleanupAncestor", depth).put("class", current.className).put("actions", JSONArray(current.actionList.map { "${it.id}:${it.label}" })))
                val action = current.actionList.singleOrNull { matchesLabel(it.label?.toString(), setOf(removeLabel)) }
                if (action != null) {
                    // Invoke the action advertised by this widget's actual launcher ancestor.
                    assertTrue("Launcher must remove its own selected widget", current.performAction(action.id))
                    return
                }
                if (moveNode == null) {
                    current.actionList.singleOrNull { matchesLabel(it.label?.toString(), moveLabels) }?.let {
                        moveNode = AccessibilityNodeInfo.obtain(current)
                        moveAction = it
                    }
                }
                node = current.parent
                current.recycle()
                if (node == null) break
                check(depth < 11) { "Launcher widget ancestry is unexpectedly deep" }
            }
            // Some launchers expose Move on the widget and Remove only during accessible drag.
            trace.put(JSONObject().put("cleanupMoveAction", moveAction?.id).put("label", moveAction?.label).put("ownedTitle", title).put("uptimeMs", SystemClock.uptimeMillis()))
            assertTrue("Launcher must start moving only its own selected widget", checkNotNull(moveNode) { "Launcher exposes neither Remove nor Move for the test widget" }.performAction(checkNotNull(moveAction).id))
        } finally { node?.recycle(); moveNode?.recycle(); root.recycle() }
        val remove = awaitNode("The launcher's Remove drop target for the selected widget") {
            it.packageName == launcher && it.enabled && it.clickable && (matchesLabel(it.text, setOf(removeLabel)) || matchesLabel(it.description, setOf(removeLabel)))
        }
        capture("09-launcher-accessible-remove-target")
        touch(awaitNode("Launcher Remove target ready for touch") { it.packageName == launcher && it.enabled && it.clickable && it.text == remove.text && it.description == remove.description && it.className == remove.className })
    }

    private data class UiNode(val id: String?, val text: String?, val description: String?, val className: String?, val packageName: String?, val bounds: Rect, val visible: Boolean, val enabled: Boolean, val clickable: Boolean, val actions: List<String>, val children: List<UiNode>) {
        fun descendants(): List<UiNode> = listOf(this) + children.flatMap(UiNode::descendants)
        fun toJson(): JSONObject = JSONObject().put("id", id).put("text", text).put("description", description).put("class", className).put("package", packageName).put("bounds", bounds.toShortString()).put("visible", visible).put("enabled", enabled).put("clickable", clickable).put("actions", JSONArray(actions))
    }

    @Suppress("DEPRECATION")
    private fun readUi(): UiNode? {
        val root = automation.rootInActiveWindow ?: return null
        var remaining = 512
        fun read(node: AccessibilityNodeInfo): UiNode? {
            check(remaining-- > 0) { "Accessibility tree exceeds the bounded widget probe" }
            // Window animation changes screen bounds before every cached accessibility node catches up.
            if (!node.refresh()) return null
            val bounds = Rect().also(node::getBoundsInScreen)
            val children = (0 until node.childCount).mapNotNull { index -> node.getChild(index)?.let { child -> try { read(child) } finally { child.recycle() } } }
            return UiNode(node.viewIdResourceName, node.text?.toString(), node.contentDescription?.toString(), node.className?.toString(), node.packageName?.toString(), bounds, node.isVisibleToUser, node.isEnabled, node.isClickable, node.actionList.map { "${it.id}:${it.label}" }, children)
        }
        return try { read(root) } finally { root.recycle() }
    }

    private fun capture(name: String) {
        val bitmap = checkNotNull(automation.takeScreenshot()) { "Native launcher screenshot was unavailable" }
        try { Evidence.outputFile("widgets", "$name.png").outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) } }
        finally { bitmap.recycle() }
        val snapshot = readUi()
        Evidence.outputFile("widgets", "$name.json").writeText(JSONObject().put("stage", stage).put("sdk", Build.VERSION.SDK_INT).put("launcher", launcher).put("displayDensity", application.resources.displayMetrics.density).put("ui", JSONArray(snapshot?.descendants()?.map(UiNode::toJson).orEmpty())).toString(2))
    }

    private fun bars(snapshot: WidgetSnapshot) = snapshot.bars.joinToString(" · ") { "${it.label} ${(it.value * 100).toInt()}%" }

    companion object { private const val TAG = "LauncherWidgetJourney" }
}
