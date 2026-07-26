package com.lumen.core.vpn

import android.content.Context
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.util.UUID

object TelemetryManager {

    private const val TELEMETRY_URL = "https://diagnostics.lumen-kvn.eu.cc/api/ingest"
    // Filled by the app layer from BuildConfig so the reported version never drifts.
    var appVersion: String = "0.7.0"
    // Keep the same cadence as desktop Lumen. The diagnostics dashboard defines
    // "online now" as a heartbeat seen in the last 45 minutes.
    private const val HEARTBEAT_INTERVAL_MS = 15 * 60 * 1000L
    private const val RETRY_INTERVAL_MS = 60 * 1000L
    private const val PREF_LAST_HEARTBEAT = "telemetry_last_heartbeat"
    /** User preference owned by App settings. Absent means enabled. */
    const val PREF_TELEMETRY_ENABLED = "telemetry_enabled"

    fun isEnabled(context: Context): Boolean =
        context.getSharedPreferences(VpnStartIntentFactory.PREFS_NAME, Context.MODE_PRIVATE)
            .getBoolean(PREF_TELEMETRY_ENABLED, true)

    fun setEnabled(context: Context, enabled: Boolean) {
        val editor = context.getSharedPreferences(
            VpnStartIntentFactory.PREFS_NAME,
            Context.MODE_PRIVATE
        ).edit().putBoolean(PREF_TELEMETRY_ENABLED, enabled)
        if (!enabled) {
            // Re-enabling must always produce a fresh heartbeat instead of inheriting
            // the throttle timestamp from an earlier consent period.
            editor.remove(PREF_LAST_HEARTBEAT)
        }
        editor.apply()
    }

    fun getInstallId(context: Context): String {
        val prefs = context.getSharedPreferences(
            VpnStartIntentFactory.PREFS_NAME,
            Context.MODE_PRIVATE
        )
        var installId = prefs.getString("install_id", null)
        if (installId.isNullOrBlank()) {
            installId = UUID.randomUUID().toString()
            prefs.edit().putString("install_id", installId).apply()
        }
        return installId
    }

    /** Compatibility entry point used by tests and one-shot app-start callers. */
    fun sendStartupHeartbeat(context: Context, scope: CoroutineScope) {
        val appContext = context.applicationContext
        if (!isEnabled(appContext)) return
        scope.launch(Dispatchers.IO) {
            sendHeartbeatIfDue(appContext)
        }
    }

    /**
     * Runs only for the lifetime of the app ViewModel and performs no work until the
     * telemetry is enabled. Failed requests are retried after one minute; accepted requests
     * are throttled to the desktop-compatible 15-minute cadence.
     */
    fun startHeartbeatLoop(context: Context, scope: CoroutineScope) {
        val appContext = context.applicationContext
        scope.launch(Dispatchers.IO) {
            while (isActive) {
                if (!isEnabled(appContext)) {
                    delay(RETRY_INTERVAL_MS)
                    continue
                }
                delay(sendHeartbeatIfDue(appContext))
            }
        }
    }

    /** Sends immediately after the user enables diagnostics in App settings. */
    fun sendHeartbeatNow(context: Context, scope: CoroutineScope) {
        val appContext = context.applicationContext
        scope.launch(Dispatchers.IO) {
            sendHeartbeat(appContext)
        }
    }

    private fun sendHeartbeatIfDue(context: Context): Long {
        val last = context.getSharedPreferences(
            VpnStartIntentFactory.PREFS_NAME,
            Context.MODE_PRIVATE
        ).getLong(PREF_LAST_HEARTBEAT, 0L)
        val elapsed = System.currentTimeMillis() - last
        if (last != 0L && elapsed >= 0L && elapsed < HEARTBEAT_INTERVAL_MS) {
            return HEARTBEAT_INTERVAL_MS - elapsed
        }
        return if (sendHeartbeat(context)) HEARTBEAT_INTERVAL_MS else RETRY_INTERVAL_MS
    }

    fun sendHeartbeat(context: Context): Boolean {
        if (!isEnabled(context)) return false
        var connection: HttpURLConnection? = null
        try {
            val installId = getInstallId(context)
            val json = JSONObject().apply {
                put("kind", "heartbeat")
                put("app_version", appVersion)
                put("platform", "android")
                put("install_id", installId)
                put("ts", System.currentTimeMillis() / 1000)
            }

            val activeConnection = (URL(TELEMETRY_URL).openConnection() as HttpURLConnection).apply {
                requestMethod = "POST"
                setRequestProperty("Content-Type", "application/json; charset=utf-8")
                setRequestProperty("Accept", "application/json")
                setRequestProperty("User-Agent", "Lumen-Android/$appVersion")
                connectTimeout = 10000
                readTimeout = 10000
                doOutput = true
            }
            connection = activeConnection

            activeConnection.outputStream.use { os ->
                os.write(json.toString().toByteArray(Charsets.UTF_8))
            }

            val responseCode = activeConnection.responseCode
            if (responseCode in 200..299) {
                context.getSharedPreferences(
                    VpnStartIntentFactory.PREFS_NAME,
                    Context.MODE_PRIVATE
                ).edit().putLong(PREF_LAST_HEARTBEAT, System.currentTimeMillis()).apply()
                VpnLogBus.info("TELEMETRY", "heartbeat accepted, response $responseCode")
                return true
            }
            val response = activeConnection.errorStream?.bufferedReader()?.use { it.readText().take(256) }
                .orEmpty()
            VpnLogBus.info(
                "TELEMETRY",
                "heartbeat rejected, response $responseCode${response.takeIf { it.isNotBlank() }?.let { ": $it" }.orEmpty()}"
            )
        } catch (e: Exception) {
            // Best effort anonymous ping: log the reason so failures are diagnosable
            VpnLogBus.info("TELEMETRY", "heartbeat failed: ${e.message}")
        } finally {
            connection?.disconnect()
        }
        return false
    }
}
