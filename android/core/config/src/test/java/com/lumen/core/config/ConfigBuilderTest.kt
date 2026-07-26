package com.lumen.core.config

import com.lumen.core.config.builder.SingboxConfigBuilder
import com.lumen.core.config.builder.SingboxConfigOptions
import com.lumen.core.config.parser.LinkParser
import com.lumen.core.config.parser.ParsedNode
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Base64

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
        assertEquals("dns-bootstrap", dnsRemote.getString("domain_resolver"))
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
        val hijack = rules.first { it.optString("action") == "hijack-dns" }
        assertEquals("logical", hijack.getString("type"))
        assertEquals("or", hijack.getString("mode"))
        val hijackRules = hijack.getJSONArray("rules")
        assertTrue((0 until hijackRules.length()).any { hijackRules.getJSONObject(it).optString("protocol") == "dns" })
        assertTrue((0 until hijackRules.length()).any { hijackRules.getJSONObject(it).optInt("port") == 53 })
        assertTrue(rules.none {
            it.optJSONArray("domain_suffix")?.toString()?.contains("cloudflare-dns.com") == true
        })
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
    fun authenticatedSocksAndHttpsProxyCredentialsReachSingbox() {
        val socks = LinkParser.parseSingle("socks5://alice:secret@socks.example.com:1080#SOCKS")
        val socksOutbound = JSONObject(SingboxConfigBuilder.buildConfig(socks))
            .getJSONArray("outbounds").getJSONObject(0)
        assertEquals("socks", socksOutbound.getString("type"))
        assertEquals("alice", socksOutbound.getString("username"))
        assertEquals("secret", socksOutbound.getString("password"))

        val https = LinkParser.parseSingle("https://bob:password@proxy.example.com:8443#HTTPS")
        val httpsOutbound = JSONObject(SingboxConfigBuilder.buildConfig(https))
            .getJSONArray("outbounds").getJSONObject(0)
        assertEquals("http", httpsOutbound.getString("type"))
        assertEquals("bob", httpsOutbound.getString("username"))
        assertEquals("password", httpsOutbound.getString("password"))
        assertTrue(httpsOutbound.getJSONObject("tls").getBoolean("enabled"))
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
    fun autoNodeWithoutPoolFailsInsteadOfSilentlyUsingDirect() {
        val node = ParsedNode("Auto", "auto", "", 0, "")
        val error = runCatching {
            SingboxConfigBuilder.buildConfig(emptyList(), node, SingboxConfigOptions())
        }.exceptionOrNull()
        assertTrue(error is IllegalArgumentException)
        assertTrue(error?.message.orEmpty().contains("no usable servers"))
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
        // A manually created Auto node uses the real non-AUTO servers as its pool.
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
        assertTrue((0 until rules.length()).none {
            rules.getJSONObject(it).optJSONArray("domain_suffix")?.toString()?.contains("dns.google") == true
        })
    }

    @Test
    fun proxyServerAndDohHostnamesUseBootstrapResolver() {
        val node = ParsedNode(
            "Domain VLESS",
            "vless",
            "edge.example.com",
            443,
            "",
            mapOf("uuid" to "00000000-0000-0000-0000-000000000003")
        )
        val json = JSONObject(SingboxConfigBuilder.buildConfig(node))
        val proxy = json.getJSONArray("outbounds").getJSONObject(0)
        assertEquals("dns-bootstrap", proxy.getString("domain_resolver"))
        val dns = json.getJSONObject("dns").getJSONArray("servers")
        val remote = (0 until dns.length())
            .map { dns.getJSONObject(it) }
            .first { it.optString("tag") == "dns-proxy-1" }
        assertEquals("dns-bootstrap", remote.getString("domain_resolver"))
    }

    @Test
    fun publicProfileDnsDoesNotReplaceWorkingProxyDns() {
        val node = ParsedNode(
            "WireGuard DNS",
            "wireguard",
            "162.159.192.1",
            2408,
            "",
            mapOf(
                "type" to "wireguard",
                "private_key" to Base64.getEncoder().encodeToString(ByteArray(32) { 1 }),
                "server" to "162.159.192.1",
                "server_port" to 2408,
                "peer_public_key" to Base64.getEncoder().encodeToString(ByteArray(32) { 2 }),
                "local_address" to listOf("172.16.0.2/32"),
                "_dns" to listOf("1.1.1.1")
            )
        )
        val dns = JSONObject(SingboxConfigBuilder.buildConfig(node)).getJSONObject("dns")
        assertEquals("dns-proxy-1", dns.getString("final"))
        val servers = dns.getJSONArray("servers")
        assertTrue((0 until servers.length()).none {
            servers.getJSONObject(it).optString("tag").startsWith("dns-vpn-")
        })
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

    @Test
    fun testSingboxMasqueOutboundKeepsDirectEndpointFieldsAtTopLevel() {
        val node = ParsedNode(
            name = "WARP Masque",
            scheme = "masque",
            server = "162.159.193.1",
            port = 2408,
            link = "clash-masque",
            outbound = mapOf(
                "type" to "masque",
                "server" to "162.159.193.1",
                "server_port" to 2408,
                "private_key" to "private_key_xyz",
                "public_key" to "public_key_abc",
                "address" to listOf("172.16.0.2/32"),
                "mtu" to 1280,
                "profile" to mapOf("detour" to "direct")
            )
        )
        val json = JSONObject(SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions()))
        val proxyOb = json.getJSONArray("outbounds").getJSONObject(0)
        assertEquals("masque", proxyOb.getString("type"))
        // The bundled -lumen core accepts the direct Clash/usque endpoint only at
        // the top level. Dropping it leaves the tunnel without an endpoint.
        assertEquals("162.159.193.1", proxyOb.getString("server"))
        assertEquals(2408, proxyOb.getInt("server_port"))
        assertEquals("private_key_xyz", proxyOb.getString("private_key"))
        assertEquals("public_key_abc", proxyOb.getString("public_key"))
        assertEquals(1280, proxyOb.getInt("mtu"))
        assertTrue(proxyOb.has("address"))
        val profile = proxyOb.getJSONObject("profile")
        assertEquals("direct", profile.getString("detour"))
        // ...and rejects every one of them inside `profile`.
        for (key in listOf("server", "server_port", "public_key", "address", "mtu")) {
            assertTrue("MASQUE profile must not contain $key", !profile.has(key))
        }
    }

    @Test
    fun testSingboxMasqueLegacyProfileStringIsSanitized() {
        // Nodes restored from the database can carry `profile` as a JSON string
        // with legacy endpoint fields inside. The core rejects the whole config
        // on any unknown field, so they must be dropped.
        val node = ParsedNode(
            name = "WARP Masque legacy",
            scheme = "masque",
            server = "",
            port = 0,
            link = "legacy-masque",
            outbound = mapOf(
                "type" to "masque",
                "profile" to """{"detour":"direct","id":"profile-uuid","auth_token":"tok","server":"162.159.193.1","server_port":2408,"mtu":1280,"legacy_unknown":true}"""
            )
        )
        val json = JSONObject(SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions()))
        val proxyOb = json.getJSONArray("outbounds").getJSONObject(0)
        assertEquals("masque", proxyOb.getString("type"))
        // Endpoint fields are hoisted out of the legacy profile, not discarded.
        assertEquals("162.159.193.1", proxyOb.getString("server"))
        assertEquals(2408, proxyOb.getInt("server_port"))
        assertEquals(1280, proxyOb.getInt("mtu"))
        val profile = proxyOb.getJSONObject("profile")
        assertEquals("direct", profile.getString("detour"))
        assertEquals("profile-uuid", profile.getString("id"))
        assertEquals("tok", profile.getString("auth_token"))
        assertTrue(!profile.has("server"))
        assertTrue(!profile.has("server_port"))
        assertTrue(!profile.has("mtu"))
        // Genuinely unknown keys are still dropped: the core fails hard on them.
        assertTrue(!profile.has("legacy_unknown"))
        assertTrue(!proxyOb.has("legacy_unknown"))
    }

    @Test
    fun shadowsocks2022AesMethodIsCorrectedFromDecodedKeySize() {
        val key32 = Base64.getEncoder().encodeToString(ByteArray(32) { it.toByte() })
        val node = ParsedNode(
            name = "CommandVPN wifi-249",
            scheme = "shadowsocks",
            server = "130.51.23.246",
            port = 443,
            link = "xray-json",
            outbound = mapOf(
                "settings" to mapOf(
                    "servers" to listOf(
                        mapOf(
                            "method" to "2022-blake3-aes-128-gcm",
                            "password" to "$key32:$key32"
                        )
                    )
                )
            )
        )

        val json = JSONObject(SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions()))
        val proxyOb = json.getJSONArray("outbounds").getJSONObject(0)
        assertEquals("2022-blake3-aes-256-gcm", proxyOb.getString("method"))
    }

    @Test
    fun testOpenVpnHttpProxyBecomesDetourOutbound() {
        val node = ParsedNode(
            name = "OVPN via proxy",
            scheme = "openvpn",
            server = "vpn.example.com",
            port = 1194,
            link = "ovpn",
            outbound = mapOf(
                "type" to "openvpn",
                "server" to "vpn.example.com",
                "server_port" to 1194,
                "lumen_proxy" to mapOf(
                    "type" to "http",
                    "server" to "10.0.0.5",
                    "server_port" to 8080,
                    "username" to "user",
                    "password" to "secret"
                )
            )
        )
        val outbounds = JSONObject(SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions()))
            .getJSONArray("outbounds")
        val proxyOb = outbounds.getJSONObject(0)
        assertEquals("openvpn", proxyOb.getString("type"))
        assertTrue(!proxyOb.has("lumen_proxy"))
        val detourTag = proxyOb.getString("detour")
        val detour = (0 until outbounds.length())
            .map { outbounds.getJSONObject(it) }
            .first { it.optString("tag") == detourTag }
        assertEquals("http", detour.getString("type"))
        assertEquals("10.0.0.5", detour.getString("server"))
        assertEquals(8080, detour.getInt("server_port"))
        assertEquals("user", detour.getString("username"))
        assertEquals("secret", detour.getString("password"))
    }

    @Test
    fun testOpenVpnObfsProxyPointsAtLocalRelay() {
        // obfs2/obfs3 are unwrapped by the in-app relay, so the core only ever
        // sees a plain loopback SOCKS5 detour.
        val node = ParsedNode(
            name = "OVPN via obfs3",
            scheme = "openvpn",
            server = "vpn.example.com",
            port = 1194,
            link = "ovpn",
            outbound = mapOf(
                "type" to "openvpn",
                "server" to "vpn.example.com",
                "server_port" to 1194,
                "lumen_proxy" to mapOf(
                    "type" to "obfs3",
                    "server" to "bridge.example.com",
                    "server_port" to 9001
                )
            )
        )
        val outbounds = JSONObject(SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions()))
            .getJSONArray("outbounds")
        val proxyOb = outbounds.getJSONObject(0)
        assertTrue(!proxyOb.has("lumen_proxy"))
        val detourTag = proxyOb.getString("detour")
        val detour = (0 until outbounds.length())
            .map { outbounds.getJSONObject(it) }
            .first { it.optString("tag") == detourTag }
        assertEquals("socks", detour.getString("type"))
        assertEquals("127.0.0.1", detour.getString("server"))
        assertEquals(SingboxConfigBuilder.OBFS_LOCAL_PORT, detour.getInt("server_port"))
    }

    @Test
    fun autoPoolNormalizesLegacyUtlsFingerprint() {
        val member = mapOf<String, Any?>(
            "name" to "Legacy Chrome",
            "scheme" to "vless",
            "server" to "edge.example.com",
            "port" to 443,
            "link" to "",
            "outbound" to mapOf(
                "protocol" to "vless",
                "singbox" to mapOf(
                    "type" to "vless",
                    "server" to "edge.example.com",
                    "server_port" to 443,
                    "uuid" to "00000000-0000-0000-0000-000000000001",
                    "tls" to mapOf(
                        "enabled" to true,
                        "utls" to mapOf(
                            "enabled" to true,
                            "fingerprint" to "HelloChrome_120"
                        )
                    )
                )
            )
        )
        val auto = ParsedNode(
            name = "AUTO",
            scheme = "auto",
            server = "",
            port = 0,
            link = "auto",
            outbound = mapOf("protocol" to "auto", "auto_members" to listOf(member))
        )

        val outbounds = JSONObject(SingboxConfigBuilder.buildConfig(auto))
            .getJSONArray("outbounds")
        val proxyMember = (0 until outbounds.length())
            .map { outbounds.getJSONObject(it) }
            .first { it.optString("tag") == "proxy-0" }
        assertEquals(
            "chrome",
            proxyMember.getJSONObject("tls").getJSONObject("utls").getString("fingerprint")
        )
    }

    @Test
    fun nativeAnyTlsGetsRequiredTlsBlock() {
        val node = ParsedNode(
            name = "AnyTLS",
            scheme = "anytls",
            server = "anytls.example.com",
            port = 443,
            link = "",
            outbound = mapOf(
                "protocol" to "anytls",
                "singbox" to mapOf(
                    "type" to "anytls",
                    "server" to "anytls.example.com",
                    "server_port" to 443,
                    "password" to "secret"
                )
            )
        )

        val proxy = JSONObject(SingboxConfigBuilder.buildConfig(node))
            .getJSONArray("outbounds")
            .getJSONObject(0)
        val tls = proxy.getJSONObject("tls")
        assertTrue(tls.getBoolean("enabled"))
        assertEquals("anytls.example.com", tls.getString("server_name"))
    }

    @Test
    fun nativeVlessVisionGetsRequiredTlsBlock() {
        val node = ParsedNode(
            name = "VLESS Vision",
            scheme = "vless",
            server = "vision.example.com",
            port = 443,
            link = "",
            outbound = mapOf(
                "protocol" to "vless",
                "singbox" to mapOf(
                    "type" to "vless",
                    "server" to "vision.example.com",
                    "server_port" to 443,
                    "uuid" to "00000000-0000-0000-0000-000000000002",
                    "flow" to "xtls-rprx-vision"
                )
            )
        )

        val proxy = JSONObject(SingboxConfigBuilder.buildConfig(node))
            .getJSONArray("outbounds")
            .getJSONObject(0)
        assertTrue(proxy.getJSONObject("tls").getBoolean("enabled"))
    }

    @Test
    fun realityWithoutFingerprintStillGetsUtls() {
        // `uTLS is required by reality client` aborts the whole config, so a
        // link without fp= must still produce a uTLS block.
        val node = ParsedNode(
            name = "REALITY no fp",
            scheme = "vless",
            server = "1.2.3.4",
            port = 443,
            link = "vless://uuid@1.2.3.4:443?security=reality&pbk=testpubkey&sid=1234",
            outbound = mapOf(
                "uuid" to "00000000-0000-0000-0000-000000000004",
                "flow" to "xtls-rprx-vision",
                "streamSettings" to mapOf(
                    "security" to "reality",
                    "realitySettings" to mapOf(
                        "publicKey" to "testpubkey",
                        "shortId" to "1234",
                        "serverName" to "example.com"
                    ),
                    "network" to "tcp"
                )
            )
        )

        val tls = JSONObject(SingboxConfigBuilder.buildConfig(node))
            .getJSONArray("outbounds").getJSONObject(0).getJSONObject("tls")
        assertTrue(tls.getJSONObject("reality").getBoolean("enabled"))
        val utls = tls.getJSONObject("utls")
        assertTrue(utls.getBoolean("enabled"))
        assertEquals("chrome", utls.getString("fingerprint"))
    }

    @Test
    fun nativeRealityOutboundWithoutUtlsIsRepaired() {
        val node = ParsedNode(
            name = "Imported REALITY",
            scheme = "vless",
            server = "edge.example.com",
            port = 443,
            link = "",
            outbound = mapOf(
                "protocol" to "vless",
                "singbox" to mapOf(
                    "type" to "vless",
                    "server" to "edge.example.com",
                    "server_port" to 443,
                    "uuid" to "00000000-0000-0000-0000-000000000005",
                    "tls" to mapOf(
                        "enabled" to true,
                        "server_name" to "edge.example.com",
                        "reality" to mapOf(
                            "enabled" to true,
                            "public_key" to "testpubkey",
                            "short_id" to "1234"
                        )
                    )
                )
            )
        )

        val tls = JSONObject(SingboxConfigBuilder.buildConfig(node))
            .getJSONArray("outbounds").getJSONObject(0).getJSONObject("tls")
        assertEquals("chrome", tls.getJSONObject("utls").getString("fingerprint"))
    }

    @Test
    fun webSocketEarlyDataLeavesThePathAndBecomesMaxEarlyData() {
        val node = ParsedNode(
            name = "WS early data",
            scheme = "vless",
            server = "host.example",
            port = 443,
            link = "",
            outbound = mapOf(
                "uuid" to "00000000-0000-0000-0000-000000000006",
                "streamSettings" to mapOf(
                    "security" to "tls",
                    "tlsSettings" to mapOf("serverName" to "cdn.example.com"),
                    "network" to "ws",
                    "wsSettings" to mapOf("path" to "/proxy?ed=2048")
                )
            )
        )

        val transport = JSONObject(SingboxConfigBuilder.buildConfig(node))
            .getJSONArray("outbounds").getJSONObject(0).getJSONObject("transport")
        assertEquals("ws", transport.getString("type"))
        assertEquals("/proxy", transport.getString("path"))
        assertEquals(2048, transport.getInt("max_early_data"))
        assertEquals("Sec-WebSocket-Protocol", transport.getString("early_data_header_name"))
    }

    @Test
    fun httpUpgradePathQueryIsStrippedWithoutEarlyDataFields() {
        // The httpupgrade transport has no early-data options; keeping the query
        // would only percent-encode '?' into the request target.
        val node = ParsedNode(
            name = "httpupgrade early data",
            scheme = "vless",
            server = "host.example",
            port = 443,
            link = "",
            outbound = mapOf(
                "uuid" to "00000000-0000-0000-0000-000000000007",
                "streamSettings" to mapOf(
                    "security" to "tls",
                    "network" to "httpupgrade",
                    "httpupgradeSettings" to mapOf("path" to "/up?ed=2560", "host" to "cdn.example.com")
                )
            )
        )

        val transport = JSONObject(SingboxConfigBuilder.buildConfig(node))
            .getJSONArray("outbounds").getJSONObject(0).getJSONObject("transport")
        assertEquals("/up", transport.getString("path"))
        assertTrue(!transport.has("max_early_data"))
        assertTrue(!transport.has("early_data_header_name"))
    }

    @Test
    fun autoPoolMembersKeepTheGeneratedTag() {
        // Provider configs carry their own outbound tags. The fallback branch used to
        // copy them over the generated proxy-N tag, so the urltest pool referenced a
        // member that was never emitted and the core aborted at startup with
        // `dependency[proxy-N] not found for outbound[proxy]` (exit code 1).
        val vless = ParsedNode(
            "v", "vless", "1.1.1.1", 443, "",
            mapOf("uuid" to "00000000-0000-0000-0000-000000000001", "tag" to "eu-1")
        )
        val hysteria = ParsedNode(
            "h", "hysteria", "2.2.2.2", 443, "",
            mapOf("tag" to "eu-29", "auth_str" to "secret", "up_mbps" to 50, "down_mbps" to 200)
        )
        val auto = ParsedNode("AUTO", "auto", "", 0, "", emptyMap())

        val outs = JSONObject(SingboxConfigBuilder.buildConfig(listOf(auto, vless, hysteria), auto))
            .getJSONArray("outbounds")
        val all = (0 until outs.length()).map { outs.getJSONObject(it) }
        val tags = all.map { it.optString("tag") }.toSet()
        val members = all.first { it.optString("tag") == "proxy" }.getJSONArray("outbounds")

        assertTrue("the pool must not be empty", members.length() > 0)
        for (i in 0 until members.length()) {
            val member = members.getString(i)
            assertTrue("pool references $member but no outbound carries that tag", member in tags)
        }
        assertTrue("the provider tag must not survive", "eu-29" !in tags)
        assertTrue("the provider tag must not survive", "eu-1" !in tags)
    }

    @Test
    fun kcpStreamSettingsBecomeAnMkcpTransport() {
        // Without this branch a kcp link was emitted with no transport at all, so
        // the core dialled raw TCP at an mKCP listener and every attempt timed out.
        val node = ParsedNode(
            name = "kcp node",
            scheme = "vless",
            server = "host.example",
            port = 443,
            link = "",
            outbound = mapOf(
                "uuid" to "00000000-0000-0000-0000-000000000008",
                "streamSettings" to mapOf(
                    "network" to "kcp",
                    "kcpSettings" to mapOf(
                        "mtu" to 1350,
                        "tti" to 50,
                        "uplinkCapacity" to 5,
                        "downlinkCapacity" to 20,
                        "seed" to "abc",
                        "congestion" to "1",
                        "header" to mapOf("type" to "srtp")
                    )
                )
            )
        )

        val transport = JSONObject(SingboxConfigBuilder.buildConfig(node))
            .getJSONArray("outbounds").getJSONObject(0).getJSONObject("transport")
        assertEquals("mkcp", transport.getString("type"))
        assertEquals("srtp", transport.getString("header_type"))
        assertEquals(1350, transport.getInt("mtu"))
        assertEquals(50, transport.getInt("tti"))
        assertEquals(5, transport.getInt("uplink_capacity"))
        assertEquals(20, transport.getInt("downlink_capacity"))
        assertEquals("abc", transport.getString("seed"))
        assertTrue(transport.getBoolean("congestion"))
    }

    @Test
    fun directDnsRuleSkipsProxyBlockAndGeoRoutingEntries() {
        val node = ParsedNode("DNS", "trojan", "1.2.3.4", 443, "", mapOf("password" to "x"))
        val json = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(
                    tunMode = false,
                    directDomains = listOf(
                        "geoip:us",
                        "geosite:category-ads",
                        "proxy:netflix.com",
                        "block:ads.doubleclick.net",
                        "gov.ru"
                    )
                )
            )
        )
        val dnsRules = json.getJSONObject("dns").getJSONArray("rules")
        val directRule = (0 until dnsRules.length())
            .map { dnsRules.getJSONObject(it) }
            .first { it.optString("server") == "dns-direct-1" }
        val suffixes = directRule.getJSONArray("domain_suffix")
        assertEquals(1, suffixes.length())
        assertEquals("gov.ru", suffixes.getString(0))
    }

    @Test
    fun fragmentationAndPreferIpv6ReachTheGeneratedConfig() {
        val node = ParsedNode("Frag", "trojan", "1.2.3.4", 443, "", mapOf("password" to "x"))
        val json = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(
                    tunMode = false,
                    enableFinalFragment = true,
                    preferIpv6 = true
                )
            )
        )
        val routeRules = json.getJSONObject("route").getJSONArray("rules")
        val fragment = (0 until routeRules.length())
            .map { routeRules.getJSONObject(it) }
            .first { it.optString("action") == "route-options" }
        assertTrue(fragment.getBoolean("tls_fragment"))
        assertEquals("500ms", fragment.getString("tls_fragment_fallback_delay"))
        assertEquals(
            "prefer_ipv6",
            json.getJSONObject("route").getJSONObject("default_domain_resolver").getString("strategy")
        )
        val dns = json.getJSONObject("dns")
        assertEquals("prefer_ipv6", dns.getString("strategy"))
        val dnsRules = dns.getJSONArray("rules")
        assertTrue((0 until dnsRules.length()).none {
            dnsRules.getJSONObject(it).optJSONArray("query_type")?.toString()?.contains("AAAA") == true
        })
    }

    @Test
    fun jsonImportedHysteria2KeepsItsTlsAndLeavesSpeedsUnset() {
        val node = ParsedNode(
            name = "hysteria2 json",
            scheme = "hysteria2",
            server = "h.example",
            port = 443,
            link = "",
            outbound = mapOf(
                "protocol" to "hysteria2",
                "password" to "p",
                "tls" to mapOf("enabled" to true, "alpn" to listOf("h3"), "insecure" to true)
            )
        )

        val proxy = JSONObject(SingboxConfigBuilder.buildConfig(node))
            .getJSONArray("outbounds").getJSONObject(0)
        val tls = proxy.getJSONObject("tls")
        assertEquals("h3", tls.getJSONArray("alpn").getString(0))
        assertTrue(tls.getBoolean("insecure"))
        assertEquals("h.example", tls.getString("server_name"))
        // Unset speeds keep Hysteria v2 on BBR instead of Brutal at 50/200.
        assertTrue(!proxy.has("up_mbps"))
        assertTrue(!proxy.has("down_mbps"))
    }

    @Test
    fun jsonImportedTuicKeepsItsTls() {
        val node = ParsedNode(
            name = "tuic json",
            scheme = "tuic",
            server = "t.example",
            port = 443,
            link = "",
            outbound = mapOf(
                "protocol" to "tuic",
                "uuid" to "00000000-0000-0000-0000-000000000008",
                "password" to "p",
                "tls" to mapOf(
                    "enabled" to true,
                    "server_name" to "sni.example",
                    "alpn" to listOf("h3"),
                    "insecure" to true
                )
            )
        )

        val tls = JSONObject(SingboxConfigBuilder.buildConfig(node))
            .getJSONArray("outbounds").getJSONObject(0).getJSONObject("tls")
        assertEquals("sni.example", tls.getString("server_name"))
        assertEquals("h3", tls.getJSONArray("alpn").getString(0))
        assertTrue(tls.getBoolean("insecure"))
    }

    @Test
    fun shadowsocksWithoutMethodIsDroppedInsteadOfKillingTheAutoPool() {
        val broken = ParsedNode(
            name = "Broken SS",
            scheme = "shadowsocks",
            server = "s.example",
            port = 443,
            link = "",
            outbound = mapOf("settings" to mapOf("servers" to listOf(mapOf("password" to "x"))))
        )
        val healthy = ParsedNode("Healthy", "trojan", "1.2.3.4", 443, "", mapOf("password" to "x"))
        val auto = ParsedNode("AUTO", "auto", "", 0, "")

        val outbounds = JSONObject(
            SingboxConfigBuilder.buildConfig(listOf(healthy, broken, auto), auto, SingboxConfigOptions())
        ).getJSONArray("outbounds")
        val poolTags = outbounds.getJSONObject(0).getJSONArray("outbounds")
        assertEquals(1, poolTags.length())
        assertEquals("proxy-0", poolTags.getString(0))
        assertTrue((0 until outbounds.length()).none {
            outbounds.getJSONObject(it).optString("type") == "shadowsocks"
        })

        val error = runCatching {
            SingboxConfigBuilder.buildConfig(broken, SingboxConfigOptions())
        }.exceptionOrNull()
        assertTrue(error is IllegalArgumentException)
    }

    @Test
    fun shadowsocksSip003PluginReachesSingbox() {
        val node = ParsedNode(
            name = "SS obfs",
            scheme = "shadowsocks",
            server = "s.example",
            port = 443,
            link = "",
            outbound = mapOf(
                "method" to "aes-128-gcm",
                "password" to "pw",
                "plugin" to "obfs-local",
                "plugin_opts" to "obfs=http;obfs-host=bing.com"
            )
        )

        val proxy = JSONObject(SingboxConfigBuilder.buildConfig(node))
            .getJSONArray("outbounds").getJSONObject(0)
        assertEquals("obfs-local", proxy.getString("plugin"))
        assertEquals("obfs=http;obfs-host=bing.com", proxy.getString("plugin_opts"))
    }

    // --- Settings backed by verified sing-box-extended fields -----------------

    private fun outboundsByTag(json: JSONObject, key: String): Map<String, JSONObject> {
        val array = json.optJSONArray(key) ?: return emptyMap()
        return (0 until array.length())
            .map { array.getJSONObject(it) }
            .associateBy { it.optString("tag") }
    }

    private fun mixedPool(): List<ParsedNode> = listOf(
        ParsedNode(
            name = "SS",
            scheme = "shadowsocks",
            server = "1.2.3.5",
            port = 8388,
            link = "",
            outbound = mapOf("method" to "aes-128-gcm", "password" to "p")
        ),
        ParsedNode(
            name = "AnyTLS",
            scheme = "anytls",
            server = "1.2.3.6",
            port = 443,
            link = "",
            outbound = mapOf(
                "protocol" to "anytls",
                "singbox" to mapOf(
                    "type" to "anytls",
                    "server" to "1.2.3.6",
                    "server_port" to 443,
                    "password" to "p"
                )
            )
        ),
        ParsedNode(
            name = "WG",
            scheme = "wireguard",
            server = "1.2.3.7",
            port = 51820,
            link = "",
            outbound = mapOf(
                "type" to "wireguard",
                "private_key" to "privkey123=",
                "peer_public_key" to "pubkey123=",
                "local_address" to "10.0.0.2"
            )
        )
    )

    private fun buildMixedPool(options: SingboxConfigOptions): JSONObject {
        val auto = ParsedNode("AUTO", "auto", "", 0, "")
        return JSONObject(SingboxConfigBuilder.buildConfig(mixedPool() + auto, auto, options))
    }

    @Test
    fun multiplexProtocolMinStreamsPaddingAndBrutalAreConfigurable() {
        val node = ParsedNode(
            "Mux", "vless", "1.2.3.4", 443, "",
            mapOf("uuid" to "00000000-0000-0000-0000-000000000009")
        )

        // Untouched multiplex options keep the shipped smux/4/padding shape.
        val default = JSONObject(
            SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions(multiplexEnabled = true))
        ).getJSONArray("outbounds").getJSONObject(0).getJSONObject("multiplex")
        assertEquals("smux", default.getString("protocol"))
        assertEquals(8, default.getInt("max_connections"))
        assertEquals(4, default.getInt("min_streams"))
        assertTrue(default.getBoolean("padding"))
        assertTrue(!default.has("brutal"))

        val tuned = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(
                    multiplexEnabled = true,
                    multiplexProtocol = "yamux",
                    multiplexConcurrency = 6,
                    multiplexMinStreams = 2,
                    multiplexPadding = false,
                    multiplexBrutalEnabled = true,
                    multiplexBrutalUpMbps = 50,
                    multiplexBrutalDownMbps = 200
                )
            )
        ).getJSONArray("outbounds").getJSONObject(0).getJSONObject("multiplex")
        assertEquals("yamux", tuned.getString("protocol"))
        assertEquals(6, tuned.getInt("max_connections"))
        assertEquals(2, tuned.getInt("min_streams"))
        assertEquals(false, tuned.getBoolean("padding"))
        val brutal = tuned.getJSONObject("brutal")
        assertTrue(brutal.getBoolean("enabled"))
        assertEquals(50, brutal.getInt("up_mbps"))
        assertEquals(200, brutal.getInt("down_mbps"))

        // `unknown protocol: bogus` and `brutal: invalid download speed` both
        // abort the whole config, so bad input must never be emitted.
        val guarded = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(
                    multiplexEnabled = true,
                    multiplexProtocol = "bogus",
                    multiplexBrutalEnabled = true,
                    multiplexBrutalUpMbps = 50
                )
            )
        ).getJSONArray("outbounds").getJSONObject(0).getJSONObject("multiplex")
        assertEquals("smux", guarded.getString("protocol"))
        assertTrue(!guarded.has("brutal"))
    }

    @Test
    fun outboundDialOptionsAreOffByDefaultAndReachEveryTransport() {
        val default = buildMixedPool(SingboxConfigOptions(tunMode = false))
        val defaultSs = outboundsByTag(default, "outbounds").getValue("proxy-0")
        for (key in listOf("tcp_fast_open", "tcp_multi_path", "udp_fragment", "connect_timeout")) {
            assertTrue("default must not emit $key", !defaultSs.has(key))
        }

        val tuned = buildMixedPool(
            SingboxConfigOptions(
                tunMode = false,
                outboundTcpFastOpen = true,
                outboundTcpMultiPath = true,
                outboundUdpFragment = true,
                outboundConnectTimeoutSeconds = 12
            )
        )
        val outbounds = outboundsByTag(tuned, "outbounds")
        val ss = outbounds.getValue("proxy-0")
        assertTrue(ss.getBoolean("tcp_fast_open"))
        assertTrue(ss.getBoolean("tcp_multi_path"))
        assertTrue(ss.getBoolean("udp_fragment"))
        assertEquals("12s", ss.getString("connect_timeout"))
        // `tcp_fast_open is not supported with anytls outbound` is fatal.
        val anytls = outbounds.getValue("proxy-1")
        assertTrue(!anytls.has("tcp_fast_open"))
        assertTrue(anytls.getBoolean("tcp_multi_path"))
        assertEquals("12s", anytls.getString("connect_timeout"))
        // Group and built-in outbounds reject or ignore dial fields.
        assertTrue(!outbounds.getValue("proxy").has("tcp_fast_open"))
        assertTrue(!outbounds.getValue("direct").has("tcp_fast_open"))
        // Endpoints take the same dial fields as outbounds.
        val wg = outboundsByTag(tuned, "endpoints").getValue("proxy-2")
        assertTrue(wg.getBoolean("tcp_fast_open"))
        assertEquals("12s", wg.getString("connect_timeout"))
    }

    @Test
    fun udpOverTcpOnlyReachesShadowsocks() {
        val default = buildMixedPool(SingboxConfigOptions(tunMode = false))
        assertTrue(!outboundsByTag(default, "outbounds").getValue("proxy-0").has("udp_over_tcp"))

        val tuned = buildMixedPool(SingboxConfigOptions(tunMode = false, udpOverTcp = true))
        val outbounds = outboundsByTag(tuned, "outbounds")
        val uot = outbounds.getValue("proxy-0").getJSONObject("udp_over_tcp")
        assertTrue(uot.getBoolean("enabled"))
        assertEquals(2, uot.getInt("version"))
        // Every other type rejects the field with `json: unknown field`.
        assertTrue(!outbounds.getValue("proxy-1").has("udp_over_tcp"))
        assertTrue(!outboundsByTag(tuned, "endpoints").getValue("proxy-2").has("udp_over_tcp"))
    }

    @Test
    fun urlTestIdleTimeoutAndInterruptAreTunable() {
        val poolNode = ParsedNode("S1", "trojan", "1.2.3.4", 443, "", mapOf("password" to "x"))
        val auto = ParsedNode("AUTO", "auto", "", 0, "")

        val default = JSONObject(
            SingboxConfigBuilder.buildConfig(listOf(poolNode, auto), auto, SingboxConfigOptions())
        ).getJSONArray("outbounds").getJSONObject(0)
        assertTrue(default.getBoolean("interrupt_exist_connections"))
        assertTrue(!default.has("idle_timeout"))

        val tuned = JSONObject(
            SingboxConfigBuilder.buildConfig(
                listOf(poolNode, auto),
                auto,
                SingboxConfigOptions(
                    urlTestIdleTimeoutMinutes = 30,
                    urlTestInterruptExistConnections = false
                )
            )
        ).getJSONArray("outbounds").getJSONObject(0)
        assertEquals("30m", tuned.getString("idle_timeout"))
        assertEquals(false, tuned.getBoolean("interrupt_exist_connections"))
    }

    @Test
    fun dnsCacheOptionsAndClientSubnetReachTheConfig() {
        val node = ParsedNode("DNS", "trojan", "1.2.3.4", 443, "", mapOf("password" to "x"))

        val default = JSONObject(
            SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions(tunMode = false))
        ).getJSONObject("dns")
        assertTrue(default.getBoolean("independent_cache"))
        assertTrue(!default.has("disable_cache"))
        assertTrue(!default.has("client_subnet"))

        val tuned = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(
                    tunMode = false,
                    dnsIndependentCache = false,
                    dnsDisableCache = true,
                    dnsClientSubnet = "203.0.113.0/24"
                )
            )
        ).getJSONObject("dns")
        assertEquals(false, tuned.getBoolean("independent_cache"))
        assertTrue(tuned.getBoolean("disable_cache"))
        assertEquals("203.0.113.0/24", tuned.getString("client_subnet"))

        // netip cannot parse this, and the core aborts the whole config on it.
        val guarded = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(tunMode = false, dnsClientSubnet = "not-an-ip")
            )
        ).getJSONObject("dns")
        assertTrue(!guarded.has("client_subnet"))
    }

    @Test
    fun fakeIpIsReachedByRuleAndUsesConfigurableRanges() {
        val node = ParsedNode("DNS", "trojan", "1.2.3.4", 443, "", mapOf("password" to "x"))
        val json = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(tunMode = false, dnsFakeIpEnabled = true)
            )
        ).getJSONObject("dns")
        // `default server cannot be fakeip` prevents the core from starting.
        assertEquals("dns-proxy-1", json.getString("final"))
        val rules = json.getJSONArray("rules")
        assertTrue((0 until rules.length()).any { rules.getJSONObject(it).optString("server") == "dns-fake" })
        val servers = json.getJSONArray("servers")
        val fake = (0 until servers.length())
            .map { servers.getJSONObject(it) }
            .first { it.optString("tag") == "dns-fake" }
        assertEquals("198.18.0.0/15", fake.getString("inet4_range"))
        assertEquals("fc00::/18", fake.getString("inet6_range"))

        val custom = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(
                    tunMode = false,
                    dnsFakeIpEnabled = true,
                    dnsFakeIpRangeIPv4 = "198.20.0.0/15",
                    dnsFakeIpRangeIPv6 = "fd00::/18"
                )
            )
        ).getJSONObject("dns").getJSONArray("servers")
        val customFake = (0 until custom.length())
            .map { custom.getJSONObject(it) }
            .first { it.optString("tag") == "dns-fake" }
        assertEquals("198.20.0.0/15", customFake.getString("inet4_range"))
        assertEquals("fd00::/18", customFake.getString("inet6_range"))

        // An unparsable prefix falls back instead of aborting the whole config.
        val guarded = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(
                    tunMode = false,
                    dnsFakeIpEnabled = true,
                    dnsFakeIpRangeIPv4 = "nope"
                )
            )
        ).getJSONObject("dns").getJSONArray("servers")
        assertEquals(
            "198.18.0.0/15",
            (0 until guarded.length())
                .map { guarded.getJSONObject(it) }
                .first { it.optString("tag") == "dns-fake" }
                .getString("inet4_range")
        )
    }

    @Test
    fun domainResolverStrategyOverridesTheDerivedDefault() {
        val node = ParsedNode("DNS", "trojan", "1.2.3.4", 443, "", mapOf("password" to "x"))

        val default = JSONObject(
            SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions(tunMode = false))
        ).getJSONObject("route").getJSONObject("default_domain_resolver")
        assertEquals("ipv4_only", default.getString("strategy"))

        val tuned = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(tunMode = false, domainResolverStrategy = "prefer_ipv6")
            )
        ).getJSONObject("route").getJSONObject("default_domain_resolver")
        assertEquals("prefer_ipv6", tuned.getString("strategy"))

        val guarded = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(tunMode = false, domainResolverStrategy = "bogus")
            )
        ).getJSONObject("route").getJSONObject("default_domain_resolver")
        assertEquals("ipv4_only", guarded.getString("strategy"))
    }

    @Test
    fun snifferListAndTimeoutTuneTheSniffAction() {
        val node = ParsedNode("Sniff", "trojan", "1.2.3.4", 443, "", mapOf("password" to "x"))

        val default = JSONObject(
            SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions(tunMode = false))
        ).getJSONObject("route").getJSONArray("rules").getJSONObject(0)
        assertEquals("sniff", default.getString("action"))
        assertTrue(!default.has("sniffer"))
        assertTrue(!default.has("timeout"))

        val tuned = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(
                    tunMode = false,
                    // `bogus` would abort the router with `unknown sniffer`.
                    sniffers = listOf("http", "TLS", "quic", "bogus"),
                    sniffTimeoutMs = 300
                )
            )
        ).getJSONObject("route").getJSONArray("rules").getJSONObject(0)
        val sniffer = tuned.getJSONArray("sniffer")
        assertEquals(3, sniffer.length())
        assertEquals("http", sniffer.getString(0))
        assertEquals("tls", sniffer.getString(1))
        assertEquals("quic", sniffer.getString(2))
        assertEquals("300ms", tuned.getString("timeout"))
    }

    @Test
    fun logLevelAcceptsTheFullCoreRange() {
        val node = ParsedNode("Log", "trojan", "1.2.3.4", 443, "", mapOf("password" to "x"))
        fun level(value: String) = JSONObject(
            SingboxConfigBuilder.buildConfig(
                node,
                SingboxConfigOptions(tunMode = false, logLevel = value)
            )
        ).getJSONObject("log").getString("level")

        assertEquals("info", level("info"))
        assertEquals("trace", level("trace"))
        assertEquals("fatal", level("fatal"))
        assertEquals("panic", level("panic"))
        assertEquals("info", level("bogus"))
    }

    @Test
    fun nativeHysteria1GetsRequiredSpeedDefaults() {
        val node = ParsedNode(
            name = "Hysteria v1",
            scheme = "hysteria",
            server = "hy.example.com",
            port = 443,
            link = "",
            outbound = mapOf(
                "protocol" to "hysteria",
                "singbox" to mapOf(
                    "type" to "hysteria",
                    "server" to "hy.example.com",
                    "server_port" to 443,
                    "auth_str" to "secret"
                )
            )
        )

        val proxy = JSONObject(SingboxConfigBuilder.buildConfig(node))
            .getJSONArray("outbounds")
            .getJSONObject(0)
        assertEquals(50, proxy.getInt("up_mbps"))
        assertEquals(200, proxy.getInt("down_mbps"))
    }

    private fun simpleNode() =
        ParsedNode("Opt", "trojan", "1.2.3.4", 443, "", mapOf("password" to "x"))

    private fun inboundsByTag(json: JSONObject): Map<String, JSONObject> {
        val array = json.getJSONArray("inbounds")
        return (0 until array.length())
            .map { array.getJSONObject(it) }
            .associateBy { it.optString("tag") }
    }

    @Test
    fun lanListenAndUdpTimeoutAreOptIn() {
        val default = inboundsByTag(
            JSONObject(SingboxConfigBuilder.buildConfig(simpleNode(), SingboxConfigOptions(tunMode = false)))
        )
        assertEquals("127.0.0.1", default.getValue("socks-in").getString("listen"))
        assertEquals("127.0.0.1", default.getValue("http-in").getString("listen"))
        assertTrue(!default.getValue("socks-in").has("udp_timeout"))

        val tuned = inboundsByTag(
            JSONObject(
                SingboxConfigBuilder.buildConfig(
                    simpleNode(),
                    SingboxConfigOptions(
                        tunMode = false,
                        allowLanConnections = true,
                        inboundUdpTimeoutSeconds = 120
                    )
                )
            )
        )
        assertEquals("0.0.0.0", tuned.getValue("socks-in").getString("listen"))
        assertEquals("0.0.0.0", tuned.getValue("http-in").getString("listen"))
        assertEquals("120s", tuned.getValue("socks-in").getString("udp_timeout"))
    }

    @Test
    fun bypassLanAddsALowPriorityPrivateIpRule() {
        fun privateRules(options: SingboxConfigOptions): List<Int> {
            val rules = JSONObject(SingboxConfigBuilder.buildConfig(simpleNode(), options))
                .getJSONObject("route").getJSONArray("rules")
            return (0 until rules.length()).filter { rules.getJSONObject(it).optBoolean("ip_is_private") }
        }

        assertTrue(privateRules(SingboxConfigOptions(tunMode = false)).isEmpty())

        val options = SingboxConfigOptions(
            tunMode = false,
            bypassLan = true,
            directDomains = listOf("proxy:netflix.com")
        )
        val json = JSONObject(SingboxConfigBuilder.buildConfig(simpleNode(), options))
        val rules = json.getJSONObject("route").getJSONArray("rules")
        val index = privateRules(options).single()
        assertEquals("direct", rules.getJSONObject(index).getString("outbound"))
        // Explicit user rules must keep winning over the blanket LAN shortcut.
        val proxyRule = (0 until rules.length())
            .first { rules.getJSONObject(it).optString("outbound") == "proxy" }
        assertTrue(proxyRule < index)
    }

    @Test
    fun tlsFragmentFallbackDelayIsTunable() {
        fun delay(options: SingboxConfigOptions): String {
            val rules = JSONObject(SingboxConfigBuilder.buildConfig(simpleNode(), options))
                .getJSONObject("route").getJSONArray("rules")
            return (0 until rules.length())
                .map { rules.getJSONObject(it) }
                .first { it.optString("action") == "route-options" }
                .getString("tls_fragment_fallback_delay")
        }

        assertEquals("500ms", delay(SingboxConfigOptions(tunMode = false, enableFinalFragment = true)))
        assertEquals(
            "1200ms",
            delay(
                SingboxConfigOptions(
                    tunMode = false,
                    enableFinalFragment = true,
                    tlsFragmentFallbackDelayMs = 1200
                )
            )
        )
        // `time: invalid duration` would abort the whole config.
        assertEquals(
            "10000ms",
            delay(
                SingboxConfigOptions(
                    tunMode = false,
                    enableFinalFragment = true,
                    tlsFragmentFallbackDelayMs = 999999
                )
            )
        )
    }

    @Test
    fun dnsCacheCapacityOverridesTheDerivedDefault() {
        fun capacity(options: SingboxConfigOptions) =
            JSONObject(SingboxConfigBuilder.buildConfig(simpleNode(), options))
                .getJSONObject("dns").getInt("cache_capacity")

        assertEquals(2048, capacity(SingboxConfigOptions(tunMode = false)))
        assertEquals(4096, capacity(SingboxConfigOptions(tunMode = false, dnsParallelQuery = true)))
        assertEquals(8192, capacity(SingboxConfigOptions(tunMode = false, dnsCacheCapacity = 8192)))
        // uint32 in the core, so an out-of-range request is clamped, not emitted.
        assertEquals(64, capacity(SingboxConfigOptions(tunMode = false, dnsCacheCapacity = 1)))
    }

    @Test
    fun directDnsRuleCarriesTheConfiguredStrategy() {
        fun directRule(options: SingboxConfigOptions): JSONObject {
            val rules = JSONObject(SingboxConfigBuilder.buildConfig(simpleNode(), options))
                .getJSONObject("dns").getJSONArray("rules")
            return (0 until rules.length())
                .map { rules.getJSONObject(it) }
                .first { it.optString("server") == "dns-direct-1" }
        }

        val base = SingboxConfigOptions(tunMode = false, directDomains = listOf("gov.ru"))
        assertTrue(!directRule(base).has("strategy"))
        assertEquals(
            "prefer_ipv4",
            directRule(base.copy(dnsDirectRuleStrategy = "PREFER_IPV4")).getString("strategy")
        )
        // `unknown domain strategy: bogus` aborts the whole config.
        assertTrue(!directRule(base.copy(dnsDirectRuleStrategy = "bogus")).has("strategy"))
    }

    @Test
    fun cacheFileIsOptInAndStoresFakeIpOnlyWithFakeIp() {
        val default = JSONObject(
            SingboxConfigBuilder.buildConfig(simpleNode(), SingboxConfigOptions(tunMode = false))
        )
        assertTrue(!default.has("experimental"))

        val enabled = JSONObject(
            SingboxConfigBuilder.buildConfig(
                simpleNode(),
                SingboxConfigOptions(tunMode = false, cacheFileEnabled = true)
            )
        ).getJSONObject("experimental").getJSONObject("cache_file")
        assertTrue(enabled.getBoolean("enabled"))
        assertTrue(!enabled.has("path"))
        assertTrue(!enabled.has("store_fakeip"))

        val withPath = JSONObject(
            SingboxConfigBuilder.buildConfig(
                simpleNode(),
                SingboxConfigOptions(
                    tunMode = false,
                    cacheFileEnabled = true,
                    cacheFilePath = "/data/data/com.lumen.app/cache/singbox.db",
                    dnsFakeIpEnabled = true
                )
            )
        ).getJSONObject("experimental").getJSONObject("cache_file")
        assertEquals("/data/data/com.lumen.app/cache/singbox.db", withPath.getString("path"))
        assertTrue(withPath.getBoolean("store_fakeip"))
    }

    @Test
    fun ntpIsOptInAndResolvesItsHostDirectly() {
        val default = JSONObject(
            SingboxConfigBuilder.buildConfig(simpleNode(), SingboxConfigOptions(tunMode = false))
        )
        assertTrue(!default.has("ntp"))

        val enabled = JSONObject(
            SingboxConfigBuilder.buildConfig(
                simpleNode(),
                SingboxConfigOptions(tunMode = false, ntpEnabled = true)
            )
        ).getJSONObject("ntp")
        assertTrue(enabled.getBoolean("enabled"))
        assertEquals("time.apple.com", enabled.getString("server"))
        assertEquals(123, enabled.getInt("server_port"))
        assertEquals("direct", enabled.getString("detour"))
        assertEquals("dns-bootstrap", enabled.getString("domain_resolver"))

        // A literal server needs no resolver at all.
        val literal = JSONObject(
            SingboxConfigBuilder.buildConfig(
                simpleNode(),
                SingboxConfigOptions(tunMode = false, ntpEnabled = true, ntpServer = "162.159.200.1")
            )
        ).getJSONObject("ntp")
        assertEquals("162.159.200.1", literal.getString("server"))
        assertTrue(!literal.has("domain_resolver"))
    }

    // --- UDP ------------------------------------------------------------------

    /**
     * hev-socks5-tunnel hands every datagram to the socks inbound over SOCKS5 UDP
     * ASSOCIATE, so a route rule that rejects UDP by port takes far more than QUIC
     * with it. Verified against core/sing-box-lumen.exe: `network=udp port=443`
     * rejects a plain UDP/443 datagram as well, while `network=udp protocol=quic
     * port=443` still matches a real QUIC client hello and lets the rest through.
     */
    @Test
    fun blockQuicRejectsQuicOnlyAndNothingElseRejectsUdp() {
        fun udpRejects(options: SingboxConfigOptions): List<JSONObject> {
            val rules = JSONObject(SingboxConfigBuilder.buildConfig(simpleNode(), options))
                .getJSONObject("route").getJSONArray("rules")
            return (0 until rules.length())
                .map { rules.getJSONObject(it) }
                .filter { it.optString("action") == "reject" && it.optString("network") == "udp" }
        }

        assertTrue(udpRejects(SingboxConfigOptions(tunMode = false)).isEmpty())

        val quicRules = udpRejects(SingboxConfigOptions(tunMode = false, blockQuic = true))
        assertEquals(1, quicRules.size)
        assertEquals(443, quicRules[0].getInt("port"))
        assertEquals("quic", quicRules[0].getString("protocol"))
    }

    // --- OpenVPN credentials --------------------------------------------------

    private fun openVpnProfile(userAuth: Boolean, credentials: Boolean): String = buildString {
        appendLine("client")
        appendLine("dev tun")
        appendLine("proto udp")
        appendLine("remote ovpn.example.com 1194")
        appendLine("<ca>")
        appendLine("-----BEGIN CERTIFICATE-----")
        appendLine("MIIB")
        appendLine("-----END CERTIFICATE-----")
        appendLine("</ca>")
        when {
            userAuth && credentials -> {
                appendLine("<auth-user-pass>")
                appendLine("myuser")
                appendLine("mypass")
                appendLine("</auth-user-pass>")
            }
            userAuth -> appendLine("auth-user-pass")
            // A genuinely certificate-only profile carries the CLIENT certificate
            // and its key. <ca> alone only verifies the server, so such a profile
            // has nothing to authenticate itself with and does need a login.
            else -> {
                appendLine("<cert>")
                appendLine("-----BEGIN CERTIFICATE-----")
                appendLine("MIIC")
                appendLine("-----END CERTIFICATE-----")
                appendLine("</cert>")
                appendLine("<key>")
                appendLine("-----BEGIN PRIVATE KEY-----")
                appendLine("MIIE")
                appendLine("-----END PRIVATE KEY-----")
                appendLine("</key>")
            }
        }
    }

    @Test
    fun openVpnWithoutCredentialsIsRejectedWithAReadableMessage() {
        val node = LinkParser.parseOpenVpnConfig(openVpnProfile(userAuth = true, credentials = false))
        val error = try {
            SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions(tunMode = false))
            null
        } catch (e: IllegalArgumentException) {
            e
        }
        assertEquals("OpenVPN node requires a username and password", error?.message)
    }

    /** A certificate-only profile asks for no login and must keep working. */
    @Test
    fun openVpnCertificateOnlyAndLoggedInProfilesStillBuild() {
        val profiles = listOf(
            openVpnProfile(userAuth = true, credentials = true),
            openVpnProfile(userAuth = false, credentials = false)
        )
        for (profile in profiles) {
            val node = LinkParser.parseOpenVpnConfig(profile)
            val proxy = JSONObject(
                SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions(tunMode = false))
            ).getJSONArray("outbounds").getJSONObject(0)
            assertEquals("openvpn", proxy.getString("type"))
        }
    }

    @Test
    fun autoPoolSkipsAnOpenVpnMemberWithoutCredentials() {
        val broken = LinkParser.parseOpenVpnConfig(openVpnProfile(userAuth = true, credentials = false))
        val auto = ParsedNode("AUTO", "auto", "", 0, "")
        val json = JSONObject(
            SingboxConfigBuilder.buildConfig(
                listOf(broken) + mixedPool() + auto,
                auto,
                SingboxConfigOptions(tunMode = false)
            )
        )
        val outbounds = outboundsByTag(json, "outbounds")
        val everything = outbounds + outboundsByTag(json, "endpoints")
        assertTrue(everything.values.none { it.optString("type") == "openvpn" })

        val pool = outbounds.getValue("proxy").getJSONArray("outbounds")
        assertEquals(
            listOf("proxy-0", "proxy-1", "proxy-2"),
            (0 until pool.length()).map { pool.getString(it) }
        )
    }
}
