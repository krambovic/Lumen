package com.lumen.core.config

import com.lumen.core.config.normalizer.AmneziaWGNormalizer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AmneziaWGNormalizerTest {

    @Test
    fun testNormalizeAmneziaWGJunkParameters() {
        val rawConfig = mapOf(
            "type" to "awg",
            "jc" to 4,
            "jmin" to 50,
            "jmax" to 100,
            "s1" to 10,
            "s2" to 20,
            "h1" to 12345,
            "i1" to 999
        )
        val normalized = AmneziaWGNormalizer.normalize(rawConfig)
        assertEquals("wireguard", normalized["type"])
        @Suppress("UNCHECKED_CAST")
        val amneziaMap = normalized["amnezia"] as? Map<String, Any?>
        assertNotNull(amneziaMap)
        assertEquals(4, amneziaMap!!["jc"])
        assertEquals(50, amneziaMap["jmin"])
        assertEquals(100, amneziaMap["jmax"])
        assertEquals(10, amneziaMap["s1"])
        assertEquals(20, amneziaMap["s2"])
        assertEquals(12345, amneziaMap["h1"])
        assertEquals(999, amneziaMap["i1"])
        assertEquals(false, normalized["system"])
    }

    @Test
    fun testNormalizeIpPrefix() {
        assertEquals("192.168.1.1/32", AmneziaWGNormalizer.normalizeIpPrefix("192.168.1.1"))
        assertEquals("10.0.0.0/24", AmneziaWGNormalizer.normalizeIpPrefix("10.0.0.0/24"))
        assertEquals("fd00::1/128", AmneziaWGNormalizer.normalizeIpPrefix("fd00::1"))
        assertEquals("2001:db8::/32", AmneziaWGNormalizer.normalizeIpPrefix("2001:db8::/32"))
        assertEquals("", AmneziaWGNormalizer.normalizeIpPrefix(""))
    }

    @Test
    fun testNormalizeIpPrefixes() {
        val input = "192.168.1.1, 10.0.0.1/24, fd00::1"
        val normalized = AmneziaWGNormalizer.normalizeIpPrefixes(input)
        assertEquals(listOf("192.168.1.1/32", "10.0.0.1/24", "fd00::1/128"), normalized)
    }

    @Test
    fun testParseReservedBytes() {
        assertEquals(listOf(1, 2, 3), AmneziaWGNormalizer.parseReservedBytes(listOf(1, 2, 3)))
        assertEquals(listOf(10, 20, 30), AmneziaWGNormalizer.parseReservedBytes("10, 20, 30"))
        assertEquals(listOf(100, 200, 250), AmneziaWGNormalizer.parseReservedBytes("[100, 200, 250]"))
        assertEquals(emptyList<Int>(), AmneziaWGNormalizer.parseReservedBytes("invalid"))
    }

    @Test
    fun testNormalizeWireGuardEndpointLegacyFields() {
        val legacyMap = mapOf(
            "type" to "wireguard",
            "server" to "1.2.3.4",
            "server_port" to 51820,
            "peer_public_key" to "abcdef1234567890=",
            "local_address" to "10.7.0.2",
            "reserved" to listOf(0, 1, 2)
        )
        val normalized = AmneziaWGNormalizer.normalizeWireGuardEndpoint(legacyMap)
        assertEquals("wireguard", normalized["type"])
        assertEquals(listOf("10.7.0.2/32"), normalized["address"])
        assertEquals(false, normalized["system"])

        @Suppress("UNCHECKED_CAST")
        val peers = normalized["peers"] as? List<Map<String, Any?>>
        assertNotNull(peers)
        assertEquals(1, peers!!.size)
        val firstPeer = peers[0]
        assertEquals("1.2.3.4", firstPeer["address"])
        assertEquals(51820, firstPeer["port"])
        assertEquals("abcdef1234567890=", firstPeer["public_key"])
    }

    @Test
    fun testCloudflareWarpReservedBytesConversion() {
        val warpMap = mapOf(
            "type" to "wireguard",
            "server" to "engage.cloudflareclient.com",
            "server_port" to 2408,
            "peer_public_key" to "bmXOC+F1FxEMF9dyiK2H5/1SUtzH0JuVo51h2wPfgyo=",
            "local_address" to "172.16.0.2",
            "reserved" to listOf(1, 2, 3)
        )
        val normalized = AmneziaWGNormalizer.normalizeWireGuardEndpoint(warpMap)
        assertEquals("warp", normalized["type"])
        // Core 2.5.1+ rejects an explicit "reserved" field on WARP endpoints:
        // the endpoint derives the reserved bytes from its profile.
        assertFalse(normalized.containsKey("reserved"))
        assertEquals(false, normalized["system"])
        @Suppress("UNCHECKED_CAST")
        val profile = normalized["profile"] as? Map<String, Any?>
        assertNotNull(profile)
        assertEquals("direct", profile!!["detour"])
    }
}
