package com.lumen.app.vm

import java.io.ByteArrayOutputStream
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.Inet4Address
import java.net.InetAddress
import java.net.InetSocketAddress
import kotlin.random.Random

/**
 * Works around resolvers that hijack, rather than fail, a lookup.
 *
 * The core resolves an outbound's hostname through its bootstrap chain, whose
 * plaintext step asks a foreign public resolver over UDP/53. Several carriers -
 * Iran's most visibly, which answers 10.10.34.x - intercept exactly that traffic
 * and reply with a block-page address. A hijacked reply is a valid NOERROR
 * answer, so no fallback chain in the core can step past it: fallbacks advance
 * on failure, and nothing here failed. The core also has no knob to reject an
 * answer by address; that was verified against this very binary.
 *
 * The app can do what the core cannot. Before the tunnel exists it asks both
 * resolvers and compares. A public VPN endpoint can never legitimately live on a
 * private, loopback, link-local or otherwise reserved address, so such an answer
 * is proof of interception. Only then - and only when the network's own resolver
 * returns something plausible, which is what every working client on that phone
 * ends up using - is the address pinned into a static table for that one
 * hostname. Every other case leaves the configuration exactly as it was.
 */
object ServerAddressPinner {

    private const val QUERY_TIMEOUT_MS = 2500
    private const val DNS_PORT = 53
    private const val TYPE_A = 1
    private const val CLASS_IN = 1

    /**
     * @param hostnames outbound server hostnames (IP literals are ignored)
     * @param foreignResolver the plaintext resolver the core's bootstrap would use
     * @return hostname -> vetted addresses, empty when nothing needs pinning
     */
    fun pinnedAddresses(
        hostnames: Collection<String>,
        foreignResolver: String
    ): Map<String, List<String>> {
        val resolver = foreignResolver.trim()
        if (resolver.isEmpty() || !isIpLiteral(resolver)) return emptyMap()
        val pinned = LinkedHashMap<String, List<String>>()
        for (raw in hostnames.distinct()) {
            val host = raw.trim().trimEnd('.').lowercase()
            if (host.isEmpty() || isIpLiteral(host) || !host.any(Char::isLetter)) continue
            val hijacked = runCatching { queryA(host, resolver) }.getOrDefault(emptyList())
            // Nothing came back, or something plausible did: leave this hostname to
            // the normal bootstrap chain. Pinning here could only make it worse.
            if (hijacked.isEmpty() || hijacked.any { !isBogusEndpointAddress(it) }) continue
            val trusted = runCatching { InetAddress.getAllByName(host).toList() }
                .getOrDefault(emptyList())
                .filterNot(::isBogusEndpointAddress)
                .mapNotNull { it.hostAddress }
                .distinct()
            if (trusted.isNotEmpty()) pinned[host] = trusted
        }
        return pinned
    }

    /**
     * Pool-sized variant. Probing every member would cost a UDP round trip each,
     * so one hostname is probed first; on a healthy network that single query is
     * the whole cost and nothing is pinned. Only once interception is proven do
     * the remaining hostnames get resolved, and that runs against the local
     * network resolver, which is cached and fast.
     */
    fun pinnedAddressesForPool(
        probeHost: String,
        hostnames: Collection<String>,
        foreignResolver: String,
        limit: Int = 64
    ): Map<String, List<String>> {
        val probe = pinnedAddresses(listOf(probeHost), foreignResolver)
        if (probe.isEmpty()) return emptyMap()
        val pinned = LinkedHashMap<String, List<String>>(probe)
        for (raw in hostnames.distinct()) {
            if (pinned.size >= limit) break
            val host = raw.trim().trimEnd('.').lowercase()
            if (host.isEmpty() || host in pinned || isIpLiteral(host) || !host.any(Char::isLetter)) continue
            val trusted = runCatching { InetAddress.getAllByName(host).toList() }
                .getOrDefault(emptyList())
                .filterNot(::isBogusEndpointAddress)
                .mapNotNull { it.hostAddress }
                .distinct()
            if (trusted.isNotEmpty()) pinned[host] = trusted
        }
        return pinned
    }

    /** An address no reachable public endpoint can legitimately have. */
    fun isBogusEndpointAddress(address: InetAddress): Boolean =
        address.isAnyLocalAddress ||
            address.isLoopbackAddress ||
            address.isLinkLocalAddress ||
            address.isSiteLocalAddress ||
            address.isMulticastAddress ||
            address.isMCGlobal ||
            isSharedOrReserved(address)

    /** CGNAT and the reserved blocks java.net has no predicate for. */
    private fun isSharedOrReserved(address: InetAddress): Boolean {
        if (address !is Inet4Address) return false
        val bytes = address.address.map { it.toInt() and 0xFF }
        return when {
            bytes[0] == 100 && bytes[1] in 64..127 -> true // 100.64.0.0/10, CGNAT
            bytes[0] == 192 && bytes[1] == 0 && bytes[2] == 0 -> true // 192.0.0.0/24
            bytes[0] == 192 && bytes[1] == 0 && bytes[2] == 2 -> true // TEST-NET-1
            bytes[0] == 198 && bytes[1] in 18..19 -> true // benchmarking, also fakeip
            bytes[0] == 198 && bytes[1] == 51 && bytes[2] == 100 -> true // TEST-NET-2
            bytes[0] == 203 && bytes[1] == 0 && bytes[2] == 113 -> true // TEST-NET-3
            bytes[0] >= 240 -> true // reserved / broadcast
            else -> false
        }
    }

    private fun isIpLiteral(value: String): Boolean {
        val host = value.removePrefix("[").removeSuffix("]")
        if (host.contains(':')) return true
        val parts = host.split('.')
        return parts.size == 4 && parts.all { part ->
            part.isNotEmpty() && part.all(Char::isDigit) && part.toIntOrNull() in 0..255
        }
    }

    /** Minimal A-record query; no library on the platform does plain UDP DNS. */
    private fun queryA(host: String, resolver: String): List<InetAddress> {
        val id = Random.nextInt(0, 0xFFFF)
        val request = buildQuery(id, host)
        DatagramSocket().use { socket ->
            socket.soTimeout = QUERY_TIMEOUT_MS
            socket.send(
                DatagramPacket(
                    request, request.size,
                    InetSocketAddress(InetAddress.getByName(resolver), DNS_PORT)
                )
            )
            val buffer = ByteArray(1500)
            val response = DatagramPacket(buffer, buffer.size)
            socket.receive(response)
            return parseAnswers(buffer, response.length, id)
        }
    }

    private fun buildQuery(id: Int, host: String): ByteArray {
        val out = ByteArrayOutputStream()
        out.write(id shr 8)
        out.write(id and 0xFF)
        out.write(0x01); out.write(0x00) // standard query, recursion desired
        out.write(0x00); out.write(0x01) // one question
        repeat(6) { out.write(0x00) } // no answer/authority/additional records
        for (label in host.split('.')) {
            val bytes = label.toByteArray(Charsets.US_ASCII)
            if (bytes.isEmpty() || bytes.size > 63) continue
            out.write(bytes.size)
            out.write(bytes)
        }
        out.write(0x00)
        out.write(0x00); out.write(TYPE_A)
        out.write(0x00); out.write(CLASS_IN)
        return out.toByteArray()
    }

    private fun parseAnswers(buffer: ByteArray, length: Int, expectedId: Int): List<InetAddress> {
        fun byteAt(index: Int): Int = buffer[index].toInt() and 0xFF
        if (length < 12) return emptyList()
        if ((byteAt(0) shl 8 or byteAt(1)) != expectedId) return emptyList()
        val questions = byteAt(4) shl 8 or byteAt(5)
        val answers = byteAt(6) shl 8 or byteAt(7)
        if (answers <= 0) return emptyList()

        // Names are length-prefixed labels and may end in a compression pointer.
        fun skipName(from: Int): Int {
            var cursor = from
            while (cursor < length) {
                val len = byteAt(cursor)
                when {
                    len == 0 -> return cursor + 1
                    len and 0xC0 == 0xC0 -> return cursor + 2
                    else -> cursor += len + 1
                }
            }
            return length
        }

        var cursor = 12
        repeat(questions) { cursor = skipName(cursor) + 4 }
        val result = mutableListOf<InetAddress>()
        repeat(answers) {
            if (cursor + 10 > length) return result
            cursor = skipName(cursor)
            if (cursor + 10 > length) return result
            val type = byteAt(cursor) shl 8 or byteAt(cursor + 1)
            val dataLength = byteAt(cursor + 8) shl 8 or byteAt(cursor + 9)
            val dataStart = cursor + 10
            if (dataStart + dataLength > length) return result
            if (type == TYPE_A && dataLength == 4) {
                runCatching {
                    InetAddress.getByAddress(buffer.copyOfRange(dataStart, dataStart + 4))
                }.getOrNull()?.let(result::add)
            }
            cursor = dataStart + dataLength
        }
        return result
    }
}
