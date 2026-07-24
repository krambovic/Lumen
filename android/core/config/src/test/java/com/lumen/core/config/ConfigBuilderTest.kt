package com.lumen.core.config

import com.lumen.core.config.builder.SingboxConfigBuilder
import com.lumen.core.config.builder.SingboxConfigOptions
import com.lumen.core.config.parser.ParsedNode
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ConfigBuilderTest {

    @Test
    fun testSingboxConfigBuilderVlessNode() {
        val node = ParsedNode(
            name = "Test VLESS REALITY",
            scheme = "vless",
            server = "1.2.3.4",
            port = 443,
            link = "vless://test-uuid@1.2.3.4:443?security=reality&pbk=testpubkey&fp=chrome#Test%20VLESS",
            outbound = mapOf(
                "uuid" to "test-uuid-1234",
                "flow" to "xtls-rprx-vision",
                "streamSettings" to mapOf(
                    "security" to "reality",
                    "realitySettings" to mapOf(
                        "publicKey" to "testpubkey",
                        "shortId" to "123456",
                        "fingerprint" to "chrome",
                        "serverName" to "example.com"
                    ),
                    "network" to "grpc",
                    "grpcSettings" to mapOf("serviceName" to "grpc-test")
                )
            )
        )

        val jsonString = SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions())
        val json = JSONObject(jsonString)

        assertTrue(json.has("inbounds"))
        assertTrue(json.has("outbounds"))
        assertTrue(json.has("dns"))

        val inbounds = json.getJSONArray("inbounds")
        val tunInbound = inbounds.getJSONObject(0)
        assertEquals("tun", tunInbound.getString("type"))
        assertEquals("172.19.0.1/30", tunInbound.getJSONArray("address").getString(0))
        assertEquals("fdfe:dcba:9876::1/126", tunInbound.getJSONArray("address").getString(1))

        val outbounds = json.getJSONArray("outbounds")
        val proxyOb = outbounds.getJSONObject(0)
        assertEquals("vless", proxyOb.getString("type"))
        assertEquals("1.2.3.4", proxyOb.getString("server"))
        assertEquals(443, proxyOb.getInt("server_port"))
        assertEquals("test-uuid-1234", proxyOb.getString("uuid"))

        assertTrue((0 until outbounds.length()).none { outbounds.getJSONObject(it).optString("type") == "dns" })
        val dnsServers = json.getJSONObject("dns").getJSONArray("servers")
        assertEquals("udp", dnsServers.getJSONObject(0).getString("type"))
        assertEquals("1.1.1.1", dnsServers.getJSONObject(0).getString("server"))
        assertTrue(!dnsServers.getJSONObject(0).has("address"))
        val route = json.getJSONObject("route")
        assertEquals("dns-direct", route.getString("default_domain_resolver"))
        val routeRules = route.getJSONArray("rules")
        // sing-box 1.12+: sniff must run before the `protocol: dns` matcher works.
        assertEquals("sniff", routeRules.getJSONObject(0).getString("action"))
        assertEquals("hijack-dns", routeRules.getJSONObject(1).getString("action"))
        assertEquals("dns", routeRules.getJSONObject(1).getString("protocol"))
        assertEquals("hijack-dns", routeRules.getJSONObject(2).getString("action"))
        assertEquals(53, routeRules.getJSONObject(2).getInt("port"))
        assertTrue(!routeRules.getJSONObject(1).has("outbound"))
        // Legacy inbound-level sniff fields were removed in sing-box 1.12+.
        assertTrue(!tunInbound.has("sniff"))
        assertTrue(!tunInbound.has("sniff_override_destination"))
    }

    @Test
    fun testSingboxSocksOnlyConfigDisablesInterfaceAutoDetection() {
        val node = ParsedNode(
            name = "Android SOCKS node",
            scheme = "trojan",
            server = "1.2.3.4",
            port = 443,
            link = "trojan://password@1.2.3.4:443",
            outbound = mapOf("password" to "password")
        )

        val jsonString = SingboxConfigBuilder.buildConfig(
            node,
            SingboxConfigOptions(tunMode = false)
        )
        val route = JSONObject(jsonString).getJSONObject("route")

        assertEquals(false, route.getBoolean("auto_detect_interface"))
    }

    @Test
    fun testSingboxConfigBuilderWireGuardNode() {
        val node = ParsedNode(
            name = "Test WireGuard AWG",
            scheme = "awg",
            server = "5.6.7.8",
            port = 51820,
            link = "wireguard://...",
            outbound = mapOf(
                "type" to "awg",
                "private_key" to "privkey123=",
                "peer_public_key" to "pubkey123=",
                "local_address" to "10.0.0.2",
                "jc" to 5,
                "jmin" to 50,
                "jmax" to 100
            )
        )

        val jsonString = SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions())
        val json = JSONObject(jsonString)

        assertTrue(json.has("endpoints"))
        val endpoints = json.getJSONArray("endpoints")
        val wgEndpoint = endpoints.getJSONObject(0)
        assertEquals("wireguard", wgEndpoint.getString("type"))
        val amneziaObj = wgEndpoint.getJSONObject("amnezia")
        assertEquals(5, amneziaObj.getInt("jc"))
        assertEquals(50, amneziaObj.getInt("jmin"))
        assertEquals(100, amneziaObj.getInt("jmax"))
        assertEquals(false, wgEndpoint.getBoolean("system"))
    }

    @Test
    fun testSingboxAutoSelectorOutbound() {
        val node1 = ParsedNode(
            name = "Node 1",
            scheme = "vless",
            server = "1.1.1.1",
            port = 443,
            link = "vless://..."
        )
        val node2 = ParsedNode(
            name = "Node 2 WireGuard",
            scheme = "wireguard",
            server = "2.2.2.2",
            port = 51820,
            link = "wireguard://...",
            outbound = mapOf(
                "type" to "wireguard",
                "private_key" to "key=",
                "peer_public_key" to "pubkey="
            )
        )
        val autoNode = ParsedNode(
            name = "Auto Node Pool",
            scheme = "auto",
            server = "",
            port = 0,
            link = ""
        )

        val jsonString = SingboxConfigBuilder.buildConfig(listOf(node1, node2, autoNode), autoNode, SingboxConfigOptions())
        val json = JSONObject(jsonString)

        val outbounds = json.getJSONArray("outbounds")
        val autoOb = outbounds.getJSONObject(0)
        assertEquals("urltest", autoOb.getString("type"))
        assertEquals("proxy", autoOb.getString("tag"))
        val poolTags = autoOb.getJSONArray("outbounds")
        assertEquals(2, poolTags.length())
        assertEquals("proxy-0", poolTags.getString(0))
        assertEquals("proxy-1", poolTags.getString(1))
    }

    @Test
    fun singboxUsesUserTrafficSettings() {
        val node = ParsedNode(
            name = "Auto",
            scheme = "auto",
            server = "",
            port = 0,
            link = ""
        )
        val json = JSONObject(
            SingboxConfigBuilder.buildConfig(
                emptyList(),
                node,
                SingboxConfigOptions(
                    logLevel = "warning",
                    proxyDnsServer = "9.9.9.9",
                    directDnsServer = "1.0.0.1",
                    urlTestUrl = "https://example.com/ping",
                    urlTestIntervalMinutes = 7,
                    urlTestToleranceMs = 125
                )
            )
        )
        assertEquals("warning", json.getJSONObject("log").getString("level"))
        val auto = json.getJSONArray("outbounds").getJSONObject(0)
        assertEquals("https://example.com/ping", auto.getString("url"))
        assertEquals("7m", auto.getString("interval"))
        assertEquals(125, auto.getInt("tolerance"))
    }
}
