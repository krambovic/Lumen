package com.lumen.core.config.builder

import com.lumen.core.config.parser.LinkParser
import com.lumen.core.config.parser.ParsedNode
import org.json.JSONObject

enum class CoreMode { HYBRID, SINGBOX, XRAY;
    companion object {
        fun of(value: String) = entries.firstOrNull { it.name.equals(value, true) } ?: HYBRID
    }
}

/** sing-box retains Android routing/DNS; Xray is spawned only for outbounds that need it. */
object HybridConfigBuilder {
    const val SIDECAR_KEY = "_lumen_xray"
    private val xrayProtocols = setOf("vless", "vmess", "trojan", "shadowsocks", "socks", "http")

    fun needsXray(node: ParsedNode): Boolean {
        val stream = node.outbound["streamSettings"] as? Map<*, *> ?: emptyMap<String, Any?>()
        val reality = stream["realitySettings"] as? Map<*, *>
        val tls = stream["tlsSettings"] as? Map<*, *>
        val network = stream["network"]?.toString().orEmpty()
        val settings = node.outbound["settings"] as? Map<*, *>
        val next = (settings?.get("vnext") as? List<*>)?.firstOrNull() as? Map<*, *>
        val user = (next?.get("users") as? List<*>)?.firstOrNull() as? Map<*, *>
        val encryption = user?.get("encryption")?.toString().orEmpty()
        return stream.keys.any { it?.toString().equals("finalmask", true) } || reality?.containsKey("mldsa65Verify") == true ||
            // The native REALITY/Firefox path can fail authentication against
            // Xray servers. Preserve their handshake and XHTTP semantics in Xray.
            (stream["security"] == "reality" &&
                (network in setOf("xhttp", "splithttp") || reality?.get("fingerprint") == "firefox")) ||
            tls?.keys?.any { it in setOf("echConfigList", "pinnedPeerCertSha256", "verifyPeerCertByName") } == true ||
            stream.containsKey("tcpSettings") || network in setOf("kcp", "http", "h2") ||
            (encryption.isNotBlank() && encryption != "none")
    }

    fun buildConfig(node: ParsedNode, options: SingboxConfigOptions, mode: CoreMode,
                    allocatePort: () -> Int): String {
        val inbounds = mutableListOf<Map<String, Any?>>()
        val outbounds = mutableListOf<Map<String, Any?>>()
        val ports = mutableSetOf(options.localSocksPort, options.localHttpPort)
        fun transform(original: ParsedNode): ParsedNode {
            val item = com.lumen.core.config.parser.ShareLinkExtensions.restore(original)
            if (item.scheme == "auto") {
                val members = LinkParser.autoMembers(item.outbound).map { transform(it) }
                return item.copy(outbound = item.outbound + ("auto_members" to members.map {
                    mapOf("name" to it.name, "scheme" to it.scheme, "server" to it.server,
                        "port" to it.port, "link" to it.link, "outbound" to it.outbound)
                }))
            }
            if (mode == CoreMode.SINGBOX) {
                require(!needsXray(item)) { "This profile requires Xray extensions; choose Hybrid or Xray" }
                return item
            }
            if (mode == CoreMode.HYBRID && !needsXray(item)) return item
            val protocol = item.outbound["protocol"]?.toString().orEmpty()
            require(protocol in xrayProtocols && !item.outbound.containsKey("singbox")) {
                "Xray cannot run ${item.scheme}; choose Hybrid or Singbox"
            }
            var port = allocatePort()
            var retries = 0
            while (port in ports && retries++ < 32) port = allocatePort()
            require(port in 1..65535 && ports.add(port)) { "Could not reserve a unique Xray sidecar port" }
            val index = outbounds.size
            val tag = "lumen-xray-$index"
            val password = java.util.UUID.randomUUID().toString()
            inbounds += mapOf("tag" to tag, "listen" to "127.0.0.1", "port" to port,
                "protocol" to "socks", "settings" to mapOf("auth" to "password", "udp" to true,
                    "accounts" to listOf(mapOf("user" to "lumen", "pass" to password))))
            val xray = JSONObject(LinkParser.toJsonString(item.outbound.filterKeys {
                it !in setOf("tag", "_singbox_dependencies")
            }))
            val stream = xray.optJSONObject("streamSettings") ?: JSONObject().also { xray.put("streamSettings", it) }
            if (stream.optString("security") == "reality") {
                val reality = stream.optJSONObject("realitySettings") ?: JSONObject()
                val publicKey = reality.optString("password").ifBlank { reality.optString("publicKey") }
                require(publicKey.isNotBlank()) {
                    "REALITY public key (pbk) is missing. Import the complete link; it may have been truncated."
                }
                // Current Xray calls the client public key 'password'; accept old profiles too.
                reality.put("password", publicKey)
                reality.remove("publicKey")
                stream.put("realitySettings", reality)
            }
            stream.optJSONObject("tlsSettings")?.let { tls ->
                require(!tls.has("certificatePublicKeySha256") || tls.has("pinnedPeerCertSha256")) {
                    "This profile pins an SPKI key; Xray requires a full certificate pin (pcs). Use Singbox for this profile."
                }
                // RFC 6455 WebSocket uses an HTTP/1.1 upgrade. Some panels offer h3/h2
                // as well, causing TLS to select a protocol this transport cannot use.
                if (stream.optString("network") == "ws") {
                    tls.optJSONArray("alpn")?.let { offered ->
                        val protocols = (0 until offered.length()).map { offered.getString(it) }
                        require("http/1.1" in protocols || protocols.isEmpty()) {
                            "WebSocket requires HTTP/1.1 in ALPN; use XHTTP for HTTP/2 or HTTP/3"
                        }
                        if (protocols.isNotEmpty()) tls.put("alpn", org.json.JSONArray(listOf("http/1.1")))
                    }
                }
            }
            val sockopt = stream.optJSONObject("sockopt") ?: JSONObject()
            if (options.outboundTcpFastOpen) sockopt.put("tcpFastOpen", true)
            if (options.outboundTcpMultiPath) sockopt.put("tcpMptcp", true)
            if (sockopt.length() > 0) stream.put("sockopt", sockopt)
            if (options.multiplexEnabled && !xray.has("mux") && stream.optString("network") != "xhttp") {
                xray.put("mux", JSONObject().put("enabled", true).put("concurrency", options.multiplexConcurrency))
            }
            if (options.enableFinalFragment && !stream.has("finalMask") && !stream.has("finalmask")) {
                stream.put("finalMask", JSONObject(LinkParser.toJsonString(mapOf("tcp" to listOf(mapOf(
                    "type" to "fragment", "settings" to mapOf("packets" to options.fragmentPackets,
                        "lengths" to listOf(options.fragmentLength), "delays" to listOf(options.fragmentDelay))))))))
            }
            val pinned = options.pinnedServerIps[item.server]?.firstOrNull()
            if (!pinned.isNullOrBlank()) {
                val settings = xray.optJSONObject("settings")
                (settings?.optJSONArray("vnext") ?: settings?.optJSONArray("servers"))?.let { servers ->
                    for (i in 0 until servers.length()) {
                        val server = servers.optJSONObject(i) ?: continue
                        if (server.optString("address") == item.server) server.put("address", pinned)
                    }
                }
                val tls = stream.optJSONObject(if (stream.optString("security") == "reality") "realitySettings" else "tlsSettings")
                if (tls != null && tls.optString("serverName").isBlank()) tls.put("serverName", item.server)
            }
            xray.put("tag", tag)
            outbounds += LinkParser.jsonToMap(xray)
            return item.copy(scheme = "socks", server = "127.0.0.1", port = port,
                outbound = mapOf("protocol" to "socks", "singbox" to mapOf("type" to "socks",
                    "server" to "127.0.0.1", "server_port" to port, "version" to "5",
                    "username" to "lumen", "password" to password)))
        }
        val selected = transform(node)
        val json = try {
            JSONObject(SingboxConfigBuilder.buildConfig(selected, options))
        } catch (error: IllegalArgumentException) {
            // Preserve valid Xray settings that have no sing-box representation.
            if (mode == CoreMode.HYBRID && outbounds.isEmpty() &&
                node.outbound["protocol"]?.toString() in xrayProtocols &&
                !node.outbound.containsKey("singbox")) {
                return buildConfig(node, options, CoreMode.XRAY, allocatePort)
            }
            throw error
        }
        if (outbounds.isNotEmpty()) {
            val rules = inbounds.map { mapOf("type" to "field", "inboundTag" to listOf(it["tag"]),
                "outboundTag" to it["tag"]) }
            json.put(SIDECAR_KEY, JSONObject(LinkParser.toJsonString(mapOf(
                "log" to mapOf("loglevel" to "warning"), "inbounds" to inbounds,
                "outbounds" to outbounds, "routing" to mapOf("rules" to rules)))))
        }
        return json.toString()
    }
}
