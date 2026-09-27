package com.lumen.app.subscription

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ImportClassifierTest {
    @Test
    fun happCryptSubscriptionKeepsEncryptedSource() {
        val encrypted = "happ://crypt/MYuWn7D9+SuUMnwuAO4aDNJtyglzW+mcEwGG19GeoxdaQijb+/U1JnlLMT0mRoKVWQB/ZtzfANf8koLv7Iq62LuhsGRASD4Gf+6JDAx/Ubk8LYBZifSulN7JHV5HXsqW1HeBRT/DLQ997XTfiDXxYTU+Ed0shUcucvOdXq4Vx1I="
        val result = ImportClassifier.classify(encrypted)
        assertTrue(result is ImportClassification.Ready)
        result as ImportClassification.Ready
        assertEquals(ImportKind.SUBSCRIPTION, result.kind)
        assertEquals(encrypted, result.normalized)
    }

    private val incyLink =
        "incy://crypt1/AAECAwQFBgcICQoLNyIQL3rDwRZqnyoD8pGKSKPC6cwYYSGTRieILcbBXDI2_rVLW0ACi9IVK63oUXhNjJAgucw3__4DCG5s9js"
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
    fun incyCryptLinkIsKeptAsEncryptedSubscriptionSource() {
        val result = ImportClassifier.classify(incyLink)
        assertTrue(result is ImportClassification.Ready)
        result as ImportClassification.Ready
        assertEquals(ImportKind.SUBSCRIPTION, result.kind)
        assertEquals(incyLink, result.normalized)
    }

    @Test
    fun undecryptableIncyCryptLinkIsRejected() {
        assertTrue(ImportClassifier.classify("incy://crypt1/not-valid") is ImportClassification.Rejected)
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
