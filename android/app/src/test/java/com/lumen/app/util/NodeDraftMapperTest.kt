package com.lumen.app.util

import com.lumen.core.database.model.NodeEntity
import com.lumen.ui.screens.NodeDraft
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NodeDraftMapperTest {

    private val awgConf = """
        [Interface]
        PrivateKey = cPrivateKey0000000000000000000000000000000=
        Address = 10.66.0.2/32
        MTU = 1280
        DNS = 1.1.1.1, 8.8.8.8
        Jc = 4
        Jmin = 40
        Jmax = 70
        S1 = 15
        S2 = 30
        H1 = 1234567
        H2 = 7654321
        H3 = 2345678
        H4 = 8765432
        I1 = <b 0xf1f2>
        Itime = 30
        FwMark = 0x1234

        [Peer]
        PublicKey = sPublicKey00000000000000000000000000000000=
        AllowedIPs = 0.0.0.0/0, ::/0
        Endpoint = awg.example.com:51820
        PersistentKeepalive = 25
    """.trimIndent()

    private fun awgEntity(): NodeEntity = NodeEntity(
        id = "node-1",
        name = "AWG",
        protocol = "awg",
        server = "awg.example.com",
        port = 51820,
        link = awgConf
    )

    @Test
    fun awgDraftKeepsHeadersAndTunnelParameters() {
        val draft = NodeDraftMapper.draftFromEntity(awgEntity())!!
        assertEquals("1234567", draft.h1)
        assertEquals("8765432", draft.h4)
        assertEquals("<b 0xf1f2>", draft.i1)
        assertEquals("30", draft.itime)
        assertEquals("1280", draft.mtu)
        assertEquals("1.1.1.1, 8.8.8.8", draft.dns)
        assertEquals("25", draft.persistentKeepalive)
    }

    @Test
    fun awgRoundTripReEmitsEveryImportedDirective() {
        val draft = NodeDraftMapper.draftFromEntity(awgEntity())!!
        val conf = NodeDraftMapper.buildLink(draft)
        listOf(
            "MTU = 1280",
            "DNS = 1.1.1.1, 8.8.8.8",
            "H1 = 1234567",
            "H2 = 7654321",
            "H3 = 2345678",
            "H4 = 8765432",
            "I1 = <b 0xf1f2>",
            "Itime = 30",
            "PersistentKeepalive = 25",
            "Endpoint = awg.example.com:51820"
        ).forEach { line ->
            assertTrue("missing '$line' in\n$conf", conf.contains(line))
        }
    }

    @Test
    fun awgRoundTripKeepsUnknownDirectives() {
        val draft = NodeDraftMapper.draftFromEntity(awgEntity())!!
        assertTrue(NodeDraftMapper.buildLink(draft).contains("FwMark = 0x1234"))
    }

    @Test
    fun plainWireGuardDraftDoesNotEmitJunkParameters() {
        val draft = NodeDraftMapper.draftFromEntity(awgEntity())!!.copy(
            protocol = "wireguard",
            jc = "", jmin = "", jmax = "", s1 = "", s2 = "", s3 = "", s4 = "",
            h1 = "", h2 = "", h3 = "", h4 = "",
            i1 = "", i2 = "", i3 = "", i4 = "", i5 = "",
            j1 = "", j2 = "", j3 = "", itime = ""
        )
        val conf = NodeDraftMapper.buildLink(draft)
        assertFalse(conf.contains("H1 ="))
        assertFalse(conf.contains("Jc ="))
        assertTrue(conf.contains("MTU = 1280"))
    }

    @Test
    fun masqueDraftBuildsAParsableLink() {
        val draft = NodeDraft(
            name = "WARP",
            protocol = "masque",
            server = "profile-id-1",
            secret = "auth+token/value=",
            sni = "example.com",
            insecure = true
        )
        val link = NodeDraftMapper.buildLink(draft)
        assertTrue(link, link.startsWith("masque://"))
        assertTrue(link, link.contains("@profile-id-1?"))
        assertTrue(link, link.contains("sni=example.com"))
        assertTrue(link, link.contains("insecure=1"))
        assertTrue(link, link.endsWith("#WARP"))
    }

    @Test
    fun masqueRoundTripKeepsWarpOnlyParameters() {
        val entity = NodeEntity(
            id = "node-2",
            name = "WARP",
            protocol = "masque",
            server = "profile-id-1",
            port = 0,
            link = "masque://token@profile-id-1?sni=example.com&use_http2=1&name=masque0#WARP"
        )
        val draft = NodeDraftMapper.draftFromEntity(entity)!!
        assertEquals("token", draft.secret)
        assertEquals("example.com", draft.sni)
        val link = NodeDraftMapper.buildLink(draft)
        assertTrue(link, link.contains("use_http2=1"))
        assertTrue(link, link.contains("name=masque0"))
        assertEquals(1, Regex("sni=").findAll(link).count())
    }

    @Test
    fun hysteriaV1EditKeepsProtocolAndUnmodeledExtendedFields() {
        val entity = NodeEntity(
            id = "hy-1",
            name = "Hysteria one",
            protocol = "hysteria",
            server = "hy.example.com",
            port = 443,
            link = "hysteria://secret@hy.example.com:443?sni=hy.example.com#Hysteria",
            outboundJson = """
                {
                  "protocol":"hysteria",
                  "singbox":{
                    "type":"hysteria",
                    "server":"hy.example.com",
                    "server_port":443,
                    "auth_str":"secret",
                    "up_mbps":75,
                    "down_mbps":250,
                    "tls":{"enabled":true,"server_name":"hy.example.com"}
                  }
                }
            """.trimIndent()
        )

        val draft = NodeDraftMapper.draftFromEntity(entity)!!
        assertEquals("hysteria", draft.protocol)
        val saved = NodeDraftMapper.entityFromDraft(draft.copy(name = "Edited"))

        assertEquals("hysteria", saved.protocol)
        assertTrue(saved.link.startsWith("hysteria://"))
        val singbox = JSONObject(saved.outboundJson).getJSONObject("singbox")
        assertEquals("hysteria", singbox.getString("type"))
        assertEquals(75, singbox.getInt("up_mbps"))
        assertEquals(250, singbox.getInt("down_mbps"))
    }

    @Test
    fun clashShadowsocksEditKeepsPluginAndOriginalMetadata() {
        val entity = NodeEntity(
            id = "ss-1",
            name = "Plugin SS",
            protocol = "ss",
            server = "ss.example.com",
            port = 8388,
            link = "ss://YWVzLTEyOC1nY206c2VjcmV0@ss.example.com:8388#Plugin",
            outboundJson = """
                {
                  "protocol":"ss",
                  "settings":{
                    "servers":[{
                      "address":"ss.example.com",
                      "port":8388,
                      "method":"aes-128-gcm",
                      "password":"secret",
                      "plugin":"v2ray-plugin",
                      "plugin_opts":"mode=websocket;host=cdn.example.com"
                    }]
                  },
                  "clash":{"type":"ss","plugin":"v2ray-plugin"}
                }
            """.trimIndent()
        )

        val saved = NodeDraftMapper.entityFromDraft(NodeDraftMapper.draftFromEntity(entity)!!)
        val json = JSONObject(saved.outboundJson)
        val server = json.getJSONObject("settings").getJSONArray("servers").getJSONObject(0)
        assertEquals("v2ray-plugin", server.getString("plugin"))
        assertEquals("mode=websocket;host=cdn.example.com", server.getString("plugin_opts"))
        assertEquals("v2ray-plugin", json.getJSONObject("clash").getString("plugin"))
    }

    @Test
    fun newManualNodePersistsNormalizedOutboundJson() {
        val saved = NodeDraftMapper.entityFromDraft(
            NodeDraft(
                name = "Manual",
                protocol = "vless",
                server = "vless.example.com",
                port = "443",
                secret = "11111111-2222-3333-4444-555555555555",
                security = "tls",
                sni = "vless.example.com"
            )
        )

        assertTrue(saved.outboundJson.startsWith("{"))
        assertEquals("vless", JSONObject(saved.outboundJson).getString("protocol"))
    }
}
