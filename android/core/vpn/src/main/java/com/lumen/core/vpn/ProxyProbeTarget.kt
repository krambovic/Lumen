package com.lumen.core.vpn

import java.net.IDN
import java.net.URI

/**
 * A bounded HTTP(S) destination used to verify that the selected outbound can
 * exchange real application data. Keeping parsing separate from the service
 * makes malformed user ping URLs harmless.
 */
internal data class ProxyProbeTarget(
    val scheme: String,
    val host: String,
    val port: Int,
    val requestTarget: String
) {
    val tls: Boolean get() = scheme == "https"

    val authority: String
        get() = if ((tls && port == 443) || (!tls && port == 80)) host else "$host:$port"

    companion object {
        fun parse(raw: String): ProxyProbeTarget? = runCatching {
            val uri = URI(raw.trim())
            val scheme = uri.scheme?.lowercase() ?: return null
            if (scheme != "http" && scheme != "https") return null
            val unicodeHost = uri.host?.takeIf(String::isNotBlank) ?: return null
            val host = IDN.toASCII(unicodeHost)
            val port = when {
                uri.port in 1..65535 -> uri.port
                scheme == "https" -> 443
                else -> 80
            }
            val path = uri.rawPath?.takeIf(String::isNotEmpty) ?: "/"
            val requestTarget = uri.rawQuery?.let { "$path?$it" } ?: path
            ProxyProbeTarget(scheme, host, port, requestTarget)
        }.getOrNull()
    }
}
