package com.lumen.app.subscription

import java.net.URI
import java.util.Locale

internal enum class ImportKind { SUBSCRIPTION, CONFIG }

internal sealed interface ImportClassification {
    data class Ready(val kind: ImportKind, val normalized: String) : ImportClassification
    data class Rejected(val message: String) : ImportClassification
}

internal object ImportClassifier {
    const val MAX_CLIPBOARD_CHARS = 1_048_576

    private val configSchemes = setOf(
        "vless", "vmess", "ss", "trojan", "tuic", "hysteria2", "hy2",
        "hysteria", "wireguard", "awg", "socks", "socks5", "http", "https"
    )

    fun classify(raw: String?): ImportClassification {
        val text = raw?.trim().orEmpty()
        if (text.isEmpty()) return ImportClassification.Rejected("Clipboard is empty")
        if (text.length > MAX_CLIPBOARD_CHARS) {
            return ImportClassification.Rejected("Import data is larger than 1 MiB")
        }

        parseHttpUrl(text)?.let {
            return ImportClassification.Ready(ImportKind.SUBSCRIPTION, it)
        }
        val firstScheme = text.substringBefore(':', "").lowercase(Locale.US)
        val hasConfigScheme = firstScheme in configSchemes - setOf("http", "https") ||
            text.lineSequence().any { line ->
                line.substringBefore(':', "").trim().lowercase(Locale.US) in
                    configSchemes - setOf("http", "https")
            }
        val lowered = text.lowercase(Locale.US)
        val isYamlOrIni = lowered.contains("proxies:") || lowered.contains("proxy-providers:") ||
            lowered.contains("proxy-groups:") || lowered.contains("payload:") || lowered.contains("outbounds:") ||
            lowered.contains("- name:") || lowered.contains("- type:") || lowered.contains("- server:") ||
            (lowered.contains("[interface]") && lowered.contains("[peer]"))
        val looksStructured = text.startsWith("{") || text.startsWith("[") || isYamlOrIni ||
            (text.length >= 24 && text.length % 4 == 0 &&
                text.all { it.isLetterOrDigit() || it in "+/=\r\n-_" })
        return if (hasConfigScheme || looksStructured) {
            ImportClassification.Ready(ImportKind.CONFIG, text)
        } else ImportClassification.Rejected("Clipboard does not contain a subscription or supported config")
    }

    private fun parseHttpUrl(text: String): String? = runCatching {
        if (text.any(Char::isWhitespace)) return null
        val uri = URI(text)
        val scheme = uri.scheme?.lowercase(Locale.US)
        if (scheme !in setOf("http", "https") || uri.host.isNullOrBlank()) null else uri.toASCIIString()
    }.getOrNull()
}
