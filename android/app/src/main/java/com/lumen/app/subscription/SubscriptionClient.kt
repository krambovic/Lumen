package com.lumen.app.subscription

import com.lumen.core.config.crypto.HappCrypt
import com.lumen.core.config.parser.LinkParser
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.HttpURLConnection
import java.net.InetSocketAddress
import java.net.Proxy
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
    val clientProfile: String,
    val metadata: SubscriptionMetadata = SubscriptionMetadata()
)

/**
 * Everything the panel says about the subscription itself, decoded and typed.
 *
 * A null field means "the panel did not send it in this response" — never "the panel
 * cleared it". The caller keeps its last known value for those, the same rule the
 * `subscription-userinfo` figures already follow.
 */
internal data class SubscriptionMetadata(
    val description: String? = null,
    val announce: String? = null,
    val announceUrl: String? = null,
    /** Happ's own header, absent from the incy spec; the reference client's "Channel / Bot". */
    val telegramUrl: String? = null,
    val supportUrl: String? = null,
    val supportEmail: String? = null,
    val websiteUrl: String? = null,
    val premiumUrl: String? = null,
    val bannerText: String? = null,
    val bannerButtonText: String? = null,
    val bannerButtonUrl: String? = null,
    val bannerBgColor: String? = null,
    val bannerButtonColor: String? = null,
    val hideUrl: Boolean? = null,
    val sortOrder: String? = null
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
        direct: Boolean = true,
        allowHttp: Boolean = false,
        proxyPort: Int? = null
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
        require(allowHttp || target.startsWith("https://")) {
            "HTTP subscription links are disabled in Subscription settings"
        }

        val profiles = clientProfiles(customUserAgent)
        var lastError: Throwable? = null
        for ((profile, userAgent) in profiles) {
            try {
                val connectionProxy = proxyPort?.takeIf { it in 1..65535 }?.let {
                    Proxy(Proxy.Type.SOCKS, InetSocketAddress("127.0.0.1", it))
                }
                val conn = (connectionProxy?.let { URL(target).openConnection(it) }
                    ?: URL(target).openConnection()) as HttpURLConnection
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
                    require(allowHttp || conn.url.protocol.equals("https", true)) {
                        "Subscription redirect to HTTP is disabled in Subscription settings"
                    }
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

    internal fun normalize(body: String, rawHeaders: Map<String, String>): SubscriptionPayload {
        var linksBody = body.trim()
        val premium = linkedMapOf<String, String>()
        // Header names are case-insensitive and panels spell them with either separator.
        val headers = rawHeaders.entries.associate { (key, value) ->
            key.trim().lowercase(Locale.US).replace('_', '-') to value.trim()
        }
        val userInfo = parseUserInfo(headers["subscription-userinfo"].orEmpty()).toMutableMap()
        var title = decodeHeader(headers.pick("profile-title", "subscription-name"))
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

        // The body may carry the same parameters as `# key: value` comments. They are
        // collected separately so the HTTP headers can win, which is what the spec says.
        val kept = mutableListOf<String>()
        val bodyMeta = linkedMapOf<String, String>()
        linksBody.lines().forEach { line ->
            val match = Regex("^\\s*#\\s*([A-Za-z0-9_-]+)\\s*:?[ \\t]*(.*?)\\s*$").matchEntire(line)
            if (match == null) {
                kept += line
                return@forEach
            }
            val key = match.groupValues[1].lowercase(Locale.US).replace('_', '-')
            val value = match.groupValues[2].trim()
            var consumed = false
            if (key in premiumKeys) {
                premium[key] = value
                consumed = true
            }
            if (key in metadataKeys) {
                if (value.isNotBlank()) bodyMeta[key] = value
                consumed = true
            }
            if (!consumed) kept += line
        }
        linksBody = kept.joinToString("\n").trim()

        if (title.isNullOrBlank()) {
            title = decodeHeader(bodyMeta.pick("profile-title", "subscription-name"))
        }
        // Last resort name: the filename the panel attached the response under.
        if (title.isNullOrBlank()) title = titleFromContentDisposition(headers["content-disposition"])
        val interval = headers.pick("profile-update-interval")?.toIntOrNull()
            ?: bodyMeta["profile-update-interval"]?.toIntOrNull()
        // Kept in the premium map for compatibility, but decoded like every other
        // announcement: it used to reach the card as a raw `base64:...` string.
        premium["announce"]?.let { raw -> premium["announce"] = decodeHeader(raw) ?: raw }

        val metaSource = linkedMapOf<String, String>()
        bodyMeta.forEach { (key, value) -> metaSource[key] = value }
        metadataKeys.forEach { key -> headers[key]?.takeIf { it.isNotBlank() }?.let { metaSource[key] = it } }

        return SubscriptionPayload(
            body = linksBody,
            premiumFeatures = premium,
            userInfo = userInfo,
            profileTitle = title,
            updateIntervalHours = interval,
            effectiveUrl = premiumUrl(premium),
            clientProfile = "",
            metadata = buildMetadata(metaSource)
        )
    }

    /**
     * Header names the panel may send as subscription metadata, including the documented
     * alternative spellings. Everything here is also recognised as a body comment.
     */
    private val metadataKeys = setOf(
        "profile-title", "subscription-name", "profile-description",
        "announce", "announcement", "announce-url", "announcement-url",
        // telegram-url is Happ's own, absent from the incy spec: the "Channel / Bot"
        // button. Without it here the whitelist below drops it before buildMetadata.
        "telegram-url", "telegram",
        "support-url", "support", "support-email",
        "profile-web-page-url", "homepage", "premium-url",
        "banner-text", "banner-button-text", "banner-button-url",
        "banner-bg-color", "banner-button-color",
        "hide-url", "sort-order", "profile-update-interval"
    )

    private val sortOrders = setOf("ping", "name", "none")

    /** First of [names] present with a non-blank value; the map is already lowercased. */
    private fun Map<String, String>.pick(vararg names: String): String? =
        names.firstNotNullOfOrNull { name -> this[name]?.trim()?.takeIf { it.isNotBlank() } }

    internal fun buildMetadata(source: Map<String, String>): SubscriptionMetadata = SubscriptionMetadata(
        description = decodeHeader(source.pick("profile-description")),
        announce = decodeHeader(source.pick("announce", "announcement")),
        announceUrl = webUrl(source.pick("announce-url", "announcement-url")),
        telegramUrl = webUrl(source.pick("telegram-url", "telegram")),
        supportUrl = webUrl(source.pick("support-url", "support")),
        supportEmail = source.pick("support-email")
            ?.removePrefix("mailto:")?.trim()?.takeIf { '@' in it && it.length <= 254 },
        websiteUrl = webUrl(source.pick("profile-web-page-url", "homepage")),
        premiumUrl = webUrl(source.pick("premium-url")),
        bannerText = decodeHeader(source.pick("banner-text")),
        bannerButtonText = decodeHeader(source.pick("banner-button-text")),
        bannerButtonUrl = webUrl(source.pick("banner-button-url")),
        bannerBgColor = hexColor(source.pick("banner-bg-color")),
        bannerButtonColor = hexColor(source.pick("banner-button-color")),
        hideUrl = source.pick("hide-url")?.let { parseFlag(it) },
        sortOrder = source.pick("sort-order")?.lowercase(Locale.US)?.takeIf { it in sortOrders }
    )

    /** Provider supplied links are opened by the user, so only http(s) is accepted. */
    /**
     * Desktop stores these links verbatim, so panels get away with sending
     * `t.me/support` or `tg://resolve?domain=x`. Requiring an explicit http(s)
     * prefix silently dropped both and left the buttons missing on Android only.
     * A bare host is promoted to https; anything that is not link-shaped is still
     * rejected rather than handed to an intent.
     */
    private fun webUrl(value: String?): String? {
        val text = value?.trim().orEmpty()
        if (text.isBlank() || text.length > 2048) return null
        if (text.any(Char::isWhitespace)) return null
        if (text.startsWith("http://", true) || text.startsWith("https://", true)) return text
        // Telegram's own scheme: the card opens it with an intent, and the opener
        // falls back when no app can handle it.
        if (text.startsWith("tg://", true)) return text
        if (text.startsWith("@") && text.length > 1) return "https://t.me/${text.drop(1)}"
        // Scheme-less "host/path": needs a dot and must not look like another scheme.
        if (text.contains("://") || text.contains('.').not()) return null
        val host = text.substringBefore('/')
        if (host.isBlank() || host.startsWith('.') || host.endsWith('.')) return null
        return "https://$text"
    }

    private val hexColorRegex = Regex("^#?([0-9A-Fa-f]{6})$")

    private fun hexColor(value: String?): String? =
        value?.trim()?.let { hexColorRegex.find(it) }?.let { "#" + it.groupValues[1].uppercase(Locale.US) }

    /** null keeps the caller's stored value: an unparseable flag is not a "false". */
    private fun parseFlag(value: String): Boolean? = when (value.trim().lowercase(Locale.US)) {
        "1", "true", "yes", "on", "enabled" -> true
        "0", "false", "no", "off", "disabled" -> false
        else -> null
    }

    private val dispositionFilename =
        Regex("filename\\*?=(?:UTF-8''|\")?([^\";]+)", RegexOption.IGNORE_CASE)
    private val dispositionSuffix = Regex("\\.(?:ya?ml|json|txt|conf)$", RegexOption.IGNORE_CASE)
    private val genericFilenames = setOf("config", "subscription", "download")

    internal fun titleFromContentDisposition(value: String?): String? {
        val header = value?.trim().orEmpty()
        if (header.isBlank()) return null
        val raw = dispositionFilename.find(header)?.groupValues?.get(1)?.trim()?.trim('"') ?: return null
        val decoded = runCatching { java.net.URLDecoder.decode(raw, "UTF-8") }.getOrDefault(raw)
        val candidate = dispositionSuffix.replace(decoded, "").trim()
        if (candidate.isBlank() || candidate.all { it.isDigit() }) return null
        if (candidate.lowercase(Locale.US) in genericFilenames) return null
        return candidate.take(160)
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

    private const val BASE64_PREFIX = "base64:"
    private val standardBase64 = Regex("^[A-Za-z0-9+/]+$")

    /**
     * Decodes a header the spec marks "supports base64".
     *
     * Only a value that actually carries the (case-insensitive) `base64:` prefix is
     * decoded — a plain value that merely looks like base64 is a name, not a payload.
     * The payload itself may use the standard or the URL-safe alphabet, may be wrapped
     * or unpadded, and anything that fails to decode falls back to the raw string so a
     * malformed announcement shows the provider's text instead of an empty card.
     */
    internal fun decodeHeader(value: String?): String? {
        val text = value?.trim().orEmpty()
        if (text.isBlank()) return null
        if (!text.startsWith(BASE64_PREFIX, ignoreCase = true)) return text
        val payload = text.substring(BASE64_PREFIX.length)
            .filterNot { it.isWhitespace() }
            .map { if (it == '-') '+' else if (it == '_') '/' else it }
            .joinToString("")
            .trimEnd('=')
        if (!standardBase64.matches(payload)) return text
        return runCatching {
            val padded = payload + "=".repeat((4 - payload.length % 4) % 4)
            String(java.util.Base64.getDecoder().decode(padded), Charsets.UTF_8).trim()
        }.getOrNull()?.takeIf { it.isNotBlank() } ?: text
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
