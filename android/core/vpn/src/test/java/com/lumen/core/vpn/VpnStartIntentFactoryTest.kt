package com.lumen.core.vpn

import android.content.Context
import android.content.SharedPreferences
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.kotlin.any
import org.mockito.kotlin.anyOrNull
import org.mockito.kotlin.eq
import org.mockito.kotlin.mock
import org.mockito.kotlin.whenever

class VpnStartIntentFactoryTest {

    private fun contextWithPrefs(
        strings: Map<String, String> = emptyMap(),
        ints: Map<String, Int> = emptyMap(),
        booleans: Map<String, Boolean> = emptyMap(),
        stringSets: Map<String, Set<String>> = emptyMap()
    ): Context {
        val prefs = mock<SharedPreferences>()
        whenever(prefs.getString(any(), anyOrNull())).thenAnswer { invocation ->
            strings[invocation.getArgument<String>(0)] ?: invocation.getArgument<String?>(1)
        }
        whenever(prefs.getInt(any(), any())).thenAnswer { invocation ->
            ints[invocation.getArgument<String>(0)] ?: invocation.getArgument<Int>(1)
        }
        whenever(prefs.getBoolean(any(), any())).thenAnswer { invocation ->
            booleans[invocation.getArgument<String>(0)] ?: invocation.getArgument<Boolean>(1)
        }
        whenever(prefs.getStringSet(any(), anyOrNull())).thenAnswer { invocation ->
            stringSets[invocation.getArgument<String>(0)]
                ?: invocation.getArgument<Set<String>?>(1)
        }
        val context = mock<Context>()
        whenever(context.getSharedPreferences(eq(VpnStartIntentFactory.PREFS_NAME), any()))
            .thenReturn(prefs)
        return context
    }

    @Test
    fun `isUsableConfig rejects the placeholder the tile used to send`() {
        assertFalse(VpnStartIntentFactory.isUsableConfig(null))
        assertFalse(VpnStartIntentFactory.isUsableConfig(""))
        assertFalse(VpnStartIntentFactory.isUsableConfig("{}"))
        assertFalse(VpnStartIntentFactory.isUsableConfig("""{"log":{}}"""))
    }

    @Test
    fun `isUsableConfig accepts a generated sing-box config`() {
        assertTrue(
            VpnStartIntentFactory.isUsableConfig("""{"outbounds":[{"type":"vless"}]}""")
        )
    }

    @Test
    fun `startParamsFromPrefs carries every start parameter the service reads`() {
        val context = contextWithPrefs(
            strings = mapOf(
                VpnStartIntentFactory.KEY_CONFIG_JSON to """{"outbounds":[]}""",
                VpnStartIntentFactory.KEY_ENGINE_TYPE to "SINGBOX",
                VpnStartIntentFactory.KEY_SPLIT_MODE to "ALLOW_LIST",
                VpnStartIntentFactory.KEY_DNS_MODE to "android",
                VpnStartIntentFactory.KEY_OBFS_TYPE to "obfs3",
                VpnStartIntentFactory.KEY_OBFS_HOST to "bridge.example.org"
            ),
            ints = mapOf(
                VpnStartIntentFactory.KEY_MTU to 1400,
                VpnStartIntentFactory.KEY_LOCAL_SOCKS_PORT to 20808,
                VpnStartIntentFactory.KEY_OBFS_PORT to 8443
            ),
            booleans = mapOf(
                VpnStartIntentFactory.KEY_RECONNECT_ON_NETWORK_CHANGE to false
            ),
            stringSets = mapOf(
                VpnStartIntentFactory.KEY_SPLIT_PACKAGES to setOf("com.example.browser")
            )
        )

        val params = VpnStartIntentFactory.startParamsFromPrefs(context)

        // The config never travels through prefs or the Binder transaction, not even
        // when an old build left a copy behind: only its path does. See VpnConfigStore.
        assertEquals("", params.configJson)
        assertEquals(VpnConfigStore.configFile(context).absolutePath, params.configPath)
        assertEquals("ALLOW_LIST", params.splitMode)
        assertEquals(setOf("com.example.browser"), params.splitPackages)
        assertEquals(1400, params.mtu)
        // The tile used to drop these four, silently breaking custom ports,
        // the DNS mode, the reconnect setting and obfs bridges.
        assertEquals(20808, params.localSocksPort)
        assertEquals("android", params.dnsMode)
        assertFalse(params.reconnectOnNetworkChange)
        assertEquals("obfs3", params.obfsType)
        assertEquals("bridge.example.org", params.obfsHost)
        assertEquals(8443, params.obfsPort)
    }

    @Test
    fun `startParamsFromPrefs falls back to the legacy config key and safe defaults`() {
        val context = contextWithPrefs(
            strings = mapOf(
                VpnStartIntentFactory.KEY_LEGACY_CONFIG_JSON to """{"outbounds":[]}"""
            )
        )

        val params = VpnStartIntentFactory.startParamsFromPrefs(context)

        // An upgrade still has the config under the legacy prefs key and nothing in the
        // store yet; the tile and the widget must still see something to connect to.
        assertTrue(VpnStartIntentFactory.hasUsableConfig(context))
        assertEquals(VpnStartIntentFactory.DEFAULT_ENGINE_TYPE, params.engineType)
        assertEquals(VpnStartIntentFactory.DEFAULT_SPLIT_MODE, params.splitMode)
        assertEquals(VpnStartIntentFactory.DEFAULT_DNS_MODE, params.dnsMode)
        assertEquals(VpnStartIntentFactory.DEFAULT_LOCAL_SOCKS_PORT, params.localSocksPort)
        assertEquals(LumenVpnService.DEFAULT_MTU, params.mtu)
        assertTrue(params.reconnectOnNetworkChange)
        assertTrue(params.obfsType.isEmpty())
    }

    @Test
    fun `startParamsFromPrefs yields an unusable config when nothing was imported`() {
        val params = VpnStartIntentFactory.startParamsFromPrefs(contextWithPrefs())

        assertFalse(VpnStartIntentFactory.isUsableConfig(params.configJson))
    }
}
