package com.lumen.core.vpn

import android.content.Context
import android.content.SharedPreferences
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.mockito.Answers
import org.mockito.kotlin.any
import org.mockito.kotlin.anyOrNull
import org.mockito.kotlin.eq
import org.mockito.kotlin.mock
import org.mockito.kotlin.never
import org.mockito.kotlin.verify
import org.mockito.kotlin.whenever
import java.io.File

/**
 * The AUTO pool crash: a 307 member pool serialises to ~100k characters, Kotlin
 * strings parcel as UTF-16, and startForegroundService() therefore pushed ~200 kB
 * of config through a Binder transaction that also carries everything else in
 * flight. 13 outbounds fitted, 86 was borderline, 307 always died with
 * TransactionTooLargeException. These tests pin the config out of the transaction.
 */
class VpnConfigStoreTest {

    @get:Rule
    val temp = TemporaryFolder()

    private lateinit var editor: SharedPreferences.Editor

    private fun context(strings: Map<String, String> = emptyMap()): Context {
        // RETURNS_SELF so the putX()/remove() chain in persistStartParams works.
        editor = mock<SharedPreferences.Editor>(defaultAnswer = Answers.RETURNS_SELF)
        val prefs = mock<SharedPreferences>()
        whenever(prefs.edit()).thenReturn(editor)
        whenever(prefs.getString(any(), anyOrNull())).thenAnswer { invocation ->
            strings[invocation.getArgument<String>(0)] ?: invocation.getArgument<String?>(1)
        }
        whenever(prefs.getInt(any(), any())).thenAnswer { it.getArgument<Int>(1) }
        whenever(prefs.getBoolean(any(), any())).thenAnswer { it.getArgument<Boolean>(1) }
        whenever(prefs.getStringSet(any(), anyOrNull())).thenAnswer {
            it.getArgument<Set<String>?>(1)
        }
        val context = mock<Context>()
        whenever(context.noBackupFilesDir).thenReturn(temp.root)
        whenever(context.getSharedPreferences(eq(VpnStartIntentFactory.PREFS_NAME), any()))
            .thenReturn(prefs)
        return context
    }

    /**
     * The real "Авто | WiFi" profile from the user's subscription: 307 Xray
     * outbounds mapped onto one sing-box urltest pool. The generated shape here is
     * the one core/sing-box-lumen.exe accepts (verified out of band, exit 0).
     */
    private fun autoPoolConfig(members: Int): String {
        val tags = (1..members).map { "node-$it" }
        val pool = tags.joinToString(",") { "\"$it\"" }
        val outbounds = tags.joinToString(",") { tag ->
            """{"type":"vless","tag":"$tag","server":"89.125.74.127","server_port":443,""" +
                """"uuid":"de2edd29-c746-4dc7-a50b-e0692b01209d","flow":"xtls-rprx-vision",""" +
                """"tls":{"enabled":true,"server_name":"yahoo.com",""" +
                """"utls":{"enabled":true,"fingerprint":"firefox"},""" +
                """"reality":{"enabled":true,""" +
                """"public_key":"QBSuzwfPwj9OW_pE0dQc5qgqnmfjh1Y9c10fq_bLZmQ",""" +
                """"short_id":"99bcf8d2"}}}"""
        }
        return """{"log":{"level":"warn"},""" +
            """"inbounds":[{"type":"socks","tag":"socks-in","listen":"127.0.0.1",""" +
            """"listen_port":10808}],""" +
            """"outbounds":[{"type":"urltest","tag":"AUTO","outbounds":[$pool],""" +
            """"url":"https://www.gstatic.com/generate_204","interval":"3m"},$outbounds,""" +
            """{"type":"direct","tag":"direct"}],""" +
            """"route":{"final":"AUTO","auto_detect_interface":true}}"""
    }

    @Test
    fun `the start intent stays small no matter how large the AUTO pool is`() {
        val context = context()
        val small = autoPoolConfig(13)
        val huge = autoPoolConfig(305)

        // The payload that used to travel in the Binder transaction. UTF-16, so
        // twice the character count.
        assertTrue(
            "expected a realistic multi-hundred-kB pool, got ${huge.length * 2} bytes",
            huge.length * 2 > 190_000
        )

        val smallPath = VpnStartIntentFactory.persistStartParams(
            context,
            VpnStartParams(configJson = small)
        )
        val smallBytes = VpnStartIntentFactory.startIntentPayloadBytes(
            context,
            VpnStartParams(configPath = smallPath)
        )
        val hugePath = VpnStartIntentFactory.persistStartParams(
            context,
            VpnStartParams(configJson = huge)
        )
        val hugeBytes = VpnStartIntentFactory.startIntentPayloadBytes(
            context,
            VpnStartParams(configPath = hugePath)
        )

        // Same file, same path, so the transaction cannot grow with the pool.
        assertEquals(smallPath, hugePath)
        assertEquals(smallBytes, hugeBytes)
        assertTrue("start intent grew to $hugeBytes bytes", hugeBytes < 4_096)
    }

    @Test
    fun `a 305 member pool round-trips through the store unchanged`() {
        val context = context()
        val config = autoPoolConfig(305)

        val path = VpnStartIntentFactory.persistStartParams(
            context,
            VpnStartParams(configJson = config)
        )

        assertEquals(config, VpnConfigStore.read(context, path))
        assertTrue(VpnConfigStore.hasStoredConfig(context))
        assertTrue(VpnStartIntentFactory.hasUsableConfig(context))
    }

    @Test
    fun `the config never goes back into SharedPreferences`() {
        val context = context()

        VpnStartIntentFactory.persistStartParams(
            context,
            VpnStartParams(configJson = autoPoolConfig(305))
        )

        // Not under either key, and any copy an older build left behind is dropped.
        verify(editor, never()).putString(eq(VpnStartIntentFactory.KEY_CONFIG_JSON), any())
        verify(editor, never()).putString(eq(VpnStartIntentFactory.KEY_LEGACY_CONFIG_JSON), any())
        verify(editor).remove(VpnStartIntentFactory.KEY_CONFIG_JSON)
        verify(editor).remove(VpnStartIntentFactory.KEY_LEGACY_CONFIG_JSON)
    }

    @Test
    fun `the config lives in no-backup storage and leaves no temp file behind`() {
        val context = context()

        VpnStartIntentFactory.persistStartParams(
            context,
            VpnStartParams(configJson = autoPoolConfig(64))
        )

        val stored = VpnConfigStore.configFile(context)
        assertTrue(stored.isFile)
        // Credentials for every server: never backed up, never on shared storage.
        assertTrue(stored.absolutePath.startsWith(temp.root.absolutePath))
        val leftovers = stored.parentFile?.listFiles()?.map { it.name }.orEmpty()
        assertEquals(listOf("active-config.json"), leftovers)
    }

    @Test
    fun `a start intent naming a foreign file is ignored`() {
        val context = context()
        VpnStartIntentFactory.persistStartParams(
            context,
            VpnStartParams(configJson = """{"outbounds":[{"type":"vless","tag":"ours"}]}""")
        )
        val foreign = File(temp.newFolder("elsewhere"), "active-config.json")
        foreign.writeText("""{"outbounds":[{"type":"vless","tag":"theirs"}]}""")

        // Falls back to our own file instead of loading whatever it was handed.
        assertTrue(VpnConfigStore.read(context, foreign.absolutePath).contains("ours"))

        // A sibling directory that merely shares our prefix is foreign too: a
        // startsWith() containment check would have accepted this one.
        val sibling = File(temp.root, "vpn-other").apply { mkdirs() }
        val lookalike = File(sibling, "active-config.json")
        lookalike.writeText("""{"outbounds":[{"type":"vless","tag":"lookalike"}]}""")
        assertTrue(VpnConfigStore.read(context, lookalike.absolutePath).contains("ours"))
    }

    @Test
    fun `an older install with the config in prefs is migrated on first read`() {
        val legacy = """{"outbounds":[{"type":"vless","tag":"legacy"}]}"""
        val context = context(mapOf(VpnStartIntentFactory.KEY_LEGACY_CONFIG_JSON to legacy))

        assertFalse(VpnConfigStore.hasStoredConfig(context))

        assertEquals(legacy, VpnConfigStore.read(context))

        // Moved to the file and struck from prefs, so the multi-hundred-kB string
        // stops being loaded with every other preference.
        assertTrue(VpnConfigStore.hasStoredConfig(context))
        assertEquals(legacy, VpnConfigStore.configFile(context).readText())
        verify(editor).remove(VpnStartIntentFactory.KEY_LEGACY_CONFIG_JSON)
    }

    @Test
    fun `replacing a stored config is atomic`() {
        val context = context()
        VpnStartIntentFactory.persistStartParams(
            context,
            VpnStartParams(configJson = autoPoolConfig(305))
        )
        val replacement = autoPoolConfig(13)

        VpnConfigStore.write(context, replacement)

        assertEquals(replacement, VpnConfigStore.read(context))
        assertFalse(File(VpnConfigStore.configFile(context).parentFile, "active-config.json.tmp").exists())
    }

    @Test
    fun `clearing the store leaves nothing to connect to`() {
        val context = context()
        VpnStartIntentFactory.persistStartParams(
            context,
            VpnStartParams(configJson = autoPoolConfig(13))
        )

        VpnConfigStore.clear(context)

        assertFalse(VpnConfigStore.hasStoredConfig(context))
        assertEquals("", VpnConfigStore.read(context))
    }
}
