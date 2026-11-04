package com.lumen.core.config.parser

import java.net.URI
import java.net.URLDecoder
import java.util.Base64

/** Client install links are wrappers around subscriptions, not proxy protocols. */
object SubscriptionLink {
    fun unwrap(text: String): String? = runCatching {
        val uri = URI(text.trim())
        if (uri.scheme?.lowercase() !in setOf("v2rayng", "v2rayn", "v2ray", "sing-box", "singbox")) return null
        val url = uri.rawQuery?.split('&')?.firstNotNullOfOrNull { pair ->
            if (pair.substringBefore('=').lowercase() in setOf("url", "subscription", "sub"))
                URLDecoder.decode(pair.substringAfter('=', ""), "UTF-8") else null
        } ?: uri.rawAuthority?.let { encoded ->
            runCatching { String(Base64.getUrlDecoder().decode(encoded), Charsets.UTF_8) }.getOrNull()
        } ?: return null
        val target = URI(url)
        require(target.scheme in setOf("http", "https") && !target.host.isNullOrBlank() && target.userInfo == null)
        target.toASCIIString()
    }.getOrNull()
}
