package com.lumen.app.subscription

import com.lumen.core.config.crypto.HappCrypt
import com.lumen.core.config.parser.LinkParser
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL
import java.util.Locale
import java.util.zip.GZIPInputStream

internal data class SubscriptionPayload(
    val body: String,
    val premiumFeatures: Map<String, String>,
    val userInfo: Map<String, Long>,
    val profileTitle: String?,
    val updateIntervalHours: Int?,
    val effectiveUrl: String?,
    val clientProfile: String
)

internal object SubscriptionClient {
    private const val MAX_BYTES = 8 * 1024 * 1024
    private class PermanentSubscriptionException(message: String) : IOException(message)
    private val premiumKeys = setOf(
        "new-url", "new-domain", "subscription-always-hwid-enable",
        "notification-subs-expire", "subscription-autoconnect",
        "subscription-auto-update-enable", "fragmentation-enable",
        "fragmentation-packets", "fragmentation-length", "fragmentation-interval",
        "ping-type", "change-user-agent", "per-app-proxy-mode",
        "per-app-proxy-list", "sniffing-enable", "subscriptions-collapse",
        "ping-result", "mux-enable", "mux-tcp-connections",
        "mux-xudp-connections", "mux-quic", "exclude-routes"
    )

    fun fetch(
        rawUrl: String,
        hwid: String?,
        customUserAgent: String? = null,
        direct: Boolean = true
    ): SubscriptionPayload {
        require(hwid == null || (hwid.length <= 256 && '\r' !in hwid && '\n' !in hwid)) { "Invalid HWID" }
        require(customUserAgent == null ||
            (customUserAgent.length <= 256 && '\r' !in customUserAgent && '\n' !in customUserAgent)
        ) { "Invalid User-Agent" }
        var target = rawUrl.trim()
        if (HappCrypt.isHappCryptLink(target)) {
            val decrypted = HappCrypt.decryptHappLink(target).trim()
            if (!decrypted.startsWith("http://") && !decrypted.startsWith("https://")) {
                val normalized = normalize(decrypted, emptyMap())
                return normalized.copy(clientProfile = "Happ crypt")
            }
            target = decrypted
        }
        require(target.startsWith("http://") || target.startsWith("https://")) { "Subscription URL must use HTTP(S)" }

        val profiles = buildList {
            if (!customUserAgent.isNullOrBlank()) add("Custom" to customUserAgent)
            add("Lumen Android" to "Lumen-Subscription/Android-1.0")
            add("SFA" to "SFA/1.11.0")
            add("Clash Meta" to "clash.meta")
            add("Happ compatible" to "Happ/2.18.3/Windows/2606241603601")
        }
        var lastError: Throwable? = null
        for ((profile, userAgent) in profiles) {
            try {
                val conn = URL(target).openConnection() as HttpURLConnection
                conn.connectTimeout = 15_000
                conn.readTimeout = 20_000
                conn.instanceFollowRedirects = true
                conn.setRequestProperty("User-Agent", userAgent)
                conn.setRequestProperty("Accept", "text/yaml,application/yaml,application/json,text/plain,*/*")
                conn.setRequestProperty("Accept-Encoding", "gzip")
                conn.setRequestProperty("Profile-Update-Interval", "24")
                if (!hwid.isNullOrBlank()) conn.setRequestProperty("X-Hwid", hwid)
                if (direct) conn.setRequestProperty("X-Lumen-Route", "direct")
                try {
                    val code = conn.responseCode
                    val rawStream = if (code in 200..299) conn.inputStream else conn.errorStream
                    val stream = rawStream?.let {
                        if (conn.getHeaderField("Content-Encoding")?.contains("gzip", ignoreCase = true) == true) {
                            GZIPInputStream(it)
                        } else it
                    }
                    val bytes = stream?.use { input ->
                        val output = ByteArrayOutputStream()
                        val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                        var total = 0
                        while (true) {
                            val count = input.read(buffer)
                            if (count < 0) break
                            total += count
                            if (total > MAX_BYTES) throw IOException("Subscription exceeds 8 MiB")
                            output.write(buffer, 0, count)
                        }
                        output.toByteArray()
                    } ?: ByteArray(0)
                    val body = bytes.toString(Charsets.UTF_8)
                    if (code !in 200..299) {
                        val message = "HTTP $code: ${body.take(160)}"
                        if (code in setOf(404, 410)) throw PermanentSubscriptionException(message)
                        // 400/401/403/422 can depend on the client profile. Try the next User-Agent.
                        throw IOException(message)
                    }
                    val headers = conn.headerFields
                        .filterKeys { it != null }
                        .mapKeys { it.key.lowercase(Locale.US) }
                        .mapValues { it.value.firstOrNull().orEmpty() }
                    val normalized = normalize(body, headers).copy(clientProfile = profile)
                    val (nodes, _) = runCatching { LinkParser.parseLinksText(normalized.body) }
                        .getOrDefault(Pair(emptyList(), emptyList()))
                    if (nodes.isNotEmpty()) return normalized
                    throw IOException("No supported servers in response")
                } finally {
                    conn.disconnect()
                }
            } catch (error: PermanentSubscriptionException) {
                throw error
            } catch (error: Throwable) {
                lastError = error
            }
        }
        throw IOException(lastError?.message ?: "Subscription download failed", lastError)
    }

    private fun normalize(body: String, headers: Map<String, String>): SubscriptionPayload {
        var linksBody = body.trim()
        val premium = linkedMapOf<String, String>()
        val userInfo = parseUserInfo(headers["subscription-userinfo"].orEmpty()).toMutableMap()
        var title = decodeHeader(headers["profile-title"])
        val interval = headers["profile-update-interval"]?.trim()?.toIntOrNull()
        premiumKeys.forEach { key -> headers[key]?.takeIf { it.isNotBlank() }?.let { premium[key] = it } }

        if (linksBody.startsWith("{")) {
            runCatching {
                val json = JSONObject(linksBody)
                json.optJSONObject("premiumFeatures")?.let { nested ->
                    nested.keys().forEach { premium[it] = nested.optString(it) }
                }
                premiumKeys.forEach { key -> if (json.has(key)) premium[key] = json.optString(key) }
                if (title.isNullOrBlank()) {
                    title = json.optString("profileTitle").ifBlank { json.optString("subscriptionName") }.ifBlank { null }
                }
                json.optJSONObject("user")?.let { user ->
                    listOf("upload", "download", "total", "expire").forEach { key ->
                        if (user.has(key)) user.optLong(key).takeIf { it > 0 }?.let { userInfo[key] = it }
                    }
                }
                json.optJSONArray("links")?.let { links ->
                    linksBody = buildString {
                        for (index in 0 until links.length()) appendLine(links.optString(index))
                    }.trim()
                }
            }
        }

        val kept = mutableListOf<String>()
        linksBody.lines().forEach { line ->
            val match = Regex("^\\s*#\\s*([A-Za-z0-9_-]+)\\s*:?[ \\t]*(.*?)\\s*$").matchEntire(line)
            if (match == null) {
                kept += line
            } else {
                val key = match.groupValues[1].lowercase(Locale.US).replace('_', '-')
                val value = match.groupValues[2].trim()
                when {
                    key in premiumKeys -> premium[key] = value
                    key == "profile-title" && title.isNullOrBlank() -> title = decodeHeader(value)
                    else -> kept += line
                }
            }
        }
        linksBody = kept.joinToString("\n").trim()

        return SubscriptionPayload(
            body = linksBody,
            premiumFeatures = premium,
            userInfo = userInfo,
            profileTitle = title,
            updateIntervalHours = interval,
            effectiveUrl = premiumUrl(premium),
            clientProfile = ""
        )
    }

    private fun parseUserInfo(value: String): Map<String, Long> = value.split(';').mapNotNull { part ->
        val key = part.substringBefore('=', "").trim().lowercase(Locale.US)
        val number = part.substringAfter('=', "").trim().toLongOrNull()
        if (key.isBlank() || number == null) null else key to number
    }.toMap()

    private fun decodeHeader(value: String?): String? {
        val text = value?.trim().orEmpty()
        if (text.isBlank()) return null
        if (!text.startsWith("base64:", ignoreCase = true)) return text
        return runCatching {
            val raw = text.substringAfter(':')
            val padding = "=".repeat((4 - raw.length % 4) % 4)
            String(java.util.Base64.getDecoder().decode(raw + padding), Charsets.UTF_8).trim()
        }.getOrDefault(text)
    }

    private fun premiumUrl(premium: Map<String, String>): String? {
        premium["new-url"]?.trim()?.let { replacement ->
            if (replacement.startsWith("https://") || replacement.startsWith("http://")) return replacement
        }
        return null
    }

    fun replaceDomain(sourceUrl: String, newDomain: String?): String? {
        if (newDomain.isNullOrBlank()) return null
        return runCatching {
            val source = URI(sourceUrl)
            val domain = URI("//${newDomain.trim()}")
            if (source.scheme !in setOf("http", "https") || domain.host.isNullOrBlank()) return null
            URI(source.scheme, source.userInfo, domain.host, domain.port, source.path, source.query, source.fragment).toString()
        }.getOrNull()
    }
}