package com.lumen.core.vpn

import android.content.Context
import android.content.SharedPreferences
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertFalse
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
