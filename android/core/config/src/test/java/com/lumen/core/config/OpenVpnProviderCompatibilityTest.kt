package com.lumen.core.config

import com.lumen.core.config.builder.SingboxConfigBuilder
import com.lumen.core.config.builder.SingboxConfigOptions
import com.lumen.core.config.parser.LinkParser
import java.io.File
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test

class OpenVpnProviderCompatibilityTest {
    private fun build(profile: String): JSONObject {
        val (nodes, errors) = LinkParser.parseLinksText(profile)
        assertTrue("The OpenVPN profile must import", errors.isEmpty())
        assertEquals(1, nodes.size)
        return JSONObject(SingboxConfigBuilder.buildConfig(nodes.single(), SingboxConfigOptions(
            tunMode = false, localSocksPort = 10878, localHttpPort = 0, bypassLan = false,
            cacheFileEnabled = false, logLevel = "error"
        )))
    }

    @Test fun telegramStyleCertificateProfileWithConnectionBlockAndTlsCrypt() {
        val json = build("""
            dev tun
            auth-nocache
            <connection>
            remote vpn.example.org 10528 udp
            nobind
            connect-retry 5 5
            </connection>
            keepalive 10 20
            dhcp-option DNS 8.8.8.8
            dhcp-option DNS 8.8.4.4
            mute-replay-warnings
            resolv-retry infinite
            sndbuf 0
            rcvbuf 0
            remote-cert-tls server
            setenv CLIENT_CERT 0
            verb 3
            redirect-gateway def1
            cipher AES-256-CBC
            auth SHA512
            client
            <ca>
            CA
            </ca>
            <cert>
            CERT
            </cert>
            <key>
            KEY
            </key>
            <tls-crypt>
            STATIC_KEY
            </tls-crypt>
        """.trimIndent().replace("\n", "\r\n"))
        val endpoint = json.getJSONArray("endpoints").getJSONObject(0)
        assertEquals("openvpn-client", endpoint.getString("type"))
        assertEquals("udp", endpoint.getString("network"))
        assertEquals(10528, endpoint.getJSONArray("servers").getJSONObject(0).getInt("server_port"))
        assertEquals("AES-256-CBC", endpoint.getString("data_ciphers_fallback"))
        assertTrue(endpoint.getJSONArray("data_ciphers").toString().contains("AES-256-GCM"))
        assertTrue(endpoint.getJSONArray("data_ciphers").toString().contains("AES-128-GCM"))
        assertEquals("SHA512", endpoint.getString("auth"))
        assertEquals("10s", endpoint.getString("ping_interval"))
        assertEquals("20s", endpoint.getString("ping_restart"))
        val tls = endpoint.getJSONObject("tls")
        assertFalse("A remote address must not become verify-x509-name", tls.has("server_name"))
        assertEquals("CA\n", tls.getString("certificate"))
        assertEquals("CERT\n", tls.getString("client_certificate"))
        assertEquals("KEY\n", tls.getString("client_key"))
        assertEquals("tls_crypt", tls.getJSONObject("control_wrap").getString("type"))
        assertFalse(endpoint.has("proto"))
        assertFalse(endpoint.has("tls_crypt"))
    }

    @Test fun explicitNegotiationListAndSeparateLegacyFallbackArePreserved() {
        val json = build("""
            client
            dev tun
            proto tcp-client
            remote vpn.example.org 443
            cipher AES-256-CBC
            data-ciphers AES-256-GCM:AES-128-GCM
            data-ciphers-fallback AES-128-CBC
            <ca>
            CA
            </ca>
        """.trimIndent())
        val endpoint = json.getJSONArray("endpoints").getJSONObject(0)
        assertEquals("tcp", endpoint.getString("network"))
        assertEquals(listOf("AES-256-GCM", "AES-128-GCM"),
            endpoint.getJSONArray("data_ciphers").let { list -> (0 until list.length()).map(list::getString) })
        assertEquals("AES-128-CBC", endpoint.getString("data_ciphers_fallback"))
    }

    @Test fun nativeOpenVpnRemoteDoesNotAddCertificateIdentityVerification() {
        for (name in listOf("", "vpn-certificate")) {
            val tls = JSONObject().put("certificate", "CA")
            if (name.isNotEmpty()) tls.put("server_name", name)
            val native = JSONObject().put("type", "openvpn-client")
                .put("server", "vpn.example.org").put("server_port", 443)
                .put("network", "tcp").put("tls", tls)
            val endpoint = build(native.toString()).getJSONArray("endpoints").getJSONObject(0)
            assertEquals(name, endpoint.getJSONObject("tls").optString("server_name"))
        }
    }

    @Test fun transportFamiliesTlsVersionsAndCompressionSurviveImport() {
        for (transport in listOf("udp", "udp4", "udp6", "tcp-client", "tcp4-client", "tcp6-client")) {
            val endpoint = build("""
                client
                dev tun
                proto $transport
                remote vpn.example.org 1194
                cipher AES-256-CBC
                tls-version-min 1.2
                tls-version-max 1.3
                compress lz4-v2
                allow-compression asym
                reneg-sec 0
                verify-x509-name CN=vpn.example.org subject
                <ca>
                CA
                </ca>
            """.trimIndent()).getJSONArray("endpoints").getJSONObject(0)
            assertEquals(transport.removeSuffix("-client"), endpoint.getString("network"))
            assertEquals("lz4-v2", endpoint.getString("compression"))
            assertEquals("asym", endpoint.getString("allow_compression"))
            assertTrue(endpoint.getBoolean("renegotiate_disabled"))
            assertEquals("1.2", endpoint.getJSONObject("tls").getString("version_min"))
            assertEquals("subject", endpoint.getJSONObject("tls").getString("server_name_type"))
        }
    }

    /** Opt-in local fixture: provider certificates and private keys never enter the repository. */
    @Test fun externalProviderProfile() {
        val path = System.getenv("LUMEN_TEST_OVPN_PATH")
        assumeTrue("No external provider fixture supplied", !path.isNullOrEmpty())
        val profile = File(requireNotNull(path)).readText()
        val password = System.getenv("LUMEN_TEST_OVPN_PASSWORD")
        val json = build(if (password.isNullOrEmpty()) profile else profile + "\n<askpass>\n$password\n</askpass>\n")
        System.getenv("LUMEN_TEST_OVPN_OUTPUT")?.let { File(it).writeText(json.toString(2)) }
        assertEquals("openvpn-client", json.getJSONArray("endpoints").getJSONObject(0).getString("type"))
    }
}
