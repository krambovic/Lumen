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
            "i1" to "999"
        )
        val normalized = AmneziaWGNormalizer.normalize(rawConfig)
        // "awg" is an import alias; the extended core expects a WireGuard
        // endpoint carrying the Amnezia parameters.
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
        assertEquals("999", amneziaMap["i1"])
        assertEquals(false, normalized["system"])
    }

    @Test
    fun testAmneziaValueTypesMatchCoreSchema() {
        // Verified against core/sing-box-lumen.exe: amnezia.jc must decode as a
        // number, amnezia.i1/j1 as a string, and h1..h4 are uint32 - so a value
        // above Int.MAX_VALUE must not be truncated into a negative number.
        val normalized = AmneziaWGNormalizer.normalize(
            mapOf(
                "type" to "awg",
                "jc" to "4",
                "i1" to "0123456",
                "j1" to "42",
                "h1" to "3000000000"
            )
        )
        @Suppress("UNCHECKED_CAST")
        val amneziaMap = normalized["amnezia"] as? Map<String, Any?>
        assertNotNull(amneziaMap)
        assertEquals(4, amneziaMap!!["jc"])
        // Leading zeros and the string type are both load-bearing for i*/j*.
        assertEquals("0123456", amneziaMap["i1"])
        assertEquals("42", amneziaMap["j1"])
        assertEquals("3000000000", amneziaMap["h1"])
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
            "local_address" to "10.7.0.2"
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
        assertFalse(firstPeer.containsKey("reserved"))
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

    private fun wireguardWithReserved(server: String): Map<String, Any?> = mapOf(
        "type" to "wireguard",
        "server" to server,
        "server_port" to 2408,
        "peer_public_key" to "bmXOC+F1FxEMF9dyiK2H5/1SUtzH0JuVo51h2wPfgyo=",
        "local_address" to "172.16.0.2",
        "reserved" to listOf(14, 84, 156)
    )

    @Test
    fun testWarpDetectionUsesNetworkContainment() {
        // 188.114.96.0/20, not the whole /16.
        for (server in listOf("162.159.192.7", "162.159.193.7", "188.114.100.5", "188.114.111.255")) {
            val normalized = AmneziaWGNormalizer.normalizeWireGuardEndpoint(wireguardWithReserved(server))
            assertEquals("warp for $server", "warp", normalized["type"])
        }
    }

    @Test
    fun testWarpDetectionMatchesCompressedIpv6() {
        for (server in listOf("2606:4700:d0::a", "2606:4700:00d1:0:0:0:0:1")) {
            val normalized = AmneziaWGNormalizer.normalizeWireGuardEndpoint(wireguardWithReserved(server))
            assertEquals("warp for $server", "warp", normalized["type"])
        }
    }

    @Test
    fun testNonWarpReservedBytesAreRejected() {
        // 188.114.5.20 is outside 188.114.96.0/20 and must not become a WARP profile.
        for (server in listOf("188.114.5.20", "188.114.200.1", "162.159.194.7", "2606:4700:d2::1")) {
            var rejected = false
            try {
                AmneziaWGNormalizer.normalizeWireGuardEndpoint(wireguardWithReserved(server))
            } catch (e: IllegalArgumentException) {
                rejected = true
                assertTrue(e.message!!.contains("Cloudflare WARP"))
            }
            assertTrue("expected rejection for $server", rejected)
        }
    }

    @Test
    fun testPeerLevelReservedBytesOnNonWarpPeerAreRejected() {
        val peerMap = mapOf(
            "type" to "wireguard",
            "local_address" to "10.7.0.2",
            "peers" to listOf(
                mapOf(
                    "endpoint" to "203.0.113.9:51820",
                    "public_key" to "bmXOC+F1FxEMF9dyiK2H5/1SUtzH0JuVo51h2wPfgyo=",
                    "reserved" to listOf(14, 84, 156)
                )
            )
        )
        var rejected = false
        try {
            AmneziaWGNormalizer.normalizeWireGuardEndpoint(peerMap)
        } catch (e: IllegalArgumentException) {
            rejected = true
        }
        assertTrue(rejected)
    }
}
