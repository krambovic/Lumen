package com.lumen.core.config.normalizer

import java.net.Inet6Address
import java.net.InetAddress
import java.util.Base64
import java.util.regex.Pattern

object AmneziaWGNormalizer {

    private val IPV4_REGEX = Pattern.compile("^([0-9]{1,3}\\.){3}[0-9]{1,3}$")
    private val IPV6_REGEX = Pattern.compile("^[0-9a-fA-F:]+$")
    private val SPLIT_DELIMITER_REGEX = Regex("[\\s,;]+")
    // Must stay in sync with LinkParser's AMNEZIA_* key sets, otherwise parsed
    // AmneziaWG 1.5 parameters are dropped before reaching the core.
    private val AMNEZIA_INT_KEYS = listOf("jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "itime")
    // uint32 in the core schema: values above Int.MAX_VALUE must not be toInt()ed.
    private val AMNEZIA_RANGE_KEYS = listOf("h1", "h2", "h3", "h4")
    // AWG 2.0 packet definitions: the core reads these as strings.
    private val AMNEZIA_STR_KEYS = listOf("i1", "i2", "i3", "i4", "i5", "j1", "j2", "j3")
    private val AMNEZIA_JUNK_KEYS = AMNEZIA_INT_KEYS + AMNEZIA_RANGE_KEYS + AMNEZIA_STR_KEYS
    // Cloudflare WARP endpoint networks (parity with the Windows client).
    private val WARP_IPV4_NETWORKS = listOf(
        Pair("162.159.192.0", 24),
        Pair("162.159.193.0", 24),
        Pair("188.114.96.0", 20)
    )
    private val WARP_IPV6_NETWORKS = listOf("2606:4700:d0::", "2606:4700:d1::")

    fun normalize(endpoint: Map<String, Any?>): Map<String, Any?> {
        return normalizeWireGuardEndpoint(endpoint)
    }

    fun normalizeIpPrefix(value: Any?): String {
        val text = value?.toString()?.trim()?.trim('"')?.trim('\'') ?: return ""
        if (text.isEmpty()) return ""
        if (text.contains("/")) {
            return text
        }
        if (IPV4_REGEX.matcher(text).matches()) {
            return "$text/32"
        }
        if (text.contains(":") && IPV6_REGEX.matcher(text).matches()) {
            return "$text/128"
        }
        return text
    }

    fun normalizeIpPrefixes(value: Any?): List<String> {
        val rawItems: List<Any?> = when (value) {
            is List<*> -> value
            is Iterable<*> -> value.toList()
            is Array<*> -> value.toList()
            null -> emptyList()
            else -> value.toString().split(",")
        }

        val result = mutableListOf<String>()
        for (item in rawItems) {
            val normalized = normalizeIpPrefix(item)
            if (normalized.isNotEmpty() && !result.contains(normalized)) {
                result.add(normalized)
            }
        }
        return result
    }

    fun parseReservedBytes(value: Any?): List<Int> {
        if (value == null || value == "" || value == emptyList<Any>()) return emptyList()

        if (value is List<*>) {
            val parsed = value.mapNotNull {
                when (it) {
                    is Number -> it.toInt()
                    is String -> it.toIntOrNull()
                    else -> null
                }
            }
            if (parsed.size == 3 && parsed.all { it in 0..255 }) {
                return parsed
            }
        }

        val text = value.toString().trim().trim('[', ']', ' ')
        val parts = text.split(SPLIT_DELIMITER_REGEX).filter { it.isNotEmpty() }

        if (parts.size == 1) {
            val encoded = parts[0].replace("-", "+").replace("_", "/")
            try {
                val padCount = (4 - (encoded.length % 4)) % 4
                val padded = encoded + "=".repeat(padCount)
                val decoded = Base64.getDecoder().decode(padded)
                if (decoded.size == 3) {
                    return decoded.map { it.toInt() and 0xFF }
                }
            } catch (_: Exception) {
            }
        }

        val parsed = parts.mapNotNull { it.toIntOrNull() }
        if (parsed.size == 3 && parsed.all { it in 0..255 }) {
            return parsed
        }

        return emptyList()
    }

    fun normalizeWireGuardEndpoint(endpoint: Map<String, Any?>): Map<String, Any?> {
        val result = endpoint.toMutableMap()
        val type = result["type"]?.toString()?.trim()?.lowercase() ?: ""

        if (type == "warp") {
            result["system"] = false
            // Core 2.5.1+ rejects an explicit "reserved" field on WARP endpoints.
            result.remove("reserved")
            return result
        }

        if (type != "wireguard" && type != "awg") {
            return result
        }

        result["system"] = false

        val privateKey = (result["private_key"] ?: result.remove("private-key") ?: result.remove("privateKey") ?: result.remove("secret_key") ?: result.remove("secret-key"))?.toString()?.trim() ?: ""
        if (privateKey.isNotEmpty()) {
            result["private_key"] = privateKey
        }

        val legacyServer = result.remove("server")?.toString()?.trim() ?: ""
        val legacyPort = positiveInt(result.remove("server_port"), 51820)
        val legacyPublicKey = result.remove("peer_public_key")?.toString()?.trim() ?: ""
        val legacyPreSharedKey = result.remove("pre_shared_key")?.toString()?.trim() ?: ""
        val legacyAllowedIps = result.remove("allowed_ips")
        val legacyReserved = parseReservedBytes(result.remove("reserved"))

        val localAddress = result.remove("local_address")
        val addressInput = if (result["address"] != null && result["address"] != "" && result["address"] != emptyList<Any>()) {
            result["address"]
        } else {
            localAddress
        }
        result["address"] = normalizeIpPrefixes(addressInput)

        var peersRaw = result["peers"]
        val peersList = mutableListOf<MutableMap<String, Any?>>()

        if (peersRaw !is List<*> || peersRaw.isEmpty()) {
            if (legacyServer.isNotEmpty() || legacyPublicKey.isNotEmpty()) {
                val singlePeer = mutableMapOf<String, Any?>(
                    "address" to legacyServer,
                    "port" to legacyPort,
                    "public_key" to legacyPublicKey,
                    "allowed_ips" to normalizeIpPrefixes(legacyAllowedIps ?: listOf("0.0.0.0/0", "::/0"))
                )
                // Only send pre_shared_key when there really is one.
                if (legacyPreSharedKey.isNotEmpty()) singlePeer["pre_shared_key"] = legacyPreSharedKey
                if (legacyReserved.isNotEmpty()) singlePeer["reserved"] = legacyReserved
                peersList.add(singlePeer)
            }
        } else {
            for (item in peersRaw) {
                if (item !is Map<*, *>) continue
                @Suppress("UNCHECKED_CAST")
                val peerMap = (item as Map<String, Any?>).toMutableMap()

                val peerEndpoint = peerMap.remove("endpoint")?.toString()?.trim() ?: ""
                var peerServer = (peerMap.remove("server") ?: peerMap["address"] ?: "").toString().trim()
                var peerPort = positiveInt(peerMap.remove("server_port") ?: peerMap["port"], 51820)

                if (peerEndpoint.isNotEmpty()) {
                    val (host, port) = splitEndpoint(peerEndpoint)
                    if (host.isNotEmpty()) peerServer = host
                    if (port > 0) peerPort = port
                }

                peerMap["address"] = peerServer
                peerMap["port"] = peerPort

                if (peerMap["public_key"] == null || peerMap["public_key"].toString().isEmpty()) {
                    peerMap["public_key"] = peerMap.remove("peer_public_key")?.toString()?.trim() ?: ""
                }

                val allowedIps = peerMap["allowed_ips"] ?: listOf("0.0.0.0/0", "::/0")
                peerMap["allowed_ips"] = normalizeIpPrefixes(allowedIps)

                val peerReserved = parseReservedBytes(peerMap.remove("reserved"))
                if (peerReserved.isNotEmpty() && legacyReserved.isEmpty()) {
                    result["_peer_reserved"] = peerReserved
                }

                peersList.add(peerMap)
            }
        }

        result["peers"] = peersList

        // Preserve AmneziaWG junk parameters inside "amnezia" sub-object for sing-box extended schema
        val amneziaSubMap = (endpoint["amnezia"] as? Map<*, *>)?.toMutableMap() ?: mutableMapOf()
        for (junkKey in AMNEZIA_JUNK_KEYS) {
            result.remove(junkKey) // Eliminate top-level unknown fields
            val value = endpoint[junkKey] ?: amneziaSubMap[junkKey]
            if (value != null) {
                when (junkKey) {
                    // A numeric i1/j1 must stay a string, and an h1 above
                    // Int.MAX_VALUE must not be truncated: both make the core
                    // abort while decoding the endpoint.
                    in AMNEZIA_STR_KEYS -> value.toString()
                        .takeIf { it.isNotEmpty() }
                        ?.let { amneziaSubMap[junkKey] = it }
                    in AMNEZIA_RANGE_KEYS -> if (value !is String || value.isNotEmpty()) {
                        amneziaSubMap[junkKey] = value
                    }
                    else -> {
                        val intVal = when (value) {
                            is Number -> value.toInt()
                            is String -> value.toIntOrNull()
                            else -> null
                        }
                        if (intVal != null) {
                            amneziaSubMap[junkKey] = intVal
                        } else if (value is String && value.isNotEmpty()) {
                            amneziaSubMap[junkKey] = value
                        }
                    }
                }
            }
        }
        // sing-box-extended follows the current endpoint schema: AmneziaWG is a
        // WireGuard endpoint carrying the extended "amnezia" options. "awg" is
        // only an import/display alias; using it as an endpoint type makes the
        // core abort with `unknown endpoint type: awg`.
        if (amneziaSubMap.isNotEmpty()) {
            @Suppress("UNCHECKED_CAST")
            result["amnezia"] = amneziaSubMap as Map<String, Any?>
        }
        result["type"] = "wireguard"

        @Suppress("UNCHECKED_CAST")
        val peerReserved = parseReservedBytes(result.remove("_peer_reserved"))
        val reservedBytes = if (legacyReserved.isNotEmpty()) legacyReserved else peerReserved
        result.remove("reserved")

        val peerServer = peersList.firstOrNull()?.get("address")?.toString() ?: legacyServer
        val isWarpType = type == "warp" || isCloudflareWarpPeer(peerServer)

        if (reservedBytes.isNotEmpty() && isWarpType) {
            // sing-box extended 2.5.1+ derives WARP reserved bytes from the profile itself.
            // An explicit "reserved" field is rejected by the config decoder with
            // 'endpoints[0].reserved: json: unknown field "reserved"', so it must be dropped.
            val warpEndpoint = mutableMapOf<String, Any?>(
                "type" to "warp",
                "tag" to (result["tag"]?.toString() ?: "proxy"),
                "system" to false,
                "udp_timeout" to (result["udp_timeout"]?.toString() ?: "5m0s"),
                "profile" to mapOf(
                    "detour" to "direct",
                    "private_key" to (result["private_key"]?.toString() ?: "")
                )
            )
            peersList.firstOrNull()?.get("persistent_keepalive_interval")?.let { keepalive ->
                val keepaliveInt = when (keepalive) {
                    is Number -> keepalive.toInt()
                    is String -> keepalive.toIntOrNull()
                    else -> null
                }
                if (keepaliveInt != null && keepaliveInt > 0) {
                    warpEndpoint["persistent_keepalive_interval"] = keepaliveInt
                }
            }
            // Parity with the Windows client: keep extended endpoint options
            // (including the AmneziaWG junk parameters) on the WARP endpoint.
            for (extraKey in listOf(
                "listen_port", "workers", "preallocated_buffers_per_pool",
                "disable_pauses", "amnezia", "domain_resolver", "detour"
            )) {
                val extraValue = result[extraKey]
                if (extraValue != null && extraValue != "" &&
                    extraValue != emptyList<Any?>() && extraValue != emptyMap<String, Any?>()
                ) {
                    warpEndpoint[extraKey] = extraValue
                }
            }
            return warpEndpoint
        }

        // The bundled extended core's WireGuardPeer schema has no `reserved`
        // field. It is reachable only through a WARP profile, so refuse the
        // combination instead of silently dropping it (parity with Windows).
        if (reservedBytes.isNotEmpty()) {
            throw IllegalArgumentException(
                "sing-box extended 2.5.x supports reserved bytes only for Cloudflare WARP profiles"
            )
        }
        peersList.forEach { it.remove("reserved") }
        result.remove("reserved")
        // Borrowed from ZapretKVN-android: conservative default MTU for userspace
        // (Amnezia)WireGuard on Android to avoid fragmentation stalls inside TUN.
        if (result["mtu"] == null) {
            result["mtu"] = 1280
        }
        return result
    }

    fun normalizeSingboxWireguardEndpoints(payload: MutableMap<String, Any?>) {
        val endpointsRaw = payload["endpoints"]
        var endpointsList = mutableListOf<Any?>()
        val hadEndpointList = endpointsRaw is List<*>

        if (endpointsRaw is List<*>) {
            for (ep in endpointsRaw) {
                if (ep is Map<*, *>) {
                    @Suppress("UNCHECKED_CAST")
                    endpointsList.add(normalizeWireGuardEndpoint(ep as Map<String, Any?>))
                } else {
                    endpointsList.add(ep)
                }
            }
        }

        val outboundsRaw = payload["outbounds"]
        if (outboundsRaw is List<*>) {
            val retainedOutbounds = mutableListOf<Any?>()
            for (ob in outboundsRaw) {
                if (ob is Map<*, *>) {
                    @Suppress("UNCHECKED_CAST")
                    val obMap = ob as Map<String, Any?>
                    val obType = obMap["type"]?.toString()?.trim()?.lowercase() ?: ""
                    if (obType == "wireguard" || obType == "awg" || obType == "warp") {
                        val migrated = normalizeWireGuardEndpoint(obMap)
                        val tag = migrated["tag"]?.toString()?.trim() ?: ""
                        if (tag.isNotEmpty()) {
                            endpointsList.removeAll { item ->
                                item is Map<*, *> && (item["tag"]?.toString()?.trim() ?: "") == tag
                            }
                        }
                        endpointsList.add(migrated)
                    } else {
                        retainedOutbounds.add(ob)
                    }
                } else {
                    retainedOutbounds.add(ob)
                }
            }
            payload["outbounds"] = retainedOutbounds
        }

        if (hadEndpointList || endpointsList.isNotEmpty()) {
            payload["endpoints"] = endpointsList
        }
    }

    private fun positiveInt(value: Any?, fallback: Int): Int {
        val parsed = when (value) {
            is Number -> value.toInt()
            is String -> value.toIntOrNull() ?: 0
            else -> 0
        }
        return if (parsed > 0) parsed else fallback
    }

    private fun splitEndpoint(value: String): Pair<String, Int> {
        val text = value.trim()
        if (text.isEmpty()) return Pair("", 0)

        if (text.startsWith("[")) {
            val closing = text.indexOf("]")
            if (closing > 0) {
                val host = text.substring(1, closing)
                val remainder = text.substring(closing + 1)
                val port = if (remainder.startsWith(":")) {
                    remainder.substring(1).toIntOrNull() ?: 0
                } else 0
                return Pair(host, port)
            }
        }

        val lastColon = text.lastIndexOf(":")
        if (lastColon > 0 && lastColon < text.length - 1) {
            val host = text.substring(0, lastColon)
            val portText = text.substring(lastColon + 1)
            val port = portText.toIntOrNull() ?: 0
            if (port > 0) {
                return Pair(host, port)
            }
        }

        return Pair(text, 0)
    }

    private fun parseIpv4(text: String): Int? {
        val parts = text.split(".")
        if (parts.size != 4) return null
        var value = 0
        for (part in parts) {
            if (part.isEmpty() || part.length > 3 || part.any { !it.isDigit() }) return null
            val octet = part.toInt()
            if (octet > 255) return null
            value = (value shl 8) or octet
        }
        return value
    }

    private fun ipv6Prefix48(text: String): ByteArray? {
        if (!text.contains(":") || !IPV6_REGEX.matcher(text).matches()) return null
        return try {
            val address = InetAddress.getByName(text)
            if (address is Inet6Address) address.address.copyOfRange(0, 6) else null
        } catch (_: Exception) {
            null
        }
    }

    private fun isCloudflareWarpPeer(server: String): Boolean {
        val host = server.trim().lowercase().trimEnd('.').trim('[', ']')
        if (host.isEmpty()) return false
        if (host == "engage.cloudflareclient.com" || host.endsWith(".cloudflareclient.com")) {
            return true
        }
        val ipv4 = parseIpv4(host)
        if (ipv4 != null) {
            return WARP_IPV4_NETWORKS.any { (network, bits) ->
                val networkValue = parseIpv4(network) ?: return@any false
                val mask = -1 shl (32 - bits)
                (ipv4 and mask) == (networkValue and mask)
            }
        }
        val prefix = ipv6Prefix48(host) ?: return false
        return WARP_IPV6_NETWORKS.any { network ->
            val networkPrefix = ipv6Prefix48(network) ?: return@any false
            prefix.contentEquals(networkPrefix)
        }
    }
}
