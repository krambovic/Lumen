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
        "mux-xudp-connections", "mux-quic", "exclude-routes", "announce"
    )

    private val placeholderMarkers = listOf(
        "client not supported", "unsupported client", "client is not supported",
        "app not supported", "unsupported app", "application is not supported",
        "update your app", "update your client", "use another client",
        // Russian equivalents: "client not supported", "unsupported client",
        // "app not supported", "update the app", "update the client", "use another client".
        "\u043a\u043b\u0438\u0435\u043d\u0442 \u043d\u0435 \u043f\u043e\u0434\u0434\u0435\u0440\u0436\u0438\u0432\u0430\u0435\u0442\u0441\u044f",
        "\u043d\u0435\u043f\u043e\u0434\u0434\u0435\u0440\u0436\u0438\u0432\u0430\u0435\u043c\u044b\u0439 \u043a\u043b\u0438\u0435\u043d\u0442",
        "\u043f\u0440\u0438\u043b\u043e\u0436\u0435\u043d\u0438\u0435 \u043d\u0435 \u043f\u043e\u0434\u0434\u0435\u0440\u0436\u0438\u0432\u0430\u0435\u0442\u0441\u044f",
        "\u043e\u0431\u043d\u043e\u0432\u0438\u0442\u0435 \u043f\u0440\u0438\u043b\u043e\u0436\u0435\u043d\u0438\u0435",
        "\u043e\u0431\u043d\u043e\u0432\u0438\u0442\u0435 \u043a\u043b\u0438\u0435\u043d\u0442",
        "\u0438\u0441\u043f\u043e\u043b\u044c\u0437\u0443\u0439\u0442\u0435 \u0434\u0440\u0443\u0433\u043e\u0439 \u043a\u043b\u0438\u0435\u043d\u0442"
    )

    /** Lumen's own subscription User-Agent; kept in the same shape as the desktop build. */
    internal val lumenUserAgent: String
        get() = "Lumen-Subscription/Android-${net.kramb.lumen.BuildConfig.VERSION_NAME}"

    /**
     * User-Agent fallback order. A user configured UA always wins; otherwise Lumen
     * asks as itself first and only falls back to the compatibility profiles when the
     * panel answers with a stub, no usable servers or a profile dependent status code.
     */
    internal fun clientProfiles(customUserAgent: String? = null): List<Pair<String, String>> = buildList {
        if (!customUserAgent.isNullOrBlank()) add("Custom" to customUserAgent)
        add("Lumen Android" to lumenUserAgent)
        add("Happ compatible" to "Happ/2.18.3/Windows/2606241603601")
        add("v2rayNG" to "v2rayNG/1.9.16")
        add("SFA" to "SFA/1.11.0")
        add("Streisand" to "Streisand/1.6.40")
        add("Clash Meta" to "clash.meta")
        add("Generic" to "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/126.0 Mobile Safari/537.36")
    }

    /**
     * Detects "stub" responses that panels return to unknown clients
     * (for example, a single fake node named "client not supported").
     */
    internal fun looksLikePlaceholder(body: String): Boolean {
        if (body.isBlank()) return true
        val decoded = runCatching {
            java.net.URLDecoder.decode(body, "UTF-8")
        }.getOrDefault(body)
        val haystack = (body + "\n" + decoded).lowercase(Locale.US)
        return placeholderMarkers.any { haystack.contains(it) }
    }

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
            // A happ payload is not something the user typed, so it may not point the
            // subscription (token in the path, X-Hwid header) at a plaintext endpoint.
            require(!decrypted.startsWith("http://")) { "Happ subscription URL must use HTTPS" }
            if (!decrypted.startsWith("https://")) {
                val normalized = normalize(decrypted, emptyMap())
                return normalized.copy(clientProfile = "Happ crypt")
            }
            target = decrypted
        }
        require(target.startsWith("http://") || target.startsWith("https://")) { "Subscription URL must use HTTP(S)" }

        val profiles = clientProfiles(customUserAgent)
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
                    if (nodes.isEmpty()) throw IOException("No supported servers in response")
                    if (nodes.size <= 2 && looksLikePlaceholder(normalized.body)) {
                        // The panel answered with a stub for this client profile; try the next User-Agent.
                        throw IOException("Subscription returned a placeholder for the \"$profile\" profile")
                    }
                    return normalized
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

    internal fun normalize(body: String, headers: Map<String, String>): SubscriptionPayload {
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
                        // takeIf { it > 0 } used to drop the two values a panel sends as a
                        // deliberate zero: total = 0 (unlimited plan) and expire = 0 (never
                        // expires). Both have to reach the caller so it can tell them apart
                        // from a field the panel did not send at all.
                        if (!user.has(key) || user.isNull(key)) return@forEach
                        val raw = user.optLong(key, -1L)
                        if (key == "expire") userInfo[key] = normalizeExpireSeconds(raw)
                        else if (raw >= 0L) userInfo[key] = raw
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

    /** 9999-12-31T23:59:59Z: no plausible expiry in seconds is ever past this. */
    private const val MAX_EXPIRE_SECONDS = 253_402_300_799L

    /**
     * `expire` is a UNIX timestamp in **seconds**. Panels that send milliseconds (and the
     * odd one that sends microseconds) used to render as a date tens of thousands of years
     * out, so anything that cannot be a seconds timestamp is rescaled until it can be.
     * Non-positive values mean "never expires" and are normalised to 0.
     */
    internal fun normalizeExpireSeconds(value: Long): Long {
        if (value <= 0L) return 0L
        var seconds = value
        while (seconds > MAX_EXPIRE_SECONDS) seconds /= 1000L
        return seconds
    }

    /**
     * Parses `subscription-userinfo`: `upload=..; download=..; total=..; expire=..`.
     *
     * A key the panel did not send stays absent from the result — the caller keeps its
     * last known value for it instead of resetting it to 0 — while a key sent as 0
     * (unlimited traffic, no expiry) is kept.
     */
    internal fun parseUserInfo(value: String): Map<String, Long> =
        value.split(';', ',').mapNotNull { part ->
            val key = part.substringBefore('=', "").trim().lowercase(Locale.US)
            val number = part.substringAfter('=', "").trim().toLongOrNull()
            if (key.isBlank() || number == null) return@mapNotNull null
            when {
                // "expire=-1" is the other spelling of "no expiry".
                key == "expire" -> key to normalizeExpireSeconds(number)
                number < 0L -> null
                else -> key to number
            }
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

    // The subscription URL carries the subscriber token and the X-Hwid header, so a
    // provider supplied replacement is only honoured over TLS.
    internal fun premiumUrl(premium: Map<String, String>): String? {
        premium["new-url"]?.trim()?.let { replacement ->
            if (replacement.startsWith("https://")) return replacement
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