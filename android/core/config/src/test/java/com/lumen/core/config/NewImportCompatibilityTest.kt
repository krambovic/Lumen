package com.lumen.core.config

import com.lumen.core.config.parser.LinkParser
import com.lumen.core.config.builder.*
import java.net.URLEncoder
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class NewImportCompatibilityTest {
    private val uuid = "00000000-0000-4000-8000-000000000001"
    @Test fun wireguardBase64SlashAndPlusSurviveQrLinkImport() {
        val key = "/+8AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
        val peer = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
        val link = "wg://$key@[2001:db8::1]:51820?publickey=$peer&address=10.0.0.2/32"
        val node = LinkParser.parseSingle(link)
        assertEquals("2001:db8::1", node.server)
        val native = node.outbound["singbox"] as Map<*, *>
        assertEquals(key, native["private_key"])
        assertEquals(51820, node.port)
    }
    @Test fun escapedIniQrRetainsIpv6Endpoint() {
        val ini = "[Interface]\\nPrivateKey = AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\\nAddress = 10.0.0.2/32\\n[Peer]\\nPublicKey = AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\\nEndpoint = [2001:db8::2]:51820\\nAllowedIPs = 0.0.0.0/0"
        val (nodes, errors) = LinkParser.parseLinksText(ini)
        assertEquals(errors.toString(), 1, nodes.size)
        assertEquals("2001:db8::2", nodes.single().server)
    }
    @Test fun advancedVlessFieldsReachXrayUnchanged() {
        val fm = "{\"tcp\":[{\"type\":\"fragment\",\"settings\":{\"packets\":\"tlshello\"}}]}"
        val extra = "{\"xPaddingBytes\":\"100-1000\",\"headers\":{\"User-Agent\":\"Example\"}}"
        val node = LinkParser.parseSingle("vless://$uuid@example.invalid:443?encryption=none&security=reality&type=xhttp&pbk=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&sid=abcd&pqv=verify&extra=${URLEncoder.encode(extra, "UTF-8")}&fm=${URLEncoder.encode(fm, "UTF-8")}")
        assertTrue(HybridConfigBuilder.needsXray(node))
        val result = JSONObject(HybridConfigBuilder.buildConfig(node, SingboxConfigOptions(tunMode=false), CoreMode.HYBRID) { 12345 })
        val sidecar = result.getJSONObject(HybridConfigBuilder.SIDECAR_KEY)
        val outbound = sidecar.getJSONArray("outbounds").getJSONObject(0)
        val stream = outbound.getJSONObject("streamSettings")
        assertEquals("verify", stream.getJSONObject("realitySettings").getString("mldsa65Verify"))
        assertEquals("Example", stream.getJSONObject("xhttpSettings").getJSONObject("extra").getJSONObject("headers").getString("User-Agent"))
        assertTrue(stream.has("finalMask"))
        assertEquals("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", stream.getJSONObject("realitySettings").getString("password"))
        val exported = com.lumen.core.config.parser.NodeExportNormalizer.export(node)
        val restored = LinkParser.parseSingle(exported)
        assertEquals(node.outbound, restored.outbound)
        assertEquals("none", outbound.getJSONObject("settings").getJSONArray("vnext").getJSONObject(0).getJSONArray("users").getJSONObject(0).getString("encryption"))
    }
    @Test fun incompleteRealityFailsBeforeNativeStartupWithActionableError() {
        val node = LinkParser.parseSingle("vless://$uuid@example.invalid:443?security=reality&type=xhttp&fm=%7B%7D")
        val error = runCatching { HybridConfigBuilder.buildConfig(node, SingboxConfigOptions(tunMode=false), CoreMode.HYBRID) { 12345 } }.exceptionOrNull()
        assertTrue(error?.message.orEmpty().contains("complete link"))
    }
    @Test fun peerCertificateHashIsNotConfusedWithSpkiHash() {
        val hash="ab".repeat(32)
        val node=LinkParser.parseSingle("vless://$uuid@example.invalid:443?security=tls&type=ws&pcs=$hash&pinSHA256=$hash")
        val stream=node.outbound["streamSettings"] as Map<*, *>
        val tls=stream["tlsSettings"] as Map<*, *>
        assertEquals(hash,tls["pinnedPeerCertSha256"])
        assertFalse(tls.containsKey("certificatePublicKeySha256"))
        assertTrue(HybridConfigBuilder.needsXray(node))
        val old=node.copy(outbound=node.outbound + ("streamSettings" to (stream + ("tlsSettings" to mapOf("certificatePublicKeySha256" to listOf(hash))))))
        val repaired=com.lumen.core.config.parser.ShareLinkExtensions.restore(old)
        assertEquals(tls,(repaired.outbound["streamSettings"] as Map<*, *>)["tlsSettings"])
    }
    @Test fun websocketNegotiatesOnlyAnOfferedHttp11ProtocolInXray() {
        val node=LinkParser.parseSingle("vless://$uuid@example.invalid:443?security=tls&type=ws&alpn=h3,h2,http%2F1.1")
        val json=JSONObject(HybridConfigBuilder.buildConfig(node,SingboxConfigOptions(tunMode=false),CoreMode.XRAY) { 12345 })
        val tls=json.getJSONObject(HybridConfigBuilder.SIDECAR_KEY).getJSONArray("outbounds").getJSONObject(0).getJSONObject("streamSettings").getJSONObject("tlsSettings")
        assertEquals("[\"http/1.1\"]",tls.getJSONArray("alpn").toString())
    }
    @Test fun ordinaryVlessDoesNotSpawnXrayInHybrid() {
        val node = LinkParser.parseSingle("vless://$uuid@example.invalid:443?encryption=none&type=ws&security=tls")
        assertFalse(HybridConfigBuilder.needsXray(node))
        val json = HybridConfigBuilder.buildConfig(node, SingboxConfigOptions(tunMode=false), CoreMode.HYBRID) { error("No sidecar expected") }
        assertFalse(JSONObject(json).has(HybridConfigBuilder.SIDECAR_KEY))
    }
    @Test fun installSubscriptionWrappersPreserveEncodedUrlTokens() {
        val url = "https://example.invalid/sub/a?token=x%2By"
        val encoded = URLEncoder.encode(url, "UTF-8")
        for (scheme in listOf("v2rayng", "v2rayn", "sing-box"))
            assertEquals(url, com.lumen.core.config.parser.SubscriptionLink.unwrap("$scheme://install-config?url=$encoded"))
        assertNull(com.lumen.core.config.parser.SubscriptionLink.unwrap("v2rayng://install-config?url=file%3A%2F%2Ftest"))
    }
    @Test fun uriVmessIsAcceptedAlongsideBase64Json() {
        val node = LinkParser.parseSingle("vmess://$uuid@example.invalid:443?type=ws&security=tls&scy=auto")
        assertEquals("vmess", node.scheme)
        assertEquals("vmess", node.outbound["protocol"])
    }
}
