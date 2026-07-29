package com.lumen.core.config.normalizer

/**
 * Compatibility boundary for the OpenVPN implementation shipped in
 * sing-box-extended 1.13.14-extended-2.5.2.
 *
 * The core exposes one data-channel cipher instead of OpenVPN's negotiated list,
 * and its TLS wrapper accepts Go crypto/tls names rather than OpenSSL names.
 * Keeping the conversion here prevents imported .ovpn and native JSON profiles
 * from reaching the core with fields that make the whole configuration abort.
 */
object OpenVpnConfigNormalizer {
    val dataCiphers: Set<String> = linkedSetOf(
        "AES-128-GCM", "AES-192-GCM", "AES-256-GCM",
        "AES-128-CBC", "AES-192-CBC", "AES-256-CBC",
        "CHACHA20-POLY1305"
    )

    val authDigests: Set<String> = linkedSetOf("MD5", "SHA1", "SHA256", "SHA384", "SHA512")

    private val goTlsCipherSuites = setOf(
        "TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA",
        "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA",
        "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
        "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA",
        "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
        "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
        "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
        "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
        "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
        "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256"
    )

    private val openSslTlsAliases = mapOf(
        "ECDHE-ECDSA-AES128-SHA" to "TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA",
        "ECDHE-ECDSA-AES256-SHA" to "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA",
        "ECDHE-RSA-AES128-SHA" to "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
        "ECDHE-RSA-AES256-SHA" to "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA",
        "ECDHE-ECDSA-AES128-GCM-SHA256" to "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
        "ECDHE-ECDSA-AES256-GCM-SHA384" to "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
        "ECDHE-RSA-AES128-GCM-SHA256" to "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
        "ECDHE-RSA-AES256-GCM-SHA384" to "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
        "ECDHE-RSA-CHACHA20-POLY1305" to "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
        "ECDHE-ECDSA-CHACHA20-POLY1305" to "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256"
    )

    fun normalizeDataCipher(value: String?): String? =
        value?.trim()?.uppercase()?.takeIf(dataCiphers::contains)

    fun normalizeAuthDigest(value: String?): String? =
        value?.trim()?.uppercase()?.takeIf(authDigests::contains)

    /**
     * Accept Go names, the IANA spelling used by Proton profiles, and common
     * OpenSSL aliases. Unknown restrictions are omitted so crypto/tls uses its
     * secure defaults instead of aborting before the VPN can start.
     */
    fun normalizeTlsCipherSuite(value: String?): String? {
        val raw = value?.trim()?.uppercase().orEmpty()
        if (raw.isEmpty()) return null
        if (raw in goTlsCipherSuites) return raw
        openSslTlsAliases[raw]?.let { return it }

        // TLS-ECDHE-ECDSA-WITH-AES-128-GCM-SHA256 -> the Go/IANA constant name.
        val iana = raw.replace('-', '_')
        return iana.takeIf(goTlsCipherSuites::contains)
    }

    fun normalizeTlsCipherSuites(value: Any?): List<String> {
        val rawValues = when (value) {
            is Iterable<*> -> value.mapNotNull { it?.toString() }
            is Array<*> -> value.mapNotNull { it?.toString() }
            null -> emptyList()
            else -> listOf(value.toString())
        }
        return rawValues
            .flatMap { it.split(':') }
            .mapNotNull(::normalizeTlsCipherSuite)
            .distinct()
    }
}
