package xyz.tironi.zoen.miniapps

import java.net.InetAddress
import java.net.UnknownHostException
import okhttp3.Dns

/** These are the addresses the socket connects to, with no second lookup after validation. */
internal class MiniAppDns(
    private val granted: () -> Set<String>,
    private val resolve: (String) -> List<InetAddress> = Dns.SYSTEM::lookup,
) : Dns {
    override fun lookup(hostname: String): List<InetAddress> {
        if (hostname.lowercase() !in granted()) throw UnknownHostException("Domain was not granted")
        return resolve(hostname).also { addresses ->
            if (addresses.isEmpty() || !addresses.all(MiniAppSandbox::publicAddress)) throw UnknownHostException("Private addresses are not available to mini-apps")
        }
    }
}
