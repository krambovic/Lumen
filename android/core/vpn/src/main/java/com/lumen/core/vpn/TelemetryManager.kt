package com.lumen.core.vpn

import android.content.Context
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
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
    // A 15-minute loop kept waking the radio for the whole session; daily is enough.
    private const val HEARTBEAT_INTERVAL_MS = 24 * 60 * 60 * 1000L
    private var heartbeatJob: Job? = null

    fun getInstallId(context: Context): String {
        val prefs = context.getSharedPreferences("lumen_prefs", Context.MODE_PRIVATE)
        var installId = prefs.getString("install_id", null)
        if (installId.isNullOrBlank()) {
            installId = UUID.randomUUID().toString()
            prefs.edit().putString("install_id", installId).apply()
        }
        return installId
    }

    fun startHeartbeatLoop(context: Context, scope: CoroutineScope) {
        stopHeartbeatLoop()
        val appContext = context.applicationContext
        heartbeatJob = scope.launch(Dispatchers.IO) {
            while (isActive) {
                sendHeartbeat(appContext)
                delay(HEARTBEAT_INTERVAL_MS)
            }
        }
    }

    /** App-start ping: the VPN loop alone never fires for users who rarely connect. */
    fun sendStartupHeartbeat(context: Context, scope: CoroutineScope) {
        val appContext = context.applicationContext
        scope.launch(Dispatchers.IO) {
            val prefs = appContext.getSharedPreferences("lumen_prefs", Context.MODE_PRIVATE)
            val last = prefs.getLong("telemetry_last_heartbeat", 0L)
            val now = System.currentTimeMillis()
            if (last != 0L && now - last < HEARTBEAT_INTERVAL_MS) return@launch
            sendHeartbeat(appContext)
        }
    }

    fun stopHeartbeatLoop() {
        heartbeatJob?.cancel()
        heartbeatJob = null
    }

    fun sendHeartbeat(context: Context) {
        try {
            val installId = getInstallId(context)
            val json = JSONObject().apply {
                put("kind", "heartbeat")
                put("app_version", appVersion)
                put("platform", "android")
                put("install_id", installId)
                put("ts", System.currentTimeMillis() / 1000)
            }

            val connection = (URL(TELEMETRY_URL).openConnection() as HttpURLConnection).apply {
                requestMethod = "POST"
                setRequestProperty("Content-Type", "application/json; charset=utf-8")
                setRequestProperty("User-Agent", "Lumen-Android/$appVersion")
                connectTimeout = 10000
                readTimeout = 10000
                doOutput = true
            }

            connection.outputStream.use { os ->
                os.write(json.toString().toByteArray(Charsets.UTF_8))
            }

            val responseCode = connection.responseCode
            connection.disconnect()
            if (responseCode in 200..299) {
                context.getSharedPreferences("lumen_prefs", Context.MODE_PRIVATE)
                    .edit().putLong("telemetry_last_heartbeat", System.currentTimeMillis()).apply()
            }
            VpnLogBus.info("TELEMETRY", "heartbeat sent, response $responseCode")
        } catch (e: Exception) {
            // Best effort anonymous ping: log the reason so failures are diagnosable
            VpnLogBus.info("TELEMETRY", "heartbeat failed: ${e.message}")
        }
    }
}
