package com.lumen.app.util

import com.lumen.core.config.parser.LinkParser
import com.lumen.core.database.model.NodeEntity
import com.lumen.ui.screens.NodeDraft
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
        val base = NodeDraft(
            id = entity.id,
            name = entity.name,
            protocol = normalizeProtocol(entity.protocol),
            server = entity.server,
            port = entity.port.toString(),
            rawConfig = entity.link
        )
        // Editing must expose every protocol parameter, so the stored link is parsed back.
        return runCatching { fillFromLink(base, entity.link) }.getOrDefault(base)
    }

    private fun fillFromLink(base: NodeDraft, link: String): NodeDraft {
        val raw = link.trim()
        if (raw.isBlank() || raw == "auto") return base
        return when (base.protocol) {
            "wireguard", "awg" -> fillWireGuard(base, raw)
            "openvpn" -> base
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
            jc = values["jc"] ?: "", jmin = values["jmin"] ?: "", jmax = values["jmax"] ?: "",
            s1 = values["s1"] ?: "", s2 = values["s2"] ?: "", s3 = values["s3"] ?: "", s4 = values["s4"] ?: ""
        )
    }

    private fun padB64(v: String): String {
        val clean = v.trim()
        val rem = clean.length % 4
        return if (rem == 0) clean else clean + "=".repeat(4 - rem)
    }

    private fun normalizeProtocol(p: String): String = when (p.lowercase()) {
        "hy", "hy2", "hysteria", "hysteria2" -> "hysteria2"
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
        return NodeEntity(
            id = draft.id ?: UUID.randomUUID().toString(),
            name = draft.name.ifBlank { parsed.name.ifBlank { parsed.server } },
            protocol = parsed.scheme,
            server = parsed.server,
            port = parsed.port,
            link = link,
            outboundJson = "",
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
                    "pbk" to d.publicKey,
                    "sid" to d.shortId,
                    "allowInsecure" to if (d.insecure) "1" else null
                )
                "trojan://${encUserinfo(d.secret)}@${d.server.trim()}:${d.port.trim()}?$params#$name"
            }
            "ss" -> "ss://" + b64url("${d.method}:${d.secret}") + "@${d.server.trim()}:${d.port.trim()}#$name"
            "hysteria2" -> {
                val params = buildParams(
                    "sni" to d.sni,
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
                    "congestion_control" to d.congestionControl,
                    "allow_insecure" to if (d.insecure) "1" else null
                )
                "tuic://${encUserinfo(d.secret)}@${d.server.trim()}:${d.port.trim()}?$params#$name"
            }
            "socks", "http" -> {
                val userinfo = if (d.secret.isBlank()) "" else encUserinfo(d.secret) + "@"
                "${d.protocol}://$userinfo${d.server.trim()}:${d.port.trim()}#$name"
            }
            "wireguard", "awg" -> buildWireGuardConf(d)
            "openvpn" -> d.rawConfig
            else -> throw IllegalArgumentException("Unsupported protocol: ${d.protocol}")
        }
    }

    private fun buildWireGuardConf(d: NodeDraft): String {
        val sb = StringBuilder()
        sb.appendLine("# ${d.name.ifBlank { d.server }}")
        sb.appendLine("[Interface]")
        sb.appendLine("PrivateKey = ${d.secret.trim()}")
        if (d.address.isNotBlank()) sb.appendLine("Address = ${d.address.trim()}")
        if (d.protocol == "awg") {
            if (d.jc.isNotBlank()) sb.appendLine("Jc = ${d.jc.trim()}")
            if (d.jmin.isNotBlank()) sb.appendLine("Jmin = ${d.jmin.trim()}")
            if (d.jmax.isNotBlank()) sb.appendLine("Jmax = ${d.jmax.trim()}")
            if (d.s1.isNotBlank()) sb.appendLine("S1 = ${d.s1.trim()}")
            if (d.s2.isNotBlank()) sb.appendLine("S2 = ${d.s2.trim()}")
            if (d.s3.isNotBlank()) sb.appendLine("S3 = ${d.s3.trim()}")
            if (d.s4.isNotBlank()) sb.appendLine("S4 = ${d.s4.trim()}")
        }
        sb.appendLine()
        sb.appendLine("[Peer]")
        sb.appendLine("PublicKey = ${d.publicKey.trim()}")
        if (d.presharedKey.isNotBlank()) sb.appendLine("PresharedKey = ${d.presharedKey.trim()}")
        sb.appendLine("AllowedIPs = ${d.allowedIps.ifBlank { "0.0.0.0/0" }}")
        sb.appendLine("Endpoint = ${d.server.trim()}:${d.port.trim()}")
        if (d.reserved.isNotBlank()) sb.appendLine("Reserved = ${d.reserved.trim()}")
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
