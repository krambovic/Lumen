package com.lumen.app.subscription

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ImportClassifierTest {
    @Test
    fun httpUrlIsSubscription() {
        val result = ImportClassifier.classify("https://example.com/sub?id=1")
        assertTrue(result is ImportClassification.Ready)
        assertEquals(ImportKind.SUBSCRIPTION, (result as ImportClassification.Ready).kind)
    }

    @Test
    fun proxyLinkIsConfig() {
        val result = ImportClassifier.classify("vless://id@example.com:443")
        assertTrue(result is ImportClassification.Ready)
        assertEquals(ImportKind.CONFIG, (result as ImportClassification.Ready).kind)
    }

    @Test
    fun arbitraryTextIsRejected() {
        assertTrue(ImportClassifier.classify("just some text") is ImportClassification.Rejected)
    }

    @Test
    fun anytlsLinkIsConfig() {
        val result = ImportClassifier.classify("anytls://password@example.com:8443#A")
        assertTrue(result is ImportClassification.Ready)
        assertEquals(ImportKind.CONFIG, (result as ImportClassification.Ready).kind)
    }

    @Test
    fun undecryptableHappCryptLinkIsRejected() {
        // The happ branch must fall through instead of throwing when the payload
        // cannot be decrypted.
        assertTrue(ImportClassifier.classify("happ://crypt/notreallyapayload") is ImportClassification.Rejected)
    }

    @Test
    fun extendedProtocolLinksAreConfig() {
        val links = listOf(
            "naive+https://user:pass@example.com:443#N",
            "quic://user:pass@example.com:443#N",
            "mieru://user:pass@example.com:2027#M",
            "masque://token@profile-id#W",
            "hysteria://auth@example.com:443#H1",
            "warp://?id=abc#WARP"
        )
        for (link in links) {
            val result = ImportClassifier.classify(link)
            assertTrue("$link should be CONFIG", result is ImportClassification.Ready)
            assertEquals(ImportKind.CONFIG, (result as ImportClassification.Ready).kind)
        }
    }
}
