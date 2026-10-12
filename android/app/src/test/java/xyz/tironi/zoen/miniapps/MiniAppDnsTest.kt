package xyz.tironi.zoen.miniapps

import java.net.InetAddress
import java.net.UnknownHostException
import org.junit.Assert.*
import org.junit.Test

class MiniAppDnsTest {
    @Test fun reboundAndMixedPublicPrivateAnswersCannotReachTheSocket() {
        val public = InetAddress.getByName("8.8.8.8")
        val private = InetAddress.getByName("127.0.0.1")
        var addresses = listOf(public)
        val dns = MiniAppDns({ setOf("tiles.example.com") }) { addresses }
        assertSame(addresses, dns.lookup("tiles.example.com"))
        listOf(listOf(private), listOf(public, private), emptyList()).forEach { answer ->
            addresses = answer
            assertThrows(UnknownHostException::class.java) { dns.lookup("tiles.example.com") }
        }
    }

    @Test fun revokedAndUnrelatedDomainsAreRejectedBeforeDnsLookup() {
        var grants = setOf("tiles.example.com")
        var lookedUp = false
        val dns = MiniAppDns({ grants }) { lookedUp = true; listOf(InetAddress.getByName("8.8.8.8")) }
        assertThrows(UnknownHostException::class.java) { dns.lookup("tiles.example.com.evil.com") }
        assertFalse(lookedUp)
        grants = emptySet()
        assertThrows(UnknownHostException::class.java) { dns.lookup("tiles.example.com") }
        assertFalse(lookedUp)
    }
}
