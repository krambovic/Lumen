package com.lumen.core.config.parser

import com.lumen.core.config.builder.SingboxConfigBuilder
import java.net.URLEncoder
import java.util.Base64
import org.json.JSONArray
import org.json.JSONObject

/** Desktop-style portable export. Never replace an outbound with a lossy URI. */
object NodeExportNormalizer {
    fun exportAll(nodes: Iterable<ParsedNode>): String = nodes.map(::export)
        .filter(String::isNotBlank).joinToString("\n")

    fun export(node: ParsedNode): String {
        val raw = node.link.trim()
        if (node.scheme.lowercase() in setOf("auto", "singbox_config", "xray_config", "openvpn") ||
            node.outbound.containsKey("_singbox_dependencies")) return compact(raw)
        return runCatching {
            val native = SingboxConfigBuilder.buildOutbound(node)
            val candidate = shareLink(node, native)
            if (candidate != null && equivalent(node, LinkParser.parseSingle(candidate))) return candidate
            // Preserve extensions in an existing portable link if reconstruction
            // cannot represent them, but never export an obsolete edited link.
            if ("://" in raw && !raw.startsWith("{") && !raw.startsWith("[") &&
                equivalent(node, LinkParser.parseSingle(raw))) {
                return raw.substringBefore('#') + "#" + encode(node.name)
            }
            if (raw.startsWith("{") || raw.startsWith("[") || !raw.contains("://")) compact(raw)
            else JSONObject((node.outbound["singbox"] as? Map<*, *>) ?: node.outbound)
                .put("tag", node.name).toString()
        }.getOrElse { compact(raw) }
    }

    private fun equivalent(source: ParsedNode, restored: ParsedNode): Boolean {
        // Dial dependencies, profile DNS and explicitly enabled multiplexing must
        // not disappear simply because the runtime builder supplies defaults.
        if (source.outbound["_dns"] != restored.outbound["_dns"]) return false
        val mux = source.outbound["mux"] as? Map<*, *>
        if (mux?.get("enabled") == true) return false
        val native = source.outbound["singbox"] as? Map<*, *>
        if ((native?.get("multiplex") as? Map<*, *>)?.get("enabled") == true) return false
        return canonicalOutbound(SingboxConfigBuilder.buildOutbound(source)) ==
            canonicalOutbound(SingboxConfigBuilder.buildOutbound(restored))
    }

    private fun canonicalOutbound(outbound: Map<String, Any?>): Any? {
        val value = outbound.toMutableMap()
        val tls = value["tls"] as? Map<*, *>
        if (tls?.get("enabled") == true && tls["server_name"]?.toString().isNullOrEmpty()) {
            value["tls"] = tls + ("server_name" to value["server"])
        }
        if (value["alter_id"] == 0) value.remove("alter_id")
        if (value["type"] == "socks" && value["version"]?.toString() == "5") value.remove("version")
        return canonical(value)
    }

    private fun canonical(value: Any?): Any? = when (value) {
        is Map<*, *> -> value.entries.filter { it.key != "tag" }
            .mapNotNull { (key, item) -> canonical(item)?.let { key.toString() to it } }.toMap()
            .takeIf { it.isNotEmpty() }
        is List<*> -> value.mapNotNull(::canonical).takeIf { it.isNotEmpty() }
        is Number -> value.toString().toBigDecimalOrNull()?.stripTrailingZeros()
        null, false, "" -> null
        else -> value
    }

    private fun shareLink(node: ParsedNode, native: Map<String, Any?>): String? {
        val type = native.text("type")
        val params = linkedMapOf<String, String>()
        fun param(key: String, value: Any?) {
            val text = when (value) { is List<*> -> value.joinToString(","); null -> ""; else -> value.toString() }
            if (text.isNotBlank()) params[key] = text
        }
        val tls = native.map("tls")
        val reality = tls.map("reality")
        param("sni", tls["server_name"])
        param("alpn", tls["alpn"])
        param("fp", tls.map("utls")["fingerprint"])
        if (tls["insecure"] == true) param("allowInsecure", "1")
        param("pinSHA256", tls["certificate_public_key_sha256"])
        val host = native.text("server").ifBlank { node.server }.removePrefix("[").removeSuffix("]")
        val port = native["server_port"]?.toString()?.toIntOrNull() ?: node.port
        val authority = "${if (':' in host) "[$host]" else host}:$port"
        fun uri(scheme: String, credentials: String = "", endpoint: String = authority): String =
            "$scheme://${if (credentials.isEmpty()) "" else "$credentials@"}$endpoint" +
                params.entries.joinToString("&", prefix = if (params.isEmpty()) "" else "?") { (k, v) -> "${encode(k)}=${encode(v)}" } +
                "#${encode(node.name)}"
        fun credentials() = if (native.text("username").isEmpty()) "" else
            "${encode(native.text("username"))}:${encode(native.text("password"))}"
        if (type !in setOf("wireguard", "awg", "masque", "warp") && (host.isBlank() || port !in 1..65535)) return null
        when (type) {
            "vless", "vmess", "trojan" -> {
                val transport = native.map("transport")
                val network = transport.text("type").ifBlank { "tcp" }
                param("type", network)
                param("security", if (reality["enabled"] == true) "reality" else if (tls["enabled"] == true) "tls" else "none")
                param("pbk", reality["public_key"])
                param("sid", reality["short_id"])
                param("path", transport["path"])
                param("host", transport.map("headers")["Host"] ?: transport["host"])
                param("serviceName", transport["service_name"])
                param("mode", transport["mode"])
                if (type == "vless") {
                    if (native.text("uuid").isBlank()) return null
                    param("flow", native["flow"])
                    param("encryption", native["encryption"] ?: "none")
                    param("packetEncoding", native["packet_encoding"] ?: "none")
                    return uri("vless", encode(native.text("uuid")))
                }
                if (type == "trojan") {
                    if (native.text("password").isBlank()) return null
                    return uri("trojan", encode(native.text("password")))
                }
                if (native.text("uuid").isBlank()) return null
                val payload = linkedMapOf<String, Any?>("v" to "2", "ps" to node.name, "add" to host,
                    "port" to port, "id" to native["uuid"], "aid" to (native["alter_id"] ?: 0),
                    "scy" to (native["security"] ?: "auto"), "net" to network,
                    "tls" to params["security"], "host" to params["host"], "path" to params["path"],
                    "sni" to params["sni"], "alpn" to params["alpn"], "fp" to params["fp"],
                    "allowInsecure" to params["allowInsecure"], "pinSHA256" to params["pinSHA256"])
                return "vmess://" + Base64.getEncoder().encodeToString(JSONObject(payload).toString().toByteArray(Charsets.UTF_8))
            }
            "shadowsocks" -> {
                val method = native.text("method")
                if (method.isBlank() || (native.text("password").isBlank() && method != "none")) return null
                param("plugin", listOf(native.text("plugin"), native.text("plugin_opts")).filter(String::isNotBlank).joinToString(";"))
                return uri("ss", Base64.getUrlEncoder().withoutPadding().encodeToString("$method:${native.text("password")}".toByteArray(Charsets.UTF_8)))
            }
            "socks", "http" -> return uri(if (type == "http" && tls["enabled"] == true) "https" else type, credentials())
            "hysteria", "hysteria2" -> {
                val password = native.text(if (type == "hysteria") "auth_str" else "password")
                if (password.isBlank()) return null
                for (key in listOf("protocol", "up_mbps", "down_mbps", "server_ports", "hop_interval")) param(key, native[key])
                val obfs = native["obfs"]
                if (obfs is Map<*, *>) { param("obfs", obfs["type"]); param("obfs-password", obfs["password"]) }
                else param("obfs", obfs)
                return uri(type, encode(password))
            }
            "tuic" -> {
                if (native.text("uuid").isBlank()) return null
                for (key in listOf("congestion_control", "udp_relay_mode", "zero_rtt_handshake")) param(key, native[key])
                return uri(type, "${encode(native.text("uuid"))}:${encode(native.text("password"))}")
            }
            "anytls" -> {
                if (native.text("password").isBlank()) return null
                for (key in listOf("idle_session_check_interval", "idle_session_timeout", "min_idle_session")) param(key, native[key])
                return uri(type, encode(native.text("password")))
            }
            "naive", "mieru" -> {
                if (native.text("username").isBlank() || native.text("password").isBlank()) return null
                for (key in listOf("quic", "insecure_concurrency", "quic_congestion_control", "server_ports", "transport", "multiplexing", "traffic_pattern")) param(key, native[key])
                return uri(type, credentials())
            }
            "wireguard", "awg" -> {
                val peers = native["peers"] as? List<*> ?: return null
                if (peers.size != 1) return null
                val peer = peers.first() as? Map<*, *> ?: return null
                val key = native.text("private_key")
                if (key.isBlank() || peer["public_key"]?.toString().isNullOrBlank()) return null
                params.clear()
                param("publickey", peer["public_key"])
                param("ip", native["address"])
                param("allowedips", peer["allowed_ips"])
                param("presharedkey", peer["pre_shared_key"])
                param("keepalive", peer["persistent_keepalive_interval"])
                param("reserved", peer["reserved"])
                param("mtu", native["mtu"])
                param("dns", node.outbound["_dns"])
                native.map("amnezia").forEach { (k, v) -> param(k.toString(), v) }
                val peerHost = (peer["address"] ?: peer["server"] ?: node.server).toString().removePrefix("[").removeSuffix("]")
                val peerPort = peer["port"] ?: peer["server_port"] ?: node.port
                return uri(if (native.map("amnezia").isEmpty() && node.scheme != "awg") "wireguard" else "awg",
                    encode(key), "${if (':' in peerHost) "[$peerHost]" else peerHost}:$peerPort")
            }
            "masque" -> {
                // Registered profiles have a URI; direct endpoints with tunnel
                // keys and addresses require JSON to remain usable.
                if (listOf("private_key", "public_key", "address", "server").any { native[it] != null }) return null
                val profile = native.map("profile")
                if (profile.text("id").isBlank() || profile["private_key"] != null) return null
                for (key in listOf("system", "name", "use_http2", "use_ipv6", "udp_timeout", "udp_keepalive_period", "reconnect_delay", "congestion_controller", "allowed_ips")) param(key, native[key])
                return uri("masque", encode(profile.text("auth_token")), encode(profile.text("id")))
            }
        }
        return null
    }

    private fun encode(value: String): String = URLEncoder.encode(value, "UTF-8").replace("+", "%20")
    private fun Map<*, *>.text(key: String): String = this[key]?.toString().orEmpty()
    private fun Map<*, *>.map(key: String): Map<*, *> = this[key] as? Map<*, *> ?: emptyMap<Any, Any>()
    private fun compact(raw: String): String = runCatching {
        when { raw.startsWith("{") -> JSONObject(raw).toString(); raw.startsWith("[") -> JSONArray(raw).toString(); else -> raw }
    }.getOrDefault(raw)
}
