package com.lumen.core.config

import com.lumen.core.config.parser.LinkParseError
import com.lumen.core.config.parser.LinkParser
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import java.util.Base64

class LinkParserTest {

    @Test
    fun testNumberedLocationServersRemainIndependentNodes() {
        val text = listOf(
            "vless://11111111-2222-3333-4444-555555555555@fr1.example.com:443?security=tls&sni=fr1.example.com#france-1",
            "vless://11111111-2222-3333-4444-555555555555@fr2.example.com:443?security=tls&sni=fr2.example.com#france-2",
            "vless://11111111-2222-3333-4444-555555555555@de1.example.com:443?security=tls&sni=de1.example.com#Germany"
        ).joinToString("\n")
        val (nodes, _) = LinkParser.parseLinksText(text)
        assertEquals(3, nodes.size)
        assertTrue(nodes.none { it.scheme == "auto" })
        assertTrue(nodes.any { it.name == "france-1" })
        assertTrue(nodes.any { it.name == "france-2" })
        assertTrue(nodes.any { it.name == "Germany" })
    }

    @Test
    fun testXrayProfileArrayImportsOnlyExplicitBalancersAsAutoNodes() {
        // Shape of the CommandVPN subscription: an array of whole Xray client
        // configs. Only their explicit routing balancers define AUTO pools.
        val text = """
            [
              {
                "remarks": "Separator",
                "profile_title": "[FREE] CommandVPN",
                "inbounds": [],
                "outbounds": [
                  {
                    "tag": "proxy",
                    "protocol": "shadowsocks",
                    "settings": {
                      "servers": [
                        {
                          "address": "127.0.0.1",
                          "port": 1,
                          "password": "separator",
                          "method": "chacha20-ietf-poly1305"
                        }
                      ]
                    }
                  },
                  { "tag": "direct", "protocol": "freedom" },
                  { "tag": "block", "protocol": "blackhole" }
                ]
              },
              {
                "remarks": "Авто | WiFi",
                "routing": { "balancers": [{ "tag": "wifi-auto", "selector": ["wifi-"] }] },
                "outbounds": [
                  {
                    "tag": "wifi-1",
                    "protocol": "vless",
                    "settings": {
                      "vnext": [
                        {
                          "address": "89.125.74.127",
                          "port": 443,
                          "users": [
                            { "id": "de2edd29-c746-4dc7-a50b-e0692b01209d", "encryption": "none", "flow": "xtls-rprx-vision" }
                          ]
                        }
                      ]
                    },
                    "streamSettings": { "network": "tcp", "security": "none" }
                  },
                  {
                    "tag": "wifi-2",
                    "protocol": "vless",
                    "settings": {
                      "vnext": [
                        {
                          "address": "83.147.255.185",
                          "port": 40443,
                          "users": [
                            { "id": "0cc733cb-ceb9-4ee5-97f7-fdad42e16d19", "encryption": "none", "flow": "" }
                          ]
                        }
                      ]
                    },
                    "streamSettings": { "network": "tcp", "security": "none" }
                  }
                ]
              },
              {
                "remarks": "Финляндия",
                "routing": { "balancers": [{ "tag": "fin-auto", "selector": ["fin-"] }] },
                "outbounds": [
                  {
                    "tag": "fin-1",
                    "protocol": "vless",
                    "settings": {
                      "vnext": [
                        {
                          "address": "fin.fast-cone.com",
                          "port": 443,
                          "users": [
                            { "id": "510a7c87-0aa0-4ead-9c9f-6e1700e12b09", "encryption": "none", "flow": "xtls-rprx-vision" }
                          ]
                        }
                      ]
                    },
                    "streamSettings": { "network": "tcp", "security": "none" }
                  },
                  {
                    "tag": "fin-2",
                    "protocol": "vless",
                    "settings": {
                      "vnext": [
                        {
                          "address": "fin.themesh.site",
                          "port": 443,
                          "users": [
                            { "id": "6fa9f6ae-877b-4858-b128-a8377fd9038b", "encryption": "none", "flow": "xtls-rprx-vision" }
                          ]
                        }
                      ]
                    },
                    "streamSettings": { "network": "tcp", "security": "none" }
                  }
                ]
              }
            ]
        """.trimIndent()

        val (nodes, _) = LinkParser.parseLinksText(text)

        // The separator profile must vanish, and each remaining profile must be
        // exactly one AUTO row named after its remarks.
        assertEquals(
            listOf("auto|Авто | WiFi", "auto|Финляндия"),
            nodes.map { it.scheme + "|" + it.name }
        )
        nodes.forEach { assertEquals(2, LinkParser.autoMembers(it.outbound).size) }
        assertTrue(nodes.none { it.name.contains("wifi-") })
    }

    @Test
    fun testOneMemberAndMultipleExplicitAutoGroupsArePreserved() {
        val text = """
            {
              "outbounds": [
                { "type": "trojan", "tag": "node-a", "server": "a.example.com", "server_port": 443, "password": "a" },
                { "type": "trojan", "tag": "node-b", "server": "b.example.com", "server_port": 443, "password": "b" },
                { "type": "urltest", "tag": "AUTO A", "outbounds": ["node-a"] },
                { "type": "selector", "tag": "AUTO B", "outbounds": ["node-b"] }
              ]
            }
        """.trimIndent()

        val (nodes, errors) = LinkParser.parseLinksText(text)

        assertTrue(errors.isEmpty())
        assertEquals(listOf("AUTO A", "AUTO B"), nodes.map { it.name })
        assertTrue(nodes.all { it.scheme == "auto" })
        assertTrue(nodes.all { LinkParser.autoMembers(it.outbound).size == 1 })
    }

    @Test
    fun testRemotePortOneIsNotTreatedAsSeparator() {
        val text = """
            [{
              "remarks": "Remote port one",
              "outbounds": [{
                "tag": "proxy",
                "protocol": "shadowsocks",
                "settings": { "servers": [{
                  "address": "edge.example.com",
                  "port": 1,
                  "password": "real-password",
                  "method": "chacha20-ietf-poly1305"
                }] }
              }]
            }]
        """.trimIndent()

        val (nodes, errors) = LinkParser.parseLinksText(text)

        assertTrue(errors.isEmpty())
        assertEquals(1, nodes.size)
        assertEquals("edge.example.com", nodes.single().server)
        assertEquals(1, nodes.single().port)
    }

    @Test
    fun testAmneziaWireGuardAliasParsesAsAwg() {
        val link = "amneziawg://private%2Bkey%3D@awg.example.com:51820" +
            "?publickey=public%2Bkey%3D&ip=10.0.0.2%2F32&jc=5&jmin=40&jmax=70#AWG"

        val node = LinkParser.parseSingle(link)

        assertEquals("awg", node.scheme)
        assertEquals("awg.example.com", node.server)
        val singbox = node.outbound["singbox"] as Map<*, *>
        assertEquals("awg", singbox["type"])
        val amnezia = singbox["amnezia"] as Map<*, *>
        assertEquals(5, amnezia["jc"])
    }

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
        assertTrue(!singbox.containsKey("server"))
        assertTrue(!singbox.containsKey("server_port"))
        assertTrue(!singbox.containsKey("private_key"))
        assertTrue(!singbox.containsKey("public_key"))
        assertTrue(!singbox.containsKey("mtu"))
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

    @Test
    fun singleWarpSelectorIsImportedAsWarpNotAuto() {
        val json = """
            {
              "outbounds": [
                {"type":"selector","tag":"proxy","outbounds":["warp-out"]},
                {
                  "type":"warp",
                  "tag":"warp-out",
                  "profile":{"detour":"direct","private_key":"secret"}
                }
              ]
            }
        """.trimIndent()

        val (nodes, errors) = LinkParser.parseLinksText(json)
        assertTrue(errors.joinToString("; "), errors.isEmpty())
        assertEquals(1, nodes.size)
        assertEquals("warp", nodes[0].scheme)
        assertTrue(nodes[0].scheme != "auto")
    }

    @Test
    fun amneziaWarpConfigKeepsAwgTransportAndWarpDisplayMetadata() {
        val config = """
            [Interface]
            PrivateKey = private=
            Address = 172.16.0.2/32, 2606:4700:110:8b0f::2/128
            DNS = 1.1.1.1
            MTU = 1280
            Jc = 4
            Jmin = 40
            Jmax = 70
            S1 = 1
            H1 = 123456

            [Peer]
            PublicKey = bmXOC+F1FxEMF9dyiK2H5/1SUtzH0JuVo51h2wPfgyo=
            AllowedIPs = 0.0.0.0/0, ::/0
            Endpoint = 8.47.69.7:1180
        """.trimIndent()

        val node = LinkParser.parseWireGuardConfig(config)
        assertEquals("WARP", node.name)
        assertEquals("awg", node.scheme)
        assertEquals(true, node.outbound["warp"])
        assertEquals("AWG/WARP", node.outbound["display_protocol"])
    }

    @Test
    fun ordinaryAmneziaConfigIsNotMarkedAsWarp() {
        val config = """
            [Interface]
            PrivateKey = private=
            Address = 10.8.0.200/32
            Jc = 5
            Jmin = 40
            Jmax = 70
            S1 = 1
            H1 = 123456

            [Peer]
            PublicKey = provider-public-key=
            AllowedIPs = 0.0.0.0/0
            Endpoint = 144.31.105.228:44315
        """.trimIndent()

        val node = LinkParser.parseWireGuardConfig(config)
        assertEquals("awg", node.scheme)
        assertTrue(node.name.startsWith("AmneziaWG-"))
        assertTrue(node.outbound["warp"] != true)
        assertEquals(null, node.outbound["display_protocol"])
    }

    @Test
    fun clashWarpMasqueSelectorKeepsPhysicalWarpNodes() {
        val yaml = """
            proxies:
              - name: WARP-1
                type: masque
                server: 162.159.198.1
                port: 443
                private-key: private-one
                public-key: public-one
                ip: 172.16.0.2/32
                ipv6: 2606:4700:110:8b0f::2/128
              - name: WARP-2
                type: masque
                server: 162.159.198.2
                port: 443
                private-key: private-two
                public-key: public-two
                ip: 172.16.0.2/32
                ipv6: 2606:4700:110:8b0f::2/128
            proxy-groups:
              - name: WARP
                type: select
                proxies:
                  - WARP-1
                  - WARP-2
        """.trimIndent()

        val (nodes, errors) = LinkParser.parseLinksText(yaml)
        assertTrue(errors.joinToString("; "), errors.isEmpty())
        assertEquals(2, nodes.size)
        assertTrue(nodes.none { it.scheme == "auto" })
        assertTrue(nodes.all { it.scheme == "masque" })
        assertTrue(nodes.all { it.outbound["warp"] == true })
        assertTrue(nodes.all { it.outbound["display_protocol"] == "MASQUE/WARP" })
    }

    @Test
    fun clashClientGroupsNeverBecomeAutoServersOrConsumeTheirMembers() {
        val yaml = """
            proxies:
              - name: Czech VMess
                type: vmess
                server: cz1.example.com
                port: 443
                uuid: 11111111-2222-3333-4444-555555555555
                tls: true
              - name: German Trojan
                type: trojan
                server: de1.example.com
                port: 443
                password: secret
                sni: de1.example.com
            proxy-groups:
              - name: Main selector
                type: select
                proxies: [Czech VMess, German Trojan]
              - name: Fastest
                type: url-test
                url: https://www.gstatic.com/generate_204
                interval: 300
                proxies: [Czech VMess, German Trojan]
        """.trimIndent()

        val (nodes, errors) = LinkParser.parseLinksText(yaml)
        assertTrue(errors.joinToString("; "), errors.isEmpty())
        assertEquals(listOf("Czech VMess", "German Trojan"), nodes.map { it.name })
        assertTrue(nodes.none { it.scheme == "auto" })
    }

    private val uuid = "11111111-2222-3333-4444-555555555555"

    private fun firstServer(node: com.lumen.core.config.parser.ParsedNode): Map<*, *> {
        val settings = node.outbound["settings"] as Map<*, *>
        return (settings["servers"] as List<*>)[0] as Map<*, *>
    }

    @Test
    fun trojanPasswordKeepsPlusInBothSpellings() {
        // '+' is form-encoding for a space only; a password must survive verbatim.
        val literal = LinkParser.parseSingle("trojan://aB+cD9x==@trojan.example.com:443#Plus")
        assertEquals("aB+cD9x==", firstServer(literal)["password"])
        val encoded = LinkParser.parseSingle("trojan://aB%2BcD9x==@trojan.example.com:443#Plus")
        assertEquals("aB+cD9x==", firstServer(encoded)["password"])
    }

    @Test
    fun queryValuesKeepPlusCharacters() {
        val node = LinkParser.parseSingle("hy2://p%2Bw@hy2.example.com:443?obfs-password=x+y&obfs=salamander#N")
        val singbox = node.outbound["singbox"] as Map<*, *>
        assertEquals("p+w", singbox["password"])
        assertEquals("x+y", (singbox["obfs"] as Map<*, *>)["password"])
    }

    @Test
    fun shadowsocksSip002TrailingSlashKeepsPortAndPlugin() {
        val link = "ss://YWVzLTI1Ni1nY206cGFzcw==@ss.example.com:2020/?plugin=obfs-local%3Bobfs%3Dhttp#Tokyo"
        val node = LinkParser.parseSingle(link)
        assertEquals("ss.example.com", node.server)
        assertEquals(2020, node.port)
        val server = firstServer(node)
        assertEquals("aes-256-gcm", server["method"])
        assertEquals("pass", server["password"])
        assertEquals("obfs-local", server["plugin"])
        assertEquals("obfs=http", server["plugin_opts"])
    }

    @Test
    fun shadowsocksIpv6LiteralKeepsHostAndPort() {
        val node = LinkParser.parseSingle("ss://YWVzLTI1Ni1nY206cGFzcw==@[2001:db8::1]:8388#v6")
        assertEquals("2001:db8::1", node.server)
        assertEquals(8388, node.port)
    }

    @Test
    fun shadowsocksWithoutCredentialsIsRejected() {
        val (nodes, errors) = LinkParser.parseLinksText("ss://YWVzLTI1Ni1nY20@host.example.com:8388#N")
        assertTrue(nodes.isEmpty())
        assertTrue(errors.joinToString("; "), errors.any { it.contains("shadowsocks") })
    }

    @Test
    fun shadowsocksInvalidPortIsRejected() {
        try {
            LinkParser.parseSingle("ss://YWVzLTI1Ni1nY206cGFzcw==@ss.example.com:notaport#N")
            fail("expected a LinkParseError")
        } catch (e: LinkParseError) {
            assertTrue(e.message.orEmpty().contains("port"))
        }
    }

    @Test
    fun allowInsecureIsHonouredForEverySpelling() {
        for (query in listOf("allowInsecure=true", "insecure=1", "allowInsecure=1", "insecure=true")) {
            val node = LinkParser.parseSingle("vless://$uuid@ip.example.com:443?security=tls&sni=example.com&$query#Self-signed")
            val stream = node.outbound["streamSettings"] as Map<*, *>
            val tls = stream["tlsSettings"] as Map<*, *>
            assertEquals(query, true, tls["allowInsecure"])
        }
    }

    @Test
    fun kcpTransportIsKeptInStreamSettings() {
        val node = LinkParser.parseSingle("vless://$uuid@kcp.example.com:443?type=kcp&headerType=srtp&seed=abc&mtu=1350&congestion=1#KCP")
        val stream = node.outbound["streamSettings"] as Map<*, *>
        assertEquals("kcp", stream["network"])
        val kcp = stream["kcpSettings"] as Map<*, *>
        assertEquals("srtp", (kcp["header"] as Map<*, *>)["type"])
        assertEquals("abc", kcp["seed"])
        assertEquals(1350, kcp["mtu"])
        assertEquals(true, kcp["congestion"])
    }

    @Test
    fun unsupportedTransportIsReportedInsteadOfSilentlyBecomingTcp() {
        val (nodes, errors) = LinkParser.parseLinksText("vless://$uuid@quic.example.com:443?type=quic#Q")
        assertTrue(nodes.isEmpty())
        assertTrue(errors.joinToString("; "), errors.any { it.contains("quic") })
    }

    @Test
    fun hysteria2ObfsPasswordAloneEnablesSalamander() {
        val node = LinkParser.parseSingle("hy2://pass@hy2.example.com:443?obfs-password=secret&sni=x#N")
        val obfs = (node.outbound["singbox"] as Map<*, *>)["obfs"] as Map<*, *>
        assertEquals("salamander", obfs["type"])
        assertEquals("secret", obfs["password"])
    }

    @Test
    fun hysteria2ObfsWithoutPasswordOmitsTheKey() {
        val node = LinkParser.parseSingle("hy2://pass@hy2.example.com:443?obfs=salamander#N")
        val obfs = (node.outbound["singbox"] as Map<*, *>)["obfs"] as Map<*, *>
        assertEquals("salamander", obfs["type"])
        assertTrue(!obfs.containsKey("password"))
    }

    @Test
    fun hysteria2AllowInsecureAliasIsHonoured() {
        val node = LinkParser.parseSingle("hy2://pass@hy2.example.com:443?allowInsecure=true#N")
        val tls = (node.outbound["singbox"] as Map<*, *>)["tls"] as Map<*, *>
        assertEquals(true, tls["insecure"])
    }

    @Test
    fun hysteria1SpeedsWithUnitSuffixArePreserved() {
        val node = LinkParser.parseSingle("hysteria://auth@hy.example.com:36712?up=200%20Mbps&down=500%20Mbps&protocol=udp#HY1")
        val singbox = node.outbound["singbox"] as Map<*, *>
        assertEquals(200, singbox["up_mbps"])
        assertEquals(500, singbox["down_mbps"])
    }

    @Test
    fun tuicKeepsInsecureZeroRttAndSniAliases() {
        val link = "tuic://$uuid:pass@tuic.example.com:8443?alpn=h3&allowInsecure=1&zero_rtt_handshake=1&peer=example.com#T"
        val node = LinkParser.parseSingle(link)
        val singbox = node.outbound["singbox"] as Map<*, *>
        assertEquals("cubic", singbox["congestion_control"])
        assertEquals(true, singbox["zero_rtt_handshake"])
        val tls = singbox["tls"] as Map<*, *>
        assertEquals("example.com", tls["server_name"])
        assertEquals(true, tls["insecure"])
    }

    @Test
    fun tuicWithoutUuidIsRejected() {
        try {
            LinkParser.parseSingle("tuic://@tuic.example.com:8443#T")
            fail("expected a LinkParseError")
        } catch (e: LinkParseError) {
            assertTrue(e.message.orEmpty().contains("uuid"))
        }
    }

    @Test
    fun parseAnyTlsLink() {
        val node = LinkParser.parseSingle("anytls://p%2Bass@anytls.example.com:8443?sni=example.com&insecure=1#AnyTLS")
        assertEquals("AnyTLS", node.name)
        assertEquals("anytls", node.scheme)
        assertEquals(8443, node.port)
        val singbox = node.outbound["singbox"] as Map<*, *>
        assertEquals("anytls", singbox["type"])
        assertEquals("p+ass", singbox["password"])
        val tls = singbox["tls"] as Map<*, *>
        assertEquals("example.com", tls["server_name"])
        assertEquals(true, tls["insecure"])
    }

    @Test
    fun wireGuardLinkWithoutCredentialsIsRejected() {
        try {
            LinkParser.parseSingle("awg://awg.example.com:51820?jc=5&jmin=50&jmax=1000")
            fail("expected a LinkParseError")
        } catch (e: LinkParseError) {
            assertTrue(e.message.orEmpty().contains("private key"))
        }
    }

    @Test
    fun amneziaPacketDefinitionsKeepTheirStringType() {
        val link = "awg://priv%2Bkey%3D@awg.example.com:51820" +
            "?publickey=pub%2Bkey%3D&ip=10.0.0.2%2F32&i1=0123456&jc=4&h1=3000000000"
        val node = LinkParser.parseSingle(link)
        val amnezia = (node.outbound["singbox"] as Map<*, *>)["amnezia"] as Map<*, *>
        assertEquals("0123456", amnezia["i1"])
        assertEquals(4, amnezia["jc"])
        assertEquals("3000000000", amnezia["h1"])
    }

    @Test
    fun clashWireGuardWithReservedStaysWireGuard() {
        val map = mapOf<String, Any?>(
            "name" to "WARP",
            "type" to "wireguard",
            "server" to "engage.cloudflareclient.com",
            "port" to 2408,
            "private-key" to "priv",
            "public-key" to "pub",
            "ip" to "172.16.0.2/32",
            "reserved" to listOf(12, 34, 56)
        )
        val node = LinkParser.parseClashProxyMap(map)
        assertEquals("wireguard", node.scheme)
        assertTrue(node.link, node.link.startsWith("wg://"))
        assertTrue(node.link, node.link.contains("reserved=12%2C34%2C56"))
    }

    @Test
    fun clashMasqueWithoutProfileIsReportedAsAnError() {
        val yaml = """
            proxies:
              - name: WARP
                type: masque
                network: h2
        """.trimIndent()

        val (nodes, errors) = LinkParser.parseLinksText(yaml)
        assertTrue(nodes.isEmpty())
        assertTrue(errors.joinToString("; "), errors.any { it.contains("MASQUE") })
    }

    @Test
    fun happStyleFragmentTitleIsDecoded() {
        val link = "vless://$uuid@ex.com:443?security=tls" +
            "#%F0%9F%87%BA%F0%9F%87%B8%20US%20East?serverDescription=UmVsYXk="
        val (nodes, errors) = LinkParser.parseLinksText(link)
        assertTrue(errors.joinToString("; "), errors.isEmpty())
        assertEquals("🇺🇸 US East", nodes[0].name)
        assertEquals("Relay", nodes[0].description)
    }

    @Test
    fun subscriptionUrlPayloadIsNotANode() {
        assertTrue(LinkParser.isSubscriptionUrl("https://panel.example.com/sub/abcdef"))
        assertTrue(!LinkParser.isSubscriptionUrl("vless://$uuid@ex.com:443"))
    }

    // A real provider ships i1 as a multi-kilobyte hex blob.
    private val awgLongI1 = "<b 0x" + "ab".repeat(2048) + "><t><r 42>"

    private val awgTitanProxyJson = """
        {"name":"AWG 🇳🇱 Titan AWG 2.0","type":"wireguard","server":"92.51.47.160","port":44320,
        "private-key":"aIKFfNg+z6wPZztFmQANk9+y/IVmvWMFQLxk5DL1C3c=",
        "public-key":"AlXMQAF+a9ZbBuHzYBiS0AjRxF7lYOE2qNiv8dDt1Bo=",
        "allowed-ips":["0.0.0.0/0","::/0"],"udp":true,"mtu":1420,"persistent-keepalive":25,
        "remote-dns-resolve":true,"dns":["1.1.1.1","1.0.0.1"],"ip":"10.8.0.115/32",
        "amnezia-wg-option":{"jc":4,"jmin":64,"jmax":160,"s1":44,"s2":63,"s3":12,"s4":8,
        "h1":"431245120-431245220","h2":"1187345001-1187345090","h3":"905120331-905120360",
        "h4":"1688457701-1688457800","i1":"$awgLongI1",
        "i2":"<b 0x4893cab7b710ca7d8f9a><t><rc 25><r 50>","i3":"<b 0x7d1eb544><rd 9><t><r 124>",
        "i4":"<b 0x2112a442><r 33><t><rc 16><r 68>","i5":"<rd 17><t><b 0x715603e43a7ec485c3934e><r 119>"}}
    """.trimIndent()

    @Test
    fun bareClashAwgJsonObjectImportsWithFullNameAndStringPacketTemplates() {
        val (nodes, errors) = LinkParser.parseLinksText(awgTitanProxyJson)

        assertTrue(errors.joinToString("; "), errors.isEmpty())
        assertEquals(1, nodes.size)
        val node = nodes.single()
        assertEquals("AWG 🇳🇱 Titan AWG 2.0", node.name)
        assertEquals("awg", node.scheme)
        assertEquals("92.51.47.160", node.server)
        assertEquals(44320, node.port)

        val singbox = node.outbound["singbox"] as Map<*, *>
        assertEquals("awg", singbox["type"])
        assertEquals(listOf("10.8.0.115/32"), singbox["address"])
        assertEquals(1420, singbox["mtu"])
        assertEquals(listOf("1.1.1.1", "1.0.0.1"), node.outbound["_dns"])
        val peer = (singbox["peers"] as List<*>).single() as Map<*, *>
        assertEquals("AlXMQAF+a9ZbBuHzYBiS0AjRxF7lYOE2qNiv8dDt1Bo=", peer["public_key"])
        assertEquals(listOf("0.0.0.0/0", "::/0"), peer["allowed_ips"])
        assertEquals(25, peer["persistent_keepalive_interval"])

        // The core types jc..s4 as integers, h1..h4 as uint32 ranges (the string
        // form is what survives a value above Int.MAX_VALUE) and i1..i5 as packet
        // templates: a number there makes the endpoint fail to decode.
        val amnezia = singbox["amnezia"] as Map<*, *>
        assertEquals(4, amnezia["jc"])
        assertEquals(8, amnezia["s4"])
        assertEquals("431245120-431245220", amnezia["h1"])
        assertEquals("1688457701-1688457800", amnezia["h4"])
        assertEquals(awgLongI1, amnezia["i1"])
        assertEquals("<b 0x4893cab7b710ca7d8f9a><t><rc 25><r 50>", amnezia["i2"])
        assertEquals("<rd 17><t><b 0x715603e43a7ec485c3934e><r 119>", amnezia["i5"])
    }

    @Test
    fun jsonArrayOfBareClashProxiesImportsEveryEntry() {
        val text = "[$awgTitanProxyJson," +
            """{"name":"AWG 🇩🇪 Titan","type":"awg","server":"1.2.3.4","port":51820,""" +
            """"private-key":"priv=","public-key":"pub=","ip":"10.8.0.9/32",""" +
            """"amnezia-wg-option":{"jc":3,"h2":"3000000000"}}]"""

        val (nodes, errors) = LinkParser.parseLinksText(text)

        assertTrue(errors.joinToString("; "), errors.isEmpty())
        assertEquals(listOf("AWG 🇳🇱 Titan AWG 2.0", "AWG 🇩🇪 Titan"), nodes.map { it.name })
        assertTrue(nodes.all { it.scheme == "awg" })
        val second = (nodes[1].outbound["singbox"] as Map<*, *>)["amnezia"] as Map<*, *>
        assertEquals(3, second["jc"])
        assertEquals("3000000000", second["h2"])
    }

    @Test
    fun bareClashProxyJsonWithoutDashedKeysKeepsItsClashPort() {
        val json = """{"name":"SS Node","type":"ss","server":"ss.example.com","port":8388,""" +
            """"cipher":"aes-256-gcm","password":"pass123"}"""

        val (nodes, errors) = LinkParser.parseLinksText(json)

        assertTrue(errors.joinToString("; "), errors.isEmpty())
        assertEquals("SS Node", nodes.single().name)
        assertEquals("ss", nodes.single().scheme)
        assertEquals(8388, nodes.single().port)
    }

    @Test
    fun awgConfigFileKeepsTheProviderName() {
        val config = """
            # Name = AWG 🇳🇱 Titan AWG 2.0
            [Interface]
            PrivateKey = private=
            Address = 10.8.0.115/32
            Jc = 4
            S3 = 12
            H1 = 431245120-431245220
            I1 = <b 0x7d1eb544><rd 9><t><r 124>

            [Peer]
            PublicKey = public=
            AllowedIPs = 0.0.0.0/0, ::/0
            Endpoint = 92.51.47.160:44320
        """.trimIndent()

        val node = LinkParser.parseWireGuardConfig(config)

        assertEquals("AWG 🇳🇱 Titan AWG 2.0", node.name)
        assertEquals("awg", node.scheme)
        val amnezia = (node.outbound["singbox"] as Map<*, *>)["amnezia"] as Map<*, *>
        assertEquals(12, amnezia["s3"])
        assertEquals("431245120-431245220", amnezia["h1"])
        assertEquals("<b 0x7d1eb544><rd 9><t><r 124>", amnezia["i1"])
    }

    @Test
    fun awgLinkKeepsTheProviderNameFromItsFragment() {
        val link = "awg://priv%2Bkey%3D@92.51.47.160:44320?publickey=pub%2Bkey%3D&ip=10.8.0.115%2F32" +
            "&jc=4&h1=431245120-431245220" +
            "&i2=%3Cb%200x4893cab7b710ca7d8f9a%3E%3Ct%3E%3Crc%2025%3E%3Cr%2050%3E" +
            "#AWG%20%F0%9F%87%B3%F0%9F%87%B1%20Titan%20AWG%202.0"

        val node = LinkParser.parseSingle(link)

        assertEquals("AWG 🇳🇱 Titan AWG 2.0", node.name)
        assertEquals("awg", node.scheme)
        val amnezia = (node.outbound["singbox"] as Map<*, *>)["amnezia"] as Map<*, *>
        assertEquals("431245120-431245220", amnezia["h1"])
        assertEquals("<b 0x4893cab7b710ca7d8f9a><t><rc 25><r 50>", amnezia["i2"])
    }

    @Test
    fun clashAwgProxyNameAndPacketTemplatesSurviveItsOwnLink() {
        val map = mapOf<String, Any?>(
            "name" to "AWG 🇳🇱 Titan AWG 2.0",
            "type" to "wireguard",
            "server" to "92.51.47.160",
            "port" to 44320,
            "private-key" to "aIKFfNg+z6wPZztFmQANk9+y/IVmvWMFQLxk5DL1C3c=",
            "public-key" to "AlXMQAF+a9ZbBuHzYBiS0AjRxF7lYOE2qNiv8dDt1Bo=",
            "ip" to "10.8.0.115/32",
            "amnezia-wg-option" to mapOf(
                "jc" to 4,
                "h1" to "431245120-431245220",
                "i2" to "<b 0x4893cab7b710ca7d8f9a><t><rc 25><r 50>"
            )
        )

        val node = LinkParser.parseClashProxyMap(map)
        assertEquals("AWG 🇳🇱 Titan AWG 2.0", node.name)
        assertEquals("awg", node.scheme)

        // The generated link is the export format, so a space inside an AWG 2.0
        // packet template has to come back as a space, not as '+'.
        val reparsed = LinkParser.parseSingle(node.link)
        assertEquals("AWG 🇳🇱 Titan AWG 2.0", reparsed.name)
        assertEquals("aIKFfNg+z6wPZztFmQANk9+y/IVmvWMFQLxk5DL1C3c=", (reparsed.outbound["singbox"] as Map<*, *>)["private_key"])
        val amnezia = (reparsed.outbound["singbox"] as Map<*, *>)["amnezia"] as Map<*, *>
        assertEquals(4, amnezia["jc"])
        assertEquals("431245120-431245220", amnezia["h1"])
        assertEquals("<b 0x4893cab7b710ca7d8f9a><t><rc 25><r 50>", amnezia["i2"])
    }

    @Test
    fun xrayProfileArrayKeepsEachProfileRemarksAsTheNodeName() {
        // Shape of the reported 38-profile v2rayNG export: every entry is a whole
        // Xray config, its balancer lists the member tags one by one, and the
        // profiles without a balancer contribute their outbounds directly.
        val text = """
            [
              {
                "remarks": "🇨🇿 Чехия",
                "routing": { "balancers": [{ "tag": "cz-auto", "selector": ["cz-1", "cz-2"] }] },
                "inbounds": [],
                "outbounds": [
                  { "tag": "cz-1", "protocol": "vless", "settings": { "vnext": [{ "address": "cz1.example.com", "port": 443, "users": [{ "id": "$uuid" }] }] } },
                  { "tag": "cz-2", "protocol": "vless", "settings": { "vnext": [{ "address": "cz2.example.com", "port": 443, "users": [{ "id": "$uuid" }] }] } },
                  { "tag": "direct", "protocol": "freedom" },
                  { "tag": "block", "protocol": "blackhole" }
                ]
              },
              {
                "remarks": "🇲🇾 Малайзия",
                "inbounds": [],
                "outbounds": [
                  { "tag": "my-1", "protocol": "vless", "settings": { "vnext": [{ "address": "my1.example.com", "port": 443, "users": [{ "id": "$uuid" }] }] } },
                  { "tag": "direct", "protocol": "freedom" }
                ]
              }
            ]
        """.trimIndent()

        val (nodes, errors) = LinkParser.parseLinksText(text)

        assertTrue(errors.joinToString("; "), errors.isEmpty())
        assertEquals(2, nodes.size)
        assertEquals("🇨🇿 Чехия", nodes[0].name)
        assertEquals("auto", nodes[0].scheme)
        assertEquals(2, LinkParser.autoMembers(nodes[0].outbound).size)
        assertEquals("🇲🇾 Малайзия · my-1", nodes[1].name)
        assertEquals("vless", nodes[1].scheme)
        assertEquals("my1.example.com", nodes[1].server)
    }
}
