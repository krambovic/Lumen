package com.lumen.core.config

import com.lumen.core.config.builder.SingboxConfigBuilder
import com.lumen.core.config.parser.LinkParser
import com.lumen.core.config.parser.NodeExportNormalizer
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class NodeExportNormalizerTest {
    private fun checkExport(raw: String, prefix: String): String {
        val original = LinkParser.parseSingle(raw)
        val exported = NodeExportNormalizer.export(original)
        assertTrue("Expected $prefix portable link", exported.startsWith(prefix))
        val restored = LinkParser.parseSingle(exported)
        assertEquals(original.name, restored.name)
        assertEquals(original.server, restored.server)
        assertEquals(original.port, restored.port)
        assertEquals(JSONObject(SingboxConfigBuilder.buildOutbound(original)).newMap(),
            JSONObject(SingboxConfigBuilder.buildOutbound(restored)).newMap())
        return exported
    }

    @Test fun xrayJsonVlessBecomesUriWithRealityAndTransportIntact() {
        val raw = """{"tag":"Тест ایران","protocol":"vless","settings":{"vnext":[{"address":"vpn.example","port":443,"users":[{"id":"test-id","encryption":"none","flow":"xtls-rprx-vision"}]}]},"streamSettings":{"network":"tcp","security":"reality","realitySettings":{"serverName":"front.example","publicKey":"public-key","shortId":"abcd","fingerprint":"chrome"}}}"""
        checkExport(raw, "vless://")
    }

    @Test fun nativeJsonVlessAndTrojanPreserveWsTlsAndSpecialCredentials() {
        for ((type, identity) in listOf("vless" to "\"uuid\":\"test-id\"", "trojan" to "\"password\":\"a+b/@:#\"")) {
            checkExport("""{"type":"$type","tag":"Imported","server":"vpn.example","server_port":443,$identity,"tls":{"enabled":true,"server_name":"front.example","utls":{"enabled":true,"fingerprint":"chrome"}},"transport":{"type":"ws","path":"/socket?q=a+b","headers":{"Host":"cdn.example"}}}""", "$type://")
        }
    }

    @Test fun nativeProtocolUrisArePortable() {
        val cases = listOf(
            """{"type":"shadowsocks","tag":"SS","server":"ss.example","server_port":8388,"method":"aes-128-gcm","password":"p+a:ss/word"}""" to "ss://",
            """{"type":"socks","tag":"SOCKS","server":"proxy.example","server_port":1080,"version":"5","username":"user","password":"p+a:ss/word"}""" to "socks://",
            """{"type":"anytls","tag":"AnyTLS","server":"vpn.example","server_port":443,"password":"secret","tls":{"enabled":true,"server_name":"vpn.example"}}""" to "anytls://",
            """{"type":"hysteria2","tag":"HY2","server":"vpn.example","server_port":443,"password":"secret","tls":{"enabled":true,"server_name":"vpn.example"}}""" to "hysteria2://",
            """{"type":"vmess","tag":"VMess","server":"vpn.example","server_port":443,"uuid":"test-id","security":"auto","alter_id":0}""" to "vmess://",
            """{"type":"naive","tag":"Naive","server":"vpn.example","server_port":443,"username":"user","password":"secret","tls":{"enabled":true,"server_name":"vpn.example"}}""" to "naive://",
            """{"type":"mieru","tag":"Mieru","server":"vpn.example","server_port":443,"username":"user","password":"secret","transport":"TCP"}""" to "mieru://"
        )
        for ((raw, prefix) in cases) {
            val exported = NodeExportNormalizer.export(LinkParser.parseSingle(raw))
            assertTrue("Expected $prefix portable link", exported.startsWith(prefix))
            assertEquals(LinkParser.parseSingle(raw).server, LinkParser.parseSingle(exported).server)
        }
    }

    @Test fun wireguardProfileRetainsKeysDnsAndAmneziaParameters() {
        val link = "awg://private%2Bkey%2F%3D@vpn.example:51820?publickey=peer%2Bkey%2F%3D&ip=172.16.0.2%2F32&dns=1.1.1.1&allowedips=0.0.0.0%2F0%2C%3A%3A%2F0&jc=4&jmin=40&jmax=70&s1=0&s2=0#WARP"
        val node = LinkParser.parseSingle(link)
        val exported = NodeExportNormalizer.export(node.copy(link = JSONObject(node.outbound["singbox"] as Map<*, *>).toString()))
        assertTrue(exported.startsWith("awg://"))
        val restored = LinkParser.parseSingle(exported)
        assertEquals(node.outbound["_dns"], restored.outbound["_dns"])
        assertEquals(SingboxConfigBuilder.buildOutbound(node), SingboxConfigBuilder.buildOutbound(restored))
    }

    @Test fun fullConfigsAndDirectMasqueRemainCompactLosslessJson() {
        val raw = """{ "type":"masque", "tag":"WARP", "server":"162.159.198.1", "server_port":443, "private_key":"private", "public_key":"public", "address":["172.16.0.2/32"] }"""
        val exported = NodeExportNormalizer.export(LinkParser.parseSingle(raw))
        assertTrue(exported.startsWith("{"))
        assertEquals(JSONObject(raw).newMap(), JSONObject(exported).newMap())
        val full = """{ "inbounds":[], "outbounds":[{"type":"direct","tag":"direct"}] }"""
        val node = com.lumen.core.config.parser.ParsedNode("Full", "singbox_config", "", 0, full)
        assertEquals(JSONObject(full).toString(), NodeExportNormalizer.export(node))
    }

    @Test fun unrepresentableTlsSettingsAreNeverSilentlyDropped() {
        val raw = """{"type":"trojan","tag":"Pinned","server":"vpn.example","server_port":443,"password":"secret","tls":{"enabled":true,"certificate":["custom certificate"]}}"""
        val exported = NodeExportNormalizer.export(LinkParser.parseSingle(raw))
        assertTrue(exported.startsWith("{"))
        assertEquals(JSONObject(raw).newMap(), JSONObject(exported).newMap())
    }

    @Test fun originalUriExtensionsAreKeptAndRenamed() {
        val node = LinkParser.parseSingle("vless://id@vpn.example:443?security=tls&type=ws&path=%2Fws#Old")
        val exported = NodeExportNormalizer.export(node.copy(name = "Новое имя"))
        assertEquals("Новое имя", LinkParser.parseSingle(exported).name)
    }

    @Test fun bulkExportContainsPortableLinksAndCanBeImportedTogether() {
        val nodes = listOf(
            LinkParser.parseSingle("""{"type":"vless","tag":"One","server":"one.example","server_port":443,"uuid":"one"}"""),
            LinkParser.parseSingle("""{"type":"trojan","tag":"Two","server":"two.example","server_port":443,"password":"two","tls":{"enabled":true}}"""))
        val exported = NodeExportNormalizer.exportAll(nodes)
        assertEquals(2, exported.lines().size)
        assertFalse(exported.contains("{"))
        val (restored, errors) = LinkParser.parseLinksText(exported)
        assertTrue(errors.isEmpty())
        assertEquals(listOf("One", "Two"), restored.map { it.name })
    }

    @Test fun websocketQueryKeepsProviderParametersWhileExtractingEarlyData() {
        val node = LinkParser.parseSingle("vless://id@vpn.example:443?type=ws&path=%2Fws%3Ftoken%3Da%2Bb%26ed%3D2048&security=tls#WS")
        val transport = SingboxConfigBuilder.buildOutbound(node)["transport"] as Map<*, *>
        assertEquals("/ws?token=a+b", transport["path"])
        assertEquals(2048, transport["max_early_data"])
    }

    private fun JSONObject.newMap(): Map<String, Any?> = LinkParser.jsonToMap(this)
}
