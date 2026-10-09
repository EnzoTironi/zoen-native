package xyz.tironi.zoen

import android.content.Intent
import android.graphics.Bitmap
import android.os.SystemClock
import android.view.MotionEvent
import android.webkit.WebView
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.miniapps.*

@RunWith(AndroidJUnit4::class)
class McpWebViewTest {
    @Test fun actualHtmlTouchChangesSignedRustStateAndNativeConfirmationControlsExternalCalls() {
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val folder = File(context.noBackupFilesDir, "web-test-${UUID.randomUUID()}").apply { mkdirs() }
        val core = RodaEngine.open(File(folder, "core.sqlite").absolutePath, "en")
        core.seedDemoIfEmpty()
        val space = core.spaces().first { it.counterpart?.handle == "zoen" }
        val item = core.installApp(space.id, "list", "{\"title\":\"Android WebView list\",\"items\":[\"Water\"]}")
        val confirm = AtomicBoolean(false)
        val prompts = AtomicInteger()
        val confirmedCalls = AtomicInteger()
        val failure = AtomicReference<String?>()
        val gateway = object : MiniAppGateway {
            override suspend fun item(id: String) = withContext(Dispatchers.IO) { core.item(id) }
            override suspend fun specs() = withContext(Dispatchers.IO) { core.appSpecs() }
            override suspend fun resource(uri: String) = withContext(Dispatchers.IO) { core.readAppResource(uri) }
            override suspend fun call(item: String, tool: String, args: String, confirmed: Boolean): AppCallOutcome = withContext(Dispatchers.IO) { if (confirmed) confirmedCalls.incrementAndGet(); core.appCallTool(item, tool, args, confirmed) }
            override suspend fun allowed(item: String, capability: String) = withContext(Dispatchers.IO) { core.appDeviceAllowed(item, capability) }
            override suspend fun grant(item: String, capability: String, purpose: String, always: Boolean) { withContext(Dispatchers.IO) { core.grantAppDevice(item, capability, purpose, always) } }
            override suspend fun message(space: String, text: String) { withContext(Dispatchers.IO) { core.sendMessage(space, text) } }
            override suspend fun refresh() {}
        }
        val session = McpAppSession(item.id, gateway, confirm = { prompts.incrementAndGet(); confirm.get() }, consent = { MiniAppConsent.DENY }, native = { _, _ -> error("No device capability was granted") }, nativeAvailable = emptySet(), openLink = { error("No external browser should open") }, onDisplay = {}, haptic = {}, onError = { failure.set(it) })
        val web = AtomicReference<WebView>()
        val loaded = CountDownLatch(1)
        val scenario = ActivityScenario.launch<MainActivity>(Intent(context, MainActivity::class.java))
        try {
            scenario.onActivity { activity ->
                CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate).launch {
                    try { session.prepare(); web.set(session.createWebView(activity)); activity.setContentView(web.get()) }
                    catch (error: Exception) { failure.set(error.toString()) }
                    finally { loaded.countDown() }
                }
            }
            assertTrue(loaded.await(20, TimeUnit.SECONDS))
            assertNull(failure.get())
            waitUntil { evaluate(web.get(), "document.querySelectorAll('#items .it').length") == "1" }
            val before = core.item(item.id).version
            touch(web.get(), "#items .it")
            waitUntil { JSONObject(core.item(item.id).app!!.viewJson).getJSONArray("items").getJSONObject(0).getBoolean("done") }
            assertTrue(core.item(item.id).version > before)
            waitUntil { evaluate(web.get(), "document.querySelector('#items .it').classList.contains('done')") == "true" }
            assertTrue(core.verifyAll().all { it.valid })
            val evidence = File(context.getExternalFilesDir(null), "evidence/mcp-webview-list.png").apply { parentFile!!.mkdirs() }
            InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot().use { bitmap -> evidence.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) } }

            // This request travels through the same HTML protocol, but cannot read another app's resource.
            evaluate(web.get(), "rpc.request('resources/read',{uri:'ui://roda/pet'}).then(()=>window.resourceOutcome='allowed').catch(e=>window.resourceOutcome=String(e.code)); null")
            waitUntil { evaluate(web.get(), "window.resourceOutcome") == "\"-32003\"" }
            touch(web.get(), "#send")
            waitUntil { prompts.get() == 1 }
            assertEquals(0, confirmedCalls.get())
            confirm.set(true)
            touch(web.get(), "#send")
            waitUntil { confirmedCalls.get() == 1 }
            assertEquals(2, prompts.get())
            assertTrue(core.verifyAll().all { it.valid })
            assertNull(failure.get())
            val authorizedVersion = core.item(item.id).version
            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                web.get().loadDataWithBaseURL("https://attacker.example/", "<html><body><script>document.body.textContent = window.zoenMcp ? 'bridge injected' : 'bridge missing'; if(window.zoenMcp) window.zoenMcp.postMessage('{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"tools/call\",\"params\":{\"name\":\"list_add\",\"arguments\":{\"text\":\"unauthorized\"}}}');</script></body></html>", "text/html", "UTF-8", null)
            }
            waitUntil { evaluate(web.get(), "document.body.textContent") == "\"bridge missing\"" }
            assertEquals(authorizedVersion, core.item(item.id).version)
        } finally {
            InstrumentationRegistry.getInstrumentation().runOnMainSync { session.dispose() }
            scenario.close(); core.destroy(); folder.deleteRecursively()
        }
    }

    private inline fun <T> Bitmap.use(block: (Bitmap) -> T): T = try { block(this) } finally { recycle() }

    private fun evaluate(web: WebView, script: String): String {
        val answer = AtomicReference<String>()
        val done = CountDownLatch(1)
        InstrumentationRegistry.getInstrumentation().runOnMainSync { web.evaluateJavascript(script) { answer.set(it); done.countDown() } }
        check(done.await(3, TimeUnit.SECONDS))
        return answer.get()
    }
    private fun touch(web: WebView, selector: String) {
        val rect = JSONArray(evaluate(web, "(() => {const r=document.querySelector(${JSONObject.quote(selector)}).getBoundingClientRect();return [r.left+r.width/2,r.top+r.height/2,innerWidth]})()"))
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            val scale = web.width / rect.getDouble(2)
            val x = (rect.getDouble(0) * scale).toFloat()
            val y = (rect.getDouble(1) * scale).toFloat()
            val at = SystemClock.uptimeMillis()
            MotionEvent.obtain(at, at, MotionEvent.ACTION_DOWN, x, y, 0).also { web.dispatchTouchEvent(it); it.recycle() }
            MotionEvent.obtain(at, at + 60, MotionEvent.ACTION_UP, x, y, 0).also { web.dispatchTouchEvent(it); it.recycle() }
        }
    }
    private fun waitUntil(condition: () -> Boolean) {
        val limit = SystemClock.uptimeMillis() + 10_000
        while (!condition()) { check(SystemClock.uptimeMillis() < limit) { "HTML did not reach the expected state" }; SystemClock.sleep(100) }
    }
}
