package com.lumen.ui.screens

/**
 * OpenVPN editor parity with desktop Lumen: a .ovpn profile is decomposed into
 * editable fields and rebuilt into a canonical profile when the node is saved,
 * so the parser and the sing-box builder keep receiving a normal profile.
 */

private val OVPN_INLINE_BLOCK = Regex(
    "(?ims)^[ \\t]*<(ca|cert|key|tls-auth|tls-crypt|tls-crypt-v2|auth-user-pass)>[ \\t]*\\r?\\n(.*?)^[ \\t]*</\\1>[ \\t]*$"
)

val OPENVPN_PROTOCOLS: List<String> = listOf("udp", "tcp")

/** Only ciphers implemented by sing-box extended, same list as desktop. */
val OPENVPN_CIPHERS: List<String> = listOf(
    "", "AES-128-GCM", "AES-192-GCM", "AES-256-GCM",
    "AES-128-CBC", "AES-192-CBC", "AES-256-CBC", "CHACHA20-POLY1305"
)

val OPENVPN_AUTH_DIGESTS: List<String> = listOf("", "SHA1", "SHA256", "SHA384", "SHA512")

val OPENVPN_KEY_DIRECTIONS: List<String> = listOf("", "0", "1", "-1")

val OPENVPN_X509_MODES: List<String> = listOf("", "name", "name-prefix", "subject")

/** Splits a stored .ovpn profile into the editor fields. */
fun openVpnDraftFromProfile(base: NodeDraft, text: String): NodeDraft {
    if (text.isBlank()) return base
    val inline = HashMap<String, String>()
    val body = OVPN_INLINE_BLOCK.replace(text) { match ->
        inline[match.groupValues[1].lowercase()] = match.groupValues[2].trim('\r', '\n')
        ""
    }
    val byKey = LinkedHashMap<String, MutableList<List<String>>>()
    for (line in body.lines()) {
        val trimmed = line.trim()
        if (trimmed.isEmpty() || trimmed.startsWith("#") || trimmed.startsWith(";")) continue
        val tokens = trimmed.split(Regex("\\s+")).map { it.trim('"', '\'') }.filter { it.isNotEmpty() }
        if (tokens.isEmpty()) continue
        byKey.getOrPut(tokens[0].trimStart('-').lowercase()) { mutableListOf() }.add(tokens.drop(1))
    }

    fun last(key: String): List<String> = byKey[key]?.lastOrNull() ?: emptyList()
    fun arg(key: String): String = last(key).firstOrNull()?.trim().orEmpty()
    fun proto(value: String): String = when {
        value.startsWith("tcp") -> "tcp"
        value.startsWith("udp") -> "udp"
        else -> ""
    }

    val remotes = byKey["remote"].orEmpty().filter { it.isNotEmpty() }
    val first = remotes.firstOrNull()
    val globalProto = proto(arg("proto").lowercase())
    val remoteProto = first?.getOrNull(2)?.lowercase()?.let { proto(it) }.orEmpty()
    val credentials = (inline["auth-user-pass"] ?: "").lines().map { it.trim() }.filter { it.isNotEmpty() }
    val cipher = listOf("cipher", "data-ciphers", "ncp-ciphers", "data-ciphers-fallback")
        .asSequence()
        .flatMap { arg(it).split(":").asSequence() }
        .map { it.trim().uppercase() }
        .firstOrNull { it.isNotEmpty() && it in OPENVPN_CIPHERS }
        .orEmpty()
    val tlsSuites = listOf("tls-cipher", "tls-ciphersuites")
        .mapNotNull { arg(it).ifBlank { null } }
        .joinToString(":")
    val verify = last("verify-x509-name")
    val dns = byKey["dhcp-option"].orEmpty()
        .filter { it.size >= 2 && it[0].uppercase() in setOf("DNS", "DNS6") }
        .map { it[1] }

    return base.copy(
        server = first?.getOrNull(0)?.trim() ?: base.server,
        port = first?.getOrNull(1)?.trim() ?: base.port.ifBlank { "1194" },
        ovpnProto = remoteProto.ifEmpty { globalProto }.ifEmpty { "udp" },
        ovpnExtraRemotes = remotes.drop(1).joinToString("\n") { it.take(2).joinToString(" ") },
        ovpnCipher = cipher,
        ovpnAuth = arg("auth").uppercase(),
        ovpnUsername = credentials.getOrNull(0).orEmpty(),
        ovpnPassword = credentials.getOrNull(1).orEmpty(),
        ovpnCa = inline["ca"].orEmpty(),
        ovpnCert = inline["cert"].orEmpty(),
        ovpnKey = inline["key"].orEmpty(),
        ovpnTlsCrypt = inline["tls-crypt"] ?: inline["tls-crypt-v2"] ?: "",
        ovpnTlsCryptV2 = inline.containsKey("tls-crypt-v2"),
        ovpnTlsAuth = inline["tls-auth"].orEmpty(),
        ovpnKeyDirection = arg("key-direction").ifBlank { last("tls-auth").getOrNull(1).orEmpty() },
        ovpnTlsCipherSuites = tlsSuites,
        ovpnVerifyX509Name = verify.getOrNull(0).orEmpty(),
        ovpnVerifyX509Mode = verify.getOrNull(1).orEmpty(),
        ovpnReconnectDelay = digitsOf(arg("connect-retry")),
        ovpnPingInterval = digitsOf(arg("ping")),
        ovpnPingRestart = digitsOf(arg("ping-restart")),
        ovpnDns = dns.joinToString("\n"),
        rawConfig = text
    )
}

/** Rebuilds a canonical .ovpn profile from the editor fields. */
fun openVpnProfileFromDraft(d: NodeDraft): String {
    val sb = StringBuilder()
    sb.appendLine("# ${d.name.ifBlank { d.server }}")
    sb.appendLine("client")
    sb.appendLine("dev tun")
    sb.appendLine("proto ${d.ovpnProto.ifBlank { "udp" }}")
    sb.appendLine("remote ${d.server.trim()} ${d.port.trim().ifBlank { "1194" }}")
    d.ovpnExtraRemotes.lines().map { it.trim() }.filter { it.isNotEmpty() }.forEach {
        sb.appendLine("remote $it")
    }
    sb.appendLine("resolv-retry infinite")
    sb.appendLine("nobind")
    sb.appendLine("persist-key")
    sb.appendLine("persist-tun")
    sb.appendLine("remote-cert-tls server")
    if (d.ovpnCipher.isNotBlank()) {
        sb.appendLine("cipher ${d.ovpnCipher.trim()}")
        sb.appendLine("data-ciphers ${d.ovpnCipher.trim()}")
    }
    if (d.ovpnAuth.isNotBlank()) sb.appendLine("auth ${d.ovpnAuth.trim()}")
    if (d.ovpnTlsCipherSuites.isNotBlank()) sb.appendLine("tls-cipher ${d.ovpnTlsCipherSuites.trim()}")
    if (d.ovpnVerifyX509Name.isNotBlank()) {
        val mode = if (d.ovpnVerifyX509Mode.isNotBlank()) " ${d.ovpnVerifyX509Mode.trim()}" else ""
        sb.appendLine("verify-x509-name ${d.ovpnVerifyX509Name.trim()}$mode")
    }
    if (d.ovpnKeyDirection.isNotBlank()) sb.appendLine("key-direction ${d.ovpnKeyDirection.trim()}")
    digitsOf(d.ovpnReconnectDelay).takeIf { it.isNotEmpty() }?.let { sb.appendLine("connect-retry $it") }
    digitsOf(d.ovpnPingInterval).takeIf { it.isNotEmpty() }?.let { sb.appendLine("ping $it") }
    digitsOf(d.ovpnPingRestart).takeIf { it.isNotEmpty() }?.let { sb.appendLine("ping-restart $it") }
    d.ovpnDns.split('\n', ',').map { it.trim() }.filter { it.isNotEmpty() }.forEach {
        sb.appendLine("dhcp-option DNS $it")
    }

    fun block(tag: String, content: String) {
        if (content.isBlank()) return
        sb.appendLine("<$tag>")
        sb.appendLine(content.trim('\r', '\n'))
        sb.appendLine("</$tag>")
    }
    // Credentials must stay inline: file references are rejected on Android.
    if (d.ovpnUsername.isNotBlank()) {
        block("auth-user-pass", d.ovpnUsername.trim() + "\n" + d.ovpnPassword.trim())
    }
    block("ca", d.ovpnCa)
    block("cert", d.ovpnCert)
    block("key", d.ovpnKey)
    block("tls-auth", d.ovpnTlsAuth)
    if (d.ovpnTlsCrypt.isNotBlank()) {
        block(if (d.ovpnTlsCryptV2) "tls-crypt-v2" else "tls-crypt", d.ovpnTlsCrypt)
    }
    return sb.toString()
}

private fun digitsOf(value: String): String = value.filter { it.isDigit() }
