package com.lumen.app.util

import com.lumen.core.config.parser.LinkParser
import com.lumen.core.config.parser.OpenVpnCredentials
import com.lumen.core.config.parser.OpenVpnSecrets
import com.lumen.core.config.builder.SingboxConfigBuilder
import com.lumen.core.config.builder.SingboxConfigOptions
import com.lumen.core.database.model.NodeEntity
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class OpenVpnCredentialPersistenceTest {
    private val source = "client\ndev tun\nproto udp\nremote vpn.example 1194\nauth-user-pass\n<ca>\ntest-certificate\n</ca>\n"
    private fun entity(raw: String): NodeEntity {
        val node = LinkParser.parseLinksText(raw).first.single()
        return NodeEntity(name = "Fixture", protocol = node.scheme, server = node.server, port = node.port,
            link = raw, outboundJson = LinkParser.toJsonString(node.outbound))
    }

    @Test fun credentialsFromConnectPromptSurviveReloadAndAppearInEditor() {
        val original = source.replace("remote vpn.example 1194", "<connection>\nremote vpn.example 443 tcp\nconnect-retry 5 5\n</connection>")
        val supplied = OpenVpnCredentials.persist(LinkParser.parseOpenVpnConfig(original),
            OpenVpnSecrets("alice", "secret+%/=", "key-secret"))
        val reopened = NodeDraftMapper.draftFromEntity(entity(supplied.link))!!
        assertEquals("alice", reopened.ovpnUsername)
        assertEquals("secret+%/=", reopened.ovpnPassword)
        assertEquals("key-secret", reopened.ovpnKeyPassword)
        assertTrue(supplied.link.contains("<connection>\nremote vpn.example 443 tcp\nconnect-retry 5 5\n</connection>"))
        assertFalse(OpenVpnCredentials.missing(LinkParser.parseOpenVpnConfig(supplied.link)).any)
    }

    @Test fun promptCredentialReplacementDoesNotKeepOldPasswordBlocksOrLoseNativeFields() {
        val first = OpenVpnCredentials.persist(LinkParser.parseOpenVpnConfig(source), OpenVpnSecrets("alice", "old-secret", "old-key"))
        val second = OpenVpnCredentials.persist(first, OpenVpnSecrets(password = "new-secret", keyPassword = "new-key"))
        assertFalse(second.link.contains("old-secret"))
        assertFalse(second.link.contains("old-key"))
        assertEquals(1, Regex("<auth-user-pass>").findAll(second.link).count())
        val raw = """{"type":"openvpn-client","server":"vpn.example","server_port":443,"network":"tcp6","tls":{"certificate":"CA","client_certificate":"CERT","client_key":"KEY"},"data_ciphers":["AES-256-GCM"]}"""
        val node = LinkParser.parseLinksText(raw).first.single()
        val patched = OpenVpnCredentials.persist(node, OpenVpnSecrets("alice", "secret"))
        val json = JSONObject(patched.link)
        assertEquals("tcp6", json.getString("network"))
        assertEquals("CERT", json.getJSONObject("tls").getString("client_certificate"))
        assertEquals("AES-256-GCM", json.getJSONArray("data_ciphers").getString(0))
        assertEquals("alice", NodeDraftMapper.draftFromEntity(entity(patched.link))!!.ovpnUsername)
    }

    @Test fun nativeCredentialEditPreservesTcpAndCertificateRolesForBothSchemas() {
        for (legacy in listOf(true, false)) {
            val tls = JSONObject().put(if (legacy) "ca" else "certificate", "ROOT_CA")
                .put(if (legacy) "certificate" else "client_certificate", "CLIENT_CERT")
                .put(if (legacy) "key" else "client_key", "CLIENT_KEY")
            val remote = JSONObject().put("server", "vpn.example").put("server_port", 443).put("network", "tcp6")
            val raw = JSONObject().put("type", if (legacy) "openvpn" else "openvpn-client")
                .put("servers", org.json.JSONArray().put(remote)).put("tls", tls)
                .put(if (legacy) "proto" else "network", "tcp6").toString()
            val draft = NodeDraftMapper.draftFromEntity(entity(raw))!!
            assertEquals("tcp6", draft.ovpnProto)
            assertEquals("ROOT_CA", draft.ovpnCa)
            assertEquals("CLIENT_CERT", draft.ovpnCert)
            assertEquals("CLIENT_KEY", draft.ovpnKey)
            val saved = NodeDraftMapper.entityFromDraft(draft.copy(ovpnUsername = "alice", ovpnPassword = "secret"))
            val reopened = NodeDraftMapper.draftFromEntity(saved)!!
            assertEquals("tcp6", reopened.ovpnProto)
            assertEquals("CLIENT_CERT", reopened.ovpnCert)
            val parsed = LinkParser.parseLinksText(saved.link).first.single()
            val endpoint = JSONObject(SingboxConfigBuilder.buildConfig(parsed,
                SingboxConfigOptions(tunMode = false, cacheFileEnabled = false)))
                .getJSONArray("endpoints").getJSONObject(0)
            assertEquals("tcp6", endpoint.getJSONArray("servers").getJSONObject(0).getString("network"))
            assertEquals("ROOT_CA", endpoint.getJSONObject("tls").getString("certificate"))
            assertEquals("CLIENT_CERT", endpoint.getJSONObject("tls").getString("client_certificate"))
            assertFalse(endpoint.getJSONObject("tls").has("cert"))
        }
    }

    @Test fun textEditorKeepsAddressFamilyAndExtraRemoteTransport() {
        val raw = source.replace("proto udp", "proto tcp6").replace("remote vpn.example 1194",
            "remote vpn.example 443 tcp6\nremote backup.example 1194 udp4")
        val draft = NodeDraftMapper.draftFromEntity(entity(raw))!!
        assertEquals("tcp6", draft.ovpnProto)
        val saved = NodeDraftMapper.entityFromDraft(draft.copy(ovpnUsername = "alice", ovpnPassword = "secret"))
        val native = LinkParser.parseOpenVpnConfig(saved.link).outbound["singbox"] as Map<*, *>
        val remotes = native["servers"] as List<*>
        assertEquals("tcp6", (remotes[0] as Map<*, *>)["network"])
        assertEquals("udp4", (remotes[1] as Map<*, *>)["network"])
    }

    @Test fun ovpnUserAndPasswordSurviveSaveAndReopening() {
        val draft = NodeDraftMapper.draftFromEntity(entity(source))!!
        val saved = NodeDraftMapper.entityFromDraft(draft.copy(ovpnUsername = "alice", ovpnPassword = "secret+%/=", ovpnKeyPassword = "key-secret"))
        val reopened = NodeDraftMapper.draftFromEntity(saved)!!
        assertEquals("alice", reopened.ovpnUsername)
        assertEquals("secret+%/=", reopened.ovpnPassword)
        assertEquals("key-secret", reopened.ovpnKeyPassword)
    }

    @Test fun nativeJsonUserAndPasswordAreSavedRatherThanDiscarded() {
        val raw = """{"type":"openvpn","tag":"Native","servers":[{"server":"vpn.example","server_port":1194,"proto":"udp"}],"tls":{"enabled":true,"ca":"test-certificate"},"cipher":"AES-128-GCM","auth":"SHA256"}"""
        val draft = NodeDraftMapper.draftFromEntity(entity(raw))!!
        assertEquals("test-certificate", draft.ovpnCa)
        val saved = NodeDraftMapper.entityFromDraft(draft.copy(ovpnUsername = "alice", ovpnPassword = "secret+%/=", ovpnKeyPassword = "key-secret"))
        val reopened = NodeDraftMapper.draftFromEntity(saved)!!
        assertEquals("alice", reopened.ovpnUsername)
        assertEquals("secret+%/=", reopened.ovpnPassword)
        assertEquals("key-secret", reopened.ovpnKeyPassword)
        val root = JSONObject(saved.link)
        assertEquals("test-certificate", root.getJSONObject("tls").getString("ca"))
    }

    @Test fun externalCredentialReferencesArePromptRequirementsAndSessionOverridesSatisfyThem() {
        val parsed = LinkParser.parseOpenVpnConfig(source.replace("auth-user-pass", "auth-user-pass login.txt"))
        val missing = OpenVpnCredentials.missing(parsed)
        assertTrue(missing.username && missing.password)
        val supplied = OpenVpnCredentials.apply(parsed, OpenVpnSecrets("alice", "secret"))
        assertFalse(OpenVpnCredentials.missing(supplied).any)
        assertTrue(OpenVpnCredentials.missing(parsed).any)
    }

    @Test fun savingCredentialsKeepsBareCompressionSeparateFromFollowingDirective() {
        for (newline in listOf("\n", "\r\n")) {
            for ((directive, nativeKey, expected) in listOf(
                Triple("comp-lzo", "compression_lzo", "adaptive"),
                Triple("compress", "compression", "stub")
            )) {
                val raw = source.replace("auth-user-pass", "$directive\npersist-key\n\nauth-user-pass")
                    .replace("\n", newline)
                val draft = NodeDraftMapper.draftFromEntity(entity(raw))!!
                val saved = NodeDraftMapper.entityFromDraft(draft.copy(ovpnUsername = "alice", ovpnPassword = "secret+%/="))
                val reopened = NodeDraftMapper.draftFromEntity(saved)!!
                assertEquals("alice", reopened.ovpnUsername)
                assertEquals("secret+%/=", reopened.ovpnPassword)
                val parsed = LinkParser.parseOpenVpnConfig(saved.link)
                val native = parsed.outbound["singbox"] as Map<*, *>
                assertEquals(expected, native[nativeKey])
                assertFalse(saved.link.contains("$directive persist-key"))
            }
        }
    }

    @Test fun savingCredentialsPreservesBareCompressionAtEndOfFile() {
        val draft = NodeDraftMapper.draftFromEntity(entity(source + "comp-lzo"))!!
        val saved = NodeDraftMapper.entityFromDraft(draft.copy(ovpnUsername = "alice", ovpnPassword = "secret"))
        val native = LinkParser.parseOpenVpnConfig(saved.link).outbound["singbox"] as Map<*, *>
        assertEquals("adaptive", native["compression_lzo"])
    }
}
