package com.lumen.app

import androidx.test.platform.app.InstrumentationRegistry
import com.lumen.app.profiles.ProtectedProfiles
import com.lumen.core.config.crypto.LumenProfileEnvelope
import com.lumen.core.config.crypto.LumenProfilePolicy
import com.lumen.core.config.crypto.LumenProfilePayload
import com.lumen.core.config.crypto.ProtectedProfileCodec
import com.lumen.core.config.parser.LinkParser
import com.lumen.core.vpn.VpnConfigStore
import java.io.File
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

/** Real Android Keystore coverage; no VPN or user settings changes. */
class LumenProfileDeviceTest {
    @Test fun expiredProfilesCannotBeImportedOrResolvedAfterTheirAbsoluteDeadline() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val key = ProtectedProfileCodec.publicKey(ProtectedProfiles.recipientKey())
        val now = com.lumen.core.vpn.ProfilePolicyEnforcer.effectiveNow(context)
        val profile = LumenProfileEnvelope.Profile("Expiry fixture", "socks://vpn.example:1080",
            policy = LumenProfilePolicy(now - 20_000, now - 10_000))
        val file = LumenProfileEnvelope.seal(profile, LumenProfileEnvelope.Delivery.RECIPIENTS, listOf(key))
        assertTrue(runCatching { ProtectedProfiles.parse(file, context) }.exceptionOrNull()
            is com.lumen.core.config.crypto.ProfilePolicyException)
    }

    @Test fun closedSnapshotsResolveOriginalSourcesForRuntimeAndPasswordImportsRewrap() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val key = ProtectedProfileCodec.publicKey(ProtectedProfiles.recipientKey())
        val source = LinkParser.parseOpenVpnConfig(
            "client\ndev tun\nproto udp\nremote vpn.example 1194\nauth-user-pass\n<ca>\ntest-certificate\n</ca>\n")
        val profile = LumenProfileEnvelope.Profile("Closed fixture", LumenProfilePayload.encode(listOf(source)))
        val password = "device test separate passphrase".toCharArray()
        val store = File(context.noBackupFilesDir, "protected-profiles")
        val before = store.listFiles().orEmpty().map { it.name }.toSet()
        try {
            for (mode in LumenProfileEnvelope.Delivery.entries) {
                val exported = LumenProfileEnvelope.seal(profile, mode, listOf(key), password)
                val imported = if (mode == LumenProfileEnvelope.Delivery.PASSWORD)
                    ProtectedProfiles.unlockForImport(exported, password) else exported
                val reference = ProtectedProfiles.parse(imported, context).first.single().link
                assertTrue(reference.startsWith(ProtectedProfileCodec.PREFIX + "store/"))
                val runtime = ProtectedProfiles.resolve(reference, context)
                assertTrue(runtime.isProtected)
                assertEquals(source.link, runtime.link)
                assertEquals(source.outbound, runtime.outbound)
                assertEquals(source.scheme, runtime.scheme)
                assertEquals(source.server, runtime.server)
                assertEquals("Closed fixture", runtime.name)
                val hash = reference.removePrefix(ProtectedProfileCodec.PREFIX + "store/").substringBefore('/')
                val persisted = File(store, "$hash.lumen").readText()
                assertFalse(persisted.contains("vpn.example"))
                assertEquals("RECIPIENTS", JSONObject(persisted).getString("delivery"))
            }
        } finally {
            password.fill('\u0000')
            store.listFiles().orEmpty().filter { it.name !in before }.forEach { it.delete() }
        }
    }

    @Test fun protectedQuickSettingsConfigIsEncryptedAuthenticatedAndLegacyCacheMigrates() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val file = VpnConfigStore.configFile(context)
        val original = if (file.isFile) file.readBytes() else null
        val config = """{"_lumen_protected":true,"outbounds":[{"type":"socks","server":"private.example","server_port":1080,"password":"fixture-secret"}]}"""
        try {
            val path = VpnConfigStore.write(context, config)
            assertFalse(file.readText().contains("fixture-secret"))
            assertTrue(file.readText().startsWith("lumen-runtime-v1:"))
            assertEquals(config, VpnConfigStore.read(context, path))
            // An old plaintext protected cache must be upgraded before reuse.
            file.writeText(config)
            assertEquals(config, VpnConfigStore.read(context))
            assertFalse(file.readText().contains("private.example"))
            val bytes = file.readText().toCharArray()
            bytes[bytes.size / 2] = if (bytes[bytes.size / 2] == 'A') 'B' else 'A'
            file.writeText(String(bytes))
            assertEquals("", VpnConfigStore.read(context))
        } finally {
            if (original == null) file.delete() else file.writeBytes(original)
        }
    }
}
