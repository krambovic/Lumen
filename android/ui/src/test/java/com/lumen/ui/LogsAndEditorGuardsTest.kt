package com.lumen.ui

import com.lumen.ui.screens.LOG_FILTER_ALL
import com.lumen.ui.screens.LOG_STORE_LEVELS
import com.lumen.ui.screens.LogEntryUi
import com.lumen.ui.screens.LumenStrings
import com.lumen.ui.screens.NodeDraft
import com.lumen.ui.screens.SettingsUiState
import com.lumen.ui.screens.logLevelRank
import com.lumen.ui.screens.openVpnCredentialsMissing
import com.lumen.ui.screens.openVpnRequiresCredentials
import com.lumen.ui.screens.stringsForLanguage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Guards the persistent-log viewer, the OpenVPN credential check and the strings
 * both of them plus the camera permission dialog need in all four languages.
 */
class LogsAndEditorGuardsTest {

    @Test
    fun logLevelsAreOrderedBySeverity() {
        assertEquals(listOf("debug", "info", "warning", "error"), LOG_STORE_LEVELS)
        assertTrue(logLevelRank("error") > logLevelRank("warning"))
        assertTrue(logLevelRank("warning") > logLevelRank("info"))
        assertTrue(logLevelRank("info") > logLevelRank("debug"))
        // Unknown levels must never outrank a real one, or they would survive the filter.
        assertEquals(-1, logLevelRank("chatter"))
        assertTrue(logLevelRank("chatter") < logLevelRank("debug"))
    }

    @Test
    fun filteringKeepsEverythingAtOrAboveTheChosenLevel() {
        val entries = LOG_STORE_LEVELS.mapIndexed { index, level ->
            LogEntryUi(index.toLong(), "00:00:0$index", level, "VPN", "line $level")
        }
        val warningsAndWorse = entries.filter { logLevelRank(it.level) >= logLevelRank("warning") }
        assertEquals(listOf("warning", "error"), warningsAndWorse.map { it.level })
        assertEquals(4, entries.filter { logLevelRank(it.level) >= logLevelRank("debug") }.size)
        assertEquals("all", LOG_FILTER_ALL)
    }

    @Test
    fun formattedEntryCarriesLevelTimeAndComponent() {
        val entry = LogEntryUi(1L, "10:11:12.130", "error", "core", "boom")
        assertEquals("10:11:12.130 ERROR [core] boom", entry.formatted())
    }

    @Test
    fun loggingIsOnByDefaultAndIsTheOnlyLogSetting() {
        // Verbosity and retention are no longer user facing: one switch owns the
        // core log, the log bus and the persisted store.
        val state = SettingsUiState()
        assertTrue(state.loggingEnabled)
        val fields = SettingsUiState::class.java.declaredFields.map { it.name }
        assertTrue(fields.none { it.startsWith("logLevel") || it.startsWith("logMin") })
        assertTrue(fields.none { it.startsWith("logPersist") || it.startsWith("logRetention") })
    }

    private fun openVpnDraft(): NodeDraft = NodeDraft(
        protocol = "openvpn",
        server = "vpn.example.com",
        port = "1194",
        ovpnCa = "-----BEGIN CERTIFICATE-----"
    )

    @Test
    fun profileWithoutClientCertificateNeedsCredentials() {
        val draft = openVpnDraft()
        assertTrue(openVpnRequiresCredentials(draft))
        assertTrue(openVpnCredentialsMissing(draft))
        assertTrue(openVpnCredentialsMissing(draft.copy(ovpnUsername = "user")))
        assertTrue(openVpnCredentialsMissing(draft.copy(ovpnPassword = "secret")))
        assertFalse(openVpnCredentialsMissing(draft.copy(ovpnUsername = "user", ovpnPassword = "secret")))
    }

    @Test
    fun certificateOnlyProfileIsNotBlocked() {
        val draft = openVpnDraft().copy(
            ovpnCert = "-----BEGIN CERTIFICATE-----",
            ovpnKey = "-----BEGIN PRIVATE KEY-----"
        )
        assertFalse(openVpnRequiresCredentials(draft))
        assertFalse(openVpnCredentialsMissing(draft))
    }

    @Test
    fun profileDeclaringAuthUserPassNeedsCredentialsEvenWithCertificate() {
        val draft = openVpnDraft().copy(
            ovpnCert = "-----BEGIN CERTIFICATE-----",
            ovpnKey = "-----BEGIN PRIVATE KEY-----",
            rawConfig = "client\ndev tun\nauth-user-pass\nremote vpn.example.com 1194\n"
        )
        assertTrue(openVpnRequiresCredentials(draft))
        assertTrue(openVpnCredentialsMissing(draft))
        // The inline block form declares it too.
        val inline = draft.copy(rawConfig = "client\n<auth-user-pass>\nuser\npass\n</auth-user-pass>\n")
        assertTrue(openVpnRequiresCredentials(inline))
    }

    @Test
    fun credentialCheckIgnoresEveryOtherProtocol() {
        assertFalse(openVpnRequiresCredentials(NodeDraft(protocol = "vless")))
        assertFalse(openVpnCredentialsMissing(NodeDraft(protocol = "wireguard")))
    }

    private val newLabels = listOf(
        "loggingEnabled", "loggingEnabledDesc",
        "logLevelAll", "logLevelDebug", "logLevelInfo", "logLevelWarning", "logLevelError",
        "loadOlderLogs", "ovpnCredentialsRequired", "ovpnCertificateOnlyHint",
        "cameraPermissionTitle", "cameraPermissionMessage", "cameraPermissionBlocked",
        "openAppSettings"
    )

    @Test
    fun everyNewLabelIsPresentInAllLanguages() {
        listOf("en", "ru", "zh", "fa").forEach { language ->
            val strings = stringsForLanguage(language)
            newLabels.forEach { name ->
                val field = LumenStrings::class.java.getDeclaredField(name)
                field.isAccessible = true
                val value = field.get(strings) as String
                assertTrue("$language is missing $name", value.isNotBlank())
            }
        }
    }
}
