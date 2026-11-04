package com.lumen.app.util

import com.lumen.core.config.builder.SingboxConfigBuilder
import com.lumen.core.config.builder.SingboxConfigOptions
import com.lumen.core.config.parser.LinkParser
import com.lumen.core.config.parser.OpenVpnCredentials
import com.lumen.core.config.parser.OpenVpnSecrets
import com.lumen.core.database.model.NodeEntity
import java.io.File
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test

/** Local opt-in fixtures; provider identities and secrets never enter test sources/results. */
class OpenVpnExternalFixtureTest {
    @Test fun importedAndEditedProviderConfigurationsPreserveEndpoint() {
        val path = System.getenv("LUMEN_OVPN_FIXTURE_MANIFEST")
        assumeTrue("No private provider fixtures supplied", !path.isNullOrBlank())
        val fixtures = JSONArray(File(requireNotNull(path)).readText())
        for (index in 0 until fixtures.length()) {
            val fixture = fixtures.getJSONObject(index)
            val source = File(fixture.getString("source")).readText()
            val secrets = OpenVpnSecrets(fixture.optString("username"), fixture.optString("password"),
                fixture.optString("keyPassword"))
            val imported = OpenVpnCredentials.persist(LinkParser.parseOpenVpnConfig(source), secrets)
            val entity = NodeEntity(name = "Private fixture", protocol = imported.scheme,
                server = imported.server, port = imported.port, link = imported.link,
                outboundJson = LinkParser.toJsonString(imported.outbound))
            val draft = requireNotNull(NodeDraftMapper.draftFromEntity(entity))
            val edited = NodeDraftMapper.entityFromDraft(draft)
            val options = SingboxConfigOptions(tunMode = false, localSocksPort = 19881,
                localHttpPort = 0, bypassLan = false, cacheFileEnabled = false, logLevel = "debug")
            val before = JSONObject(SingboxConfigBuilder.buildConfig(imported, options))
            val after = JSONObject(SingboxConfigBuilder.buildConfig(
                LinkParser.parseLinksText(edited.link).first.single(), options))
            val originalEndpoint = before.getJSONArray("endpoints").getJSONObject(0)
            val editedEndpoint = after.getJSONArray("endpoints").getJSONObject(0)
            // Do not put endpoint values (including credentials) into assertion messages.
            assertTrue("Fixture $index transport changed", originalEndpoint.optString("network") == editedEndpoint.optString("network"))
            assertTrue("Fixture $index remotes changed", originalEndpoint.optJSONArray("servers")?.toString() ==
                editedEndpoint.optJSONArray("servers")?.toString())
            assertTrue("Fixture $index negotiated ciphers changed", originalEndpoint.optJSONArray("data_ciphers")?.toString() ==
                editedEndpoint.optJSONArray("data_ciphers")?.toString())
            for (key in listOf("username", "password", "auth", "data_ciphers_fallback")) {
                assertTrue("Fixture $index field $key changed", originalEndpoint.optString(key) == editedEndpoint.optString(key))
            }
            for (key in listOf("certificate", "client_certificate", "client_key", "cipher", "version_min")) {
                assertTrue("Fixture $index TLS field $key changed", originalEndpoint.getJSONObject("tls").optString(key) ==
                    editedEndpoint.getJSONObject("tls").optString(key))
            }
            val wrapBefore = originalEndpoint.getJSONObject("tls").optJSONObject("control_wrap")
            val wrapAfter = editedEndpoint.getJSONObject("tls").optJSONObject("control_wrap")
            for (key in listOf("type", "key", "direction")) {
                assertTrue("Fixture $index control protection changed", wrapBefore?.optString(key) == wrapAfter?.optString(key))
            }
            fixture.optString("outputDirectory").takeIf { it.isNotBlank() }?.let { output ->
                File(output).mkdirs()
                File(output, "$index-imported.json").writeText(before.toString())
                File(output, "$index-edited.json").writeText(after.toString())
            }
        }
    }
}
