package com.lumen.app.subscription

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SubscriptionClientTest {
    @Test
    fun providerCannotOverrideTheUsersPingMethod() {
        val payload = SubscriptionClient.normalize(
            "# ping-type: tcp\nvless://test@example.com:443",
            mapOf("ping-type" to "TCPing", "change-user-agent" to "Provider/1.0")
        )
        assertFalse(payload.premiumFeatures.containsKey("ping-type"))
        assertEquals("Provider/1.0", payload.premiumFeatures["change-user-agent"])
    }
    @Test
    fun encryptedSubscriptionSourcesArePreservedAcrossRefreshes() {
        assertTrue(SubscriptionClient.isEncryptedSource("incy://crypt1/payload"))
        assertTrue(SubscriptionClient.isEncryptedSource("happ://crypt5/payload"))
        assertFalse(SubscriptionClient.isEncryptedSource("https://example.com/sub"))
    }

    @Test
    fun httpsNewUrlIsAccepted() {
        assertEquals(
            "https://panel.example/sub/token",
            SubscriptionClient.premiumUrl(mapOf("new-url" to "https://panel.example/sub/token"))
        )
    }

    @Test
    fun httpNewUrlIsIgnored() {
        // The subscription URL is a bearer credential: a provider response must not
        // be able to pin it to plaintext.
        assertNull(SubscriptionClient.premiumUrl(mapOf("new-url" to "http://panel.example/sub/token")))
    }

    @Test
    fun missingNewUrlIsIgnored() {
        assertNull(SubscriptionClient.premiumUrl(emptyMap()))
        assertNull(SubscriptionClient.premiumUrl(mapOf("new-url" to "")))
    }

    @Test
    fun replaceDomainKeepsSourceScheme() {
        assertEquals(
            "https://mirror.example/sub/token",
            SubscriptionClient.replaceDomain("https://panel.example/sub/token", "mirror.example")
        )
        assertNull(SubscriptionClient.replaceDomain("https://panel.example/sub/token", ""))
    }
}
