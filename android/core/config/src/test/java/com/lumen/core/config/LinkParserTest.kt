package com.lumen.core.config

import com.lumen.core.config.parser.LinkParser
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Base64

class LinkParserTest {

    @Test
    fun testParseVlessLink() {
        val link = "vless://11111111-2222-3333-4444-555555555555@example.com:443?type=ws&security=tls&path=%2Fws&host=example.com&sni=example.com#TestVless"
        val node = LinkParser.parseSingle(link)
        assertEquals("TestVless", node.name)
        assertEquals("vless", node.scheme)
        assertEquals("example.com", node.server)
        assertEquals(443, node.port)
        assertNotNull(node.outbound)
        assertEquals("vless", node.outbound["protocol"])
    }

    @Test
    fun testParseVmessLink() {
        val jsonPayload = """
            {
              "v": "2",
              "ps": "VMessTest",
              "add": "1.2.3.4",
              "port": 8443,
              "id": "11111111-2222-3333-4444-555555555555",
              "aid": 0,
              "scy": "auto",
              "net": "ws",
              "type": "none",
              "host": "test.server.com",
              "path": "/path",
              "tls": "tls",
              "sni": "test.server.com"
            }
        """.trimIndent()
        val b64 = Base64.getEncoder().encodeToString(jsonPayload.toByteArray(Charsets.UTF_8))
        val link = "vmess://$b64"

        val node = LinkParser.parseSingle(link)
        assertEquals("VMessTest", node.name)
        assertEquals("vmess", node.scheme)
        assertEquals("1.2.3.4", node.server)
        assertEquals(8443, node.port)
        assertEquals("vmess", node.outbound["protocol"])
    }

    @Test
    fun testParseTrojanLink() {
        val link = "trojan://secretpassword@trojan.example.com:443?security=tls&sni=trojan.example.com#TrojanServer"
        val node = LinkParser.parseSingle(link)
        assertEquals("TrojanServer", node.name)
        assertEquals("trojan", node.scheme)
        assertEquals("trojan.example.com", node.server)
        assertEquals(443, node.port)
        assertEquals("trojan", node.outbound["protocol"])
    }

    @Test
    fun testParseShadowsocksLink() {
        val link = "ss://YWVzLTI1Ni1nY206c2VjcmV0cGFzc3dvcmQ=@ss.example.com:8388#ShadowsocksNode"
        val node = LinkParser.parseSingle(link)
        assertEquals("ShadowsocksNode", node.name)
        assertEquals("ss", node.scheme)
        assertEquals("ss.example.com", node.server)
        assertEquals(8388, node.port)
        assertEquals("shadowsocks", node.outbound["protocol"])
    }

    @Test
    fun testParseHysteria2Link() {
        val link = "hy2://myauthpass@hy2.example.com:443?sni=hy2.example.com&insecure=1#Hy2Node"
        val node = LinkParser.parseSingle(link)
        assertEquals("Hy2Node", node.name)
        assertEquals("hysteria2", node.scheme)
        assertEquals("hy2.example.com", node.server)
        assertEquals(443, node.port)
        assertEquals("hysteria2", node.outbound["protocol"])
    }

    @Test
    fun testParseTuicLink() {
        val link = "tuic://11111111-2222-3333-4444-555555555555:tuicpassword@tuic.example.com:8443?congestion_control=bbr&alpn=h3#TuicNode"
        val node = LinkParser.parseSingle(link)
        assertEquals("TuicNode", node.name)
        assertEquals("tuic", node.scheme)
        assertEquals("tuic.example.com", node.server)
        assertEquals(8443, node.port)
        assertEquals("tuic", node.outbound["protocol"])
    }

    @Test
    fun testParseWireGuardConfig() {
        val config = """
            [Interface]
            PrivateKey = aaaaaa=
            Address = 10.0.0.2/32, fd00::2/128
            DNS = 1.1.1.1
            MTU = 1420
            
            [Peer]
            PublicKey = bbbbbb=
            Endpoint = wg.example.com:51820
            AllowedIPs = 0.0.0.0/0, ::/0
        """.trimIndent()

        val (nodes, errors) = LinkParser.parseLinksText(config)
        assertTrue(errors.isEmpty())
        assertEquals(1, nodes.size)
        val node = nodes[0]
        assertEquals("wireguard", node.scheme)
        assertEquals("wg.example.com", node.server)
        assertEquals(51820, node.port)
    }

    @Test
    fun testParseOpenVpnConfig() {
        val ovpn = """
            client
            dev tun
            proto udp
            remote ovpn.example.com 1194
            resolv-retry infinite
            nobind
            <ca>
            -----BEGIN CERTIFICATE-----
            -----END CERTIFICATE-----
            </ca>
        """.trimIndent()

        val (nodes, errors) = LinkParser.parseLinksText(ovpn)
        assertTrue(errors.isEmpty())
        assertEquals(1, nodes.size)
        val node = nodes[0]
        assertEquals("openvpn", node.scheme)
        assertEquals("ovpn.example.com", node.server)
        assertEquals(1194, node.port)
    }

    @Test
    fun testParseBase64SubscriptionList() {
        val link1 = "vless://11111111-2222-3333-4444-555555555555@sub1.example.com:443?type=tcp#Node1"
        val link2 = "trojan://password@sub2.example.com:443#Node2"
        val subText = "$link1\n$link2"
        val b64Sub = Base64.getEncoder().encodeToString(subText.toByteArray(Charsets.UTF_8))

        val (nodes, errors) = LinkParser.parseLinksText(b64Sub)
        assertTrue(errors.isEmpty())
        assertEquals(2, nodes.size)
        assertEquals("Node1", nodes[0].name)
        assertEquals("sub1.example.com", nodes[0].server)
        assertEquals("Node2", nodes[1].name)
        assertEquals("sub2.example.com", nodes[1].server)
    }

    @Test
    fun testParseClashYaml() {
        val yaml = """
            proxies:
              - name: "ClashVless"
                type: vless
                server: clash.example.com
                port: 443
                uuid: 11111111-2222-3333-4444-555555555555
                udp: true
              - name: "ClashTrojan"
                type: trojan
                server: clash2.example.com
                port: 443
                password: secretpassword
        """.trimIndent()

        val (nodes, errors) = LinkParser.parseLinksText(yaml)
        assertTrue(errors.isEmpty())
        assertEquals(2, nodes.size)
        assertEquals("ClashVless", nodes[0].name)
        assertEquals("clash.example.com", nodes[0].server)
        assertEquals("ClashTrojan", nodes[1].name)
        assertEquals("clash2.example.com", nodes[1].server)
    }

    @Test
    fun testParseClashYamlWithPayload() {
        val yaml = """
            payload:
              - name: "PayloadVless"
                type: vless
                server: payload.example.com
                port: 443
                uuid: 11111111-2222-3333-4444-555555555555
              - name: "PayloadHy2"
                type: hysteria2
                server: hy2.payload.com
                port: 8443
                password: pass
        """.trimIndent()

        val (nodes, errors) = LinkParser.parseLinksText(yaml)
        assertTrue(errors.isEmpty())
        assertEquals(2, nodes.size)
        assertEquals("PayloadVless", nodes[0].name)
        assertEquals("payload.example.com", nodes[0].server)
        assertEquals("PayloadHy2", nodes[1].name)
        assertEquals("hy2.payload.com", nodes[1].server)
    }

    @Test
    fun testParseClashYamlWithMihomoTagsAndUnquoted() {
        val yaml = """
            proxies:
              - name: !vless "MihomoVless"
                type: vless
                server: mihomo.example.com
                port: 443
                uuid: 11111111-2222-3333-4444-555555555555
                network: ws
                ws-opts:
                  path: /ws
              - name: "MihomoTuic"
                type: tuic
                server: tuic.mihomo.com
                port: 8443
                uuid: 22222222-3333-4444-5555-666666666666
                password: secret
                congestion-controller: bbr
        """.trimIndent()

        val (nodes, errors) = LinkParser.parseLinksText(yaml)
        assertTrue(errors.isEmpty())
        assertEquals(2, nodes.size)
        assertEquals("MihomoVless", nodes[0].name)
        assertEquals("mihomo.example.com", nodes[0].server)
        assertEquals("MihomoTuic", nodes[1].name)
        assertEquals("tuic.mihomo.com", nodes[1].server)
    }

    @Test
    fun testParseAllClashProxyTypes() {
        val proxyMaps = listOf(
            mapOf("name" to "P_Vless", "type" to "vless", "server" to "v.com", "port" to 443, "uuid" to "uuid123"),
            mapOf("name" to "P_Vmess", "type" to "vmess", "server" to "vm.com", "port" to 443, "uuid" to "uuid123", "cipher" to "auto"),
            mapOf("name" to "P_Trojan", "type" to "trojan", "server" to "t.com", "port" to 443, "password" to "pass123"),
            mapOf("name" to "P_SS", "type" to "ss", "server" to "ss.com", "port" to 8388, "cipher" to "aes-256-gcm", "password" to "pass123"),
            mapOf("name" to "P_Hy2", "type" to "hy2", "server" to "hy.com", "port" to 8443, "password" to "pass123"),
            mapOf("name" to "P_Tuic", "type" to "tuic", "server" to "tuic.com", "port" to 8443, "uuid" to "uuid123", "password" to "pass123"),
            mapOf("name" to "P_Wg", "type" to "wireguard", "server" to "wg.com", "port" to 51820, "private-key" to "priv123", "public-key" to "pub123"),
            mapOf("name" to "P_Socks", "type" to "socks5", "server" to "s.com", "port" to 1080, "username" to "user", "password" to "pass"),
            mapOf("name" to "P_Http", "type" to "http", "server" to "h.com", "port" to 8080, "username" to "user", "password" to "pass")
        )

        for (map in proxyMaps) {
            val node = LinkParser.parseClashProxyMap(map)
            assertEquals(map["name"], node.name)
            assertEquals(map["server"], node.server)
            assertEquals((map["port"] as Number).toInt(), node.port)
            assertTrue(node.link.isNotEmpty())
            assertTrue(node.outbound.isNotEmpty())
        }
    }

    @Test
    fun testParseNaiveLink() {
        val link = "naive+https://user:pass@naive.example.com:443?sni=cdn.example.com#NaiveNode"
        val node = LinkParser.parseSingle(link)
        assertEquals("NaiveNode", node.name)
        assertEquals("naive", node.scheme)
        assertEquals("naive.example.com", node.server)
        assertEquals(443, node.port)
        val singbox = node.outbound["singbox"] as Map<*, *>
        assertEquals("naive", singbox["type"])
        assertEquals("user", singbox["username"])
        assertEquals("pass", singbox["password"])
        assertEquals(false, singbox["quic"])
        val tls = singbox["tls"] as Map<*, *>
        assertEquals("cdn.example.com", tls["server_name"])
    }

    @Test
    fun testParseNaiveQuicLink() {
        val node = LinkParser.parseSingle("quic://user:pass@naive.example.com:443#QuicNode")
        assertEquals("naive", node.scheme)
        val singbox = node.outbound["singbox"] as Map<*, *>
        assertEquals(true, singbox["quic"])
    }

    @Test
    fun testParseNaiveProxyJsonConfig() {
        val config = """{"listen": "socks://127.0.0.1:1080", "proxy": "https://user:pass@naive.example.com"}"""
        val (nodes, errors) = LinkParser.parseLinksText(config)
        assertTrue(errors.isEmpty())
        assertEquals(1, nodes.size)
        assertEquals("naive", nodes[0].scheme)
        assertEquals("naive.example.com", nodes[0].server)
        assertEquals(443, nodes[0].port)
    }

    @Test
    fun testParseMieruLink() {
        val link = "mieru://user:pass@mieru.example.com:2027?transport=tcp&multiplexing=MULTIPLEXING_LOW#MieruNode"
        val node = LinkParser.parseSingle(link)
        assertEquals("MieruNode", node.name)
        assertEquals("mieru", node.scheme)
        assertEquals("mieru.example.com", node.server)
        assertEquals(2027, node.port)
        val singbox = node.outbound["singbox"] as Map<*, *>
        assertEquals("mieru", singbox["type"])
        assertEquals("TCP", singbox["transport"])
        assertEquals("MULTIPLEXING_LOW", singbox["multiplexing"])
    }

    @Test
    fun testParseMasqueLink() {
        val link = "masque://token123@profile-uuid?use_http2=true#WarpMasque"
        val node = LinkParser.parseSingle(link)
        assertEquals("WarpMasque", node.name)
        assertEquals("masque", node.scheme)
        val singbox = node.outbound["singbox"] as Map<*, *>
        assertEquals("masque", singbox["type"])
        assertEquals(true, singbox["use_http2"])
        val profile = singbox["profile"] as Map<*, *>
        assertEquals("profile-uuid", profile["id"])
        assertEquals("token123", profile["auth_token"])
        assertEquals("direct", profile["detour"])
    }

    @Test
    fun testParseHysteria1Link() {
        val link = "hysteria://myauth@hy1.example.com:443?upmbps=100&downmbps=200&protocol=udp&obfs=xplus#Hy1Node"
        val node = LinkParser.parseSingle(link)
        assertEquals("Hy1Node", node.name)
        assertEquals("hysteria", node.scheme)
        val singbox = node.outbound["singbox"] as Map<*, *>
        assertEquals("hysteria", singbox["type"])
        assertEquals("myauth", singbox["auth_str"])
        assertEquals(100, singbox["up_mbps"])
        assertEquals(200, singbox["down_mbps"])
        assertEquals("xplus", singbox["obfs"])
    }

    @Test
    fun testOpenVpnConfigProducesNativeSingboxOutbound() {
        val ovpn = """
            client
            dev tun
            proto tcp
            remote ovpn2.example.com 443
            cipher AES-256-GCM
            auth SHA256
            <ca>
            -----BEGIN CERTIFICATE-----
            MIIB
            -----END CERTIFICATE-----
            </ca>
            <auth-user-pass>
            myuser
            mypass
            </auth-user-pass>
        """.trimIndent()
        val (nodes, errors) = LinkParser.parseLinksText(ovpn)
        assertTrue(errors.joinToString("; "), errors.isEmpty())
        assertEquals(1, nodes.size)
        assertEquals("openvpn", nodes[0].scheme)
        assertEquals("ovpn2.example.com", nodes[0].server)
        assertEquals(443, nodes[0].port)
        val singbox = nodes[0].outbound["singbox"] as Map<*, *>
        assertEquals("openvpn", singbox["type"])
        assertEquals("tcp", singbox["proto"])
        assertEquals("AES-256-GCM", singbox["cipher"])
        assertEquals("SHA256", singbox["auth"])
        assertEquals("myuser", singbox["username"])
        assertEquals("mypass", singbox["password"])
        assertTrue(!singbox.containsKey("config_str"))
        val servers = singbox["servers"] as List<*>
        val firstServer = servers[0] as Map<*, *>
        assertEquals("ovpn2.example.com", firstServer["server"])
        assertEquals(443, firstServer["server_port"])
        val tls = singbox["tls"] as Map<*, *>
        assertTrue(tls["ca"].toString().contains("BEGIN CERTIFICATE"))
    }

    @Test
    fun testParseClashMasqueProxy() {
        val map = mapOf<String, Any?>(
            "name" to "WarpClash",
            "type" to "masque",
            "server" to "162.159.198.1",
            "port" to 443,
            "private-key" to "privkey123",
            "public-key" to "pubkey123",
            "ip" to "172.16.0.2/32",
            "sni" to "consumer-masque.cloudflareclient.com"
        )
        val node = LinkParser.parseClashProxyMap(map)
        assertEquals("masque", node.scheme)
        val singbox = node.outbound["singbox"] as Map<*, *>
        assertEquals("masque", singbox["type"])
        assertEquals("162.159.198.1", singbox["server"])
        assertEquals(443, singbox["server_port"])
        assertEquals("privkey123", singbox["private_key"])
        assertEquals("pubkey123", singbox["public_key"])
        assertEquals(1280, singbox["mtu"])
        val profile = singbox["profile"] as Map<*, *>
        assertEquals("direct", profile["detour"])
        assertTrue(!profile.containsKey("private_key"))
    }

    @Test
    fun testParseClashNaiveProxy() {
        val map = mapOf<String, Any?>(
            "name" to "NaiveClash",
            "type" to "naive",
            "server" to "naive.example.com",
            "port" to 443,
            "username" to "user",
            "password" to "pass",
            "sni" to "cdn.example.com"
        )
        val node = LinkParser.parseClashProxyMap(map)
        assertEquals("naive", node.scheme)
        val singbox = node.outbound["singbox"] as Map<*, *>
        assertEquals("naive", singbox["type"])
        assertEquals("user", singbox["username"])
        val tls = singbox["tls"] as Map<*, *>
        assertEquals(true, tls["enabled"])
        assertEquals("cdn.example.com", tls["server_name"])
    }

    @Test
    fun testParseJsonNativeSingboxOutbound() {
        val json = """{"type": "mieru", "tag": "MieruJson", "server": "m.example.com", "server_port": 2027, "transport": "TCP", "username": "u", "password": "p"}"""
        val (nodes, errors) = LinkParser.parseLinksText(json)
        assertTrue(errors.isEmpty())
        assertEquals(1, nodes.size)
        assertEquals("mieru", nodes[0].scheme)
        assertEquals("m.example.com", nodes[0].server)
        assertEquals(2027, nodes[0].port)
        val singbox = nodes[0].outbound["singbox"] as Map<*, *>
        assertEquals("mieru", singbox["type"])
    }
}
