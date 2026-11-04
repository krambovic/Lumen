package com.lumen.core.config

import com.lumen.core.config.builder.SingboxConfigBuilder
import com.lumen.core.config.crypto.LumenProfileEnvelope
import com.lumen.core.config.crypto.LumenProfilePayload
import com.lumen.core.config.parser.LinkParser
import com.lumen.core.config.parser.ParsedNode
import java.security.KeyPairGenerator
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class LumenProfilePayloadTest {
    private val uuid = "00000000-0000-4000-8000-000000000001"
    private fun fixtures(): List<ParsedNode> {
        val links = listOf(
            "vless://$uuid@vpn.example:443?security=reality&pbk=public&sid=abcd&type=tcp#VLESS",
            "vless://$uuid@vpn.example:443?security=tls&type=xhttp&extra=%7B%22xPaddingBytes%22%3A%22100-1000%22%7D#XHTTP",
            "vmess://$uuid@vpn.example:443?type=ws&security=tls#VMess",
            "trojan://p%2Ba%3Ass@vpn.example:443?security=tls&type=grpc&serviceName=service#Trojan",
            "ss://YWVzLTEyOC1nY206c2VjcmV0@vpn.example:8388#SS",
            "socks://user:password@vpn.example:1080#SOCKS",
            "http://user:password@vpn.example:8080#HTTP",
            "https://user:password@vpn.example:443#HTTPS",
            "hysteria://secret@vpn.example:443?upmbps=10&downmbps=20#HY",
            "hysteria2://secret@vpn.example:443?obfs=salamander&obfs-password=secret#HY2",
            "tuic://$uuid:secret@vpn.example:443?congestion_control=bbr#TUIC",
            "anytls://secret@vpn.example:443#AnyTLS",
            "naive://user:password@vpn.example:443#Naive",
            "mieru://user:password@vpn.example:443?transport=TCP#Mieru",
            "wireguard://private@vpn.example:51820?publickey=public&ip=10.0.0.2%2F32#WG",
            "awg://private@vpn.example:51820?publickey=public&ip=10.0.0.2%2F32&jc=4&h1=1-2&i1=test#AWG",
            """{"type":"masque","tag":"MASQUE","server":"162.159.198.1","server_port":443,"private_key":"private","public_key":"public","address":["172.16.0.2/32"]}""",
            """{"type":"trojan","tag":"Pinned","server":"vpn.example","server_port":443,"password":"secret","tls":{"enabled":true,"certificate":["custom certificate"]},"multiplex":{"enabled":true,"protocol":"h2mux"}}"""
        )
        val ordinary = links.map(LinkParser::parseSingle)
        val ovpn = LinkParser.parseOpenVpnConfig("""
            client
            dev tun
            proto udp
            remote vpn.example 1194
            auth-user-pass
            <auth-user-pass>
            test-user
            test-password
            </auth-user-pass>
            <ca>
            test-certificate
            </ca>
        """.trimIndent())
        return ordinary + ovpn + listOf(
            ParsedNode("Full sing-box", "singbox_config", "", 0,
                """{"dns":{"servers":[{"address":"1.1.1.1"}]},"outbounds":[{"type":"direct","tag":"direct"}]}"""),
            ParsedNode("Full Xray", "xray_config", "", 0,
                """{"outbounds":[{"protocol":"freedom","tag":"direct"}]}"""),
            ParsedNode("AUTO", "auto", "", 0, "auto", mapOf("auto_members" to ordinary.take(2).map {
                mapOf("name" to it.name, "scheme" to it.scheme, "server" to it.server,
                    "port" to it.port, "link" to it.link, "outbound" to it.outbound)
            }))
        )
    }

    @Test fun encryptionAndSnapshotPreserveEveryProtocolFieldAndSource() {
        val originals = fixtures()
        val source = LumenProfilePayload.encode(originals)
        val key = KeyPairGenerator.getInstance("RSA").apply { initialize(2048) }.generateKeyPair()
        val file = LumenProfileEnvelope.seal(LumenProfileEnvelope.Profile("Mixed ایران", source),
            LumenProfileEnvelope.Delivery.RECIPIENTS, listOf(key.public))
        val restored = LumenProfilePayload.decode(LumenProfileEnvelope.open(file, key.private).config)
        assertEquals(originals.size, restored.size)
        originals.zip(restored).forEach { (before, after) ->
            assertEquals(before.name, before, after)
            assertFalse(after.isProtected)
            if (before.scheme !in setOf("auto", "singbox_config", "xray_config") && before.name != "XHTTP") {
                assertEquals(before.name, SingboxConfigBuilder.buildOutbound(before), SingboxConfigBuilder.buildOutbound(after))
            }
        }
        assertTrue(restored.all(LumenProfilePayload::isValidForImport))
    }

    @Test fun unencryptedAndPasswordExportsRetainWholeConfigAndOpenVpnSource() {
        val source = LumenProfilePayload.encode(fixtures())
        val profile = LumenProfileEnvelope.Profile("Bundle", source, locked = false)
        for (delivery in listOf(LumenProfileEnvelope.Delivery.ANYONE, LumenProfileEnvelope.Delivery.PASSWORD)) {
            val password = "correct horse separate battery".toCharArray()
            val file = LumenProfileEnvelope.seal(profile, delivery, password = password)
            val result = LumenProfileEnvelope.open(file, password = password)
            assertEquals(profile, result)
            assertTrue(LumenProfilePayload.decode(result.config).single { it.scheme == "openvpn" }.link.contains("auth-user-pass"))
        }
    }

    @Test fun legacyLinksStillOpenButNestedProtectedReferencesAndUnknownVersionsFailClosed() {
        assertEquals("vpn.example", LumenProfilePayload.decode("socks://vpn.example:1080#Legacy").single().server)
        assertTrue(runCatching { LumenProfilePayload.encode(listOf(
            ParsedNode("Bad", "socks", "vpn.example", 1080, "lumen-locked://store/abc/0"))) }.isFailure)
        val json = JSONObject(LumenProfilePayload.encode(fixtures().take(1))).put("version", 99)
        assertTrue(runCatching { LumenProfilePayload.decode(json.toString()) }.isFailure)
        val privateNode = LinkParser.parseSingle("socks://vpn.example:1080#Private").copy(isProtected = true)
        assertTrue(runCatching { LumenProfilePayload.encode(listOf(privateNode)) }.isFailure)
        assertTrue(runCatching { com.lumen.core.config.parser.NodeExportNormalizer.export(privateNode) }.isFailure)
    }
}
