package xyz.tironi.zoen

import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import android.os.Bundle
import android.os.SystemClock
import android.view.accessibility.AccessibilityNodeInfo
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import kotlinx.coroutines.runBlocking
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.theme.AppAppearance

/** Uses the real activity and platform input connection, not Compose's intercepted test IME. */
@RunWith(AndroidJUnit4::class)
class BottomComposerParityCaptureTest {
    @Test fun composerLight() = capture(false)
    @Test fun composerDark() = capture(true)

    private fun await(condition: () -> Boolean) {
        val deadline = SystemClock.uptimeMillis() + 30_000
        while (!condition()) {
            check(SystemClock.uptimeMillis() < deadline) { "Real composer/IME did not become ready" }
            SystemClock.sleep(100)
        }
    }

    private fun editable(node: AccessibilityNodeInfo?): AccessibilityNodeInfo? {
        if (node == null) return null
        if (node.isEditable) return node
        for (index in 0 until node.childCount) editable(node.getChild(index))?.let { return it }
        return null
    }

    private fun capture(dark: Boolean) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        org.junit.Assume.assumeTrue("Opt-in deterministic parity capture", InstrumentationRegistry.getArguments().getString("parityFixture") != null)
        val app = ApplicationProvider.getApplicationContext<ZoenApplication>()
        NativeParityFixtureSetup.install(app)
        val previous = app.repository.preferences.getString("appearance", "auto") ?: "auto"
        app.repository.preferences.edit().putBoolean("demo", true).putBoolean("onboarded", true).commit()
        instrumentation.runOnMainSync { AppAppearance.set(app, if (dark) "dark" else "light") }
        try {
            ActivityScenario.launch<MainActivity>(Intent(app, MainActivity::class.java).putExtra("demo", true)).use { scenario ->
                await { app.repository.state.value.ready && app.repository.state.value.me != null }
                runBlocking { app.repository.resetDemo() }
                fun save(name: String) {
                    SystemClock.sleep(1_000)
                    await { instrumentation.uiAutomation.rootInActiveWindow?.packageName?.toString() == app.packageName }
                    NativeParityFixtureSetup.attest(app, app.repository.state.value)
                    val image = checkNotNull(instrumentation.uiAutomation.takeScreenshot())
                    try {
                        val file = File(app.cacheDir, "device-evidence/android-parity/bottom-$name-${if (dark) "dark" else "light"}.png")
                        file.parentFile!!.mkdirs()
                        file.outputStream().use { check(image.compress(Bitmap.CompressFormat.PNG, 100, it)) }
                    } finally { image.recycle() }
                }
                save("chats")
                val chat = app.repository.state.value.spaces.first { it.counterpart?.handle == "marina" }.id
                scenario.onActivity { activity -> activity.startActivity(Intent(activity, MainActivity::class.java)
                    .setAction(Intent.ACTION_VIEW).setData(Uri.parse("zoen://chat/$chat"))
                    .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)) }
                try {
                    await { app.repository.activeSpace == chat && editable(instrumentation.uiAutomation.rootInActiveWindow) != null }
                } catch (error: IllegalStateException) {
                    throw IllegalStateException("chat=$chat active=${app.repository.activeSpace} root=${instrumentation.uiAutomation.rootInActiveWindow}", error)
                }
                fun imeVisible(): Boolean {
                    var visible = false
                    scenario.onActivity { visible = ViewCompat.getRootWindowInsets(it.window.decorView)?.isVisible(WindowInsetsCompat.Type.ime()) == true }
                    return visible
                }
                val emptyInput = checkNotNull(editable(instrumentation.uiAutomation.rootInActiveWindow))
                check(emptyInput.performAction(AccessibilityNodeInfo.ACTION_FOCUS))
                check(emptyInput.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                scenario.onActivity { activity ->
                    val manager = activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE) as android.view.inputmethod.InputMethodManager
                    manager.showSoftInput(activity.currentFocus, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT)
                }
                await { imeVisible() && editable(instrumentation.uiAutomation.rootInActiveWindow)?.text.isNullOrBlank() }
                save("composer-empty")
                instrumentation.uiAutomation.performGlobalAction(android.accessibilityservice.AccessibilityService.GLOBAL_ACTION_BACK)
                await { !imeVisible() }
                fun findLabel(node: AccessibilityNodeInfo?, label: String): AccessibilityNodeInfo? {
                    if (node == null) return null
                    if (node.contentDescription?.toString() == label || node.text?.toString() == label) return node
                    for (i in 0 until node.childCount) findLabel(node.getChild(i), label)?.let { return it }
                    return null
                }
                val attachment = checkNotNull(findLabel(instrumentation.uiAutomation.rootInActiveWindow, app.getString(R.string.attach)))
                check(attachment.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                await { findLabel(instrumentation.uiAutomation.rootInActiveWindow, app.getString(R.string.chat_attachment_files)) != null }
                save("attachment-menu")
                instrumentation.uiAutomation.performGlobalAction(android.accessibilityservice.AccessibilityService.GLOBAL_ACTION_BACK)
                SystemClock.sleep(500)
                val input = checkNotNull(editable(instrumentation.uiAutomation.rootInActiveWindow))
                check(input.performAction(AccessibilityNodeInfo.ACTION_FOCUS))
                check(input.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                check(input.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, Bundle().apply {
                    putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, "See you tomorrow")
                }))
                scenario.onActivity { activity ->
                    val manager = activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE) as android.view.inputmethod.InputMethodManager
                    manager.showSoftInput(activity.currentFocus, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT)
                }
                await {
                    var visible = false
                    scenario.onActivity { visible = ViewCompat.getRootWindowInsets(it.window.decorView)?.isVisible(WindowInsetsCompat.Type.ime()) == true }
                    visible && editable(instrumentation.uiAutomation.rootInActiveWindow)?.text?.toString() == "See you tomorrow"
                }
                SystemClock.sleep(1_500)
                save("composer-text")
            }
        } finally { instrumentation.runOnMainSync { AppAppearance.set(app, previous) } }
    }
}
