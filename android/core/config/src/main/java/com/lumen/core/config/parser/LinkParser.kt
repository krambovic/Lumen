package com.lumen.core.config.parser

import com.lumen.core.config.crypto.HappCrypt
import org.json.JSONArray
import org.json.JSONObject
import org.yaml.snakeyaml.Yaml
import java.net.URI
import java.net.URLDecoder
import java.util.Base64
import java.util.regex.Pattern

open class LinkParseError(message: String, cause: Throwable? = null) : Exception(message, cause)

data class ParsedNode(
    var name: String,
    var scheme: String,
    var server: String,
    var port: Int,
    var link: String,
    var outbound: Map<String, Any?> = emptyMap(),
    var description: String = ""
)

object LinkParser {

    private val WHITESPACE_REGEX = Regex("\\s+")
    // Used to split glued or space separated links inside a single line.
    private val SCHEME_SPLIT_REGEX = Regex(
        "(?i)\\b(vless|vmess|trojan|ss|ssr|hysteria2|hysteria|hy2|hy|tuic|wireguard|wg|awg|" +
            "amneziawg|warp|naive\\+https|naive\\+quic|naive|mierus|mieru|masque|socks5|socks|" +
            "https|http|happ|snell|juicity|anytls)://"
    )
    private val AMNEZIA_JUNK_KEYS = setOf(
        "jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "h1", "h2", "h3", "h4",
        "i1", "i2", "i3", "i4", "i5", "j1", "j2", "j3", "itime"
    )

    const val MAX_IMPORT_BYTES = 8 * 1024 * 1024
    const val MAX_IMPORT_LINES = 20000
    const val MAX_IMPORT_NODES = 20000

    private val AUTO_GROUP_TYPES = setOf("urltest", "url-test", "selector")

    /** Packs a urltest/selector pool into one AUTO node (parity with desktop Lumen). */
    private fun autoNodeFromMembers(name: String, members: List<ParsedNode>): ParsedNode {
        val packed = members.map { member ->
            mapOf(
                "name" to member.name,
                "scheme" to member.scheme,
                "server" to member.server,
                "port" to member.port,
                "link" to member.link,
                "outbound" to member.outbound
            )
        }
        return ParsedNode(
            name = name,
            scheme = "auto",
            server = "",
            port = 0,
            link = "auto",
            outbound = mapOf("protocol" to "auto", "auto_members" to packed)
        )
    }

    /** Restores the servers packed into an AUTO node by [autoNodeFromMembers]. */
    fun autoMembers(outbound: Map<String, Any?>): List<ParsedNode> {
        val packed = outbound["auto_members"] as? List<*> ?: return emptyList()
        return packed.mapNotNull { item ->
            val map = item as? Map<*, *> ?: return@mapNotNull null
            @Suppress("UNCHECKED_CAST")
            val memberOutbound = (map["outbound"] as? Map<String, Any?>) ?: emptyMap()
            val port = (map["port"] as? Number)?.toInt() ?: map["port"]?.toString()?.toIntOrNull() ?: 0
            ParsedNode(
                name = map["name"]?.toString().orEmpty(),
                scheme = map["scheme"]?.toString().orEmpty(),
                server = map["server"]?.toString().orEmpty(),
                port = port,
                link = map["link"]?.toString().orEmpty(),
                outbound = memberOutbound
            )
        }
    }

    fun parseLinksText(text: String): Pair<List<ParsedNode>, List<String>> {
        val bytes = text.toByteArray(Charsets.UTF_8)
        if (bytes.size > MAX_IMPORT_BYTES) {
            return Pair(emptyList(), listOf("Import data exceeds the $MAX_IMPORT_BYTES-byte limit"))
        }

        var stripped = text.trim().removePrefix("\uFEFF")
        if (HappCrypt.isHappCryptLink(stripped)) {
            try {
                stripped = HappCrypt.decryptHappLink(stripped).trim()
            } catch (e: Exception) {
                return Pair(emptyList(), listOf("Happ crypt decryption failed: ${e.message}"))
            }
        }

        if (stripped.startsWith("{") || stripped.startsWith("[")) {
            try {
                return parseJsonNodesText(stripped)
            } catch (e: Exception) {
                // If the top-level JSON parse failed and input starts with '{',
                // try NDJSON (newline-delimited JSON objects, one per line).
                if (stripped.startsWith("{")) {
                    val ndjson = tryParseJsonLines(stripped)
                    if (ndjson != null) return ndjson
                }
                // Fall through to text lines
            }
        }

        if (looksLikeClashYaml(stripped)) {
            try {
                return parseClashYamlNodesText(stripped)
            } catch (e: Exception) {
                return Pair(emptyList(), listOf("Clash YAML error: ${e.message}"))
            }
        }

        if (looksLikeOpenVpnConfig(stripped)) {
            try {
                val node = parseOpenVpnConfig(stripped)
                return Pair(listOf(node), emptyList())
            } catch (e: Exception) {
                return Pair(emptyList(), listOf("OpenVPN config error: ${e.message}"))
            }
        }

        val lowered = stripped.lowercase()
        if ("[interface]" in lowered && "[peer]" in lowered) {
            try {
                val node = parseWireGuardConfig(stripped)
                return Pair(listOf(node), emptyList())
            } catch (e: Exception) {
                return Pair(emptyList(), listOf("WireGuard config error: ${e.message}"))
            }
        }

        // Check if entire stripped string is a Base64 subscription
        if (isBase64Blob(stripped)) {
            try {
                val decoded = decodeB64(stripped).trim()
                if (decoded.isNotEmpty() && (decoded.contains("://") || decoded.contains("\n"))) {
                    return parseLinksText(decoded)
                }
            } catch (e: Exception) {
                // Ignore base64 decoding failure, fall through to lines
            }
        }

        val lines = splitImportTokens(stripped)
        if (lines.size > MAX_IMPORT_LINES) {
            return Pair(emptyList(), listOf("Import contains more than $MAX_IMPORT_LINES non-empty lines"))
        }

        val nodes = mutableListOf<ParsedNode>()
        val errors = mutableListOf<String>()

        for ((idx, line) in lines.withIndex()) {
            try {
                var currentLine = line
                if (HappCrypt.isHappCryptLink(currentLine)) {
                    currentLine = HappCrypt.decryptHappLink(currentLine).trim()
                }
                val node = parseSingle(currentLine)
                applyHappServerMetadata(node, currentLine)
                nodes.add(node)
            } catch (e: Exception) {
                // A single entry can itself be base64 (per-line encoded subscriptions).
                val nested = if (isBase64Blob(line)) {
                    runCatching { parseLinksText(decodeB64(line)) }.getOrNull()
                } else null
                if (nested != null && nested.first.isNotEmpty()) {
                    nodes.addAll(nested.first)
                } else {
                    errors.add("Line ${idx + 1}: ${e.message}")
                }
            }
        }

        if (nodes.isEmpty()) {
            // Last resort: the whole payload may be base64 that failed the strict
            // blob checks above (stray characters, percent-encoded padding).
            val candidate = runCatching {
                decodeB64(URLDecoder.decode(stripped, "UTF-8"))
            }.getOrNull() ?: runCatching { decodeB64(stripped) }.getOrNull()
            if (!candidate.isNullOrBlank() && candidate.trim() != stripped) {
                val salvaged = runCatching { parseLinksText(candidate) }.getOrNull()
                if (salvaged != null && salvaged.first.isNotEmpty()) return salvaged
            }
        }

        return Pair(nodes, errors)
    }

    /**
     * Splits a payload into candidate entries: newline separated, but also comma,
     * whitespace and glued links such as "vless://a...vless://b...".
     */
    private fun splitImportTokens(text: String): List<String> {
        val result = mutableListOf<String>()
        for (rawLine in text.lines()) {
            var line = rawLine.trim().trim('\uFEFF').removeSurrounding("\"").trim()
            if (line.startsWith("- ")) line = line.removePrefix("- ").trim()
            if (line.isEmpty() || line.startsWith("//") || line.startsWith("#") || line.startsWith(";")) continue
            val starts = SCHEME_SPLIT_REGEX.findAll(line).map { it.range.first }.toList()
            if (starts.size > 1) {
                for ((i, start) in starts.withIndex()) {
                    val end = if (i + 1 < starts.size) starts[i + 1] else line.length
                    line.substring(start, end).trim().trimEnd(',', ';', '|')
                        .takeIf { it.isNotEmpty() }?.let { result += it }
                }
            } else {
                result += line.trimEnd(',', ';', '|')
            }
        }
        return result
    }

    fun parseSingle(raw: String): ParsedNode {
        val text = raw.trim()
        if (text.isEmpty()) {
            throw LinkParseError("Empty input")
        }

        if (HappCrypt.isHappCryptLink(text)) {
            val decrypted = HappCrypt.decryptHappLink(text).trim()
            return parseSingle(decrypted)
        }

        if (text.startsWith("{")) {
            return parseJsonOutbound(text)
        }

        val scheme = try {
            val idx = text.indexOf("://")
            if (idx != -1) text.substring(0, idx).lowercase() else ""
        } catch (e: Exception) {
            ""
        }

        return when (scheme) {
            "vless" -> parseVless(text)
            "vmess" -> parseVmess(text)
            "trojan" -> parseTrojan(text)
            "ss" -> parseShadowsocks(text)
            "socks", "socks5" -> parseSocks(text)
            "http", "https" -> parseHttp(text)
            "wireguard", "wg", "awg", "warp" -> parseWireGuardLink(text)
            "hysteria", "hy" -> parseHysteria1(text)
            "hysteria2", "hy2" -> parseHysteria2(text)
            "tuic" -> parseTuic(text)
            "naive", "naive+https", "naive+quic", "quic" -> parseNaiveLink(text, scheme)
            "mieru", "mierus" -> parseMieru(text)
            "masque" -> parseMasque(text)
            else -> throw LinkParseError("Unsupported scheme: ${if (scheme.isEmpty()) "unknown" else scheme}")
        }
    }

    private fun safeCreateUri(link: String): URI {
        val raw = link.trim()
        val hashIdx = raw.indexOf('#')
        val linkWithoutFragment = if (hashIdx != -1) raw.substring(0, hashIdx) else raw
        return try {
            URI(linkWithoutFragment)
        } catch (e: Exception) {
            val safe = linkWithoutFragment.replace(" ", "%20")
            URI(safe)
        }
    }

    private fun extractFragment(link: String): String? {
        val idx = link.indexOf('#')
        return if (idx != -1) unquote(link.substring(idx + 1).trim()) else null
    }

    private fun isBase64Blob(text: String): Boolean {
        val trimmed = text.trim()
        if (trimmed.startsWith("vless://") || trimmed.startsWith("vmess://") ||
            trimmed.startsWith("trojan://") || trimmed.startsWith("ss://") ||
            trimmed.startsWith("hy2://") || trimmed.startsWith("hysteria2://") ||
            trimmed.startsWith("tuic://") || trimmed.startsWith("happ://") ||
            trimmed.startsWith("{") || trimmed.startsWith("[") ||
            trimmed.lowercase().contains("[interface]")
        ) {
            return false
        }
        val clean = trimmed.filterNot { it.isWhitespace() }
        if (clean.length < 16) return false
        if (!clean.all { it.isLetterOrDigit() || it == '+' || it == '/' || it == '-' || it == '_' || it == '=' }) return false
        val decoded = try { decodeB64(clean).trim() } catch (e: Exception) { "" }
        return decoded.isNotEmpty() && (
            decoded.contains("://") || decoded.contains("\n") ||
            decoded.lowercase().contains("proxies:") || decoded.lowercase().contains("[interface]") ||
            decoded.startsWith("{") || decoded.startsWith("[")
        )
    }

    private fun decodeB64(data: String): String {
        var clean = data.trim().replace("-", "+").replace("_", "/")
        clean = clean.filterNot { it.isWhitespace() }
        val padLen = (4 - (clean.length % 4)) % 4
        clean += "=".repeat(padLen)
        val bytes = try {
            Base64.getDecoder().decode(clean)
        } catch (e: Exception) {
            Base64.getUrlDecoder().decode(clean)
        }
        return String(bytes, Charsets.UTF_8)
    }

    private fun unquote(str: String): String {
        return try {
            URLDecoder.decode(str, "UTF-8")
        } catch (e: Exception) {
            str
        }
    }

    private fun parseQueryParams(queryString: String?): Map<String, String> {
        if (queryString.isNullOrEmpty()) return emptyMap()
        val params = mutableMapOf<String, String>()
        val pairs = queryString.split("&")
        for (pair in pairs) {
            val idx = pair.indexOf("=")
            if (idx != -1) {
                val key = unquote(pair.substring(0, idx)).lowercase()
                val value = unquote(pair.substring(idx + 1))
                params[key] = value
            } else if (pair.isNotEmpty()) {
                params[unquote(pair).lowercase()] = ""
            }
        }
        return params
    }

    private fun cleanName(name: String?, fallback: String): String {
        if (name.isNullOrBlank()) return fallback
        val valUnquoted = unquote(name).split("?")[0].trim()
        return valUnquoted.ifEmpty { fallback }
    }

    private fun parseVless(link: String): ParsedNode {
        val uri = safeCreateUri(link)
        val fragment = extractFragment(link)
        val userId = uri.userInfo?.let { unquote(it) } ?: throw LinkParseError("Invalid VLESS link: missing UUID")
        val server = uri.host ?: throw LinkParseError("Invalid VLESS link: missing host")
        val port = if (uri.port > 0) uri.port else 443

        val params = parseQueryParams(uri.rawQuery)
        val flow = params["flow"] ?: ""
        val encryption = params["encryption"] ?: "none"

        val user = mutableMapOf<String, Any?>("id" to userId, "encryption" to encryption)
        if (flow.isNotEmpty()) user["flow"] = flow

        val streamSettings = buildStreamSettings(params, defaultNetwork = "tcp", defaultSecurity = params["security"] ?: "none")
        val outbound = mutableMapOf<String, Any?>(
            "protocol" to "vless",
            "settings" to mapOf(
                "vnext" to listOf(
                    mapOf(
                        "address" to server,
                        "port" to port,
                        "users" to listOf(user)
                    )
                )
            ),
            "streamSettings" to streamSettings
        )

        if ((streamSettings["network"] as? String)?.lowercase() == "xhttp") {
            outbound["mux"] = mapOf("enabled" to false, "concurrency" to -1)
        }

        val name = cleanName(fragment, "vless-$server:$port")
        return ParsedNode(name = name, scheme = "vless", server = server, port = port, link = link, outbound = outbound)
    }

    private fun parseVmess(link: String): ParsedNode {
        val body = link.substringAfter("vmess://").trim()
        if (body.isEmpty()) throw LinkParseError("Empty VMess payload")

        val payloadJson = try {
            decodeB64(body)
        } catch (e: Exception) {
            throw LinkParseError("Invalid VMess base64 payload", e)
        }

        val json = try {
            JSONObject(payloadJson)
        } catch (e: Exception) {
            throw LinkParseError("Invalid VMess JSON payload", e)
        }

        val server = json.optString("add").ifEmpty { json.optString("server") }
        val port = json.optInt("port", 443)
        val id = json.optString("id")
        val aid = json.optInt("aid", 0)
        val scy = json.optString("scy", "auto").ifEmpty { "auto" }

        if (server.isEmpty() || id.isEmpty()) {
            throw LinkParseError("Invalid VMess JSON: missing add or id")
        }

        val params = mutableMapOf<String, String>()
        params["type"] = json.optString("net", "tcp")
        params["security"] = json.optString("tls", "none")
        params["host"] = json.optString("host")
        params["path"] = json.optString("path")
        params["sni"] = json.optString("sni")
        params["alpn"] = json.optString("alpn")
        params["fp"] = json.optString("fp")

        val user = mapOf("id" to id, "alterId" to aid, "security" to scy)
        val streamSettings = buildStreamSettings(params, defaultNetwork = "tcp", defaultSecurity = params["security"] ?: "none")

        val outbound = mapOf<String, Any?>(
            "protocol" to "vmess",
            "settings" to mapOf(
                "vnext" to listOf(
                    mapOf(
                        "address" to server,
                        "port" to port,
                        "users" to listOf(user)
                    )
                )
            ),
            "streamSettings" to streamSettings
        )

        val name = json.optString("ps").ifEmpty { "vmess-$server:$port" }
        return ParsedNode(name = name, scheme = "vmess", server = server, port = port, link = link, outbound = outbound)
    }

    private fun parseTrojan(link: String): ParsedNode {
        val uri = safeCreateUri(link)
        val fragment = extractFragment(link)
        val password = uri.userInfo?.let { unquote(it) } ?: throw LinkParseError("Invalid Trojan link: missing password")
        val server = uri.host ?: throw LinkParseError("Invalid Trojan link: missing host")
        val port = if (uri.port > 0) uri.port else 443

        val params = parseQueryParams(uri.rawQuery)
        val streamSettings = buildStreamSettings(params, defaultNetwork = "tcp", defaultSecurity = params["security"] ?: "tls")

        val outbound = mapOf<String, Any?>(
            "protocol" to "trojan",
            "settings" to mapOf(
                "servers" to listOf(
                    mapOf(
                        "address" to server,
                        "port" to port,
                        "password" to password
                    )
                )
            ),
            "streamSettings" to streamSettings
        )

        val name = cleanName(fragment, "trojan-$server:$port")
        return ParsedNode(name = name, scheme = "trojan", server = server, port = port, link = link, outbound = outbound)
    }

    private fun parseShadowsocks(link: String): ParsedNode {
        val body = link.substringAfter("ss://").trim()
        val fragment = extractFragment(link)
        val mainPart = body.substringBefore("#")

        var userInfo = ""
        var server = ""
        var port = 8388
        var method = ""
        var password = ""

        if (mainPart.contains("@")) {
            val userAndHost = mainPart.substringBefore("?")
            val userStr = userAndHost.substringBeforeLast("@")
            val hostStr = userAndHost.substringAfterLast("@")

            server = hostStr.substringBefore(":")
            port = hostStr.substringAfter(":", "8388").toIntOrNull() ?: 8388

            if (userStr.contains(":")) {
                method = unquote(userStr.substringBefore(":"))
                password = unquote(userStr.substringAfter(":"))
            } else {
                val decodedUser = try { decodeB64(userStr) } catch (e: Exception) { userStr }
                if (decodedUser.contains(":")) {
                    method = decodedUser.substringBefore(":")
                    password = decodedUser.substringAfter(":")
                } else {
                    password = decodedUser
                }
            }
        } else {
            val decoded = try { decodeB64(mainPart.substringBefore("?")) } catch (e: Exception) { "" }
            if (decoded.contains("@")) {
                val userStr = decoded.substringBeforeLast("@")
                val hostStr = decoded.substringAfterLast("@")
                server = hostStr.substringBefore(":")
                port = hostStr.substringAfter(":", "8388").toIntOrNull() ?: 8388
                method = decoded.substringBefore(":")
                password = userStr.substringAfter(":")
            } else {
                throw LinkParseError("Invalid Shadowsocks link format")
            }
        }

        if (method.isEmpty()) method = "aes-256-gcm"

        val outbound = mapOf<String, Any?>(
            "protocol" to "shadowsocks",
            "settings" to mapOf(
                "servers" to listOf(
                    mapOf(
                        "address" to server,
                        "port" to port,
                        "method" to method,
                        "password" to password
                    )
                )
            )
        )

        val name = cleanName(fragment, "ss-$server:$port")
        return ParsedNode(name = name, scheme = "ss", server = server, port = port, link = link, outbound = outbound)
    }

    private fun parseHysteria1(link: String): ParsedNode {
        // Hysteria v1 is a distinct sing-box outbound type with auth_str,
        // up/down speeds and a plain-string obfs (parity with desktop Lumen).
        val uri = safeCreateUri(link)
        val fragment = extractFragment(link)
        val params = parseQueryParams(uri.rawQuery)
        val server = uri.host ?: params["server"] ?: params["address"] ?: params["host"]
            ?: throw LinkParseError("Invalid Hysteria link: missing host")
        val port = if (uri.port > 0) uri.port else params["port"]?.toIntOrNull() ?: 443
        val auth = uri.userInfo?.let { unquote(it) }?.takeIf { it.isNotEmpty() }
            ?: params["auth"] ?: params["auth_str"] ?: params["authstr"] ?: params["password"] ?: ""
        if (auth.isEmpty()) throw LinkParseError("hysteria link must contain server and auth/password")

        val singbox = mutableMapOf<String, Any?>(
            "type" to "hysteria",
            "tag" to "proxy",
            "server" to server,
            "server_port" to port,
            "auth_str" to auth
        )
        params["protocol"]?.takeIf { it.isNotEmpty() }?.let { singbox["protocol"] = it }
        (params["upmbps"] ?: params["up_mbps"] ?: params["up"])?.toIntOrNull()?.let { singbox["up_mbps"] = it }
        (params["downmbps"] ?: params["down_mbps"] ?: params["down"])?.toIntOrNull()?.let { singbox["down_mbps"] = it }

        val tls = mutableMapOf<String, Any?>("enabled" to true)
        tls["server_name"] = params["sni"] ?: params["peer"] ?: params["server_name"] ?: params["servername"] ?: server
        (params["insecure"] ?: params["allowinsecure"])?.let { if (toBool(it)) tls["insecure"] = true }
        params["alpn"]?.takeIf { it.isNotEmpty() }?.let {
            tls["alpn"] = it.split(",").map { s -> s.trim() }.filter { s -> s.isNotEmpty() }
        }
        singbox["tls"] = tls

        val obfsType = params["obfs"] ?: params["obfs_type"] ?: params["obfstype"] ?: ""
        val obfsPassword = params["obfs-password"] ?: params["obfs_password"] ?: params["obfspassword"]
            ?: params["obfs-pass"] ?: params["obfspass"] ?: ""
        if ((obfsType.isNotEmpty() || obfsPassword.isNotEmpty()) && obfsType != "none") {
            // Hysteria v1 expects the obfuscation password as a plain string.
            singbox["obfs"] = obfsPassword.ifEmpty { obfsType }
        }

        val outbound = mapOf<String, Any?>(
            "protocol" to "hysteria",
            "singbox" to singbox
        )

        val name = cleanName(fragment, "hysteria-$server:$port")
        return ParsedNode(name = name, scheme = "hysteria", server = server, port = port, link = link, outbound = outbound)
    }

    private fun parseHysteria2(link: String): ParsedNode {
        val uri = safeCreateUri(link)
        val fragment = extractFragment(link)
        val auth = uri.userInfo?.let { unquote(it) } ?: ""
        val server = uri.host ?: throw LinkParseError("Invalid Hysteria2 link: missing host")
        val port = if (uri.port > 0) uri.port else 443
        val params = parseQueryParams(uri.rawQuery)

        val sni = params["sni"] ?: params["peer"] ?: server
        val insecure = params["insecure"] == "1" || params["insecure"] == "true"
        val obfs = params["obfs"] ?: ""
        val obfsPassword = params["obfs-password"] ?: ""

        val singbox = mutableMapOf<String, Any?>(
            "type" to "hysteria2",
            "server" to server,
            "server_port" to port,
            "password" to auth,
            "tls" to mapOf(
                "enabled" to true,
                "server_name" to sni,
                "insecure" to insecure
            )
        )

        if (obfs.isNotEmpty()) {
            singbox["obfs"] = mapOf(
                "type" to obfs,
                "password" to obfsPassword
            )
        }

        val outbound = mapOf<String, Any?>(
            "protocol" to "hysteria2",
            "singbox" to singbox
        )

        val name = cleanName(fragment, "hy2-$server:$port")
        return ParsedNode(name = name, scheme = "hysteria2", server = server, port = port, link = link, outbound = outbound)
    }

    private fun parseTuic(link: String): ParsedNode {
        val uri = safeCreateUri(link)
        val fragment = extractFragment(link)
        val userInfo = uri.userInfo?.let { unquote(it) } ?: ""
        val uuid = userInfo.substringBefore(":")
        val password = userInfo.substringAfter(":", "")

        val server = uri.host ?: throw LinkParseError("Invalid TUIC link: missing host")
        val port = if (uri.port > 0) uri.port else 8443
        val params = parseQueryParams(uri.rawQuery)

        val sni = params["sni"] ?: server
        val congestionControl = params["congestion_control"] ?: "bbr"
        val udpRelayMode = params["udp_relay_mode"] ?: "native"
        val alpnStr = params["alpn"] ?: "h3"
        val alpn = alpnStr.split(",").map { it.trim() }.filter { it.isNotEmpty() }

        val singbox = mapOf<String, Any?>(
            "type" to "tuic",
            "server" to server,
            "server_port" to port,
            "uuid" to uuid,
            "password" to password,
            "congestion_control" to congestionControl,
            "udp_relay_mode" to udpRelayMode,
            "tls" to mapOf(
                "enabled" to true,
                "server_name" to sni,
                "alpn" to alpn
            )
        )

        val outbound = mapOf<String, Any?>(
            "protocol" to "tuic",
            "singbox" to singbox
        )

        val name = cleanName(fragment, "tuic-$server:$port")
        return ParsedNode(name = name, scheme = "tuic", server = server, port = port, link = link, outbound = outbound)
    }

    private fun toBool(value: Any?): Boolean = when (value) {
        is Boolean -> value
        is Number -> value.toInt() != 0
        is String -> value.trim().lowercase() in setOf("1", "true", "yes", "on")
        else -> false
    }

    private fun clashStringList(value: Any?): List<String> = when (value) {
        is List<*> -> value.mapNotNull { it?.toString()?.trim() }.filter { it.isNotEmpty() }
        null -> emptyList()
        else -> value.toString().split(",").map { it.trim() }.filter { it.isNotEmpty() }
    }

    private fun parseNaiveLink(link: String, sourceScheme: String = ""): ParsedNode {
        // NaiveProxy URI (parity with desktop Lumen): the explicit naive*
        // schemes are Lumen aliases; https/quic arrive from NaiveProxy JSON
        // configs via sourceScheme so plain https:// links stay HTTP proxies.
        val uri = safeCreateUri(link)
        val fragment = extractFragment(link)
        val scheme = (sourceScheme.ifEmpty { uri.scheme ?: "" }).trim().lowercase()
        if (scheme !in setOf("naive", "naive+https", "naive+quic", "https", "quic")) {
            throw LinkParseError("unsupported NaiveProxy transport: ${scheme.ifEmpty { "unknown" }}")
        }
        val server = uri.host ?: throw LinkParseError("NaiveProxy URI must contain server and port")
        val port = if (uri.port > 0) uri.port else 443
        val params = parseQueryParams(uri.rawQuery)
        val userInfo = uri.userInfo?.let { unquote(it) } ?: ""
        val username = userInfo.substringBefore(":", userInfo)
        val password = if (userInfo.contains(":")) userInfo.substringAfter(":") else ""
        val serverName = params["sni"] ?: params["server_name"] ?: params["servername"] ?: server
        val isQuic = scheme in setOf("quic", "naive+quic") || toBool(params["quic"])

        val singbox = mutableMapOf<String, Any?>(
            "type" to "naive",
            "tag" to "proxy",
            "server" to server,
            "server_port" to port,
            "username" to username,
            "password" to password,
            "quic" to isQuic,
            "tls" to mapOf("enabled" to true, "server_name" to serverName)
        )
        (params["insecure_concurrency"] ?: params["insecure-concurrency"])?.toIntOrNull()?.let {
            singbox["insecure_concurrency"] = it
        }
        (params["quic_congestion_control"] ?: params["quic-congestion-control"])?.takeIf { it.isNotEmpty() }?.let {
            singbox["quic_congestion_control"] = it
        }
        (params["udp_over_tcp"] ?: params["udp-over-tcp"])?.takeIf { it.isNotEmpty() }?.let { raw ->
            val trimmedRaw = raw.trim()
            if (trimmedRaw.startsWith("{")) {
                try {
                    singbox["udp_over_tcp"] = jsonToMap(JSONObject(trimmedRaw))
                } catch (e: Exception) {
                    singbox["udp_over_tcp"] = toBool(trimmedRaw)
                }
            } else {
                singbox["udp_over_tcp"] = toBool(trimmedRaw)
            }
        }
        (params["extra_headers"] ?: params["extra-headers"])?.takeIf { it.isNotEmpty() }?.let { raw ->
            val headers = try {
                JSONObject(raw.trim())
            } catch (e: Exception) {
                throw LinkParseError("NaiveProxy extra_headers must be a JSON object")
            }
            singbox["extra_headers"] = jsonToMap(headers)
        }

        val outbound = mapOf<String, Any?>("protocol" to "naive", "singbox" to singbox)
        val name = cleanName(fragment, "naive-$server:$port")
        return ParsedNode(name = name, scheme = "naive", server = server, port = port, link = link, outbound = outbound)
    }

    private fun parseMieru(link: String): ParsedNode {
        val uri = safeCreateUri(link)
        val fragment = extractFragment(link)
        val params = parseQueryParams(uri.rawQuery)
        val server = uri.host ?: params["server"] ?: params["address"] ?: params["host"] ?: ""
        val port = if (uri.port > 0) uri.port else (params["port"] ?: params["server_port"])?.toIntOrNull() ?: 0
        val userInfo = uri.userInfo?.let { unquote(it) } ?: ""
        val username = userInfo.substringBefore(":", userInfo).ifEmpty { params["username"] ?: params["user"] ?: "" }
        val password = (if (userInfo.contains(":")) userInfo.substringAfter(":") else "")
            .ifEmpty { params["password"] ?: params["pass"] ?: "" }
        if (server.isEmpty() || username.isEmpty() || password.isEmpty()) {
            throw LinkParseError("mieru link must contain server, username and password")
        }

        val singbox = mutableMapOf<String, Any?>(
            "type" to "mieru",
            "tag" to "proxy",
            "server" to server,
            "transport" to ((params["transport"]?.takeIf { it.isNotEmpty() } ?: "TCP").uppercase()),
            "username" to username,
            "password" to password
        )
        if (port > 0) singbox["server_port"] = port
        (params["server_ports"] ?: params["ports"])?.takeIf { it.isNotEmpty() }?.let {
            singbox["server_ports"] = it.split(",").map { p -> p.trim() }.filter { p -> p.isNotEmpty() }
        }
        (params["multiplexing"] ?: params["mux"])?.takeIf { it.isNotEmpty() }?.let { singbox["multiplexing"] = it }
        (params["traffic_pattern"] ?: params["trafficpattern"])?.takeIf { it.isNotEmpty() }?.let {
            singbox["traffic_pattern"] = it
        }

        val outbound = mapOf<String, Any?>("protocol" to "mieru", "singbox" to singbox)
        val name = cleanName(fragment, "mieru-$server:${if (port > 0) port.toString() else "range"}")
        return ParsedNode(name = name, scheme = "mieru", server = server, port = port, link = link, outbound = outbound)
    }

    private fun parseMasque(link: String): ParsedNode {
        // masque://<auth_token>@<profile_id>?... — parsed manually because the
        // auth token may contain base64 characters java.net.URI rejects.
        val trimmed = link.trim()
        val schemeSep = trimmed.indexOf("://")
        if (schemeSep <= 0) throw LinkParseError("Invalid MASQUE URL")
        val fragmentIdx = trimmed.indexOf('#')
        val withoutFragment = if (fragmentIdx != -1) trimmed.substring(0, fragmentIdx) else trimmed
        val afterScheme = withoutFragment.substring(schemeSep + 3)
        val queryIdx = afterScheme.indexOf('?')
        val authority = (if (queryIdx != -1) afterScheme.substring(0, queryIdx) else afterScheme).substringBefore('/')
        val rawQuery = if (queryIdx != -1) afterScheme.substring(queryIdx + 1) else null
        val params = parseQueryParams(rawQuery)

        val atIdx = authority.lastIndexOf('@')
        val authToken = (if (atIdx > 0) percentDecodeKeepPlus(authority.substring(0, atIdx)) else "")
            .ifEmpty { params["auth_token"] ?: params["token"] ?: "" }
        val profileId = unquote((if (atIdx >= 0) authority.substring(atIdx + 1) else authority).trim())
            .ifEmpty { params["id"] ?: params["profile_id"] ?: "" }

        val profile = mutableMapOf<String, Any?>("detour" to "direct")
        if (profileId.isNotEmpty()) profile["id"] = profileId
        if (authToken.isNotEmpty()) profile["auth_token"] = authToken

        val singbox = mutableMapOf<String, Any?>(
            "type" to "masque",
            "tag" to "proxy",
            "system" to toBool(params["system"]),
            "name" to (params["name"]?.takeIf { it.isNotEmpty() } ?: "masque0"),
            "use_http2" to (toBool(params["use_http2"]) || toBool(params["http2"])),
            "use_ipv6" to (toBool(params["use_ipv6"]) || toBool(params["ipv6"])),
            "profile" to profile,
            "udp_timeout" to ((params["udp_timeout"] ?: params["udptimeout"])?.takeIf { it.isNotEmpty() } ?: "5m0s"),
            "udp_keepalive_period" to ((params["udp_keepalive_period"] ?: params["udpkeepaliveperiod"])?.takeIf { it.isNotEmpty() } ?: "30s"),
            "reconnect_delay" to ((params["reconnect_delay"] ?: params["reconnectdelay"])?.takeIf { it.isNotEmpty() } ?: "5s"),
            "congestion_controller" to ((params["congestion_controller"] ?: params["congestioncontroller"])?.takeIf { it.isNotEmpty() } ?: "bbr")
        )
        (params["allowed_ips"] ?: params["allowedips"])?.takeIf { it.isNotEmpty() }?.let {
            singbox["allowed_ips"] = it.split(",").map { s -> s.trim() }.filter { s -> s.isNotEmpty() }
        }
        val serverName = params["sni"] ?: params["server_name"] ?: params["servername"] ?: ""
        val insecure = params["insecure"] ?: params["allowinsecure"] ?: ""
        if (serverName.isNotEmpty() || insecure.isNotEmpty()) {
            val tls = mutableMapOf<String, Any?>()
            if (serverName.isNotEmpty()) tls["server_name"] = serverName
            if (insecure.isNotEmpty()) tls["insecure"] = toBool(insecure)
            singbox["tls"] = tls
        }

        val outbound = mapOf<String, Any?>("protocol" to "masque", "singbox" to singbox)
        val fragment = if (fragmentIdx != -1) percentDecodeKeepPlus(trimmed.substring(fragmentIdx + 1)) else null
        val name = cleanName(fragment, "MASQUE")
        return ParsedNode(name = name, scheme = "masque", server = profileId, port = 0, link = link, outbound = outbound)
    }

    private fun parseSocks(link: String): ParsedNode {
        val uri = try { URI(link) } catch (e: Exception) { throw LinkParseError("Invalid SOCKS URL: ${e.message}") }
        val server = uri.host ?: throw LinkParseError("Invalid SOCKS link: missing host")
        val port = if (uri.port > 0) uri.port else 1080
        val userInfo = uri.userInfo?.let { unquote(it) } ?: ""

        val serverMap = mutableMapOf<String, Any?>("address" to server, "port" to port)
        if (userInfo.contains(":")) {
            val user = mutableMapOf<String, Any?>(
                "user" to userInfo.substringBefore(":"),
                "pass" to userInfo.substringAfter(":")
            )
            serverMap["users"] = listOf(user)
        }

        val outbound = mapOf<String, Any?>(
            "protocol" to "socks",
            "settings" to mapOf("servers" to listOf(serverMap))
        )
        val name = cleanName(uri.rawFragment, "socks-$server:$port")
        return ParsedNode(name = name, scheme = "socks", server = server, port = port, link = link, outbound = outbound)
    }

    private fun parseHttp(link: String): ParsedNode {
        val uri = try { URI(link) } catch (e: Exception) { throw LinkParseError("Invalid HTTP URL: ${e.message}") }
        val server = uri.host ?: throw LinkParseError("Invalid HTTP link: missing host")
        val port = if (uri.port > 0) uri.port else 8080
        val userInfo = uri.userInfo?.let { unquote(it) } ?: ""

        val serverMap = mutableMapOf<String, Any?>("address" to server, "port" to port)
        if (userInfo.contains(":")) {
            val user = mutableMapOf<String, Any?>(
                "user" to userInfo.substringBefore(":"),
                "pass" to userInfo.substringAfter(":")
            )
            serverMap["users"] = listOf(user)
        }

        val outbound = mapOf<String, Any?>(
            "protocol" to "http",
            "settings" to mapOf("servers" to listOf(serverMap))
        )
        val name = cleanName(uri.rawFragment, "http-$server:$port")
        return ParsedNode(name = name, scheme = "http", server = server, port = port, link = link, outbound = outbound)
    }

    // Percent-decodes a string WITHOUT treating '+' as a space (base64 keys contain '+').
    private fun percentDecodeKeepPlus(str: String): String {
        if (!str.contains('%')) return str
        return try {
            URLDecoder.decode(str.replace("+", "%2B"), "UTF-8")
        } catch (e: Exception) {
            str
        }
    }

    private fun parseWireGuardLink(link: String): ParsedNode {
        // Parsed manually: WireGuard/AWG private keys are base64 and may contain
        // '/', '+' and '=' which java.net.URI rejects or corrupts in the userinfo
        // part (this caused "missing private key" for some AWG servers).
        val trimmed = link.trim()
        val schemeSep = trimmed.indexOf("://")
        if (schemeSep <= 0) throw LinkParseError("Invalid WireGuard URL")
        val fragmentIdx = trimmed.indexOf('#')
        val withoutFragment = if (fragmentIdx != -1) trimmed.substring(0, fragmentIdx) else trimmed
        val afterScheme = withoutFragment.substring(schemeSep + 3)
        val queryIdx = afterScheme.indexOf('?')
        val authority = (if (queryIdx != -1) afterScheme.substring(0, queryIdx) else afterScheme).substringBefore('/')
        val rawQuery = if (queryIdx != -1) afterScheme.substring(queryIdx + 1) else null

        val params = mutableMapOf<String, String>()
        rawQuery?.split("&")?.forEach { pair ->
            val eq = pair.indexOf('=')
            if (eq > 0) {
                params[percentDecodeKeepPlus(pair.substring(0, eq)).lowercase()] = percentDecodeKeepPlus(pair.substring(eq + 1))
            }
        }

        val atIdx = authority.lastIndexOf('@')
        var privateKey = if (atIdx > 0) percentDecodeKeepPlus(authority.substring(0, atIdx)) else ""
        if (privateKey.isBlank()) {
            privateKey = params["privatekey"] ?: params["private_key"] ?: params["private-key"]
                ?: params["secretkey"] ?: params["secret_key"] ?: params["secret-key"] ?: params["sk"] ?: ""
        }

        val hostPort = if (atIdx >= 0) authority.substring(atIdx + 1) else authority
        val server: String
        var port = 51820
        if (hostPort.startsWith("[")) {
            val close = hostPort.indexOf(']')
            if (close <= 0) throw LinkParseError("Invalid WireGuard link: missing host")
            server = hostPort.substring(1, close)
            val rest = hostPort.substring(close + 1)
            if (rest.startsWith(":")) port = rest.substring(1).toIntOrNull() ?: 51820
        } else {
            val colonIdx = hostPort.lastIndexOf(':')
            if (colonIdx > 0) {
                server = hostPort.substring(0, colonIdx)
                port = hostPort.substring(colonIdx + 1).toIntOrNull() ?: 51820
            } else {
                server = hostPort
            }
        }
        if (server.isBlank()) throw LinkParseError("Invalid WireGuard link: missing host")

        val publicKey = params["publickey"] ?: params["public_key"] ?: params["public-key"] ?: params["pk"] ?: ""
        val addressStr = params["ip"] ?: params["address"] ?: ""
        val address = addressStr.split(",").map { it.trim() }.filter { it.isNotEmpty() }

        val amneziaMap = mutableMapOf<String, Any?>()
        for (junkKey in AMNEZIA_JUNK_KEYS) {
            params[junkKey]?.let { valStr ->
                valStr.toIntOrNull()?.let { amneziaMap[junkKey] = it } ?: run { amneziaMap[junkKey] = valStr }
            }
        }
        val isAwg = link.lowercase().startsWith("awg://") || link.lowercase().startsWith("amneziawg://") || amneziaMap.isNotEmpty()

        // Optional peer/interface parameters: dropping them silently broke
        // split-tunnel AllowedIPs, PSK peers and Warp-like reserved bytes.
        val allowedIps = (params["allowedips"] ?: params["allowed_ips"] ?: params["allowed-ips"] ?: "")
            .split(",").map { it.trim() }.filter { it.isNotEmpty() }
            .ifEmpty { listOf("0.0.0.0/0", "::/0") }
        val preSharedKey = (params["presharedkey"] ?: params["pre_shared_key"] ?: params["pre-shared-key"] ?: params["psk"] ?: "").trim()
        val keepalive = (params["persistentkeepalive"] ?: params["persistent_keepalive"] ?: params["keepalive"])?.trim()?.toIntOrNull()
        val reservedParam = (params["reserved"] ?: "").trim()
        val mtuParam = params["mtu"]?.trim()?.toIntOrNull()
        val dnsServers = (params["dns"] ?: "").split(",").map { it.trim() }.filter { it.isNotEmpty() }

        val peer = mutableMapOf<String, Any?>(
            "public_key" to publicKey,
            "server" to server,
            "server_port" to port,
            "allowed_ips" to allowedIps
        )
        if (preSharedKey.isNotEmpty()) peer["pre_shared_key"] = preSharedKey
        if (keepalive != null && keepalive > 0) peer["persistent_keepalive_interval"] = keepalive
        if (reservedParam.isNotEmpty()) peer["reserved"] = reservedParam

        val singbox = mutableMapOf<String, Any?>(
            "type" to if (isAwg) "awg" else "wireguard",
            "server" to server,
            "server_port" to port,
            "private_key" to privateKey,
            "address" to address,
            "peers" to listOf(peer)
        )
        if (mtuParam != null && mtuParam > 0) singbox["mtu"] = mtuParam
        if (amneziaMap.isNotEmpty()) {
            singbox["amnezia"] = amneziaMap
        }

        val outbound = mutableMapOf<String, Any?>(
            "protocol" to if (isAwg) "awg" else "wireguard",
            "singbox" to singbox
        )
        if (dnsServers.isNotEmpty()) outbound["_dns"] = dnsServers
        val scheme = if (isAwg) "awg" else "wireguard"
        val fragment = if (fragmentIdx != -1) percentDecodeKeepPlus(trimmed.substring(fragmentIdx + 1)) else null
        val name = cleanName(fragment, if (isAwg) "awg-$server:$port" else "wg-$server:$port")
        return ParsedNode(name = name, scheme = scheme, server = server, port = port, link = link, outbound = outbound)
    }

    fun parseWireGuardConfig(text: String): ParsedNode {
        var privateKey = ""
        val addressList = mutableListOf<String>()
        var publicKey = ""
        var endpointHost = ""
        var endpointPort = 51820
        var currentSection = ""
        // [Interface]/[Peer] extras that used to be dropped by this parser.
        var mtu = 0
        val dnsList = mutableListOf<String>()
        var preSharedKey = ""
        var keepalive = 0
        var reserved = ""
        val allowedIpsList = mutableListOf<String>()

        val amneziaMap = mutableMapOf<String, Any?>()

        for (rawLine in text.lines()) {
            val line = rawLine.trim()
            if (line.isEmpty() || line.startsWith("#") || line.startsWith(";")) continue
            if (line.startsWith("[") && line.endsWith("]")) {
                currentSection = line.substring(1, line.length - 1).trim().lowercase()
                continue
            }

            val parts = line.split("=", limit = 2)
            if (parts.size != 2) continue
            val key = parts[0].trim().lowercase()
            val value = parts[1].trim()

            if (currentSection == "interface") {
                when (key) {
                    "privatekey" -> privateKey = value
                    "address" -> addressList.addAll(value.split(",").map { it.trim() }.filter { it.isNotEmpty() })
                    "mtu" -> mtu = value.toIntOrNull() ?: 0
                    "dns" -> dnsList.addAll(value.split(",").map { it.trim() }.filter { it.isNotEmpty() })
                    "jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "i1", "i2", "i3", "i4", "i5", "j1", "j2", "j3", "itime" -> {
                        value.toIntOrNull()?.let { amneziaMap[key] = it } ?: run { amneziaMap[key] = value }
                    }
                    "h1", "h2", "h3", "h4" -> {
                        amneziaMap[key] = value
                    }
                }
            } else if (currentSection == "peer") {
                when (key) {
                    "publickey" -> publicKey = value
                    "presharedkey" -> preSharedKey = value
                    "persistentkeepalive" -> keepalive = value.toIntOrNull() ?: 0
                    "reserved" -> reserved = value
                    "allowedips" -> allowedIpsList.addAll(value.split(",").map { it.trim() }.filter { it.isNotEmpty() })
                    "endpoint" -> {
                        val hostPort = value.split(":")
                        if (hostPort.size >= 2) {
                            endpointHost = hostPort.dropLast(1).joinToString(":")
                            endpointPort = hostPort.last().toIntOrNull() ?: 51820
                        } else {
                            endpointHost = value
                        }
                    }
                }
            }
        }

        if (endpointHost.isEmpty()) {
            throw LinkParseError("WireGuard config missing Endpoint in [Peer]")
        }

        val isAwg = amneziaMap.isNotEmpty()
        val scheme = if (isAwg) "awg" else "wireguard"
        val peer = mutableMapOf<String, Any?>(
            "public_key" to publicKey,
            "server" to endpointHost,
            "server_port" to endpointPort,
            "allowed_ips" to allowedIpsList.ifEmpty { listOf("0.0.0.0/0", "::/0") }
        )
        if (preSharedKey.isNotEmpty()) peer["pre_shared_key"] = preSharedKey
        if (keepalive > 0) peer["persistent_keepalive_interval"] = keepalive
        if (reserved.isNotEmpty()) peer["reserved"] = reserved

        val singbox = mutableMapOf<String, Any?>(
            "type" to scheme,
            "server" to endpointHost,
            "server_port" to endpointPort,
            "private_key" to privateKey,
            "address" to addressList,
            "peers" to listOf(peer)
        )
        if (mtu > 0) singbox["mtu"] = mtu

        if (amneziaMap.isNotEmpty()) {
            singbox["amnezia"] = amneziaMap
        }

        val outbound = mutableMapOf<String, Any?>(
            "protocol" to scheme,
            "singbox" to singbox
        )
        if (dnsList.isNotEmpty()) outbound["_dns"] = dnsList

        return ParsedNode(name = if (isAwg) "AmneziaWG-$endpointHost" else "WireGuard-$endpointHost", scheme = scheme, server = endpointHost, port = endpointPort, link = text, outbound = outbound)
    }

    private val OPENVPN_INLINE_BLOCK_REGEX = Regex(
        "(?ims)^[ \\t]*<(ca|cert|key|tls-auth|tls-crypt|tls-crypt-v2|auth-user-pass)>[ \\t]*\\r?\\n(.*?)^[ \\t]*</\\1>[ \\t]*$"
    )
    private val OPENVPN_SUPPORTED_CIPHERS = setOf(
        "AES-128-GCM", "AES-192-GCM", "AES-256-GCM",
        "AES-128-CBC", "AES-192-CBC", "AES-256-CBC", "CHACHA20-POLY1305"
    )
    private val OPENVPN_UNSAFE_DIRECTIVES = setOf(
        "askpass", "http-proxy", "http-proxy-user-pass", "pkcs12", "secret", "socks-proxy", "socks-proxy-retry"
    )

    // Full structured port of the desktop openvpn_import.py: the extended
    // sing-box core rejects the old {"type":"openvpn","config_str":...} shape,
    // so the profile must be decomposed into native outbound fields.
    fun parseOpenVpnConfig(text: String): ParsedNode {
        if (!looksLikeOpenVpnConfig(text)) {
            throw LinkParseError("not an OpenVPN client profile")
        }

        val inline = mutableMapOf<String, String>()
        val directivesText = OPENVPN_INLINE_BLOCK_REGEX.replace(text) { match ->
            val tag = match.groupValues[1].lowercase()
            val body = match.groupValues[2].trim('\r', '\n')
            inline[tag] = if (body.isEmpty()) "" else body + "\n"
            ""
        }

        val byKey = mutableMapOf<String, MutableList<List<String>>>()
        for (rawLine in directivesText.lines()) {
            val strippedLine = rawLine.trim()
            if (strippedLine.isEmpty() || strippedLine.startsWith("#") || strippedLine.startsWith(";")) continue
            val tokens = tokenizeOpenVpnLine(strippedLine)
            if (tokens.isEmpty()) continue
            val key = tokens[0].trimStart('-').trim().lowercase()
            if (key.isEmpty()) continue
            byKey.getOrPut(key) { mutableListOf() }.add(tokens.drop(1))
        }

        for (key in OPENVPN_UNSAFE_DIRECTIVES) {
            if (key in byKey) throw LinkParseError("OpenVPN directive `$key` is not supported by sing-box extended")
        }
        val dev = openVpnLastArg(byKey, "dev").lowercase()
        if (dev.startsWith("tap")) throw LinkParseError("OpenVPN TAP profiles are not supported; a TUN profile is required")
        for (key in listOf("compress", "comp-lzo")) {
            if (key !in byKey) continue
            val value = openVpnLastArg(byKey, key).lowercase()
            if (value.isNotEmpty() && value !in setOf("no", "disable", "stub", "stub-v2")) {
                throw LinkParseError("OpenVPN compression `$value` is not supported")
            }
        }

        val globalProto = normalizeOpenVpnProto(openVpnLastArg(byKey, "proto").ifEmpty { "udp" })
        val servers = mutableListOf<Map<String, Any?>>()
        val remoteProtos = mutableSetOf<String>()
        for (values in byKey["remote"] ?: emptyList()) {
            if (values.isEmpty()) continue
            val server = values[0].trim()
            if (server.isEmpty()) continue
            val portText = values.getOrNull(1)?.trim() ?: "1194"
            val port = portText.toIntOrNull()?.takeIf { it in 1..65535 }
                ?: throw LinkParseError("invalid OpenVPN remote port `$portText`")
            val remoteProto = values.getOrNull(2)?.let { normalizeOpenVpnProto(it) } ?: globalProto
            remoteProtos.add(remoteProto)
            servers.add(mapOf("server" to server, "server_port" to port))
        }
        if (servers.isEmpty()) throw LinkParseError("OpenVPN profile does not contain a usable `remote` server")
        if (remoteProtos.size > 1) {
            throw LinkParseError("OpenVPN profile mixes TCP and UDP remotes, which this core cannot represent")
        }
        val proto = remoteProtos.firstOrNull() ?: globalProto

        val native = mutableMapOf<String, Any?>(
            "type" to "openvpn",
            "tag" to "proxy",
            "system" to false,
            "name" to "openvpn0",
            "servers" to servers,
            "proto" to proto
        )

        val cipher = selectOpenVpnCipher(byKey)
        if (cipher.isNotEmpty()) native["cipher"] = cipher
        val auth = openVpnLastArg(byKey, "auth")
        if (auth.isNotEmpty() && auth.lowercase() != "none") native["auth"] = auth.uppercase()

        val credentials = inline["auth-user-pass"] ?: ""
        if (credentials.isEmpty() && openVpnLastValues(byKey, "auth-user-pass").isNotEmpty()) {
            throw LinkParseError("OpenVPN `auth-user-pass` file references are not supported on Android; embed the credentials inline")
        }
        if (credentials.isNotEmpty()) {
            val credLines = credentials.lines().map { it.trim() }.filter { it.isNotEmpty() }
            if (credLines.isNotEmpty()) native["username"] = credLines[0]
            if (credLines.size > 1) native["password"] = credLines[1]
        }

        for ((directive, nativeKey) in listOf("tls-auth" to "tls_auth", "tls-crypt" to "tls_crypt", "tls-crypt-v2" to "tls_crypt")) {
            val content = inline[directive] ?: ""
            val values = openVpnLastValues(byKey, directive)
            if (content.isEmpty() && values.isNotEmpty()) {
                throw LinkParseError("OpenVPN `$directive` file references are not supported on Android; embed the key inline")
            }
            if (content.isNotEmpty()) {
                native[nativeKey] = content
                if (directive == "tls-crypt-v2") native["tls_crypt_v2"] = true
            }
            if (directive == "tls-auth" && values.size > 1) native["key_direction"] = openVpnKeyDirection(values[1])
        }
        val direction = openVpnLastArg(byKey, "key-direction")
        if (direction.isNotEmpty()) native["key_direction"] = openVpnKeyDirection(direction)

        for ((directive, nativeKey) in listOf("connect-retry" to "reconnect_delay", "ping" to "ping_interval", "ping-restart" to "ping_restart")) {
            val value = openVpnLastArg(byKey, directive)
            if (value.isNotEmpty()) native[nativeKey] = openVpnDurationSeconds(value, directive)
        }

        val tls = mutableMapOf<String, Any?>()
        for ((directive, nativeKey) in listOf("ca" to "ca", "cert" to "certificate", "key" to "key")) {
            val content = inline[directive] ?: ""
            val values = openVpnLastValues(byKey, directive)
            if (content.isEmpty() && values.isNotEmpty()) {
                throw LinkParseError("OpenVPN `$directive` file references are not supported on Android; embed the certificate inline")
            }
            if (content.isNotEmpty()) tls[nativeKey] = content
        }
        val tlsCiphers = mutableListOf<String>()
        for (directive in listOf("tls-cipher", "tls-ciphersuites")) {
            val value = openVpnLastArg(byKey, directive)
            if (value.isNotEmpty()) tlsCiphers.addAll(value.split(":").map { it.trim() }.filter { it.isNotEmpty() })
        }
        if (tlsCiphers.isNotEmpty()) tls["cipher_suites"] = tlsCiphers
        val verifyValues = openVpnLastValues(byKey, "verify-x509-name")
        if (verifyValues.isNotEmpty()) {
            tls["verify_x509_name"] = verifyValues[0]
            if (verifyValues.size > 1) tls["verify_x509_name_mode"] = openVpnVerifyNameMode(verifyValues[1])
        }
        if ((tls["ca"] as? String).isNullOrEmpty()) {
            throw LinkParseError("OpenVPN profile does not contain a CA certificate supported by this core")
        }
        native["tls"] = tls

        val dnsServers = mutableListOf<String>()
        for (values in byKey["dhcp-option"] ?: emptyList()) {
            if (values.size >= 2 && values[0].trim().uppercase() in setOf("DNS", "DNS6")) {
                val address = values[1].trim()
                if (address.isNotEmpty() && address !in dnsServers) dnsServers.add(address)
            }
        }

        val outbound = mutableMapOf<String, Any?>(
            "protocol" to "openvpn",
            "singbox" to native
        )
        if (dnsServers.isNotEmpty()) outbound["_dns"] = dnsServers

        val firstServer = servers[0]["server"].toString()
        val firstPort = (servers[0]["server_port"] as Number).toInt()
        return ParsedNode(
            name = "OpenVPN-$firstServer",
            scheme = "openvpn",
            server = firstServer,
            port = firstPort,
            link = text,
            outbound = outbound
        )
    }

    private fun tokenizeOpenVpnLine(line: String): List<String> {
        val tokens = mutableListOf<String>()
        val current = StringBuilder()
        var quote: Char? = null
        var hasToken = false
        var i = 0
        while (i < line.length) {
            val c = line[i]
            when {
                quote != null -> if (c == quote) quote = null else current.append(c)
                c == '"' || c == '\'' -> { quote = c; hasToken = true }
                c.isWhitespace() -> {
                    if (hasToken || current.isNotEmpty()) {
                        tokens.add(current.toString()); current.setLength(0); hasToken = false
                    }
                }
                c == '\\' && i + 1 < line.length -> { current.append(line[i + 1]); i++ }
                else -> current.append(c)
            }
            i++
        }
        if (quote != null) throw LinkParseError("OpenVPN line has an unterminated quote: $line")
        if (hasToken || current.isNotEmpty()) tokens.add(current.toString())
        return tokens
    }

    private fun openVpnLastValues(byKey: Map<String, MutableList<List<String>>>, key: String): List<String> =
        byKey[key]?.lastOrNull() ?: emptyList()

    private fun openVpnLastArg(byKey: Map<String, MutableList<List<String>>>, key: String): String =
        openVpnLastValues(byKey, key).firstOrNull()?.trim() ?: ""

    private fun normalizeOpenVpnProto(value: String): String {
        val proto = value.trim().lowercase().ifEmpty { "udp" }
        if (proto.startsWith("udp")) return "udp"
        if (proto in setOf("tcp", "tcp-client", "tcp4", "tcp4-client", "tcp6", "tcp6-client")) return "tcp"
        throw LinkParseError("unsupported OpenVPN transport `$value`")
    }

    private fun selectOpenVpnCipher(byKey: Map<String, MutableList<List<String>>>): String {
        for (directive in listOf("cipher", "data-ciphers", "ncp-ciphers", "data-ciphers-fallback")) {
            val value = openVpnLastArg(byKey, directive)
            if (value.isEmpty()) continue
            for (candidate in value.split(":")) {
                val normalized = candidate.trim().uppercase()
                if (normalized in OPENVPN_SUPPORTED_CIPHERS) return normalized
            }
        }
        return ""
    }

    private fun openVpnKeyDirection(value: String): Int {
        val normalized = value.trim().lowercase()
        return when (normalized) {
            "0" -> 0
            "1" -> 1
            "bidirectional", "bi", "-1" -> -1
            else -> throw LinkParseError("invalid OpenVPN key-direction `$value`")
        }
    }

    private fun openVpnDurationSeconds(value: String, directive: String): String {
        val seconds = value.trim().toIntOrNull()
            ?: throw LinkParseError("invalid OpenVPN `$directive` value `$value`")
        if (seconds < 0) throw LinkParseError("invalid OpenVPN `$directive` value `$value`")
        return "${seconds}s"
    }

    private fun openVpnVerifyNameMode(value: String): String {
        val normalized = value.trim().lowercase()
        return when (normalized) {
            "name", "name-prefix", "subject" -> normalized
            else -> throw LinkParseError("invalid OpenVPN verify-x509-name mode `$value`")
        }
    }

    private fun looksLikeOpenVpnConfig(text: String): Boolean {
        val lowered = text.lowercase()
        if (!Regex("(?m)^\\s*(?:--)?remote\\s+\\S+").containsMatchIn(lowered)) return false
        return Regex("(?m)^\\s*(?:--)?client\\s*$").containsMatchIn(lowered) ||
            Regex("(?m)^\\s*(?:--)?tls-client\\s*$").containsMatchIn(lowered) ||
            lowered.contains("<ca>")
    }

    private fun looksLikeClashYaml(text: String): Boolean {
        val lowered = text.lowercase()
        return lowered.contains("proxies:") || lowered.contains("proxy-providers:") ||
                lowered.contains("proxy-groups:") || lowered.contains("outbounds:") || lowered.contains("outbound:") ||
                lowered.contains("payload:") || lowered.contains("- name:") || lowered.contains("- type:") ||
                (lowered.contains("mode:") && lowered.contains("rules:"))
    }

    private fun sanitizeYamlText(text: String): String {
        var cleaned = text.replace("\t", "  ")
        cleaned = cleaned.lines()
            .filterNot { it.trimStart().startsWith("%") }
            .joinToString("\n")
        // Strip custom Mihomo / Clash tags like !vless, !select, !<tag> that break SnakeYAML
        cleaned = cleaned.replace(Regex("(?<=\\s|^|\\[|\\{)!(<[^>]+>|[A-Za-z0-9_.-]+)"), "")
        return cleaned
    }

    private fun extractProxyMaps(data: Any?): List<Map<String, Any?>> {
        val result = mutableListOf<Map<String, Any?>>()

        fun processMap(map: Map<*, *>) {
            val type = map["type"]?.toString()
            val server = map["server"]?.toString() ?: map["address"]?.toString() ?: map["host"]?.toString()
            // WARP MASQUE entries may be profile-only, so they have no server field.
            val serverless = type?.lowercase() == "masque"
            if (!type.isNullOrBlank() && (!server.isNullOrBlank() || serverless)) {
                val stringKeyMap = map.entries.associate { (it.key?.toString() ?: "") to it.value }
                result.add(stringKeyMap)
                return
            }

            val keysToCheck = listOf("proxies", "payload", "outbounds", "outbound")
            for (key in keysToCheck) {
                val list = map[key] as? List<*>
                list?.forEach { item ->
                    if (item is Map<*, *>) {
                        processMap(item)
                    }
                }
            }

            val providers = map["proxy-providers"] as? Map<*, *>
            providers?.values?.forEach { provider ->
                if (provider is Map<*, *>) {
                    processMap(provider)
                }
            }
        }

        when (data) {
            is Map<*, *> -> processMap(data)
            is List<*> -> data.forEach { item ->
                if (item is Map<*, *>) processMap(item)
            }
        }

        return result
    }

    private fun parseClashYamlNodesText(text: String): Pair<List<ParsedNode>, List<String>> {
        val nodes = mutableListOf<ParsedNode>()
        val errors = mutableListOf<String>()

        val sanitized = sanitizeYamlText(text)
        val yaml = Yaml()
        val data = try {
            yaml.load<Any>(sanitized)
        } catch (e: Exception) {
            throw LinkParseError("Invalid Clash YAML structure: ${e.message}")
        }

        val proxyMaps = extractProxyMaps(data)
        if (proxyMaps.isEmpty()) {
            return Pair(emptyList(), listOf("No proxies found in Clash YAML"))
        }

        for ((idx, map) in proxyMaps.withIndex()) {
            try {
                val node = parseClashProxyMap(map)
                nodes.add(node)
            } catch (e: Exception) {
                errors.add("Proxy ${idx + 1}: ${e.message}")
            }
        }

        // Every Clash url-test/selector group collapses into a single AUTO node and its
        // members are removed from the flat list, exactly like desktop Lumen shows them.
        val proxyGroups = (((data as? Map<*, *>)?.get("proxy-groups")
            ?: (data as? Map<*, *>)?.get("proxy_groups")) as? List<*>).orEmpty()
        val autoNodes = mutableListOf<ParsedNode>()
        val consumed = mutableListOf<ParsedNode>()
        for (group in proxyGroups) {
            val groupMap = group as? Map<*, *> ?: continue
            val groupType = groupMap["type"]?.toString()?.trim()?.lowercase() ?: ""
            if (groupType !in AUTO_GROUP_TYPES) continue
            val memberNames = (groupMap["proxies"] as? List<*>)?.mapNotNull { it?.toString()?.trim() }.orEmpty()
            val members = memberNames.mapNotNull { memberName ->
                nodes.firstOrNull { it.name.equals(memberName, ignoreCase = true) }
            }.distinct()
            if (members.size < 2) continue
            val groupName = groupMap["name"]?.toString()?.trim().orEmpty().ifEmpty { "AUTO" }
            autoNodes.add(autoNodeFromMembers(groupName, members))
            consumed.addAll(members)
        }
        if (autoNodes.isNotEmpty()) {
            val remaining = nodes.filterNot { node -> consumed.any { it === node } }
            return Pair(remaining + autoNodes, errors)
        }

        return Pair(nodes, errors)
    }

    fun parseClashProxyMap(map: Map<String, Any?>): ParsedNode {
        val name = map["name"]?.toString()?.ifBlank { "Proxy" } ?: "Proxy"
        val rawType = map["type"]?.toString()?.lowercase() ?: throw LinkParseError("Missing proxy type")
        // WARP-generator MASQUE entries may be profile-only (no server field).
        val serverOrNull = map["server"]?.toString() ?: map["address"]?.toString() ?: map["host"]?.toString()
        if (serverOrNull == null && rawType != "masque") throw LinkParseError("Missing proxy server")
        val server = serverOrNull ?: ""
        val port = (map["port"] ?: map["server_port"])?.toString()?.toIntOrNull() ?: 443

        val hasAmneziaParams = map.keys.any { it.toString().lowercase() in AMNEZIA_JUNK_KEYS } || map["reserved"] != null
        val scheme = if (rawType == "awg" || rawType == "amneziawg" || rawType == "amnezia-wg" || (rawType in setOf("wg", "wireguard") && hasAmneziaParams)) "awg" else when (rawType) {
            "shadowsocks" -> "ss"
            "hy2" -> "hysteria2"
            "hy" -> "hysteria"
            "wg" -> "wireguard"
            "socks5" -> "socks"
            "https" -> "http"
            else -> rawType
        }

        val outbound = mutableMapOf<String, Any?>(
            "protocol" to scheme,
            "clash" to map
        )

        val link = buildClashUriLink(name, scheme, server, port, map, outbound)

        return ParsedNode(
            name = name,
            scheme = scheme,
            server = server,
            port = port,
            link = link,
            outbound = outbound
        )
    }

    private fun buildClashUriLink(
        name: String,
        scheme: String,
        server: String,
        port: Int,
        map: Map<String, Any?>,
        outbound: MutableMap<String, Any?>
    ): String {
        return try {
            val encodedName = java.net.URLEncoder.encode(name, "UTF-8").replace("+", "%20")
            when (scheme) {
                "vless" -> {
                    val uuid = map["uuid"]?.toString() ?: map["id"]?.toString() ?: ""
                    val flow = map["flow"]?.toString() ?: ""
                    val net = map["network"]?.toString() ?: map["type"]?.toString() ?: "tcp"
                    val tls = map["tls"] == true || map["tls"]?.toString() == "true"
                    val sni = map["servername"]?.toString() ?: map["sni"]?.toString() ?: server
                    val realityOpts = map["reality-opts"] as? Map<*, *>
                    val wsOpts = map["ws-opts"] as? Map<*, *>
                    val grpcOpts = map["grpc-opts"] as? Map<*, *>
                    val fp = map["client-fingerprint"]?.toString() ?: map["fp"]?.toString() ?: ""
                    val security = if (realityOpts != null) "reality" else if (tls) "tls" else "none"

                    val params = mutableMapOf<String, String>()
                    params["type"] = net
                    params["security"] = security
                    if (sni.isNotEmpty()) params["sni"] = sni
                    if (fp.isNotEmpty()) params["fp"] = fp
                    if (flow.isNotEmpty()) params["flow"] = flow

                    if (wsOpts != null) {
                        wsOpts["path"]?.toString()?.let { params["path"] = it }
                        (wsOpts["headers"] as? Map<*, *>)?.get("Host")?.toString()?.let { params["host"] = it }
                    }
                    if (grpcOpts != null) {
                        grpcOpts["grpc-service-name"]?.toString()?.let { params["servicename"] = it }
                    }
                    if (realityOpts != null) {
                        realityOpts["public-key"]?.toString()?.let { params["pbk"] = it }
                        realityOpts["short-id"]?.toString()?.let { params["sid"] = it }
                    }

                    val user = mutableMapOf<String, Any?>("id" to uuid, "encryption" to "none")
                    if (flow.isNotEmpty()) user["flow"] = flow
                    val streamSettings = buildStreamSettings(params, defaultNetwork = "tcp", defaultSecurity = security)

                    outbound["settings"] = mapOf("vnext" to listOf(mapOf("address" to server, "port" to port, "users" to listOf(user))))
                    outbound["streamSettings"] = streamSettings

                    val q = params.entries.joinToString("&") { "${it.key}=${java.net.URLEncoder.encode(it.value, "UTF-8")}" }
                    "vless://$uuid@$server:$port?$q#$encodedName"
                }
                "vmess" -> {
                    val uuid = map["uuid"]?.toString() ?: ""
                    val alterId = (map["alterId"] as? Number)?.toInt() ?: 0
                    val cipher = map["cipher"]?.toString() ?: map["security"]?.toString() ?: "auto"
                    val net = map["network"]?.toString() ?: "tcp"
                    val tls = if (map["tls"] == true || map["tls"]?.toString() == "true") "tls" else "none"
                    val sni = map["servername"]?.toString() ?: map["sni"]?.toString() ?: ""
                    val wsOpts = map["ws-opts"] as? Map<*, *>
                    val path = wsOpts?.get("path")?.toString() ?: ""
                    val host = (wsOpts?.get("headers") as? Map<*, *>)?.get("Host")?.toString() ?: ""

                    val user = mapOf("id" to uuid, "alterId" to alterId, "security" to cipher)
                    val params = mutableMapOf("type" to net, "security" to tls, "host" to host, "path" to path, "sni" to sni)
                    val streamSettings = buildStreamSettings(params, defaultNetwork = "tcp", defaultSecurity = tls)

                    outbound["settings"] = mapOf("vnext" to listOf(mapOf("address" to server, "port" to port, "users" to listOf(user))))
                    outbound["streamSettings"] = streamSettings

                    val json = JSONObject(mapOf(
                        "v" to "2", "ps" to name, "add" to server, "port" to port, "id" to uuid,
                        "aid" to alterId, "scy" to cipher, "net" to net, "type" to "none",
                        "host" to host, "path" to path, "tls" to tls, "sni" to sni
                    )).toString()
                    "vmess://${Base64.getEncoder().encodeToString(json.toByteArray(Charsets.UTF_8))}"
                }
                "trojan" -> {
                    val password = map["password"]?.toString() ?: ""
                    val sni = map["servername"]?.toString() ?: map["sni"]?.toString() ?: server
                    val net = map["network"]?.toString() ?: "tcp"
                    val wsOpts = map["ws-opts"] as? Map<*, *>
                    val path = wsOpts?.get("path")?.toString() ?: ""

                    val params = mutableMapOf("type" to net, "security" to "tls", "sni" to sni, "path" to path)
                    val streamSettings = buildStreamSettings(params, defaultNetwork = "tcp", defaultSecurity = "tls")

                    outbound["settings"] = mapOf("servers" to listOf(mapOf("address" to server, "port" to port, "password" to password)))
                    outbound["streamSettings"] = streamSettings

                    "trojan://$password@$server:$port?security=tls&sni=${java.net.URLEncoder.encode(sni, "UTF-8")}&type=$net#$encodedName"
                }
                "ss" -> {
                    val method = map["cipher"]?.toString() ?: map["method"]?.toString() ?: "aes-256-gcm"
                    val password = map["password"]?.toString() ?: ""

                    outbound["settings"] = mapOf("servers" to listOf(mapOf("address" to server, "port" to port, "method" to method, "password" to password)))

                    val userPassB64 = Base64.getEncoder().encodeToString("$method:$password".toByteArray(Charsets.UTF_8))
                    "ss://$userPassB64@$server:$port#$encodedName"
                }
                "hysteria2" -> {
                    val password = map["password"]?.toString() ?: map["auth"]?.toString() ?: ""
                    val sni = map["servername"]?.toString() ?: map["sni"]?.toString() ?: server
                    val insecure = map["skip-cert-verify"] == true || map["insecure"] == true
                    val obfs = map["obfs"]?.toString() ?: ""
                    val obfsPassword = map["obfs-password"]?.toString() ?: ""

                    val singbox = mutableMapOf<String, Any?>(
                        "type" to "hysteria2",
                        "server" to server,
                        "server_port" to port,
                        "password" to password,
                        "tls" to mapOf("enabled" to true, "server_name" to sni, "insecure" to insecure)
                    )
                    if (obfs.isNotEmpty()) singbox["obfs"] = mapOf("type" to obfs, "password" to obfsPassword)
                    outbound["singbox"] = singbox

                    "hy2://$password@$server:$port?sni=${java.net.URLEncoder.encode(sni, "UTF-8")}&insecure=${if (insecure) 1 else 0}#$encodedName"
                }
                "hysteria" -> {
                    // Hysteria v1 (parity with desktop _clash_to_singbox_outbound).
                    val singbox = mutableMapOf<String, Any?>(
                        "type" to "hysteria",
                        "tag" to "proxy",
                        "server" to server,
                        "server_port" to port
                    )
                    val authStr = (map["auth-str"] ?: map["auth_str"] ?: map["auth"] ?: map["password"])?.toString() ?: ""
                    if (authStr.isNotEmpty()) singbox["auth_str"] = authStr
                    (map["up"] ?: map["up-speed"])?.toString()?.filter { it.isDigit() }?.toIntOrNull()?.let { singbox["up_mbps"] = it }
                    (map["down"] ?: map["down-speed"])?.toString()?.filter { it.isDigit() }?.toIntOrNull()?.let { singbox["down_mbps"] = it }
                    map["protocol"]?.toString()?.takeIf { it.isNotBlank() && it.lowercase() != "hysteria" }?.let { singbox["protocol"] = it }
                    map["obfs"]?.toString()?.takeIf { it.isNotBlank() }?.let { singbox["obfs"] = it }
                    val tls = mutableMapOf<String, Any?>("enabled" to true)
                    tls["server_name"] = (map["servername"] ?: map["sni"])?.toString()?.takeIf { it.isNotBlank() } ?: server
                    if (toBool(map["skip-cert-verify"])) tls["insecure"] = true
                    val alpnValue = map["alpn"]
                    val alpnList = when (alpnValue) {
                        is List<*> -> alpnValue.mapNotNull { it?.toString()?.trim() }.filter { it.isNotEmpty() }
                        null -> emptyList()
                        else -> alpnValue.toString().split(",").map { it.trim() }.filter { it.isNotEmpty() }
                    }
                    if (alpnList.isNotEmpty()) tls["alpn"] = alpnList
                    singbox["tls"] = tls
                    outbound["singbox"] = singbox
                    toJsonString(singbox)
                }
                "naive" -> {
                    val singbox = mutableMapOf<String, Any?>(
                        "type" to "naive",
                        "tag" to "proxy",
                        "server" to server,
                        "server_port" to port,
                        "username" to (map["username"]?.toString() ?: ""),
                        "password" to (map["password"]?.toString() ?: ""),
                        "quic" to toBool(map["quic"])
                    )
                    val tls = mutableMapOf<String, Any?>("enabled" to true)
                    // Cronet cannot skip certificate verification, so no insecure here.
                    tls["server_name"] = (map["servername"] ?: map["sni"])?.toString()?.takeIf { it.isNotBlank() } ?: server
                    singbox["tls"] = tls
                    ((map["insecure-concurrency"] ?: map["insecure_concurrency"]))?.toString()?.toIntOrNull()?.let {
                        singbox["insecure_concurrency"] = it
                    }
                    (map["quic-congestion-control"] ?: map["quic_congestion_control"])?.toString()?.takeIf { it.isNotBlank() }?.let {
                        singbox["quic_congestion_control"] = it
                    }
                    val uot = map["udp-over-tcp"] ?: map["udp_over_tcp"]
                    if (uot is Boolean || uot is Map<*, *>) singbox["udp_over_tcp"] = uot
                    val extraHeaders = map["extra-headers"] ?: map["extra_headers"]
                    if (extraHeaders is Map<*, *>) singbox["extra_headers"] = extraHeaders
                    outbound["singbox"] = singbox
                    toJsonString(singbox)
                }
                "mieru" -> {
                    val singbox = mutableMapOf<String, Any?>(
                        "type" to "mieru",
                        "tag" to "proxy",
                        "server" to server,
                        "server_port" to port,
                        "transport" to ((map["transport"]?.toString()?.takeIf { it.isNotBlank() } ?: "TCP").uppercase()),
                        "username" to (map["username"]?.toString() ?: ""),
                        "password" to (map["password"]?.toString() ?: "")
                    )
                    map["multiplexing"]?.toString()?.takeIf { it.isNotBlank() }?.let { singbox["multiplexing"] = it }
                    val serverPorts = clashStringList(map["server-ports"] ?: map["server_ports"] ?: map["ports"])
                    if (serverPorts.isNotEmpty()) singbox["server_ports"] = serverPorts
                    outbound["singbox"] = singbox
                    toJsonString(singbox)
                }
                "masque" -> {
                    // Port of the desktop _parse_clash_masque_payload (WARP generators).
                    val network = map["network"]?.toString()?.lowercase() ?: ""
                    val profile = mutableMapOf<String, Any?>("detour" to "direct")
                    (map["profile-id"] ?: map["profile_id"])?.toString()?.takeIf { it.isNotBlank() }?.let { profile["id"] = it }
                    (map["auth-token"] ?: map["auth_token"])?.toString()?.takeIf { it.isNotBlank() }?.let { profile["auth_token"] = it }
                    (map["masque-private-key"] ?: map["masque_private_key"])?.toString()?.takeIf { it.isNotBlank() }?.let { profile["private_key"] = it }
                    val privateKey = (map["private-key"] ?: map["private_key"])?.toString()?.trim() ?: ""
                    val publicKey = (map["public-key"] ?: map["public_key"])?.toString()?.trim() ?: ""
                    val singbox = mutableMapOf<String, Any?>(
                        "type" to "masque",
                        "tag" to "proxy",
                        "system" to false,
                        "name" to "masque0",
                        "use_http2" to (network == "h2" || network == "http2"),
                        "use_ipv6" to toBool(map["use-ipv6"] ?: map["use_ipv6"]),
                        "profile" to profile,
                        "udp_timeout" to "5m0s",
                        "udp_keepalive_period" to "30s",
                        "reconnect_delay" to "5s",
                        "congestion_controller" to "bbr"
                    )
                    if (privateKey.isNotEmpty() || publicKey.isNotEmpty()) {
                        // Direct mode: connects straight to the MASQUE endpoint.
                        if (privateKey.isEmpty() || publicKey.isEmpty() || server.isEmpty()) {
                            throw LinkParseError("direct MASQUE proxy must contain server, private-key and public-key")
                        }
                        val address = clashStringList(map["ip"] ?: map["address"]).toMutableList()
                        for (item in clashStringList(map["ipv6"])) if (item !in address) address.add(item)
                        if (address.isEmpty()) throw LinkParseError("direct MASQUE proxy must contain ip, ipv6 or address")
                        singbox["server"] = server
                        singbox["server_port"] = port
                        singbox["private_key"] = privateKey
                        singbox["public_key"] = publicKey
                        singbox["address"] = address
                        singbox["mtu"] = map["mtu"]?.toString()?.toIntOrNull() ?: 1280
                    } else if (profile["id"] == null && profile["auth_token"] == null && profile["private_key"] == null) {
                        throw LinkParseError("MASQUE proxy must contain profile-id/auth-token or private-key/public-key")
                    }
                    val allowedIps = clashStringList(map["allowed-ips"] ?: map["allowed_ips"])
                    if (allowedIps.isNotEmpty()) singbox["allowed_ips"] = allowedIps
                    (map["sni"] ?: map["servername"])?.toString()?.takeIf { it.isNotBlank() }?.let {
                        singbox["tls"] = mapOf("server_name" to it)
                    }
                    outbound["singbox"] = singbox
                    toJsonString(singbox)
                }
                "tuic" -> {
                    val uuid = map["uuid"]?.toString() ?: ""
                    val password = map["password"]?.toString() ?: ""
                    val sni = map["servername"]?.toString() ?: map["sni"]?.toString() ?: server
                    val cc = map["congestion-controller"]?.toString() ?: map["congestion_control"]?.toString() ?: "bbr"
                    val alpnStr = (map["alpn"] as? List<*>)?.joinToString(",") ?: map["alpn"]?.toString() ?: "h3"

                    val singbox = mapOf<String, Any?>(
                        "type" to "tuic",
                        "server" to server,
                        "server_port" to port,
                        "uuid" to uuid,
                        "password" to password,
                        "congestion_control" to cc,
                        "tls" to mapOf("enabled" to true, "server_name" to sni, "alpn" to alpnStr.split(","))
                    )
                    outbound["singbox"] = singbox

                    "tuic://$uuid:$password@$server:$port?sni=${java.net.URLEncoder.encode(sni, "UTF-8")}&congestion_control=$cc#$encodedName"
                }
                "wireguard", "awg" -> {
                    val privateKey = map["private-key"]?.toString() ?: map["private_key"]?.toString()
                        ?: map["privateKey"]?.toString() ?: map["secret-key"]?.toString() ?: map["secret_key"]?.toString() ?: ""
                    val publicKey = map["public-key"]?.toString() ?: map["public_key"]?.toString() ?: ""
                    val ipStr = (map["ip"] as? List<*>)?.joinToString(",") ?: map["ip"]?.toString() ?: map["address"]?.toString() ?: ""
                    val ips = ipStr.split(",").map { it.trim() }.filter { it.isNotEmpty() }

                    val amneziaOpts = map["amnezia-wg-option"] as? Map<*, *>
                        ?: map["amnezia_wg_option"] as? Map<*, *>
                        ?: map["amnezia"] as? Map<*, *>
                        ?: map["amnezia-options"] as? Map<*, *>

                    val amneziaMap = mutableMapOf<String, Any?>()
                    for (junkKey in AMNEZIA_JUNK_KEYS) {
                        val v = map[junkKey] ?: amneziaOpts?.get(junkKey)
                        if (v != null) {
                            val intVal = when (v) {
                                is Number -> v.toInt()
                                is String -> v.toIntOrNull()
                                else -> null
                            }
                            if (intVal != null) amneziaMap[junkKey] = intVal
                            else if (v is String && v.isNotEmpty()) amneziaMap[junkKey] = v
                        }
                    }
                    val isAwg = scheme == "awg" || amneziaMap.isNotEmpty()
                    val actualScheme = if (isAwg) "awg" else "wireguard"

                    val singbox = mutableMapOf<String, Any?>(
                        "type" to actualScheme,
                        "server" to server,
                        "server_port" to port,
                        "private_key" to privateKey,
                        "address" to ips,
                        "peers" to listOf(mapOf("public_key" to publicKey, "server" to server, "server_port" to port, "allowed_ips" to listOf("0.0.0.0/0", "::/0")))
                    )
                    if (amneziaMap.isNotEmpty()) {
                        singbox["amnezia"] = amneziaMap
                    }
                    if (map["reserved"] != null) {
                        singbox["reserved"] = map["reserved"]
                    }
                    outbound["protocol"] = actualScheme
                    outbound["singbox"] = singbox

                    val amneziaQuery = StringBuilder()
                    for ((junkKey, junkVal) in amneziaMap) amneziaQuery.append("&").append(junkKey).append("=").append(junkVal)
                    "${if (isAwg) "awg" else "wg"}://${java.net.URLEncoder.encode(privateKey, "UTF-8")}@$server:$port?publickey=${java.net.URLEncoder.encode(publicKey, "UTF-8")}$amneziaQuery#$encodedName"
                }
                "socks" -> {
                    val user = map["username"]?.toString() ?: ""
                    val pass = map["password"]?.toString() ?: ""
                    val serverMap = mutableMapOf<String, Any?>("address" to server, "port" to port)
                    if (user.isNotEmpty()) serverMap["users"] = listOf(mapOf("user" to user, "pass" to pass))
                    outbound["settings"] = mapOf("servers" to listOf(serverMap))

                    val auth = if (user.isNotEmpty()) "$user:$pass@" else ""
                    "socks5://$auth$server:$port#$encodedName"
                }
                "http" -> {
                    val user = map["username"]?.toString() ?: ""
                    val pass = map["password"]?.toString() ?: ""
                    val serverMap = mutableMapOf<String, Any?>("address" to server, "port" to port)
                    if (user.isNotEmpty()) serverMap["users"] = listOf(mapOf("user" to user, "pass" to pass))
                    outbound["settings"] = mapOf("servers" to listOf(serverMap))

                    val auth = if (user.isNotEmpty()) "$user:$pass@" else ""
                    "http://$auth$server:$port#$encodedName"
                }
                else -> {
                    toJsonString(map)
                }
            }
        } catch (_: Exception) {
            toJsonString(map)
        }
    }

    private fun parseJsonNodesText(text: String): Pair<List<ParsedNode>, List<String>> {
        val nodes = mutableListOf<ParsedNode>()
        val errors = mutableListOf<String>()

        if (text.startsWith("[")) {
            val array = JSONArray(text)
            for (i in 0 until array.length()) {
                // One bad entry must not abort a whole array of configs.
                try {
                    when (val item = array.get(i)) {
                        is JSONObject ->
                            // Panels ship arrays of whole client configs, not bare outbounds.
                            if (item.has("outbounds") || item.has("endpoints") || item.has("proxies") || item.has("inbounds")) {
                                val (inner, innerErrors) = parseJsonNodesText(item.toString())
                                val label = listOf("remarks", "profile_title", "name", "tag")
                                    .firstNotNullOfOrNull { key -> item.optString(key).takeIf { it.isNotBlank() } }
                                    .orEmpty()
                                nodes += if (label.isEmpty()) inner else inner.map { node ->
                                    val suffix = node.name.takeIf { it.isNotBlank() && it != label }
                                    node.copy(name = if (suffix == null) label else "$label · $suffix")
                                }
                                errors += innerErrors.map { "Config ${i + 1}: $it" }
                            } else {
                                nodes.add(parseJsonItem(item))
                            }
                        is String -> nodes.add(parseSingle(item))
                        else -> Unit
                    }
                } catch (e: Exception) {
                    errors.add("Item ${i + 1}: ${e.message}")
                }
            }
        } else if (text.startsWith("{")) {
            val json = JSONObject(text)
            if (json.has("proxy") && !json.has("type") && !json.has("protocol") &&
                !json.has("outbounds") && !json.has("endpoints") && !json.has("proxies")
            ) {
                // NaiveProxy config.json: {"listen": ..., "proxy": "https://user:pass@host"}
                val rawProxy = json.get("proxy")
                val proxyList = if (rawProxy is JSONArray) {
                    (0 until rawProxy.length()).map { rawProxy.get(it).toString() }
                } else {
                    listOf(rawProxy.toString())
                }
                for ((i, rawUri) in proxyList.withIndex()) {
                    val uriText = rawUri.trim()
                    if (uriText.isEmpty()) continue
                    val sepIdx = uriText.indexOf("://")
                    val proxyScheme = if (sepIdx > 0) uriText.substring(0, sepIdx).lowercase() else ""
                    if (proxyScheme != "https" && proxyScheme != "quic") {
                        errors.add("NaiveProxy proxy ${i + 1}: unsupported transport `${proxyScheme.ifEmpty { "unknown" }}`")
                        continue
                    }
                    try {
                        nodes.add(parseNaiveLink(uriText, proxyScheme))
                    } catch (e: Exception) {
                        errors.add("NaiveProxy proxy ${i + 1}: ${e.message}")
                    }
                }
                return Pair(nodes, errors)
            }
            val skipProtocols = setOf("freedom", "blackhole", "dns", "direct", "block", "selector", "urltest", "url-test", "loopback")
            var handled = false
            val nodesByTag = mutableMapOf<String, ParsedNode>()
            val autoGroupDefs = mutableListOf<Pair<String, List<String>>>()
            for (arrayKey in listOf("outbounds", "endpoints")) {
                if (!json.has(arrayKey)) continue
                handled = true
                val outbounds = json.optJSONArray(arrayKey) ?: continue
                for (i in 0 until outbounds.length()) {
                    // Outbound arrays may also hold plain links or nested strings.
                    val raw = outbounds.get(i)
                    if (raw is String) {
                        runCatching { nodes.add(parseSingle(raw)) }
                            .onFailure { errors.add("Outbound ${i + 1}: ${it.message}") }
                        continue
                    }
                    val item = raw as? JSONObject ?: continue
                    val protocol = item.optString("protocol", item.optString("type"))
                    if (protocol.lowercase() in skipProtocols) {
                        // Every urltest/selector group is imported as one AUTO node
                        // instead of loose servers.
                        if (protocol.lowercase() in AUTO_GROUP_TYPES) {
                            val memberTagList = mutableListOf<String>()
                            item.optJSONArray("outbounds")?.let { memberTags ->
                                for (m in 0 until memberTags.length()) {
                                    val memberTag = memberTags.optString(m).trim()
                                    if (memberTag.isNotEmpty()) memberTagList.add(memberTag)
                                }
                            }
                            if (memberTagList.isNotEmpty()) {
                                autoGroupDefs.add(item.optString("tag").trim() to memberTagList)
                            }
                        }
                        continue
                    }
                    if (protocol.isEmpty() && item.optString("server").isEmpty()) continue
                    try {
                        val parsedItem = parseJsonItem(item)
                        nodes.add(parsedItem)
                        val itemTag = item.optString("tag").trim()
                        if (itemTag.isNotEmpty()) nodesByTag[itemTag] = parsedItem
                    } catch (e: Exception) {
                        errors.add("Outbound ${i + 1}: ${e.message}")
                    }
                }
            }
            if (autoGroupDefs.isNotEmpty()) {
                val profileLabel = listOf("remarks", "profile_title", "profileTitle")
                    .firstNotNullOfOrNull { key -> json.optString(key).takeIf { it.isNotBlank() } }
                val autoNodes = mutableListOf<ParsedNode>()
                val consumed = mutableListOf<ParsedNode>()
                for ((groupTag, memberTags) in autoGroupDefs) {
                    val members = memberTags.mapNotNull { nodesByTag[it] }.distinct()
                    if (members.size < 2) continue
                    val label = if (autoGroupDefs.size == 1) {
                        profileLabel ?: groupTag.ifEmpty { "AUTO" }
                    } else {
                        groupTag.ifEmpty { "AUTO" }
                    }
                    autoNodes.add(autoNodeFromMembers(label, members))
                    consumed.addAll(members)
                }
                if (autoNodes.isNotEmpty()) {
                    // Pool members live inside the AUTO node, never as separate servers.
                    val remaining = nodes.filterNot { node -> consumed.any { it === node } }
                    return Pair(remaining + autoNodes, errors)
                }
            }
            for (arrayKey in listOf("proxies", "servers", "nodes", "configs", "links", "subs")) {
                if (handled || !json.has(arrayKey)) continue
                val array = json.optJSONArray(arrayKey) ?: continue
                handled = true
                for (i in 0 until array.length()) {
                    try {
                        when (val item = array.get(i)) {
                            is JSONObject -> nodes.add(parseJsonItem(item))
                            is String -> nodes.add(parseSingle(item))
                            else -> Unit
                        }
                    } catch (e: Exception) {
                        errors.add("$arrayKey ${i + 1}: ${e.message}")
                    }
                }
            }
            if (!handled) {
                nodes.add(parseJsonItem(json))
            }
        }

        // Fall back to the other formats instead of reporting an empty import.
        if (nodes.isEmpty()) throw LinkParseError("No servers found in JSON payload")

        return Pair(nodes, errors)
    }

    /**
     * NDJSON: each line is a self-contained JSON object (one outbound per line).
     * Used by some export tools and panel APIs.
     * Returns null if the text does not look like NDJSON (prevents false positives).
     */
    private fun tryParseJsonLines(text: String): Pair<List<ParsedNode>, List<String>>? {
        val jsonLines = text.lines().map { it.trim() }.filter { it.startsWith("{") && it.endsWith("}") }
        if (jsonLines.isEmpty()) return null
        val nodes = mutableListOf<ParsedNode>()
        val errors = mutableListOf<String>()
        for ((i, line) in jsonLines.withIndex()) {
            try {
                val obj = JSONObject(line)
                nodes.add(parseJsonItem(obj))
            } catch (e: Exception) {
                errors.add("NDJSON line ${i + 1}: ${e.message}")
            }
        }
        if (nodes.isEmpty()) return null
        return Pair(nodes, errors)
    }

    // Config arrays mix sing-box/v2ray outbounds with Clash-style proxy maps.
    private fun parseJsonItem(item: JSONObject): ParsedNode = try {
        parseJsonObjectOutbound(item)
    } catch (e: Exception) {
        try {
            parseClashProxyMap(jsonToMap(item))
        } catch (_: Exception) {
            throw e
        }
    }

    private fun parseJsonOutbound(text: String): ParsedNode {
        val json = JSONObject(text)
        return parseJsonObjectOutbound(json)
    }

    fun parseJsonObjectOutbound(json: JSONObject): ParsedNode {
        val typeValue = json.optString("type")
        val protocolValue = json.optString("protocol")

        if (protocolValue.isEmpty() && typeValue.isNotEmpty()) {
            // Native sing-box outbound/endpoint object: wrap it as a singbox
            // pass-through, matching the desktop _native_singbox_outbound path.
            val native = jsonToMap(json).toMutableMap()
            var protocol = typeValue.lowercase()
            when (protocol) {
                "hy" -> { protocol = "hysteria"; native["type"] = "hysteria" }
                "hy2" -> { protocol = "hysteria2"; native["type"] = "hysteria2" }
                "openvpn" -> {
                    native["system"] = false
                    native["name"] = native["name"]?.toString()?.takeIf { it.isNotBlank() } ?: "openvpn0"
                }
            }
            if (protocol == "wireguard" && native["amnezia"] is Map<*, *>) protocol = "awg"

            var server = native["server"]?.toString() ?: ""
            var port = (native["server_port"] as? Number)?.toInt()
                ?: native["server_port"]?.toString()?.toIntOrNull() ?: 0
            if (protocol == "masque" && server.isEmpty()) {
                server = ((native["profile"] as? Map<*, *>)?.get("id"))?.toString() ?: ""
            }
            if (protocol == "openvpn") {
                val firstServer = (native["servers"] as? List<*>)?.firstOrNull() as? Map<*, *>
                if (server.isEmpty()) server = firstServer?.get("server")?.toString() ?: ""
                if (port <= 0) {
                    port = (firstServer?.get("server_port") as? Number)?.toInt()
                        ?: firstServer?.get("server_port")?.toString()?.toIntOrNull() ?: 0
                }
            }
            if (protocol in setOf("wireguard", "awg", "warp") && server.isEmpty()) {
                val peer = (native["peers"] as? List<*>)?.firstOrNull() as? Map<*, *>
                server = peer?.get("address")?.toString() ?: peer?.get("server")?.toString() ?: ""
                if (port <= 0) {
                    port = (peer?.get("port") as? Number)?.toInt()
                        ?: (peer?.get("server_port") as? Number)?.toInt() ?: 0
                }
            }

            val name = json.optString("tag").ifEmpty { json.optString("name", "$protocol-node") }
            val outbound = mapOf<String, Any?>("protocol" to protocol, "singbox" to native)
            return ParsedNode(name = name, scheme = protocol, server = server, port = port, link = json.toString(), outbound = outbound)
        }

        val protocol = protocolValue.ifEmpty { json.optString("type", "unknown") }
        val name = json.optString("tag").ifEmpty { json.optString("name", "$protocol-node") }
        var server = json.optString("server").ifEmpty { json.optString("address") }
        var port = json.optInt("port", json.optInt("server_port", 443))

        if (json.has("settings") && json.getJSONObject("settings").has("vnext")) {
            val vnext = json.getJSONObject("settings").getJSONArray("vnext")
            if (vnext.length() > 0) {
                val target = vnext.getJSONObject(0)
                server = target.optString("address")
                port = target.optInt("port", 443)
            }
        }

        val map = jsonToMap(json)
        return ParsedNode(name = name, scheme = protocol, server = server, port = port, link = json.toString(), outbound = map)
    }

    fun jsonToMap(json: JSONObject): Map<String, Any?> {
        val map = mutableMapOf<String, Any?>()
        val keys = json.keys()
        while (keys.hasNext()) {
            val key = keys.next()
            val value = json.get(key)
            map[key] = when (value) {
                is JSONObject -> jsonToMap(value)
                is JSONArray -> jsonToList(value)
                JSONObject.NULL -> null
                else -> value
            }
        }
        return map
    }

    fun jsonToList(array: JSONArray): List<Any?> {
        val list = mutableListOf<Any?>()
        for (i in 0 until array.length()) {
            val value = array.get(i)
            list.add(
                when (value) {
                    is JSONObject -> jsonToMap(value)
                    is JSONArray -> jsonToList(value)
                    JSONObject.NULL -> null
                    else -> value
                }
            )
        }
        return list
    }

    fun toJsonString(value: Any?): String {
        return when (val safe = toJsonSafe(value)) {
            is JSONObject -> safe.toString()
            is JSONArray -> safe.toString()
            null -> ""
            else -> safe.toString()
        }
    }

    fun toJsonSafe(value: Any?): Any? {
        if (value == null) return null
        return when (value) {
            is Number -> {
                val d = value.toDouble()
                if (d.isInfinite() || d.isNaN()) null else value
            }
            is Map<*, *> -> {
                val jsonObj = JSONObject()
                for ((k, v) in value) {
                    if (k != null) {
                        val safeV = toJsonSafe(v)
                        if (safeV != null && safeV != JSONObject.NULL) {
                            jsonObj.put(k.toString(), safeV)
                        }
                    }
                }
                jsonObj
            }
            is List<*> -> {
                val jsonArr = JSONArray()
                for (item in value) {
                    val safeItem = toJsonSafe(item)
                    if (safeItem != null && safeItem != JSONObject.NULL) {
                        jsonArr.put(safeItem)
                    }
                }
                jsonArr
            }
            is Array<*> -> {
                val jsonArr = JSONArray()
                for (item in value) {
                    val safeItem = toJsonSafe(item)
                    if (safeItem != null && safeItem != JSONObject.NULL) {
                        jsonArr.put(safeItem)
                    }
                }
                jsonArr
            }
            is String -> {
                if (value.equals("Infinity", ignoreCase = true) || value.equals("-Infinity", ignoreCase = true) || value.equals("NaN", ignoreCase = true)) {
                    null
                } else value
            }
            is Boolean -> value
            else -> value.toString()
        }
    }

    private fun buildStreamSettings(params: Map<String, String>, defaultNetwork: String, defaultSecurity: String): Map<String, Any?> {
        val network = (params["type"] ?: params["net"] ?: defaultNetwork).lowercase()
        var security = (params["security"] ?: defaultSecurity).lowercase()
        if (security == "none" && params["tls"] == "tls") security = "tls"

        val stream = mutableMapOf<String, Any?>(
            "network" to network,
            "security" to security
        )

        val host = params["host"] ?: ""
        val path = params["path"] ?: ""

        when (network) {
            "ws" -> {
                val ws = mutableMapOf<String, Any?>()
                if (path.isNotEmpty()) ws["path"] = path
                if (host.isNotEmpty()) ws["headers"] = mapOf("Host" to host)
                stream["wsSettings"] = ws
            }
            "grpc" -> {
                val grpc = mutableMapOf<String, Any?>()
                params["servicename"]?.let { if (it.isNotEmpty()) grpc["serviceName"] = it }
                params["authority"]?.let { if (it.isNotEmpty()) grpc["authority"] = it }
                stream["grpcSettings"] = grpc
            }
            "xhttp" -> {
                val xhttp = mutableMapOf<String, Any?>()
                if (path.isNotEmpty()) xhttp["path"] = path
                if (host.isNotEmpty()) xhttp["host"] = host
                params["mode"]?.let { if (it.isNotEmpty()) xhttp["mode"] = it }
                stream["xhttpSettings"] = xhttp
            }
            "http", "h2" -> {
                val http = mutableMapOf<String, Any?>()
                if (host.isNotEmpty()) http["host"] = host.split(",").map { it.trim() }.filter { it.isNotEmpty() }
                if (path.isNotEmpty()) http["path"] = path
                stream["httpSettings"] = http
            }
            "httpupgrade" -> {
                val httpUpgrade = mutableMapOf<String, Any?>()
                if (path.isNotEmpty()) httpUpgrade["path"] = path
                if (host.isNotEmpty()) httpUpgrade["host"] = host
                stream["httpupgradeSettings"] = httpUpgrade
            }
        }

        if (security == "tls") {
            val tls = mutableMapOf<String, Any?>()
            params["sni"]?.let { if (it.isNotEmpty()) tls["serverName"] = it }
            params["alpn"]?.let { if (it.isNotEmpty()) tls["alpn"] = it.split(",").map { s -> s.trim() } }
            params["fp"]?.let { if (it.isNotEmpty()) tls["fingerprint"] = it }
            if (params["allowinsecure"] == "1" || params["insecure"] == "true") tls["allowInsecure"] = true
            stream["tlsSettings"] = tls
        } else if (security == "reality") {
            val reality = mutableMapOf<String, Any?>()
            params["sni"]?.let { if (it.isNotEmpty()) reality["serverName"] = it }
            params["fp"]?.let { if (it.isNotEmpty()) reality["fingerprint"] = it }
            params["pbk"]?.let { if (it.isNotEmpty()) reality["publicKey"] = it }
            params["sid"]?.let { if (it.isNotEmpty()) reality["shortId"] = it }
            params["spx"]?.let { if (it.isNotEmpty()) reality["spiderX"] = it }
            reality["show"] = false
            stream["realitySettings"] = reality
        }

        return stream
    }

    private fun applyHappServerMetadata(node: ParsedNode, raw: String) {
        val text = raw.trim()
        val fragment = try { URI(text).rawFragment } catch (e: Exception) { null } ?: return
        if (fragment.contains("?")) {
            val queryText = fragment.substringAfter("?")
            val params = parseQueryParams(queryText)
            val desc = params["serverdescription"]
            if (!desc.isNullOrEmpty()) {
                val decoded = try { decodeB64(unquote(desc)) } catch (e: Exception) { unquote(desc) }
                node.description = decoded.take(30)
            }
            val title = fragment.substringBefore("?").trim()
            if (title.isNotEmpty()) {
                node.name = title
            }
        }
    }
}
