package com.lumen.app.subscription

import com.lumen.core.config.crypto.HappCrypt
import com.lumen.core.config.parser.LinkParser
import org.json.JSONObject
import java.io.IOException
import java.net.URI
import java.net.URL
import java.util.Locale

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
        proxyPort: Int? = null,
        proxyUsername: String? = null,
        proxyPassword: String? = null,
        cancelled: () -> Boolean = { false }
    ): SubscriptionPayload {
        SubscriptionHttp.checkCancelled(cancelled)
        require(hwid == null || (hwid.length <= 256 && '\r' !in hwid && '\n' !in hwid)) { "Invalid HWID" }
        require(customUserAgent == null || (customUserAgent.length <= 256 && '\r' !in customUserAgent && '\n' !in customUserAgent)) { "Invalid User-Agent" }
        require(proxyPort == null || proxyPort in 1..65535) { "Invalid local proxy port" }
        var target = rawUrl.trim()
        if (HappCrypt.isHappCryptLink(target)) {
            val decrypted = HappCrypt.decryptHappLink(target).trim()
            require(!decrypted.startsWith("http://", true)) { "Happ subscription URL must use HTTPS" }
            if (!decrypted.startsWith("https://", true)) {
                SubscriptionHttp.checkCancelled(cancelled)
                return normalize(decrypted, emptyMap()).copy(clientProfile = "Happ crypt")
            }
            target = decrypted
        }
        val initialUrl = URL(target)
        require(initialUrl.protocol in setOf("http", "https")) { "Subscription URL must use HTTP(S)" }
        require(allowHttp || initialUrl.protocol == "https") { "HTTP subscriptions are disabled in settings" }
        var lastError: Exception? = null
        for ((profile, userAgent) in clientProfiles(customUserAgent)) {
            SubscriptionHttp.checkCancelled(cancelled)
            try {
                var current = initialUrl
                var redirects = 0
                var forwardHwid = true
                while (true) {
                    SubscriptionHttp.checkCancelled(cancelled)
                    val requestHeaders = linkedMapOf(
                        "User-Agent" to userAgent,
                        "Accept" to "text/yaml,application/yaml,application/json,text/plain,*/*",
                        "Accept-Encoding" to "gzip",
                        "Profile-Update-Interval" to "24"
                    )
                    if (forwardHwid && !hwid.isNullOrBlank()) requestHeaders["X-Hwid"] = hwid
                    if (direct) requestHeaders["X-Lumen-Route"] = "direct"
                    val response = SubscriptionHttp.get(current, requestHeaders, proxyPort, proxyUsername, proxyPassword, cancelled)
                    val code = response.code
                    if (code in setOf(301, 302, 303, 307, 308)) {
                        if (++redirects > 5) throw PermanentSubscriptionException("Too many subscription redirects")
                        val location = response.headers["location"] ?: throw PermanentSubscriptionException("Redirect has no Location")
                        val next = URL(current, location)
                        if (next.protocol !in setOf("http", "https") || next.userInfo != null ||
                            (!allowHttp && next.protocol != "https") || (current.protocol == "https" && next.protocol != "https")) {
                            throw PermanentSubscriptionException("Unsafe subscription redirect rejected")
                        }
                        forwardHwid = forwardHwid && sameOrigin(current, next)
                        current = next
                        continue
                    }
                    if (code !in 200..299) {
                        // Provider error bodies can echo access tokens or credentials.
                        if (code in setOf(404, 410, 429)) throw PermanentSubscriptionException("Subscription HTTP $code")
                        throw IOException("Subscription HTTP $code")
                    }
                    val normalized = normalize(response.body.toString(Charsets.UTF_8), response.headers).copy(clientProfile = profile)
                    SubscriptionHttp.checkCancelled(cancelled)
                    val (nodes, _) = LinkParser.parseLinksText(normalized.body)
                    if (nodes.isEmpty()) throw IOException("No supported servers in response")
                    if (nodes.size <= 2 && looksLikePlaceholder(normalized.body)) {
                        throw IOException("Subscription returned a compatibility placeholder")
                    }
                    SubscriptionHttp.checkCancelled(cancelled)
                    return normalized
                }
            } catch (error: java.util.concurrent.CancellationException) {
                throw error
            } catch (error: PermanentSubscriptionException) {
                throw error
            } catch (error: Exception) {
                SubscriptionHttp.checkCancelled(cancelled)
                lastError = error
            }
        }
        throw IOException(lastError?.message ?: "Subscription download failed", lastError)
    }

    internal fun sameOrigin(first: URL, second: URL): Boolean =
        first.protocol.equals(second.protocol, true) && first.host.equals(second.host, true) &&
            (if (first.port > 0) first.port else first.defaultPort) == (if (second.port > 0) second.port else second.defaultPort)

    internal fun normalize(body: String, rawHeaders: Map<String, String>): SubscriptionPayload {
        var linksBody = body.trim().removePrefix("\uFEFF").trim()
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
                // Keep the complete structured body. Flattening only `links` loses
                // sibling containers and stringifies object nodes into damaged URIs.
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
