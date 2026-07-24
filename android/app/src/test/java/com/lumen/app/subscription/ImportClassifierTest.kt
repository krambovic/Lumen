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
}
