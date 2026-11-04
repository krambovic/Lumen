package com.lumen.core.config.parser

data class OpenVpnSecrets(val username: String = "", val password: String = "", val keyPassword: String = "")
data class OpenVpnMissingCredentials(val username: Boolean, val password: Boolean, val keyPassword: Boolean) {
    val any: Boolean get() = username || password || keyPassword
}

object OpenVpnCredentials {
    /** Patch credentials only: never rebuild remotes, TLS policy or connection blocks. */
    fun persist(node: ParsedNode, secret: OpenVpnSecrets): ParsedNode {
        require(node.scheme.equals("openvpn", true))
        val applied = apply(node, secret)
        val native = applied.outbound["singbox"] as? Map<*, *> ?: applied.outbound
        val source = if (node.link.trimStart().startsWith("{")) {
            val root = org.json.JSONObject(node.link)
            val target = root.optJSONObject("singbox") ?: root
            for (key in listOf("username", "password", "key_password")) {
                native[key]?.toString()?.takeIf { it.isNotEmpty() }?.let { target.put(key, it) }
            }
            root.toString()
        } else {
            var text = node.link.replace("\r\n", "\n").replace('\r', '\n')
            text = Regex("(?ims)^[ \\t]*<(auth-user-pass|askpass)>[ \\t]*\\n.*?^[ \\t]*</\\1>[ \\t]*(?:\\n|$)")
                .replace(text, "")
            text = Regex("(?im)^[ \\t]*(?:(?:--)?(?:auth-user-pass|askpass)(?:[ \\t]+[^\\n]*)?|#[ \\t]+lumen-key-password[ \\t]+[^\\n]*)[ \\t]*(?:\\n|$)")
                .replace(text, "")
            buildString {
                append(text.trimEnd()); append('\n')
                val username = native["username"]?.toString().orEmpty()
                val password = native["password"]?.toString().orEmpty()
                val keyPassword = native["key_password"]?.toString().orEmpty()
                require(listOf(username, password, keyPassword).none { '\n' in it || '\r' in it }) {
                    "OpenVPN credentials must fit on a single line"
                }
                if (username.isNotEmpty()) append("<auth-user-pass>\n$username\n$password\n</auth-user-pass>\n")
                else if (LinkParser.openVpnRequiresUserAuth(node.link)) append("auth-user-pass\n")
                if (keyPassword.isNotEmpty()) append("<askpass>\n$keyPassword\n</askpass>\n")
            }
        }
        return applied.copy(link = source)
    }

    fun missing(node: ParsedNode): OpenVpnMissingCredentials {
        if (!node.scheme.equals("openvpn", true)) return OpenVpnMissingCredentials(false, false, false)
        val native = node.outbound["singbox"] as? Map<*, *> ?: node.outbound
        val tls = native["tls"] as? Map<*, *>
        val key = (tls?.get("key") ?: tls?.get("client_key"))?.toString().orEmpty().uppercase()
        val auth = LinkParser.openVpnRequiresUserAuth(node.link) || node.outbound["openvpn_requires_auth"] == true ||
            !native["username"]?.toString().isNullOrEmpty() || !native["password"]?.toString().isNullOrEmpty()
        val encrypted = key.contains("ENCRYPTED PRIVATE KEY") || (key.contains("PROC-TYPE:") && key.contains("ENCRYPTED"))
        return OpenVpnMissingCredentials(auth && native["username"]?.toString().isNullOrBlank(),
            auth && native["password"]?.toString().isNullOrBlank(), encrypted && native["key_password"]?.toString().isNullOrBlank())
    }

    fun apply(node: ParsedNode, secret: OpenVpnSecrets): ParsedNode {
        val native = (node.outbound["singbox"] as? Map<*, *> ?: node.outbound).entries
            .associate { it.key.toString() to it.value }.toMutableMap()
        if (secret.username.isNotEmpty()) native["username"] = secret.username
        if (secret.password.isNotEmpty()) native["password"] = secret.password
        if (secret.keyPassword.isNotEmpty()) native["key_password"] = secret.keyPassword
        return node.copy(outbound = if (node.outbound["singbox"] is Map<*, *>) node.outbound + ("singbox" to native) else native)
    }
}
