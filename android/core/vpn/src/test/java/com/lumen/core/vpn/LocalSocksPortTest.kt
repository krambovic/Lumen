package com.lumen.core.vpn

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.net.InetAddress
import java.net.ServerSocket

class LocalSocksPortTest {

    private val generatedConfig = """
        {
          "inbounds": [
            {
              "type": "socks",
              "tag": "socks-in",
              "listen": "127.0.0.1",
              "listen_port": 10808,
              "udp_fragment": true
            },
            {
              "type": "http",
              "tag": "http-in",
              "listen": "127.0.0.1",
              "listen_port": 10809
            }
          ]
        }
    """.trimIndent()

    @Test
    fun `resolve keeps the configured port when it is free`() {
        assertEquals(
            10808,
            LocalSocksPort.resolve(10808, isFree = { true }, ephemeral = { 45000 })
        )
    }

    @Test
    fun `resolve moves off a port a dying core still holds`() {
        assertEquals(
            45000,
            LocalSocksPort.resolve(10808, isFree = { it != 10808 }, ephemeral = { 45000 })
        )
    }

    @Test
    fun `resolve falls back to the configured port when no alternative can be found`() {
        // The core then reports the conflict itself instead of binding somewhere random.
        assertEquals(
            10808,
            LocalSocksPort.resolve(10808, isFree = { false }, ephemeral = { 0 })
        )
    }

    @Test
    fun `resolve clamps a configured port outside the usable range`() {
        assertEquals(1024, LocalSocksPort.resolve(80, isFree = { true }, ephemeral = { 0 }))
    }

    @Test
    fun `isFree reports a port that is actually listening as taken`() {
        ServerSocket(0, 1, InetAddress.getByName("127.0.0.1")).use { held ->
            assertFalse(LocalSocksPort.isFree(held.localPort))
        }
    }

    @Test
    fun `the http inbound port is discoverable and rewritable too`() {
        // The HTTP inbound races for its port exactly like the SOCKS one, and losing
        // that race killed the core with
        // `start inbound/http[http-in]: listen tcp 127.0.0.1:10809: bind: address
        // already in use`. Its port is never passed to the service, so it has to be
        // read back out of the generated config.
        assertEquals(10809, LocalSocksPort.portOf(generatedConfig, LocalSocksPort.HTTP_INBOUND_TAG))
        assertEquals(10808, LocalSocksPort.portOf(generatedConfig))

        val rewritten = LocalSocksPort.applyToConfig(
            generatedConfig,
            45999,
            LocalSocksPort.HTTP_INBOUND_TAG
        )
        assertEquals(45999, LocalSocksPort.portOf(rewritten, LocalSocksPort.HTTP_INBOUND_TAG))
        // Rewriting one inbound must not disturb the other.
        assertEquals(10808, LocalSocksPort.portOf(rewritten))
    }

    @Test
    fun `portOf returns null when the config has no such inbound`() {
        assertEquals(null, LocalSocksPort.portOf("""{"inbounds":[]}""", LocalSocksPort.HTTP_INBOUND_TAG))
    }

    @Test
    fun `applyToConfig rewrites only the socks inbound`() {
        val rewritten = LocalSocksPort.applyToConfig(generatedConfig, 45123)
        assertTrue(rewritten.contains("\"listen_port\": 45123"))
        assertFalse(rewritten.contains("10808"))
        // The optional local HTTP inbound keeps the port the user configured.
        assertTrue(rewritten.contains("\"listen_port\": 10809"))
        assertTrue(rewritten.contains("\"udp_fragment\": true"))
    }

    @Test
    fun `applyToConfig ignores a route rule that only names the inbound`() {
        val withRule = """
            {
              "inbounds": [
                { "type": "socks", "tag": "socks-in", "listen_port": 10808 }
              ],
              "route": { "rules": [ { "inbound": ["socks-in"], "outbound": "proxy" } ] }
            }
        """.trimIndent()
        val rewritten = LocalSocksPort.applyToConfig(withRule, 45123)
        assertTrue(rewritten.contains("\"listen_port\": 45123"))
        assertTrue(rewritten.contains("\"inbound\": [\"socks-in\"]"))
    }

    @Test
    fun `applyToConfig leaves a config without a socks inbound alone`() {
        val tunOnly = """{"inbounds":[{"type":"tun","tag":"tun-in","mtu":1500}]}"""
        assertEquals(tunOnly, LocalSocksPort.applyToConfig(tunOnly, 45123))
    }
}
