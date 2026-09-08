package com.lumen.core.config

import com.lumen.core.config.builder.SingboxConfigBuilder
import com.lumen.core.config.builder.SingboxConfigOptions
import com.lumen.core.config.parser.LinkParser
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class LocalProxyCredentialsRegressionTest {
    private val node = LinkParser.parseSingle("vless://11111111-1111-1111-1111-111111111111@one.example:443?security=tls#one")
    private fun build(user: String, password: String) = JSONObject(SingboxConfigBuilder.buildConfig(node,
        SingboxConfigOptions(tunMode = false, allowLanConnections = true, socksAuthEnabled = true,
            socksUsername = user, socksPassword = password)))

    @Test fun customCredentialsReachCoreWithoutTrimmingOrTruncation() {
        val username = " логин "
        val password = " пароль+/🚀 "
        val inbounds = build(username, password).getJSONArray("inbounds")
        val socks = (0 until inbounds.length()).map { inbounds.getJSONObject(it) }.first { it.getString("type") == "socks" }
        val account = socks.getJSONArray("users").getJSONObject(0)
        assertEquals(username, account.getString("username"))
        assertEquals(password, account.getString("password"))
        assertFalse((0 until inbounds.length()).any { inbounds.getJSONObject(it).getString("type") == "http" })
    }
    @Test fun invalidCredentialsNeverPublishAnUnauthenticatedListener() {
        assertThrows(IllegalArgumentException::class.java) { build("", "password") }
        assertThrows(IllegalArgumentException::class.java) { build("username", "я".repeat(128)) }
        assertThrows(IllegalArgumentException::class.java) { build("username", "pass\nword") }
    }
    @Test fun fullRfcByteLengthIsSupported() {
        build("u".repeat(255), "p".repeat(255))
    }
}
