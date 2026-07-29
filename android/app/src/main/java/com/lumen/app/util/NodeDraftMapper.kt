package com.lumen.app.util

import com.lumen.core.config.parser.LinkParser
import com.lumen.core.database.model.NodeEntity
import com.lumen.ui.screens.NodeDraft
import com.lumen.ui.screens.openVpnDraftFromProfile
import com.lumen.ui.screens.openVpnProfileFromDraft
import org.json.JSONObject
import java.net.URLEncoder
import java.util.Base64
import java.util.UUID

/**
 * Converts editor drafts to [NodeEntity] by building a canonical share link
 * and re-parsing it with [LinkParser], so validation and outbound generation
 * stay in one place (same behavior as the desktop app).
 */
object NodeDraftMapper {

    fun draftFromEntity(entity: NodeEntity?): NodeDraft? {
        if (entity == null) return null
        val protocol = normalizeProtocol(entity.protocol)
        val preservedConfig = if (
            protocol !in setOf("wireguard", "awg", "openvpn", "masque") &&
            entity.outboundJson.trim().let { it.startsWith("{") && it.endsWith("}") }
        ) {
            entity.outboundJson
        } else {
            entity.link
        }
        val base = NodeDraft(
            id = entity.id,
            name = entity.name,
            protocol = protocol,
            server = entity.server,
            port = entity.port.toString(),
            rawConfig = preservedConfig
        )
        // Editing must expose every protocol parameter, so the stored link is parsed back.
        return runCatching { fillFromLink(base, entity.link) }.getOrDefault(base)
    }

    private fun fillFromLink(base: NodeDraft, link: String): NodeDraft {
        val raw = link.trim()
        if (raw.isBlank() || raw == "auto") return base
        return when (base.protocol) {
            "wireguard", "awg" -> fillWireGuard(base, raw)
            "openvpn" -> openVpnDraftFromProfile(base, raw)
            "vmess" -> fillVmess(base, raw)
            "ss" -> fillShadowsocks(base, raw)
            else -> fillUri(base, raw)
        }
    }

    private fun decode(v: String): String =
        runCatching { java.net.URLDecoder.decode(v, "UTF-8") }.getOrDefault(v)

    private fun queryOf(raw: String): Map<String, String> {
        val q = raw.substringAfter('?', "").substringBefore('#')
        if (q.isBlank()) return emptyMap()
        return q.split("&").mapNotNull {
            val k = it.substringBefore('=')
            if (k.isBlank()) null else k.lowercase() to decode(it.substringAfter('=', ""))
        }.toMap()
    }

    private fun fillUri(base: NodeDraft, raw: String): NodeDraft {
        val afterScheme = raw.substringAfter("://")
        val authority = afterScheme.substringBefore('?').substringBefore('#')
        val userinfo = if (authority.contains('@')) authority.substringBeforeLast('@') else ""
        val q = queryOf(raw)
        val security = q["security"] ?: if (q["tls"] == "1") "tls" else "none"
        return base.copy(
            secret = decode(userinfo),
            flow = q["flow"] ?: "",
            network = q["type"] ?: q["net"] ?: base.network,
            security = security,
            path = q["path"] ?: "",
            host = q["host"] ?: "",
            serviceName = q["servicename"] ?: "",
            sni = q["sni"] ?: q["peer"] ?: "",
            alpn = q["alpn"] ?: "",
            fingerprint = q["fp"] ?: "",
            certificateSha256 = q["pinsha256"] ?: q["certificate_public_key_sha256"] ?: "",
            publicKey = q["pbk"] ?: "",
            shortId = q["sid"] ?: "",
            obfs = q["obfs"] ?: "",
            obfsPassword = q["obfs-password"] ?: q["obfs_password"] ?: "",
            congestionControl = q["congestion_control"] ?: base.congestionControl,
            insecure = q["allowinsecure"] == "1" || q["insecure"] == "1" || q["allow_insecure"] == "1"
        )
    }

    private fun fillVmess(base: NodeDraft, raw: String): NodeDraft {
        val payload = raw.substringAfter("://").substringBefore('#').trim()
        val json = JSONObject(String(Base64.getDecoder().decode(padB64(payload)), Charsets.UTF_8))
        return base.copy(
            secret = json.optString("id"),
            network = json.optString("net").ifBlank { base.network },
            security = if (json.optString("tls").isNotBlank()) "tls" else "none",
            path = json.optString("path"),
            host = json.optString("host"),
            sni = json.optString("sni"),
            alpn = json.optString("alpn"),
            fingerprint = json.optString("fp")
                .ifBlank { json.optString("fingerprint") },
            certificateSha256 = json.optString("pinSHA256")
                .ifBlank { json.optString("certificate_public_key_sha256") }
        )
    }

    private fun fillShadowsocks(base: NodeDraft, raw: String): NodeDraft {
        val body = raw.substringAfter("://").substringBefore('#')
        val userinfo = if (body.contains('@')) body.substringBeforeLast('@') else body
        val decoded = runCatching {
            String(Base64.getUrlDecoder().decode(padB64(userinfo)), Charsets.UTF_8)
        }.getOrDefault(decode(userinfo))
        val method = decoded.substringBefore(':', base.method)
        val password = decoded.substringAfter(':', "")
        return base.copy(method = method.ifBlank { base.method }, secret = password)
    }

    private fun fillWireGuard(base: NodeDraft, raw: String): NodeDraft {
        val values = raw.lines().mapNotNull { line ->
            val t = line.trim()
            if (!t.contains('=') || t.startsWith("#") || t.startsWith("[")) null
            else t.substringBefore('=').trim().lowercase() to t.substringAfter('=').trim()
        }.toMap()
        return base.copy(
            secret = values["privatekey"] ?: "",
            publicKey = values["publickey"] ?: "",
            presharedKey = values["presharedkey"] ?: "",
            address = values["address"] ?: "",
            allowedIps = values["allowedips"] ?: base.allowedIps,
            reserved = values["reserved"] ?: "",
            mtu = values["mtu"] ?: "",
            dns = values["dns"] ?: "",
            persistentKeepalive = values["persistentkeepalive"] ?: "",
            jc = values["jc"] ?: "", jmin = values["jmin"] ?: "", jmax = values["jmax"] ?: "",
            s1 = values["s1"] ?: "", s2 = values["s2"] ?: "", s3 = values["s3"] ?: "", s4 = values["s4"] ?: "",
            h1 = values["h1"] ?: "", h2 = values["h2"] ?: "", h3 = values["h3"] ?: "", h4 = values["h4"] ?: "",
            i1 = values["i1"] ?: "", i2 = values["i2"] ?: "", i3 = values["i3"] ?: "",
            i4 = values["i4"] ?: "", i5 = values["i5"] ?: "",
            j1 = values["j1"] ?: "", j2 = values["j2"] ?: "", j3 = values["j3"] ?: "",
            itime = values["itime"] ?: ""
        )
    }

    private fun padB64(v: String): String {
        val clean = v.trim()
        val rem = clean.length % 4
        return if (rem == 0) clean else clean + "=".repeat(4 - rem)
    }

    private fun normalizeProtocol(p: String): String = when (p.lowercase()) {
        "hy", "hysteria" -> "hysteria"
        "hy2", "hysteria2" -> "hysteria2"
        "shadowsocks", "ss" -> "ss"
        "wg" -> "wireguard"
        "socks5" -> "socks"
        else -> p.lowercase()
    }

    fun entityFromDraft(draft: NodeDraft): NodeEntity {
        if (draft.protocol == "auto") {
            return NodeEntity(
                id = draft.id ?: UUID.randomUUID().toString(),
                name = draft.name.ifBlank { "Auto" },
                protocol = "auto",
                server = "",
                port = 0,
                link = "auto",
                outboundJson = "",
                isAutoNode = true
            )
        }
        val link = buildLink(draft)
        val parsed = LinkParser.parseLinksText(link).first.firstOrNull()
            ?: throw IllegalArgumentException("Could not parse node configuration")
        val preservedOutbound = parseStoredOutbound(draft.rawConfig)
        val outbound = if (preservedOutbound == null) {
            parsed.outbound
        } else {
            @Suppress("UNCHECKED_CAST")
            deepMerge(preservedOutbound, parsed.outbound) as Map<String, Any?>
        }
        return NodeEntity(
            id = draft.id ?: UUID.randomUUID().toString(),
            name = draft.name.ifBlank { parsed.name.ifBlank { parsed.server } },
            protocol = parsed.scheme,
            server = parsed.server,
            port = parsed.port,
            link = link,
            outboundJson = LinkParser.toJsonString(outbound),
            isAutoNode = false
        )
    }

    fun buildLink(d: NodeDraft): String {
        val name = enc(d.name.ifBlank { d.server })
        return when (d.protocol) {
            "vless" -> {
                val params = buildParams(
                    "type" to d.network,
                    "encryption" to "none",
                    "security" to d.security.takeIf { it != "none" },
                    "flow" to d.flow,
                    "path" to d.path,
                    "host" to d.host,
                    "serviceName" to d.serviceName,
                    "sni" to d.sni,
                    "alpn" to d.alpn,
                    "fp" to d.fingerprint,
                    "pinSHA256" to d.certificateSha256,
                    "pbk" to d.publicKey,
                    "sid" to d.shortId,
                    "allowInsecure" to if (d.insecure) "1" else null
                )
                "vless://${d.secret.trim()}@${d.server.trim()}:${d.port.trim()}?$params#$name"
            }
            "vmess" -> {
                val json = JSONObject()
                json.put("v", "2")
                json.put("ps", d.name.ifBlank { d.server })
                json.put("add", d.server.trim())
                json.put("port", d.port.trim())
                json.put("id", d.secret.trim())
                json.put("aid", "0")
                json.put("scy", "auto")
                json.put("net", d.network)
                json.put("type", "none")
                json.put("host", d.host)
                json.put("path", d.path)
                json.put("tls", if (d.security == "tls") "tls" else "")
                json.put("sni", d.sni)
                json.put("alpn", d.alpn)
                json.put("fp", d.fingerprint)
                json.put("pinSHA256", d.certificateSha256)
                "vmess://" + b64(json.toString())
            }
            "trojan" -> {
                val params = buildParams(
                    "type" to d.network,
                    "security" to d.security.takeIf { it != "none" },
                    "path" to d.path,
                    "host" to d.host,
                    "serviceName" to d.serviceName,
                    "sni" to d.sni,
                    "alpn" to d.alpn,
                    "fp" to d.fingerprint,
                    "pinSHA256" to d.certificateSha256,
                    "pbk" to d.publicKey,
                    "sid" to d.shortId,
                    "allowInsecure" to if (d.insecure) "1" else null
                )
                "trojan://${encUserinfo(d.secret)}@${d.server.trim()}:${d.port.trim()}?$params#$name"
            }
            "ss" -> "ss://" + b64url("${d.method}:${d.secret}") + "@${d.server.trim()}:${d.port.trim()}#$name"
            "hysteria" -> {
                val params = buildParams(
                    "sni" to d.sni,
                    "pinSHA256" to d.certificateSha256,
                    "insecure" to if (d.insecure) "1" else null,
                    "obfs" to d.obfsPassword.ifBlank { d.obfs }
                )
                "hysteria://${encUserinfo(d.secret)}@${d.server.trim()}:${d.port.trim()}?$params#$name"
            }
            "hysteria2" -> {
                val params = buildParams(
                    "sni" to d.sni,
                    "pinSHA256" to d.certificateSha256,
                    "insecure" to if (d.insecure) "1" else null,
                    "obfs" to d.obfs,
                    "obfs-password" to d.obfsPassword
                )
                "hysteria2://${encUserinfo(d.secret)}@${d.server.trim()}:${d.port.trim()}?$params#$name"
            }
            "tuic" -> {
                val params = buildParams(
                    "sni" to d.sni,
                    "alpn" to d.alpn,
                    "pinSHA256" to d.certificateSha256,
                    "congestion_control" to d.congestionControl,
                    "allow_insecure" to if (d.insecure) "1" else null
                )
                "tuic://${encUserinfo(d.secret)}@${d.server.trim()}:${d.port.trim()}?$params#$name"
            }
            "socks", "http" -> {
                val userinfo = if (d.secret.isBlank()) "" else encUserinfo(d.secret) + "@"
                "${d.protocol}://$userinfo${d.server.trim()}:${d.port.trim()}#$name"
            }
            // masque://<auth token>@<profile id>?sni=&insecure=, the shape LinkParser.parseMasque reads.
            "masque" -> {
                val params = listOf(
                    buildParams(
                        "sni" to d.sni,
                        "insecure" to if (d.insecure) "1" else null
                    ),
                    masqueExtraParams(d.rawConfig)
                ).filter { it.isNotBlank() }.joinToString("&")
                val userinfo = if (d.secret.isBlank()) "" else enc(d.secret.trim()) + "@"
                val query = if (params.isBlank()) "" else "?$params"
                "masque://$userinfo${d.server.trim()}$query#$name"
            }
            "wireguard", "awg" -> buildWireGuardConf(d)
            // Structured fields win; the pasted profile is the fallback.
            "openvpn" -> if (d.ovpnCa.isNotBlank()) openVpnProfileFromDraft(d) else d.rawConfig.ifBlank {
                throw IllegalArgumentException("OpenVPN profile is empty; paste the .ovpn config")
            }
            else -> throw IllegalArgumentException("Unsupported protocol: ${d.protocol}")
        }
    }

    private fun parseStoredOutbound(rawConfig: String): Map<String, Any?>? {
        val raw = rawConfig.trim()
        if (!raw.startsWith("{") || !raw.endsWith("}")) return null
        return runCatching { LinkParser.jsonToMap(JSONObject(raw)) }.getOrNull()
    }

    private fun deepMerge(base: Any?, overlay: Any?): Any? = when {
        base is Map<*, *> && overlay is Map<*, *> -> {
            val merged = linkedMapOf<String, Any?>()
            base.forEach { (key, value) -> key?.toString()?.let { merged[it] = value } }
            overlay.forEach { (key, value) ->
                val textKey = key?.toString() ?: return@forEach
                merged[textKey] = if (merged.containsKey(textKey)) {
                    deepMerge(merged[textKey], value)
                } else {
                    value
                }
            }
            merged
        }
        base is List<*> && overlay is List<*> -> {
            val size = maxOf(base.size, overlay.size)
            List(size) { index ->
                when {
                    index >= overlay.size -> base[index]
                    index >= base.size -> overlay[index]
                    else -> deepMerge(base[index], overlay[index])
                }
            }
        }
        else -> overlay
    }

    /** Query keys the masque editor owns; the WARP extras of the imported link survive as-is. */
    private val MASQUE_MANAGED_PARAMS = setOf(
        "sni", "server_name", "servername", "insecure", "allowinsecure",
        "id", "profile_id", "auth_token", "token"
    )

    private fun masqueExtraParams(rawConfig: String): String {
        val raw = rawConfig.trim()
        if (!raw.startsWith("masque://", ignoreCase = true)) return ""
        val query = raw.substringAfter('?', "").substringBefore('#')
        if (query.isBlank()) return ""
        return query.split("&")
            .filter { it.isNotBlank() && it.substringBefore('=').lowercase() !in MASQUE_MANAGED_PARAMS }
            .joinToString("&")
    }

    /** Every directive the editor owns; anything else is copied over from [NodeDraft.rawConfig]. */
    private val WG_MANAGED_KEYS = setOf(
        "privatekey", "address", "mtu", "dns",
        "jc", "jmin", "jmax", "s1", "s2", "s3", "s4",
        "h1", "h2", "h3", "h4", "i1", "i2", "i3", "i4", "i5", "j1", "j2", "j3", "itime",
        "publickey", "presharedkey", "allowedips", "endpoint", "reserved", "persistentkeepalive"
    )

    /**
     * [Interface]/[Peer] lines of the imported profile the editor does not model,
     * so re-saving a node can never drop an unknown directive.
     */
    private fun wireGuardExtras(rawConfig: String): Map<String, List<String>> {
        if (!rawConfig.contains("[Interface]", ignoreCase = true)) return emptyMap()
        val extras = mutableMapOf<String, MutableList<String>>()
        var section = ""
        rawConfig.lines().forEach { rawLine ->
            val line = rawLine.trim()
            if (line.isEmpty() || line.startsWith("#") || line.startsWith(";")) return@forEach
            if (line.startsWith("[") && line.endsWith("]")) {
                section = line.substring(1, line.length - 1).trim().lowercase()
                return@forEach
            }
            if (section != "interface" && section != "peer") return@forEach
            if (!line.contains('=')) return@forEach
            val key = line.substringBefore('=').trim().lowercase()
            if (key in WG_MANAGED_KEYS) return@forEach
            extras.getOrPut(section) { mutableListOf() }.add(line)
        }
        return extras
    }

    private fun buildWireGuardConf(d: NodeDraft): String {
        val extras = wireGuardExtras(d.rawConfig)
        val sb = StringBuilder()
        sb.appendLine("# ${d.name.ifBlank { d.server }}")
        sb.appendLine("[Interface]")
        sb.appendLine("PrivateKey = ${d.secret.trim()}")
        if (d.address.isNotBlank()) sb.appendLine("Address = ${d.address.trim()}")
        if (d.mtu.isNotBlank()) sb.appendLine("MTU = ${d.mtu.trim()}")
        if (d.dns.isNotBlank()) sb.appendLine("DNS = ${d.dns.trim()}")
        if (d.protocol == "awg") {
            if (d.jc.isNotBlank()) sb.appendLine("Jc = ${d.jc.trim()}")
            if (d.jmin.isNotBlank()) sb.appendLine("Jmin = ${d.jmin.trim()}")
            if (d.jmax.isNotBlank()) sb.appendLine("Jmax = ${d.jmax.trim()}")
            if (d.s1.isNotBlank()) sb.appendLine("S1 = ${d.s1.trim()}")
            if (d.s2.isNotBlank()) sb.appendLine("S2 = ${d.s2.trim()}")
            if (d.s3.isNotBlank()) sb.appendLine("S3 = ${d.s3.trim()}")
            if (d.s4.isNotBlank()) sb.appendLine("S4 = ${d.s4.trim()}")
            if (d.h1.isNotBlank()) sb.appendLine("H1 = ${d.h1.trim()}")
            if (d.h2.isNotBlank()) sb.appendLine("H2 = ${d.h2.trim()}")
            if (d.h3.isNotBlank()) sb.appendLine("H3 = ${d.h3.trim()}")
            if (d.h4.isNotBlank()) sb.appendLine("H4 = ${d.h4.trim()}")
            if (d.i1.isNotBlank()) sb.appendLine("I1 = ${d.i1.trim()}")
            if (d.i2.isNotBlank()) sb.appendLine("I2 = ${d.i2.trim()}")
            if (d.i3.isNotBlank()) sb.appendLine("I3 = ${d.i3.trim()}")
            if (d.i4.isNotBlank()) sb.appendLine("I4 = ${d.i4.trim()}")
            if (d.i5.isNotBlank()) sb.appendLine("I5 = ${d.i5.trim()}")
            if (d.j1.isNotBlank()) sb.appendLine("J1 = ${d.j1.trim()}")
            if (d.j2.isNotBlank()) sb.appendLine("J2 = ${d.j2.trim()}")
            if (d.j3.isNotBlank()) sb.appendLine("J3 = ${d.j3.trim()}")
            if (d.itime.isNotBlank()) sb.appendLine("Itime = ${d.itime.trim()}")
        }
        extras["interface"]?.forEach { sb.appendLine(it) }
        sb.appendLine()
        sb.appendLine("[Peer]")
        sb.appendLine("PublicKey = ${d.publicKey.trim()}")
        if (d.presharedKey.isNotBlank()) sb.appendLine("PresharedKey = ${d.presharedKey.trim()}")
        sb.appendLine("AllowedIPs = ${d.allowedIps.ifBlank { "0.0.0.0/0" }}")
        sb.appendLine("Endpoint = ${d.server.trim()}:${d.port.trim()}")
        if (d.persistentKeepalive.isNotBlank()) sb.appendLine("PersistentKeepalive = ${d.persistentKeepalive.trim()}")
        if (d.reserved.isNotBlank()) sb.appendLine("Reserved = ${d.reserved.trim()}")
        extras["peer"]?.forEach { sb.appendLine(it) }
        return sb.toString()
    }

    private fun enc(v: String): String = URLEncoder.encode(v, "UTF-8").replace("+", "%20")

    /** Encodes each userinfo segment separately so "user:pass" keeps its colon. */
    private fun encUserinfo(v: String): String =
        v.trim().split(":", limit = 2).joinToString(":") { enc(it) }

    private fun b64(v: String): String =
        Base64.getEncoder().encodeToString(v.toByteArray(Charsets.UTF_8))

    private fun b64url(v: String): String =
        Base64.getUrlEncoder().withoutPadding().encodeToString(v.toByteArray(Charsets.UTF_8))

    private fun buildParams(vararg pairs: Pair<String, String?>): String =
        pairs.filter { !it.second.isNullOrBlank() }
            .joinToString("&") { "${it.first}=${enc(it.second!!.trim())}" }
}
