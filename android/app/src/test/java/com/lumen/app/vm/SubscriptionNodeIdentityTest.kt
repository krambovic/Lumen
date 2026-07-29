package com.lumen.app.vm

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class SubscriptionNodeIdentityTest {
    @Test
    fun autoSelectorsUseTheirProviderNameAsStableIdentity() {
        val netherlands = subscriptionNodeKey("", 0, "auto", "Netherlands AUTO")
        val unitedStates = subscriptionNodeKey("", 0, "AUTO", "United States AUTO")

        assertNotEquals(netherlands, unitedStates)
        assertEquals(
            netherlands,
            subscriptionNodeKey("", 0, "AUTO", "  NETHERLANDS AUTO ")
        )
    }

    @Test
    fun regularNodesKeepEndpointIdentityAcrossRenames() {
        assertEquals(
            subscriptionNodeKey("example.com", 443, "vless", "Old name"),
            subscriptionNodeKey("EXAMPLE.COM", 443, "VLESS", "New name")
        )
    }
}
