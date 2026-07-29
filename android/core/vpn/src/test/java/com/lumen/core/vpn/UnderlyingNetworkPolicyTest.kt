package com.lumen.core.vpn

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class UnderlyingNetworkPolicyTest {

    @Test
    fun `only validated non VPN Internet networks are ready`() {
        assertTrue(
            UnderlyingNetworkPolicy.isReady(
                hasInternet = true,
                isValidated = true,
                isVpn = false
            )
        )
        assertFalse(
            UnderlyingNetworkPolicy.isReady(
                hasInternet = true,
                isValidated = false,
                isVpn = false
            )
        )
        assertFalse(
            UnderlyingNetworkPolicy.isReady(
                hasInternet = true,
                isValidated = true,
                isVpn = true
            )
        )
        assertFalse(
            UnderlyingNetworkPolicy.isReady(
                hasInternet = false,
                isValidated = true,
                isVpn = false
            )
        )
    }

    @Test
    fun `active default wins over a validated standby candidate`() {
        val ready = setOf("wifi", "cellular")

        val selected = UnderlyingNetworkPolicy.select(
            active = "wifi",
            pending = "cellular",
            previous = "wifi",
            isReady = ready::contains
        )

        assertEquals("wifi", selected)
    }

    @Test
    fun `validated pending network replaces an unusable old default`() {
        val selected = UnderlyingNetworkPolicy.select(
            active = "old-wifi",
            pending = "new-wifi",
            previous = "old-wifi"
        ) { it == "new-wifi" }

        assertEquals("new-wifi", selected)
    }

    @Test
    fun `previous network is only a final ready fallback`() {
        assertEquals(
            "previous",
            UnderlyingNetworkPolicy.select(
                active = null,
                pending = null,
                previous = "previous"
            ) { true }
        )
        assertNull(
            UnderlyingNetworkPolicy.select(
                active = "vpn",
                pending = "joining-wifi",
                previous = "lost-wifi"
            ) { false }
        )
    }
}
