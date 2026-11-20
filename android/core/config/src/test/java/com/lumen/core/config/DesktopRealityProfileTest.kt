package com.lumen.core.config

import com.lumen.core.config.builder.*
import com.lumen.core.config.parser.LinkParser
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class DesktopRealityProfileTest {
    private fun wrapper(network: String, port: Int, keyName: String = "publicKey"): JSONObject {
        val user = JSONObject().put("id", "00000000-0000-4000-8000-000000000001").put("encryption", "none")
        if (network == "tcp") user.put("flow", "xtls-rprx-vision")
        val stream = JSONObject().put("network", network).put("security", "reality")
            .put("realitySettings", JSONObject().put("fingerprint", "firefox").put("serverName", "cover.example")
                .put(keyName, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").put("shortId", "0123456789abcdef"))
        if (network == "xhttp") stream.put("xhttpSettings", JSONObject().put("path", "/test/").put("mode", "stream-up"))
        val outbound = JSONObject().put("protocol", "vless").put("tag", "inner")
            .put("settings", JSONObject().put("vnext", JSONArray().put(JSONObject()
                .put("address", "192.0.2.1").put("port", port).put("users", JSONArray().put(user)))))
            .put("streamSettings", stream)
        return JSONObject().put("protocol", "xray_config").put("tag", "Desktop-$port")
            .put("__lumen_scheme", "vless").put("__lumen_server", "192.0.2.1").put("__lumen_port", port)
            .put("serverDescription", "Profile description")
            .put("xray_config", JSONObject().put("outbounds", JSONArray().put(outbound)))
    }

    @Test fun desktopRealityWrappersKeepAllConnectionFields() {
        val profiles = listOf(wrapper("xhttp", 443), wrapper("tcp", 2053), wrapper("xhttp", 8443))
        val (nodes, errors) = LinkParser.parseLinksText(profiles.joinToString("\n"))
        assertEquals(emptyList<String>(), errors)
        assertEquals(3, nodes.size)
        nodes.forEachIndexed { index, node ->
            val port = listOf(443, 2053, 8443)[index]
            assertEquals("vless", node.scheme)
            assertEquals("192.0.2.1", node.server)
            assertEquals(port, node.port)
            assertEquals("Desktop-$port", node.name)
            assertEquals("Profile description", node.description)
            val result = JSONObject(HybridConfigBuilder.buildConfig(node, SingboxConfigOptions(tunMode = false), CoreMode.HYBRID) { 19080 })
            val outbound = result.getJSONObject(HybridConfigBuilder.SIDECAR_KEY).getJSONArray("outbounds").getJSONObject(0)
            val stream = outbound.getJSONObject("streamSettings")
            assertEquals("firefox", stream.getJSONObject("realitySettings").getString("fingerprint"))
            assertEquals("cover.example", stream.getJSONObject("realitySettings").getString("serverName"))
            if (port != 2053) {
                assertEquals("stream-up", stream.getJSONObject("xhttpSettings").getString("mode"))
                assertEquals("/test/", stream.getJSONObject("xhttpSettings").getString("path"))
            } else {
                assertEquals("xtls-rprx-vision", outbound.getJSONObject("settings").getJSONArray("vnext")
                    .getJSONObject(0).getJSONArray("users").getJSONObject(0).getString("flow"))
            }
        }
    }

    @Test fun singleWrapperAndCurrentRealityKeyAliasAreSupported() {
        val node = LinkParser.parseSingle(wrapper("tcp", 2053, "password").toString())
        val result = JSONObject(SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions(tunMode = false)))
        val tls = result.getJSONArray("outbounds").getJSONObject(0).getJSONObject("tls")
        assertEquals("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", tls.getJSONObject("reality").getString("public_key"))
    }

    @Test fun nestedDesktopWrapperCannotBypassDepthLimit() {
        var input = wrapper("tcp", 2053)
        repeat(20) { input = JSONObject().put("protocol", "xray_config").put("xray_config", input) }
        val (nodes, errors) = LinkParser.parseLinksText(input.toString())
        assertTrue(nodes.isEmpty())
        assertTrue(errors.isNotEmpty())
    }
}
