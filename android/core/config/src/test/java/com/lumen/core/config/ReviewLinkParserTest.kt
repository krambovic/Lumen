package com.lumen.core.config

import com.lumen.core.config.parser.LinkParser
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ReviewLinkParserTest {
    private fun native(link: String): Map<*, *> {
        val (nodes, errors) = LinkParser.parseLinksText(link)
        assertTrue(errors.toString(), errors.isEmpty())
        return nodes.single().outbound["singbox"] as Map<*, *>
    }

    @Test fun hysteria2QueryAuthIsDecodedExactlyOnce() {
        val node = native("hy2://example.invalid:443?auth=%20secret%2Bvalue%3A%40%2F%25%D0%96%20")
        assertEquals(" secret+value:@/%Ж ", node["password"])
    }

    @Test fun hysteriaV1SupportsUnderscoreInsecureAlias() {
        val tls = native("hysteria://token@example.invalid:443?allow_insecure=1")["tls"] as Map<*, *>
        assertEquals(true, tls["insecure"])
    }

    @Test fun conflictingTlsAliasesDoNotWeakenVerification() {
        for (scheme in listOf("hysteria", "hysteria2")) {
            val tls = native("$scheme://token@example.invalid:443?insecure=0&allow_insecure=1")["tls"] as Map<*, *>
            assertEquals(false, tls["insecure"] ?: false)
        }
    }

    @Test fun rawMasqueTokenMayContainBase64Slashes() {
        val profile = native("masque://auth+token/with/slash==@profile-id")["profile"] as Map<*, *>
        assertEquals("auth+token/with/slash==", profile["auth_token"])
        assertEquals("profile-id", profile["id"])
    }
}
