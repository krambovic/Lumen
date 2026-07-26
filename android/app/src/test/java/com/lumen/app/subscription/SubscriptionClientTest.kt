package com.lumen.app.subscription

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SubscriptionClientTest {
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
