package com.lumen.core.config.parser

/** Recover newly supported fields in records normalized by older Lumen versions. */
object ShareLinkExtensions {
    fun restore(node: ParsedNode): ParsedNode {
        if (node.scheme !in setOf("vless", "vmess", "trojan") || !node.link.contains("://")) return node
        val original = runCatching { LinkParser.parseSingle(node.link) }.getOrNull() ?: return node
        if (original.scheme != node.scheme || original.server != node.server || original.port != node.port) return node
        val incoming = original.outbound["streamSettings"] as? Map<*, *> ?: return node
        val existing = node.outbound["streamSettings"] as? Map<*, *> ?: return node
        val stream = existing.entries.associate { it.key.toString() to it.value }.toMutableMap()
        incoming["finalMask"]?.let { stream.putIfAbsent("finalMask", it) }
        for ((section, keys) in mapOf(
            "realitySettings" to listOf("mldsa65Verify"),
            "xhttpSettings" to listOf("extra"),
            "tlsSettings" to listOf("pinnedPeerCertSha256", "verifyPeerCertByName", "echConfigList", "echForceQuery")
        )) {
            val added = incoming[section] as? Map<*, *> ?: continue
            val old = (stream[section] as? Map<*, *>)?.entries?.associate { it.key.toString() to it.value }.orEmpty().toMutableMap()
            keys.forEach { key -> added[key]?.let { old.putIfAbsent(key, it) } }
            if (old.containsKey("pinnedPeerCertSha256")) old.remove("certificatePublicKeySha256")
            if (old.isNotEmpty()) stream[section] = old
        }
        return node.copy(outbound = node.outbound + ("streamSettings" to stream))
    }
}
