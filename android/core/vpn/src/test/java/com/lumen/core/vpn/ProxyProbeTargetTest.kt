package com.lumen.core.vpn

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ProxyProbeTargetTest {
    @Test
    fun `https target keeps path query and default port`() {
        val target = ProxyProbeTarget.parse("https://example.com/generate_204?source=lumen")!!

        assertTrue(target.tls)
        assertEquals("example.com", target.host)
        assertEquals(443, target.port)
        assertEquals("/generate_204?source=lumen", target.requestTarget)
        assertEquals("example.com", target.authority)
    }

    @Test
    fun `http target accepts an explicit port`() {
        val target = ProxyProbeTarget.parse("http://example.com:8080/health")!!

        assertFalse(target.tls)
        assertEquals(8080, target.port)
        assertEquals("example.com:8080", target.authority)
    }

    @Test
    fun `non http and malformed targets are rejected`() {
        assertNull(ProxyProbeTarget.parse("icmp://1.1.1.1"))
        assertNull(ProxyProbeTarget.parse("not a URL"))
    }
}
