package xyz.tironi.zoen.miniapps

import java.net.URI
import java.security.MessageDigest
import org.json.JSONObject

/** Manifest declarations authorize no access by themselves; every device use needs a core Grant. */
data class MiniAppManifest(
    val id: String,
    val name: String,
    val capabilities: Map<String, String>,
    val allowedDomains: Set<String>,
    val networkPurpose: String,
    val linkDomains: Set<String>,
    val sha256: String,
    val bytes: Int,
) {
    fun verifies(html: String): Boolean {
        val data = html.toByteArray(Charsets.UTF_8)
        return data.size == bytes && MessageDigest.getInstance("SHA-256").digest(data).joinToString("") { "%02x".format(it) } == sha256
    }
    companion object {
        fun decode(json: String, expectedApp: String): MiniAppManifest? = runCatching {
            require(json.toByteArray().size <= 32_768)
            val obj = JSONObject(json)
            require(obj.getString("id") == expectedApp)
            val capabilities = obj.getJSONArray("capabilities")
            require(capabilities.length() <= 32)
            val caps = (0 until capabilities.length()).associate { index ->
                val cap = capabilities.getJSONObject(index)
                val id = cap.getString("id")
                val purpose = cap.getString("purpose")
                require(Regex("[a-z][a-z0-9.]{0,63}").matches(id) && purpose.isNotBlank() && purpose.length <= 1_000)
                id to purpose
            }
            fun domains(key: String): Set<String> {
                val array = obj.optJSONArray(key) ?: return emptySet()
                require(array.length() <= 20)
                return (0 until array.length()).map { array.getString(it).lowercase().also { host -> require(MiniAppSandbox.validDomain(host)) } }.toSet()
            }
            val sha = obj.getString("sha256").lowercase()
            val bytes = obj.getInt("bytes")
            require(Regex("[a-f0-9]{64}").matches(sha) && bytes in 1..8_388_608)
            MiniAppManifest(expectedApp, obj.getString("name").take(120), caps, domains("allowedDomains"), obj.optString("networkPurpose").take(1_000), domains("linkDomains"), sha, bytes)
        }.getOrNull()
    }
}

object MiniAppSandbox {
    fun publicAddress(address: java.net.InetAddress): Boolean {
        if (address.isAnyLocalAddress || address.isLoopbackAddress || address.isLinkLocalAddress || address.isSiteLocalAddress || address.isMulticastAddress) return false
        val bytes = address.address.map { it.toInt() and 255 }
        fun ipv4(b: List<Int>): Boolean = b[0] in 1..223 && b[0] !in setOf(10, 127) && !(b[0] == 100 && b[1] in 64..127) && !(b[0] == 169 && b[1] == 254) && !(b[0] == 172 && b[1] in 16..31) && !(b[0] == 192 && b[1] in setOf(0, 168)) && !(b[0] == 192 && b[1] == 88 && b[2] == 99) && !(b[0] == 198 && (b[1] in 18..19 || b[1] == 51 && b[2] == 100)) && !(b[0] == 203 && b[1] == 0 && b[2] == 113)
        if (bytes.size == 4) return ipv4(bytes)
        if (bytes.size != 16) return false
        val mapped = bytes.take(10).all { it == 0 } && bytes[10] == 255 && bytes[11] == 255
        val nat64 = bytes.take(12) == listOf(0,100,255,155,0,0,0,0,0,0,0,0)
        if (mapped || nat64) return ipv4(bytes.takeLast(4))
        if (bytes[0] !in 32..63 || bytes[0] == 32 && bytes[1] == 2) return false
        return !(bytes.take(4) in listOf(listOf(32,1,0,0), listOf(32,1,13,184), listOf(32,1,0,2)))
    }
    private val domains = Regex("(?=.{1,253}$)([a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\\.)+[a-z]{2,63}")
    fun validDomain(host: String): Boolean = domains.matches(host) && !host.endsWith(".local") && !host.endsWith(".localhost") && !host.endsWith(".internal") && !host.endsWith(".invalid")
    fun acceptsOrigin(actual: String, expected: String, mainFrame: Boolean): Boolean = mainFrame && runCatching {
        val a = URI(actual); val e = URI(expected)
        a.scheme == "https" && a.host == e.host && a.userInfo == null && a.port in setOf(-1, 443) && a.path.orEmpty().isEmpty() && a.query == null && a.fragment == null
    }.getOrDefault(false)
    fun allowedNetwork(url: String, granted: Set<String>): Boolean = runCatching {
        val uri = URI(url)
        uri.scheme == "https" && uri.userInfo == null && uri.port in setOf(-1, 443) && uri.host?.lowercase() in granted && validDomain(uri.host.lowercase())
    }.getOrDefault(false)
    fun openLink(url: String): Boolean = runCatching {
        val uri = URI(url)
        url.length <= 2_048 && when (uri.scheme) {
            "https", "http" -> uri.userInfo == null && uri.host != null && uri.port in setOf(-1, 80, 443)
            "mailto" -> uri.schemeSpecificPart.isNotBlank() && !url.contains('\n') && !url.contains('\r')
            "zoen" -> uri.host in setOf("app", "item", "chat", "join") && uri.rawPath?.matches(Regex("/[A-Za-z0-9_.-]{1,128}")) == true && uri.query == null && uri.fragment == null
            else -> false
        }
    }.getOrDefault(false)
    fun csp(granted: Set<String>): String {
        require(granted.all(::validDomain))
        val net = granted.sorted().joinToString(" ") { "https://$it" }
        return "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: $net; media-src 'self' data:; font-src 'self' data:; connect-src ${net.ifEmpty { "'none'" }}; worker-src blob:; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"
    }
    fun document(html: String, granted: Set<String>, dark: Boolean? = null): String {
        val appearance = dark?.let(MiniAppAppearance::initialScript).orEmpty()
        val guard = "<meta http-equiv=\"Content-Security-Policy\" content=\"${csp(granted)}\"><meta name=\"referrer\" content=\"no-referrer\"><script>$appearance\n$shim</script>"
        val head = Regex("<head(?:\\s[^>]*)?>", RegexOption.IGNORE_CASE).find(html)
        return if (head != null) html.replaceRange(head.range.last + 1, head.range.last + 1, guard) else "<!doctype html><head>$guard</head>$html"
    }
    val shim = """
        (() => {
          if (window.top !== window) return;
          const bridge = window.zoenMcp;
          const host = Object.freeze({postMessage: (m) => bridge.postMessage(JSON.stringify(m))});
          Object.defineProperty(window, 'parent', {get: () => host});
          bridge.onmessage = (e) => {try {window.dispatchEvent(new MessageEvent('message', {data: JSON.parse(e.data)}));} catch (_) {}};
          window.open = () => null;
          for (const name of ['RTCPeerConnection','webkitRTCPeerConnection','WebTransport']) {
            try {Object.defineProperty(window, name, {value: undefined, writable: false, configurable: false});} catch (_) {}
          }
          if (navigator.serviceWorker) navigator.serviceWorker.register = () => Promise.reject(new Error('Service workers are disabled'));
        })();
    """.trimIndent()
}

data class McpMessage(val id: Any?, val method: String, val params: JSONObject)
object McpPayload {
    const val MAX_BYTES = 262_144
    fun parse(raw: String): McpMessage? = runCatching {
        require(raw.length <= MAX_BYTES && raw.toByteArray().size <= MAX_BYTES)
        // Limit nesting before the JSON parser can recurse through attacker-controlled input.
        var depth = 0; var quoted = false; var escaped = false
        raw.forEach { ch ->
            if (quoted) { if (escaped) escaped = false else if (ch == '\\') escaped = true else if (ch == '"') quoted = false }
            else when (ch) { '"' -> quoted = true; '{', '[' -> { depth++; require(depth <= 20) }; '}', ']' -> depth-- }
        }
        val obj = JSONObject(raw)
        require(obj.opt("jsonrpc") == "2.0")
        val method = obj.opt("method") as? String ?: error("Invalid method")
        require(method.length <= 96 && Regex("[A-Za-z0-9_./-]+").matches(method))
        val id = obj.opt("id").takeUnless { it == JSONObject.NULL }
        require(id == null || id is String && id.length <= 128 || id is Number && id.toDouble().isFinite() && id.toDouble() == id.toLong().toDouble())
        require(!obj.has("params") || obj.opt("params") is JSONObject)
        require(!obj.has("result") && !obj.has("error"))
        McpMessage(id, method, obj.optJSONObject("params") ?: JSONObject())
    }.getOrNull()
    fun strings(value: Any?): List<String> = when (value) {
        is String -> listOf(value)
        is JSONObject -> value.keys().asSequence().flatMap { strings(value.opt(it)).asSequence() }.toList()
        is org.json.JSONArray -> (0 until value.length()).flatMap { strings(value.opt(it)) }
        else -> emptyList()
    }
}

internal fun java.io.InputStream.readBounded(limit: Int): ByteArray {
    val out = java.io.ByteArrayOutputStream()
    val buffer = ByteArray(16_384)
    while (true) {
        val count = read(buffer, 0, minOf(buffer.size, limit + 1 - out.size()))
        if (count < 0) break
        out.write(buffer, 0, count)
        require(out.size() <= limit) { "Resource is too large" }
    }
    return out.toByteArray()
}
