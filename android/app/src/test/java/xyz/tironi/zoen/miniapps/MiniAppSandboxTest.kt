package xyz.tironi.zoen.miniapps

import org.junit.Assert.*
import org.junit.Test

class MiniAppSandboxTest {
    private val origin = "https://miniapp-test.invalid"
    @Test fun bridgeAcceptsOnlyItsExactHttpsMainFrameOrigin() {
        assertTrue(MiniAppSandbox.acceptsOrigin(origin, origin, true))
        assertFalse(MiniAppSandbox.acceptsOrigin(origin, origin, false))
        listOf("null", "file:///data/data/xyz.tironi.zoen/", "content://xyz.tironi.zoen.files/", "http://miniapp-test.invalid", "https://miniapp-test.invalid.evil.example", "https://miniapp-test.invalid:444", "https://user@miniapp-test.invalid", "$origin/path", "$origin?x=1").forEach { assertFalse(it, MiniAppSandbox.acceptsOrigin(it, origin, true)) }
    }
    @Test fun networkGrantMatchesExactHttpsHostAndDefaultPort() {
        val grants = setOf("tiles.openfreemap.org")
        assertTrue(MiniAppSandbox.allowedNetwork("https://tiles.openfreemap.org/a.pbf", grants))
        listOf("http://tiles.openfreemap.org/a", "https://tiles.openfreemap.org:444/a", "https://tiles.openfreemap.org.evil.example/a", "https://evil.example@tiles.openfreemap.org/a", "file:///etc/passwd", "content://xyz.tironi.zoen.files/exports", "https://127.0.0.1/a").forEach { assertFalse(it, MiniAppSandbox.allowedNetwork(it, grants)) }
        assertFalse(MiniAppSandbox.allowedNetwork("https://tiles.openfreemap.org/a", emptySet()))
    }
    @Test fun manifestDomainsCannotInjectCspOrReachLocalNetworks() {
        listOf("*.example.com", "example.com/path", "localhost", "127.0.0.1", "example.local", "example.internal", "example.com; connect-src *", "example.com:443").forEach { assertFalse(it, MiniAppSandbox.validDomain(it)) }
        assertTrue(MiniAppSandbox.validDomain("tiles.openfreemap.org"))
        assertTrue(MiniAppSandbox.csp(emptySet()).contains("connect-src 'none'"))
        assertTrue(MiniAppSandbox.csp(setOf("tiles.openfreemap.org")).contains("connect-src https://tiles.openfreemap.org"))
    }
    @Test fun networkDnsAnswersMustBePublicAddresses() {
        listOf("127.0.0.1", "10.0.2.2", "192.168.1.1", "172.16.1.1", "169.254.169.254", "100.64.0.1", "::1", "fc00::1", "fe80::1").forEach { assertFalse(it, MiniAppSandbox.publicAddress(java.net.InetAddress.getByName(it))) }
        assertTrue(MiniAppSandbox.publicAddress(java.net.InetAddress.getByName("8.8.8.8")))
        assertTrue(MiniAppSandbox.publicAddress(java.net.InetAddress.getByName("2001:4860:4860::8888")))
    }
    @Test fun documentPolicyPrecedesAnyApplicationScript() {
        val html = "<html><HEAD data-app='example'><script>window.parent.postMessage({});</script></HEAD></html>"
        val document = MiniAppSandbox.document(html, emptySet())
        assertTrue(document.indexOf("Content-Security-Policy") < document.indexOf("window.parent.postMessage({})"))
        assertTrue(document.contains("frame-src 'none'"))
        assertTrue(document.contains("object-src 'none'"))
    }
    @Test fun externalLinksRequireAnExplicitSupportedScheme() {
        assertTrue(MiniAppSandbox.openLink("https://example.com/article"))
        assertTrue(MiniAppSandbox.openLink("zoen://app/it_123"))
        listOf("javascript:alert(1)", "intent://anything", "file:///data/data/xyz.tironi.zoen/", "content://contacts/1", "zoen://app/../../account", "https://user:secret@example.com").forEach { assertFalse(it, MiniAppSandbox.openLink(it)) }
    }
}
