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
        val dnsRemote = (0 until dnsServers.length())
            .map { dnsServers.getJSONObject(it) }
            .first { it.optString("tag") == "dns-proxy-1" }
        assertEquals("https", dnsRemote.getString("type"))
        assertEquals("cloudflare-dns.com", dnsRemote.getString("server"))
        assertEquals("proxy", dnsRemote.getString("detour"))
        // 1.12+ dial fields: strategy moved inside domain_resolver.
        assertEquals("dns-bootstrap", dnsRemote.getJSONObject("domain_resolver").getString("server"))
        assertTrue(!dnsRemote.has("domain_strategy"))
        assertTrue(!dnsRemote.has("address"))
        val bootstrap = (0 until dnsServers.length())
            .map { dnsServers.getJSONObject(it) }
            .first { it.optString("tag") == "dns-bootstrap" }
        assertEquals("udp", bootstrap.getString("type"))
        assertEquals("direct", bootstrap.getString("detour"))
        val route = json.getJSONObject("route")
        assertEquals("dns-bootstrap", route.getJSONObject("default_domain_resolver").getString("server"))
        val routeRules = route.getJSONArray("rules")
        val rules = (0 until routeRules.length()).map { routeRules.getJSONObject(it) }
        assertEquals("sniff", rules.first().getString("action"))
        assertTrue(rules.any { it.optString("protocol") == "dns" && it.optString("action") == "hijack-dns" })
        assertTrue(rules.any { it.optInt("port") == 53 && it.optString("action") == "hijack-dns" })
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
    fun autoNodeWithoutPoolStaysValid() {
        val node = ParsedNode("Auto", "auto", "", 0, "")
        val json = JSONObject(SingboxConfigBuilder.buildConfig(emptyList(), node, SingboxConfigOptions()))
        val proxy = json.getJSONArray("outbounds").getJSONObject(0)
        assertEquals("proxy", proxy.getString("tag"))
        assertEquals("direct", proxy.getString("type"))
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
        // Auto needs at least one real server: an empty pool now falls back to direct.
        val poolNode = ParsedNode("S1", "trojan", "1.2.3.4", 443, "", mapOf("password" to "x"))
        val json = JSONObject(
            SingboxConfigBuilder.buildConfig(
                listOf(poolNode, node),
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

    @Test
    fun managedDnsUsesProxyHostsAndIpv4Policy() {
        val node = ParsedNode("DNS", "trojan", "1.2.3.4", 443, "", mapOf("password" to "x"))
        val json = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(
                    tunMode = false,
                    dnsMode = "secure",
                    dnsProxyServers = listOf("dns.google"),
                    dnsHosts = mapOf("example.test" to listOf("192.0.2.10")),
                    dnsOverrideEnabled = true,
                    dnsOverrideHostname = "ntc.party",
                    dnsOverrideIpv4 = "130.255.77.28",
                    dnsProxyIpv4Only = true
                )
            )
        )
        val dns = json.getJSONObject("dns")
        val servers = (0 until dns.getJSONArray("servers").length())
            .map { dns.getJSONArray("servers").getJSONObject(it) }
        val remote = servers.first { it.optString("tag") == "dns-proxy-1" }
        assertEquals("proxy", remote.getString("detour"))
        val hosts = servers.first { it.optString("tag") == "dns-hosts" }.getJSONObject("predefined")
        assertEquals("130.255.77.28", hosts.getJSONArray("ntc.party").getString(0))
        assertEquals("192.0.2.10", hosts.getJSONArray("example.test").getString(0))
        val rules = dns.getJSONArray("rules")
        assertTrue((0 until rules.length()).any { rules.getJSONObject(it).optJSONArray("query_type")?.toString()?.contains("AAAA") == true })
    }

    @Test
    fun androidDnsDoesNotHijackPort53() {
        val node = ParsedNode("DNS", "trojan", "1.2.3.4", 443, "", mapOf("password" to "x"))
        val json = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(tunMode = false, dnsMode = "android")
            )
        )
        assertEquals("dns-direct-1", json.getJSONObject("dns").getString("final"))
        val rules = json.getJSONObject("route").getJSONArray("rules")
        assertTrue((0 until rules.length()).none { rules.getJSONObject(it).optString("action") == "hijack-dns" })
        assertTrue((0 until rules.length()).any {
            val rule = rules.getJSONObject(it)
            rule.optInt("port") == 53 && rule.optString("outbound") == "direct"
        })
    }
}
