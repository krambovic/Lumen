package com.lumen.core.config

import com.lumen.core.config.builder.SingboxConfigBuilder
import com.lumen.core.config.builder.SingboxConfigOptions
import com.lumen.core.config.parser.ParsedNode
import com.lumen.core.config.warp.WarpProfiles
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.util.Base64
import kotlin.random.Random

class WarpProfilesTest {
    private fun registration() = WarpProfiles.registration(JSONObject()
        .put("privKey", Base64.getEncoder().encodeToString(ByteArray(32) { 42 }))
        .put("peer_pub", Base64.getEncoder().encodeToString(ByteArray(32) { 84 }))
        .put("client_ipv4", "172.16.0.2/32").put("client_ipv6", "2606:4700:110:8765::2/128").toString())

    private fun masqueRegistration() = WarpProfiles.registration(JSONObject()
        .put("privKey", "MHcCAQEEIA25zQLXqGd935Et7ihZptBZTgHAOt1nUqGMQi+AfLK3oAoGCCqGSM49AwEHoUQDQgAE1bRHAEiia2r7rfYRVZMIEaEAb4pQSKlQeZ4+bOnrDVgru0VwW+uq7c182uLtUjSuNDMzhaLqnBl998/JOcl1Sw==")
        .put("peer_pub", "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEIaU7MToJm9NKp8YfGxR6r+/h4mcG7SxI8tsW8OR1A5tv/zCzVbCRRh2t87/kxnP6lAy0lkr7qYwu+ox+k3dr6w==")
        .put("client_ipv4", "172.16.0.2")
        .put("client_ipv6", "2606:4700:cf1:1000::1").toString(), masque = true)

    @Test fun desktopDefaultsProduceThreeAwgAndTwoMasqueProfilesOffline() {
        val awg = WarpProfiles.awg(registration(), Random(42))
        val masque = WarpProfiles.masque(masqueRegistration())
        assertEquals(3, awg.size)
        assertEquals(2, masque.size)
        assertEquals(5, (awg + masque).map { it.name }.distinct().size)
        assertTrue(awg.all { it.name.startsWith("WARP AWG") && it.outbound.isNotEmpty() })
        assertTrue(masque.all { it.scheme == "masque" && it.outbound.isNotEmpty() })
        (awg + masque).forEach { node ->
            val config = JSONObject(SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions(tunMode = false)))
            if (node.scheme == "masque") {
                val outbounds = config.getJSONArray("outbounds")
                val proxy = (0 until outbounds.length()).map(outbounds::getJSONObject)
                    .first { it.optString("tag") == "proxy" }
                assertEquals("masque", proxy.getString("type"))
                assertEquals("162.159.198.2", proxy.getString("server"))
                assertEquals(443, proxy.getInt("server_port"))
                assertTrue(proxy.getString("private_key").startsWith("MHcCAQEE"))
                assertTrue(proxy.getString("public_key").startsWith("MFkwEwYH"))
                assertEquals(2, proxy.getJSONArray("tunnel_address").length())
            }
        }
    }

    @Test(expected = IllegalArgumentException::class)
    fun aHelperResponseWithoutKeysIsRejected() {
        WarpProfiles.registration("{\"client_ipv4\":\"172.16.0.2\"}")
    }

    @Test fun directRoutingFallbackReachesTheEngineConfiguration() {
        val node = ParsedNode("test", "socks", "127.0.0.1", 1080, "",
            mapOf("type" to "socks", "server" to "127.0.0.1", "server_port" to 1080))
        val direct = JSONObject(SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions(tunMode = false, routingFinalOutbound = "direct")))
        val proxy = JSONObject(SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions(tunMode = false)))
        assertEquals("direct", direct.getJSONObject("route").getString("final"))
        assertEquals("proxy", proxy.getJSONObject("route").getString("final"))
    }
}
