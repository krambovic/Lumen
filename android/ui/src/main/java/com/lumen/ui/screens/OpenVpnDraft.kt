package com.lumen.ui.screens

/**
 * OpenVPN editor parity with desktop Lumen: a .ovpn profile is decomposed into
 * editable fields and rebuilt into a canonical profile when the node is saved,
 * so the parser and the sing-box builder keep receiving a normal profile.
 */

private val OVPN_INLINE_BLOCK = Regex(
    "(?ims)^[ \\t]*<(ca|cert|key|tls-auth|tls-crypt|tls-crypt-v2|auth-user-pass|askpass)>[ \\t]*\\r?\\n(.*?)^[ \\t]*</\\1>[ \\t]*$"
)

val OPENVPN_PROTOCOLS: List<String> = listOf("udp", "tcp")

/** Only ciphers implemented by sing-box extended, same list as desktop. */
val OPENVPN_CIPHERS: List<String> = listOf(
    "", "AES-128-GCM", "AES-192-GCM", "AES-256-GCM",
    "AES-128-CBC", "AES-192-CBC", "AES-256-CBC", "CHACHA20-POLY1305"
)

val OPENVPN_AUTH_DIGESTS: List<String> = listOf("", "MD5", "SHA1", "SHA256", "SHA384", "SHA512")

val OPENVPN_KEY_DIRECTIONS: List<String> = listOf("", "0", "1", "-1")

val OPENVPN_X509_MODES: List<String> = listOf("", "name", "name-prefix", "name-suffix")

/**
 * "Use proxy" options for OpenVPN nodes. Http/Socks are handled by sing-box
 * (a proxy outbound plus a detour), obfs2/obfs3 by the in-app pluggable
 * transport relay, so no external obfsproxy binary is shipped.
 */
val OPENVPN_PROXY_TYPES: List<String> = listOf("", "http", "socks", "obfs3", "obfs2", "obfs2-legacy")

/** obfs variants are terminated locally, everything else is a real proxy server. */
fun isObfsProxy(type: String): Boolean = type.startsWith("obfs2") || type == "obfs3"

/** Human readable label for the proxy dropdown, matching the desktop client. */
fun openVpnProxyLabel(type: String): String = when (type) {
    "http" -> "Http"
    "socks" -> "Socks"
    "obfs3" -> "obfsproxy (obfs3)"
    "obfs2" -> "obfsproxy (obfs2)"
    "obfs2-legacy" -> "obfsproxy (obfs2-legacy)"
    else -> "None"
}

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
    fun proto(value: String): String = when (value.trim().lowercase()) {
        "tcp", "tcp-client", "tcp4", "tcp4-client", "tcp6", "tcp6-client" -> "tcp"
        "udp", "udp4", "udp6" -> "udp"
        else -> ""
    }

    // "Use proxy": http-proxy/socks-proxy are standard OpenVPN directives, while
    // obfs2/obfs3 and the credentials live in Lumen comments so that other
    // clients simply ignore them.
    fun comment(prefix: String): List<String> = text.lines()
        .map { it.trim() }
        .lastOrNull { it.startsWith("# $prefix ") }
        ?.removePrefix("# $prefix ")
        ?.split(Regex("\\s+"))
        ?.filter { it.isNotEmpty() }
        .orEmpty()

    val obfsProxy = comment("lumen-proxy")
    val proxyAuth = comment("lumen-proxy-auth")
    val httpProxy = byKey["http-proxy"]?.lastOrNull().orEmpty()
    val socksProxy = byKey["socks-proxy"]?.lastOrNull().orEmpty()
    val proxyType = when {
        obfsProxy.firstOrNull().orEmpty().let { isObfsProxy(it) } -> obfsProxy[0]
        httpProxy.isNotEmpty() -> "http"
        socksProxy.isNotEmpty() -> "socks"
        else -> ""
    }
    val proxyEndpoint = when (proxyType) {
        "" -> emptyList()
        "http" -> httpProxy
        "socks" -> socksProxy
        else -> obfsProxy.drop(1)
    }

    val remotes = byKey["remote"].orEmpty().filter { it.isNotEmpty() }
    val first = remotes.firstOrNull()
    val globalProto = proto(arg("proto").lowercase())
    val remoteProto = first?.getOrNull(2)?.lowercase()?.let { proto(it) }.orEmpty()
    val credentials = (inline["auth-user-pass"] ?: "").lines().map { it.trim() }.filter { it.isNotEmpty() }
    val cipher = listOf("data-ciphers", "ncp-ciphers", "cipher", "data-ciphers-fallback")
        .asSequence()
        .flatMap { arg(it).split(":").asSequence() }
        .map { it.trim().uppercase() }
        .firstOrNull { it.isNotEmpty() && it in OPENVPN_CIPHERS }
        .orEmpty()
    // tls-ciphersuites is TLS 1.3-only; the bundled extended core exposes only
    // the TLS <=1.2 cipher list, so keep the representable directive separate.
    val tlsSuites = arg("tls-cipher")
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
        ovpnKeyPassword = (inline["askpass"] ?: "").lines()
            .map { it.trim() }
            .firstOrNull { it.isNotEmpty() }
            ?: comment("lumen-key-password").firstOrNull().orEmpty(),
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
        ovpnProxyType = proxyType,
        ovpnProxyServer = proxyEndpoint.getOrNull(0).orEmpty(),
        ovpnProxyPort = digitsOf(proxyEndpoint.getOrNull(1).orEmpty()),
        ovpnProxyUsername = proxyAuth.getOrNull(0).orEmpty(),
        ovpnProxyPassword = proxyAuth.getOrNull(1).orEmpty(),
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

    // "Use proxy". Http/Socks stay standard directives so the profile remains
    // portable; obfs is a Lumen comment because it is terminated in-app.
    val proxyHost = d.ovpnProxyServer.trim()
    val proxyPort = digitsOf(d.ovpnProxyPort)
    if (d.ovpnProxyType.isNotBlank() && proxyHost.isNotEmpty() && proxyPort.isNotEmpty()) {
        when (d.ovpnProxyType) {
            "http" -> sb.appendLine("http-proxy $proxyHost $proxyPort")
            "socks" -> sb.appendLine("socks-proxy $proxyHost $proxyPort")
            else -> sb.appendLine("# lumen-proxy ${d.ovpnProxyType} $proxyHost $proxyPort")
        }
        if (d.ovpnProxyUsername.isNotBlank()) {
            sb.appendLine("# lumen-proxy-auth ${d.ovpnProxyUsername.trim()} ${d.ovpnProxyPassword.trim()}")
        }
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
    } else if (openVpnRequiresCredentials(d)) {
        // The source profile asked for a login but the fields are still empty. Dropping
        // the directive would turn the profile into a certificate-only one on the way
        // out, and the builder's "OpenVPN node requires a username and password" check
        // — which reads the stored profile — would never fire again.
        sb.appendLine("auth-user-pass")
    }
    block("ca", d.ovpnCa)
    block("cert", d.ovpnCert)
    block("key", d.ovpnKey)
    // Passphrase of an encrypted private key. Inline, because a file reference
    // cannot be resolved on Android; the parser reads exactly this block.
    if (d.ovpnKeyPassword.isNotBlank()) block("askpass", d.ovpnKeyPassword.trim())
    block("tls-auth", d.ovpnTlsAuth)
    if (d.ovpnTlsCrypt.isNotBlank()) {
        block(if (d.ovpnTlsCryptV2) "tls-crypt-v2" else "tls-crypt", d.ovpnTlsCrypt)
    }
    return sb.toString()
}

private fun digitsOf(value: String): String = value.filter { it.isDigit() }
