package com.lumen.core.vpn

import android.content.Context
import android.content.SharedPreferences
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertFalse
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.kotlin.any
import org.mockito.kotlin.anyOrNull
import org.mockito.kotlin.eq
import org.mockito.kotlin.mock
import org.mockito.kotlin.never
import org.mockito.kotlin.verify
import org.mockito.kotlin.whenever

class TelemetryManagerTest {

    @Test
    fun `signature headers match the diagnostics server contract`() {
        val body = "{\"kind\":\"heartbeat\",\"app_version\":\"1.1.1\",\"platform\":\"android\",\"install_id\":\"test\",\"ts\":1700000000}"

        assertEquals(
            "1700000000",
            TelemetryManager.signatureHeaders(body.toByteArray(Charsets.UTF_8), 1700000000L)
                .getValue("X-Diag-Timestamp")
        )
        assertEquals(
            "66fa4390b23611013da267c0b0ac5e4dc4c5035b84edbae486d0389e769da599",
            TelemetryManager.signatureHeaders(body.toByteArray(Charsets.UTF_8), 1700000000L)
                .getValue("X-Diag-Signature")
        )
    }

    @Test
    fun `error batch uses the desktop ingest shape`() {
        val entry = VpnLogEntry(
            timestamp = 1_700_000_000_000L,
            formattedTime = "00:00:00.000",
            level = VpnLogLevel.ERROR,
            component = "CORE",
            message = "fatal tunnel failure"
        )

        val body = String(
            TelemetryManager.buildErrorBatchBody("1.2.3", "install-id", listOf(entry)),
            Charsets.UTF_8
        )
        assertEquals(
            "{\"kind\":\"error-batch\",\"app_version\":\"1.2.3\",\"platform\":\"android\",\"install_id\":\"install-id\",\"events\":[{\"ts\":\"2023-11-14T22:13:20Z\",\"level\":\"ERROR\",\"domain\":\"core\",\"logger\":\"android.core\",\"msg\":\"fatal tunnel failure\"}]}",
            body
        )
    }

    @Test
    fun `error filter matches desktop warning noise rules`() {
        fun entry(component: String, level: VpnLogLevel, message: String) = VpnLogEntry(
            timestamp = 1_700_000_000_000L,
            formattedTime = "00:00:00.000",
            level = level,
            component = component,
            message = message
        )

        assertTrue(TelemetryManager.shouldUploadError(entry("VPN", VpnLogLevel.ERROR, "permission denied")))
        assertTrue(TelemetryManager.shouldUploadError(entry("VPN", VpnLogLevel.WARNING, "network unavailable")))
        assertFalse(TelemetryManager.shouldUploadError(entry("VPN", VpnLogLevel.INFO, "informational")))
        assertFalse(TelemetryManager.shouldUploadError(entry("CORE", VpnLogLevel.ERROR, "[error] common/errors: noisy core line")))
        assertFalse(TelemetryManager.shouldUploadError(entry("CORE", VpnLogLevel.WARNING, "handshake failed")))
        assertFalse(
            TelemetryManager.shouldUploadError(
                entry(
                    "CORE",
                    VpnLogLevel.WARNING,
                    "[singbox] +0300 2026-09-01 18:34:35 INFO router: " +
                        "failed to search process: process not found for 172.18.0.1:63918"
                )
            )
        )
        assertFalse(
            TelemetryManager.shouldUploadError(
                entry(
                    "CORE",
                    VpnLogLevel.ERROR,
                    "[singbox] +0600 2026-09-01 21:30:53 ERROR [703792730 3m38s] router: " +
                        "process DNS packet: unpack request: bad question name: dns: bad rdata"
                )
            )
        )
        assertFalse(
            TelemetryManager.shouldUploadError(
                entry(
                    "CORE",
                    VpnLogLevel.ERROR,
                    "[singbox] +0600 2026-09-01 21:30:50 ERROR [703792730 3m35s] router: " +
                        "process DNS packet: unpack request: bad question name: dns: buffer size too small"
                )
            )
        )
        assertFalse(
            TelemetryManager.shouldUploadError(
                entry(
                    "CORE",
                    VpnLogLevel.WARNING,
                    "[singbox] +0600 2026-09-01 21:20:29 INFO router: " +
                        "failed to search process: Access is denied."
                )
            )
        )
    }

    private fun contextWith(prefs: SharedPreferences): Context {
        val context = mock<Context>()
        whenever(context.getSharedPreferences(eq(VpnStartIntentFactory.PREFS_NAME), any()))
            .thenReturn(prefs)
        whenever(context.applicationContext).thenReturn(context)
        return context
    }

    private fun prefsWithTelemetry(enabled: Boolean?): SharedPreferences {
        val prefs = mock<SharedPreferences>()
        whenever(prefs.getBoolean(eq(TelemetryManager.PREF_TELEMETRY_ENABLED), any()))
            .thenAnswer { invocation -> enabled ?: invocation.getArgument<Boolean>(1) }
        return prefs
    }

    @Test
    fun `telemetry is enabled by default`() {
        assertTrue(TelemetryManager.isEnabled(contextWith(prefsWithTelemetry(null))))
    }

    @Test
    fun `telemetry reports disabled once the user turns it off`() {
        assertFalse(TelemetryManager.isEnabled(contextWith(prefsWithTelemetry(false))))
    }

    @Test
    fun `sendHeartbeat reads nothing and sends nothing while telemetry is off`() {
        val prefs = prefsWithTelemetry(false)

        TelemetryManager.sendHeartbeat(contextWith(prefs))

        verify(prefs, never()).getString(eq("install_id"), anyOrNull())
        verify(prefs, never()).edit()
    }

    @Test
    fun `sendStartupHeartbeat does not even consult the throttle while telemetry is off`() {
        val prefs = prefsWithTelemetry(false)

        TelemetryManager.sendStartupHeartbeat(
            contextWith(prefs),
            CoroutineScope(Dispatchers.Unconfined)
        )

        verify(prefs, never()).getLong(eq("telemetry_last_heartbeat"), any())
    }
}
