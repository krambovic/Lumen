package com.lumen.core.config.warp

import com.lumen.core.config.parser.LinkParser
import com.lumen.core.config.parser.ParsedNode
import org.json.JSONObject
import java.util.Base64
import kotlin.random.Random

data class WarpRegistration(val privateKey: String, val publicKey: String, val ipv4: String, val ipv6: String)

/** Desktop's public helper list, masks and profile defaults; no registration happens here. */
object WarpProfiles {
    private val defaults by lazy {
        val raw = checkNotNull(javaClass.getResourceAsStream("/warp-profiles.json"))
            .bufferedReader().use { it.readText() }
        JSONObject(raw)
    }

    fun endpoints(masque: Boolean): List<String> = defaults.getJSONArray(
        if (masque) "masqueEndpoints" else "awgEndpoints"
    ).let { array -> (0 until array.length()).map { array.getString(it) } }

    fun registration(raw: String, masque: Boolean = false): WarpRegistration {
        val json = JSONObject(raw)
        fun field(primary: String, fallback: String) = json.optString(primary).ifBlank { json.optString(fallback) }.trim()
        fun key(value: String, name: String): String {
            val decoded = runCatching { Base64.getDecoder().decode(value) }.getOrNull()
            val valid = if (masque) {
                // The MASQUE helper returns DER encoded EC keys; AWG uses raw
                // 32-byte WireGuard keys. Both are base64, but they are not
                // interchangeable registration formats.
                decoded != null && decoded.size in 48..256 && decoded.firstOrNull() == 0x30.toByte()
            } else {
                decoded?.size == 32
            }
            require(valid) { "Invalid WARP $name" }
            return value
        }
        val ip = field("client_ipv4", "ipv4")
        val address = ip.substringBefore('/').split('.')
        require(address.size == 4 && address.all { part -> part.isNotEmpty() && part.all(Char::isDigit) && part.toIntOrNull() in 0..255 } &&
            (!ip.contains('/') || ip.substringAfter('/').toIntOrNull() in 0..32)) { "Invalid WARP address" }
        val ipv6 = field("client_ipv6", "ipv6")
        require(ipv6.isBlank() || (ipv6.contains(':') && ipv6.all { it in "0123456789abcdefABCDEF:/" })) { "Invalid WARP IPv6 address" }
        return WarpRegistration(
            key(field("privKey", "private_key"), "private key"),
            key(field("peer_pub", "public_key"), "public key"),
            ip,
            ipv6
        )
    }

    fun awg(registration: WarpRegistration, random: Random = Random.Default): List<ParsedNode> {
        val profiles = defaults.getJSONArray("profiles")
        val prefixes = defaults.getJSONArray("prefixes")
        val ports = defaults.getJSONArray("ports")
        return (0 until profiles.length()).map { index ->
            val profile = profiles.getJSONObject(index)
            val prefix = prefixes.getString(random.nextInt(prefixes.length()))
            val host = if (prefix.endsWith('.')) prefix + random.nextInt(1, 11) else prefix
            val endpoint = "$host:${ports.getInt(random.nextInt(ports.length()))}"
            val masks = profile.getJSONObject("masks")
            val config = buildString {
                append("[Interface]\nPrivateKey = ${registration.privateKey}\nAddress = ${registration.ipv4}\n")
                append("DNS = 1.1.1.1, 1.0.0.1\nMTU = 1280\nS1 = 0\nS2 = 0\nS3 = 0\nS4 = 0\n")
                append("Jc = 4\nJmin = 40\nJmax = 70\nH1 = 1\nH2 = 2\nH3 = 3\nH4 = 4\n")
                masks.keys().forEach { append("$it = <b 0x${masks.getString(it)}>\n") }
                append("\n[Peer]\nPublicKey = ${registration.publicKey}\nAllowedIPs = 0.0.0.0/0, ::/0\nEndpoint = $endpoint\n")
            }
            LinkParser.parseWireGuardConfig(config).copy(name = profile.getString("name"))
        }
    }

    fun masque(registration: WarpRegistration): List<ParsedNode> = listOf(false, true).map { h2 ->
        val profile = linkedMapOf<String, Any?>(
            "name" to if (h2) "WARP MASQUE · HTTP/2" else "WARP MASQUE",
            "type" to "masque", "server" to "162.159.198.2", "port" to 443, "sni" to "4pda.to",
            "private-key" to registration.privateKey, "public-key" to registration.publicKey,
            "ip" to registration.ipv4, "mtu" to 1280, "udp" to true,
            "remote-dns-resolve" to true, "dns" to listOf("1.1.1.1", "1.0.0.1", "2606:4700:4700::1111", "2606:4700:4700::1001")
        )
        if (registration.ipv6.isNotBlank()) profile["ipv6"] = registration.ipv6
        if (h2) profile["network"] = "h2"
        LinkParser.parseClashProxyMap(profile)
    }
}
