package xyz.tironi.zoen.miniapps

import android.annotation.SuppressLint
import android.content.Context
import android.net.Uri
import android.webkit.PermissionRequest
import android.webkit.RenderProcessGoneDetail
import android.webkit.SslErrorHandler
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.WebMessageCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import androidx.webkit.ProfileStore
import java.io.ByteArrayInputStream
import java.net.Proxy
import java.net.URL
import java.util.TimeZone
import java.util.UUID
import okhttp3.CookieJar
import okhttp3.OkHttpClient
import okhttp3.Request
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.ZoenRepository

interface MiniAppGateway {
    suspend fun item(id: String): ItemDetail
    suspend fun specs(): List<AppSpecDto>
    suspend fun resource(uri: String): AppResourceDto
    suspend fun call(item: String, tool: String, args: String, confirmed: Boolean): AppCallOutcome
    suspend fun allowed(item: String, capability: String): Boolean
    suspend fun grant(item: String, capability: String, purpose: String, always: Boolean)
    suspend fun message(space: String, text: String)
    suspend fun refresh()
}

class RepositoryMiniAppGateway(private val repository: ZoenRepository) : MiniAppGateway {
    override suspend fun item(id: String) = repository.query { it.item(id) }
    override suspend fun specs() = repository.query { it.appSpecs() }
    override suspend fun resource(uri: String) = repository.query { it.readAppResource(uri) }
    override suspend fun call(item: String, tool: String, args: String, confirmed: Boolean) = repository.change { it.appCallTool(item, tool, args, confirmed) }
    override suspend fun allowed(item: String, capability: String) = repository.query { it.appDeviceAllowed(item, capability) }
    override suspend fun grant(item: String, capability: String, purpose: String, always: Boolean) { repository.change { it.grantAppDevice(item, capability, purpose, always) } }
    override suspend fun message(space: String, text: String) { repository.change { it.sendMessage(space, text) } }
    override suspend fun refresh() { repository.refresh() }
}

enum class MiniAppConsent { DENY, ONCE, ALWAYS }
data class MiniAppPrompt(val title: String, val detail: String, val appName: String, val capability: String? = null)

/** The WebView cannot choose an Item, a resource, a Grant, a URI, or a confirmed flag. */
class McpAppSession(
    val itemId: String,
    private val gateway: MiniAppGateway,
    private val confirm: suspend (MiniAppPrompt) -> Boolean,
    private val consent: suspend (MiniAppPrompt) -> MiniAppConsent,
    private val native: suspend (String, JSONObject) -> Any,
    private val nativeAvailable: Set<String>,
    private val openLink: (String) -> Unit,
    private val onDisplay: (String) -> Unit,
    private val haptic: (String) -> Unit,
    private val onError: (String) -> Unit,
    private val onHeight: (Float) -> Unit = {},
    private val onToolCompleted: (String) -> Unit = {},
) {
    val origin = "https://miniapp-${UUID.randomUUID()}.invalid"
    val mainUrl = "$origin/index.html"
    private val profileName = "zoen-miniapp-${UUID.randomUUID()}"
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val messages = Channel<Pair<McpMessage, JavaScriptReplyProxy>>(32)
    private var view: WebView? = null
    private var proxy: JavaScriptReplyProxy? = null
    private var initialized = false
    private var disposed = false
    private var active = true
    private var latest: ItemDetail? = null
    private var specs: List<AppSpecDto> = emptyList()
    private var manifest: MiniAppManifest? = null
    @Volatile private var grantedHosts = emptySet<String>()
    @Volatile private var document: ByteArray = ByteArray(0)
    var locale = "en"
    var dark = false
    var width = 360f
    var fontScale = 1f
    var displayMode = "fullscreen"
    var members = JSONArray()
    private var contextSignature = ""
    private var sentState = ""
    var modelContext: JSONObject = JSONObject(); private set
    private val photos = MiniAppPhotoVault()
    private val network = OkHttpClient.Builder().followRedirects(false).followSslRedirects(false)
        .proxy(Proxy.NO_PROXY).cookieJar(CookieJar.NO_COOKIES)
        .connectTimeout(8, java.util.concurrent.TimeUnit.SECONDS).readTimeout(8, java.util.concurrent.TimeUnit.SECONDS)
        .callTimeout(12, java.util.concurrent.TimeUnit.SECONDS)
        .dns(MiniAppDns({ grantedHosts })).build()

    init {
        scope.launch {
            for ((message, reply) in messages) {
                if (disposed) break
                proxy = reply
                try { handle(message) }
                catch (failure: Exception) {
                    if (failure is CancellationException) throw failure
                    if (message.id != null) error(message.id, -32000, failure.message ?: "Could not complete this action")
                }
            }
        }
    }

    suspend fun prepare() {
        check(WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER) && WebViewFeature.isFeatureSupported(WebViewFeature.MULTI_PROFILE)) { "Update Android System WebView to open this mini-app" }
        latest = gateway.item(itemId)
        val app = requireNotNull(latest!!.app)
        specs = gateway.specs()
        val resource = gateway.resource(app.resourceUri)
        require(resource.uri == app.resourceUri && resource.mimeType == "text/html;profile=mcp-app")
        require(resource.text.toByteArray().size <= 8_388_608)
        if (resource.manifestJson.isNotEmpty()) {
            manifest = MiniAppManifest.decode(resource.manifestJson, app.appId) ?: error("This mini-app has an invalid manifest")
            require(manifest!!.verifies(resource.text)) { "This mini-app does not match its signed bundle" }
        }
        val wanted = manifest?.allowedDomains.orEmpty()
        val allowed = wanted.filter { gateway.allowed(itemId, "net:$it") }.toSet()
        if (wanted.isNotEmpty() && allowed.size < wanted.size) {
            val missing = wanted - allowed
            val decision = consent(MiniAppPrompt("Allow internet access?", "${manifest!!.networkPurpose}\n\n${missing.joinToString()}\n\n${latest!!.spaceTitle}", app.name, "net:${missing.joinToString()}"))
            if (decision != MiniAppConsent.DENY) {
                for (host in missing) gateway.grant(itemId, "net:$host", manifest!!.networkPurpose, decision == MiniAppConsent.ALWAYS)
            }
        }
        grantedHosts = wanted.filter { gateway.allowed(itemId, "net:$it") }.toSet()
        document = MiniAppSandbox.document(resource.text, grantedHosts).toByteArray(Charsets.UTF_8)
    }

    @SuppressLint("SetJavaScriptEnabled")
    fun createWebView(context: Context): WebView {
        check(document.isNotEmpty())
        check(WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER) && WebViewFeature.isFeatureSupported(WebViewFeature.MULTI_PROFILE)) { "Update Android System WebView to open this mini-app" }
        return WebView(context).also { web ->
            if (WebViewFeature.isFeatureSupported(WebViewFeature.MULTI_PROFILE)) {
                MiniAppWebProfiles.prepare()
                WebViewCompat.setProfile(web, profileName)
                WebViewCompat.getProfile(web).cookieManager.setAcceptCookie(false)
            } else error("Update Android System WebView to open this mini-app")
            MiniAppWebProfiles.live.add(profileName)
            view = web
            web.setBackgroundColor(android.graphics.Color.TRANSPARENT)
            web.contentDescription = latest?.app?.name
            web.settings.apply {
                javaScriptEnabled = true
                allowFileAccess = false
                allowContentAccess = false
                @Suppress("DEPRECATION")
                allowFileAccessFromFileURLs = false
                @Suppress("DEPRECATION")
                allowUniversalAccessFromFileURLs = false
                domStorageEnabled = false
                @Suppress("DEPRECATION")
                databaseEnabled = false
                javaScriptCanOpenWindowsAutomatically = false
                setSupportMultipleWindows(false)
                mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
                mediaPlaybackRequiresUserGesture = true
                cacheMode = WebSettings.LOAD_NO_CACHE
                blockNetworkLoads = true
                textZoom = (fontScale * 100).toInt().coerceIn(80, 200)
                safeBrowsingEnabled = true
            }
            android.webkit.CookieManager.getInstance().setAcceptThirdPartyCookies(web, false)
            web.setDownloadListener { _, _, _, _, _ -> }
            web.webChromeClient = object : WebChromeClient() {
                override fun onPermissionRequest(request: PermissionRequest) { request.deny() }
                override fun onShowFileChooser(webView: WebView, callback: android.webkit.ValueCallback<Array<Uri>>, params: FileChooserParams): Boolean { callback.onReceiveValue(null); return true }
                override fun onGeolocationPermissionsShowPrompt(origin: String, callback: android.webkit.GeolocationPermissions.Callback) { callback.invoke(origin, false, false) }
                override fun onJsAlert(view: WebView, url: String, message: String, result: android.webkit.JsResult): Boolean { result.cancel(); return true }
                override fun onJsConfirm(view: WebView, url: String, message: String, result: android.webkit.JsResult): Boolean { result.cancel(); return true }
                override fun onJsPrompt(view: WebView, url: String, message: String, defaultValue: String?, result: android.webkit.JsPromptResult): Boolean { result.cancel(); return true }
            }
            web.webViewClient = object : WebViewClient() {
                override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean = request.url.toString() != mainUrl
                @Deprecated("Platform compatibility")
                override fun shouldOverrideUrlLoading(view: WebView, url: String): Boolean = url != mainUrl
                override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse {
                    if (disposed) return blocked()
                    if (request.isForMainFrame && request.url.toString() == mainUrl && request.method == "GET") {
                        return WebResourceResponse("text/html", "UTF-8", 200, "OK", mapOf("Content-Security-Policy" to MiniAppSandbox.csp(grantedHosts), "Cache-Control" to "no-store", "Referrer-Policy" to "no-referrer"), ByteArrayInputStream(document))
                    }
                    if (!request.isForMainFrame && request.method == "GET" && MiniAppSandbox.allowedNetwork(request.url.toString(), grantedHosts)) {
                        return fetch(request.url.toString())
                    }
                    return blocked()
                }
                override fun onReceivedSslError(view: WebView, handler: SslErrorHandler, error: android.net.http.SslError) { handler.cancel() }
                override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean { onError("The mini-app stopped. Close it and open it again."); dispose(); return true }
            }
            if (WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) WebViewCompat.addWebMessageListener(web, "zoenMcp", setOf(origin)) { _, message, sourceOrigin, isMainFrame, reply ->
                if (disposed || message.type != WebMessageCompat.TYPE_STRING || !MiniAppSandbox.acceptsOrigin(sourceOrigin.toString(), origin, isMainFrame)) return@addWebMessageListener
                val parsed = McpPayload.parse(message.data ?: return@addWebMessageListener) ?: return@addWebMessageListener
                if (!messages.trySend(parsed to reply).isSuccess && parsed.id != null) reply.postMessage(JSONObject().put("jsonrpc", "2.0").put("id", parsed.id).put("error", JSONObject().put("code", -32001).put("message", "Too many pending requests")).toString())
            } else error("Update Android System WebView to open this mini-app")
            web.loadUrl(mainUrl)
        }
    }

    private fun blocked() = WebResourceResponse("text/plain", "UTF-8", 403, "Blocked", mapOf("Cache-Control" to "no-store"), ByteArrayInputStream(ByteArray(0)))

    /** WebView itself never connects. Every redirect and DNS answer stays inside the Grant. */
    private fun fetch(url: String): WebResourceResponse = runCatching {
        var address = url
        repeat(4) {
            require(MiniAppSandbox.allowedNetwork(address, grantedHosts))
            require(kotlinx.coroutines.runBlocking { gateway.allowed(itemId, "net:${URL(address).host.lowercase()}") })
            val target = URL(address)
            val request = Request.Builder().url(address).get().header("Accept", "*/*").header("User-Agent", "ZoenMiniApp/0.1 Android").build()
            network.newCall(request).execute().use { response ->
                if (response.code in 300..399) {
                    address = URL(target, response.header("Location") ?: error("No redirect target")).toExternalForm()
                    return@repeat
                }
                require(response.code in 200..299 && response.body.contentLength() <= 8_388_608)
                val bytes = response.body.byteStream().use { it.readBounded(8_388_608) }
                require(bytes.size <= 8_388_608)
                val mime = response.body.contentType()?.let { "${it.type}/${it.subtype}" } ?: "application/octet-stream"
                return WebResourceResponse(mime, "UTF-8", 200, "OK", mapOf("Access-Control-Allow-Origin" to origin, "Cache-Control" to "no-store"), ByteArrayInputStream(bytes))
            }
        }
        blocked()
    }.getOrElse { blocked() }

    private suspend fun handle(message: McpMessage) {
        val id = message.id
        val params = message.params
        if (id == null) {
            when (message.method) {
                "ui/notifications/initialized" -> { if (!initialized) { initialized = true; sendState(true) } }
                "ui/notifications/size-changed" -> params.optDouble("height", Double.NaN).takeIf { it.isFinite() }?.let { onHeight(it.toFloat().coerceIn(120f, 4_000f)) }
                "zoen/haptics" -> if (initialized && active) haptic(params.optString("kind"))
            }
            return
        }
        if (message.method == "ui/initialize") {
            val version = params.optString("protocolVersion", PROTOCOL)
            if (version != PROTOCOL) { error(id, -32602, "Unsupported MCP Apps protocol version"); return }
            respond(id, initialize()); return
        }
        if (message.method == "ping") { respond(id, JSONObject()); return }
        if (!initialized || !active) { error(id, -32000, "This mini-app is not active"); return }
        when (message.method) {
            "tools/call" -> {
                val name = params.optString("name")
                if (params.has("arguments") && params.opt("arguments") !is JSONObject) { error(id, -32602, "Invalid tool arguments"); return }
                val args = params.optJSONObject("arguments") ?: JSONObject()
                val spec = specs.firstOrNull { it.id == latest?.app?.appId }
                if (spec?.tools?.none { it.name == name && "app" in it.visibility } != false) { error(id, -32003, "This tool is not available to this mini-app"); return }
                val strings = McpPayload.strings(args)
                if (strings.any { it.startsWith("data:image", true) }) { error(id, -32602, "Use the photo picker and confirm Share with group"); return }
                val tokens = strings.filter { it.startsWith("zoen-photo:") }.distinct()
                val resolved = if (tokens.isNotEmpty()) {
                    if (tokens.any { photos.resolve(it) == null }) { error(id, -32602, "Unknown photo"); return }
                    if (!confirm(MiniAppPrompt("Share ${tokens.size} photo(s) with ${latest!!.spaceTitle}?", "Everyone in this chat will see the selected photos. Location data was removed.", latest!!.app!!.name))) { error(id, -32000, "You did not share these photos"); return }
                    photos.replace(args) as JSONObject
                } else args
                var result = gateway.call(itemId, name, resolved.toString(), false)
                if (result.status == AppCallStatus.NEEDS_CONFIRMATION) {
                    if (!confirm(MiniAppPrompt(result.confirmTitle ?: name, result.confirmDetail ?: result.message, latest!!.app!!.name))) { error(id, -32000, "You did not confirm this action"); return }
                    result = gateway.call(itemId, name, resolved.toString(), true)
                }
                if (result.status != AppCallStatus.DONE) { error(id, -32003, result.message); return }
                respond(id, JSONObject(result.resultJson)); latest = result.item ?: gateway.item(itemId); sendState(false)
                onToolCompleted(name)
            }
            "resources/read" -> {
                if (params.optString("uri") != latest!!.app!!.resourceUri) { error(id, -32003, "Only this mini-app's resource is available"); return }
                val resource = gateway.resource(latest!!.app!!.resourceUri)
                respond(id, JSONObject().put("contents", JSONArray().put(JSONObject().put("uri", resource.uri).put("mimeType", resource.mimeType).put("text", resource.text))))
            }
            "ui/message" -> {
                val content = params.optJSONObject("content") ?: params.optJSONArray("content")?.optJSONObject(0)
                val text = content?.optString("text").orEmpty()
                if (text.isBlank() || text.length > 4_000 || content?.optString("type") != "text") { error(id, -32602, "Invalid message"); return }
                if (!confirm(MiniAppPrompt("Share in ${latest!!.spaceTitle}?", text, latest!!.app!!.name))) { error(id, -32000, "Message sharing denied"); return }
                gateway.message(latest!!.spaceId, text); respond(id, JSONObject())
            }
            "ui/open-link" -> {
                val url = params.optString("url")
                if (!MiniAppSandbox.openLink(url)) { error(id, -32602, "Invalid link"); return }
                if (!confirm(MiniAppPrompt("Open link?", url, latest!!.app!!.name))) { error(id, -32000, "Link opening denied"); return }
                openLink(url); respond(id, JSONObject())
            }
            "ui/request-display-mode" -> {
                val mode = params.optString("mode")
                if (mode !in setOf("inline", "fullscreen")) { error(id, -32602, "Unsupported display mode"); return }
                respond(id, JSONObject().put("mode", mode)); onDisplay(mode)
            }
            "ui/update-model-context" -> {
                if (params.toString().toByteArray().size > 8_192) { error(id, -32602, "Model context is too large"); return }
                modelContext = JSONObject(params.toString()); MiniAppModelContext.set(itemId, modelContext.toString()); respond(id, JSONObject())
            }
            "zoen/widget/set-snapshot" -> {
                val snapshot = params.optJSONObject("snapshot")
                if (snapshot == null || !MiniAppSnapshots.publish(latest!!, snapshot)) { error(id, -32602, "Snapshot does not match the widget schema"); return }
                gateway.refresh(); respond(id, JSONObject().put("accepted", true))
            }
            else -> if (message.method.startsWith("zoen/native/")) {
                val cap = message.method.removePrefix("zoen/native/")
                val purpose = manifest?.capabilities?.get(cap)
                if (purpose == null || cap !in nativeAvailable) { error(id, -32003, "This mini-app does not declare an available capability"); return }
                if (!gateway.allowed(itemId, cap)) {
                    val decision = consent(MiniAppPrompt("Allow $cap?", "$purpose\n\nOnly ${latest!!.app!!.name}, in ${latest!!.spaceTitle}. Nothing is shared until you confirm.", latest!!.app!!.name, cap))
                    if (decision == MiniAppConsent.DENY) { error(id, -32003, "You did not allow it"); return }
                    gateway.grant(itemId, cap, purpose, decision == MiniAppConsent.ALWAYS)
                }
                if (!active || disposed || !gateway.allowed(itemId, cap)) { error(id, -32003, "This mini-app is not active or its access was revoked"); return }
                val result = native(cap, params)
                respond(id, photos.captureResult(cap, result))
            } else error(id, -32601, "Method not found: ${message.method}")
        }
    }

    fun update(item: ItemDetail, newLocale: String, newDark: Boolean, newWidth: Float, newFontScale: Float, newMembers: JSONArray) {
        require(item.id == itemId)
        latest = item; locale = newLocale; dark = newDark; width = newWidth; members = newMembers; fontScale = newFontScale
        view?.settings?.textZoom = (fontScale * 100).toInt().coerceIn(80, 200)
        if (initialized) {
            val context = context().toString()
            if (context != contextSignature) { contextSignature = context; notify("ui/notifications/host-context-changed", JSONObject(context)) }
            if (sentState != item.app?.viewJson) sendState(false)
        }
    }
    fun visible(visible: Boolean) { active = visible; if (visible) view?.onResume() else view?.onPause(); if (initialized) notify("ui/notifications/host-context-changed", JSONObject().put("visibility", if (visible) "visible" else "hidden")) }
    fun dispose() {
        if (disposed) return
        if (initialized) deliver(JSONObject().put("jsonrpc", "2.0").put("id", 1_000_001).put("method", "ui/resource-teardown").put("params", JSONObject().put("reason", "Mini-app closed")))
        disposed = true; messages.close(); scope.cancel(); photos.clear()
        network.dispatcher.cancelAll(); network.connectionPool.evictAll(); network.dispatcher.executorService.shutdown()
        view?.apply {
            stopLoading()
            if (WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) WebViewCompat.removeWebMessageListener(this, "zoenMcp")
            if (WebViewFeature.isFeatureSupported(WebViewFeature.MULTI_PROFILE)) {
                val profile = WebViewCompat.getProfile(this)
                profile.cookieManager.removeAllCookies(null); profile.webStorage.deleteAllData()
            }
            clearCache(true)
            clearHistory(); removeAllViews(); destroy()
            MiniAppWebProfiles.live.remove(profileName)
        }
        view = null; proxy = null
    }

    private fun initialize(): JSONObject {
        val native = nativeAvailable.filter { manifest?.capabilities?.containsKey(it) == true }
        return JSONObject().put("protocolVersion", PROTOCOL).put("hostInfo", JSONObject().put("name", "zoen").put("version", "0.3.0"))
            .put("hostCapabilities", JSONObject().put("openLinks", JSONObject()).put("serverTools", JSONObject()).put("serverResources", JSONObject()).put("logging", JSONObject())
                .put("sandbox", JSONObject().put("csp", JSONObject().put("connectDomains", JSONArray(grantedHosts.map { "https://$it" }))).put("permissions", JSONObject()))
                .put("experimental", JSONObject().put("zoen", JSONObject().put("native", JSONArray(native)).put("services", JSONArray(listOf("state", "members", "haptics", "share", "widget", "theme") + grantedHosts.map { "net:$it" })).put("me", (0 until members.length()).map { members.getJSONObject(it) }.firstOrNull { it.optBoolean("isMe") } ?: JSONObject()))))
            .put("hostContext", context())
    }
    private fun context(): JSONObject {
        val variables = JSONObject().put("--font-sans", "Roboto, system-ui, sans-serif")
            .put("--color-background-primary", if (dark) "#191C18" else "#FCFAF4")
            .put("--color-background-secondary", if (dark) "#252A22" else "#F0EEE6")
            .put("--color-text-primary", if (dark) "#ECEFE3" else "#1C2119")
            .put("--color-text-secondary", if (dark) "#BDC7B5" else "#687062")
            .put("--color-border-primary", if (dark) "#424A3B" else "#D8DCCF")
        val ctx = JSONObject().put("theme", if (dark) "dark" else "light").put("styles", JSONObject().put("variables", variables))
            .put("displayMode", displayMode).put("availableDisplayModes", JSONArray(listOf("inline", "fullscreen")))
            .put("containerDimensions", JSONObject().put("width", width.toDouble()).also { if (displayMode == "inline") it.put("maxHeight", 640) })
            .put("locale", locale).put("timeZone", TimeZone.getDefault().id).put("platform", "mobile").put("userAgent", "Zoen/0.3 Android")
            .put("deviceCapabilities", JSONObject().put("touch", true).put("hover", false))
            .put("safeAreaInsets", JSONObject().put("top", 0).put("right", 0).put("bottom", 0).put("left", 0))
            .put("zoen", JSONObject().put("members", members))
        specs.firstOrNull { it.id == latest?.app?.appId }?.let { spec -> spec.tools.firstOrNull { it.visibility == listOf("model") }?.let { tool ->
            ctx.put("toolInfo", JSONObject().put("tool", JSONObject().put("name", tool.name).put("description", tool.description).put("inputSchema", JSONObject(tool.inputSchemaJson)).put("_meta", JSONObject().put("ui", JSONObject().put("resourceUri", spec.resourceUri).put("visibility", JSONArray(tool.visibility))))))
        } }
        return ctx
    }
    private fun sendState(input: Boolean) {
        val app = latest?.app ?: return
        if (input) notify("ui/notifications/tool-input", JSONObject().put("arguments", JSONObject()))
        notify("ui/notifications/tool-result", JSONObject().put("content", JSONArray().put(JSONObject().put("type", "text").put("text", app.headline))).put("structuredContent", JSONObject(app.viewJson)).put("isError", false))
        sentState = app.viewJson
    }
    private fun notify(method: String, params: JSONObject) = deliver(JSONObject().put("jsonrpc", "2.0").put("method", method).put("params", params))
    private fun respond(id: Any, result: Any) = deliver(JSONObject().put("jsonrpc", "2.0").put("id", id).put("result", result))
    private fun error(id: Any, code: Int, message: String) = deliver(JSONObject().put("jsonrpc", "2.0").put("id", id).put("error", JSONObject().put("code", code).put("message", message.take(2_000))))
    private fun deliver(message: JSONObject) { if (!disposed && WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) proxy?.postMessage(message.toString()) }
    companion object { const val PROTOCOL = "2026-01-26" }
}

private object MiniAppWebProfiles {
    val live = mutableSetOf<String>()
    private var cleaned = false
    fun prepare() {
        if (cleaned) return
        cleaned = true
        if (WebViewFeature.isFeatureSupported(WebViewFeature.MULTI_PROFILE)) {
            val store = ProfileStore.getInstance()
            store.allProfileNames.filter { it.startsWith("zoen-miniapp-") && it !in live }.forEach { name -> runCatching { store.deleteProfile(name) } }
        }
    }
}
