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
    private const val HEARTBEAT_INTERVAL_MS = 15 * 60 * 1000L // 15 minutes
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

    fun stopHeartbeatLoop() {
        heartbeatJob?.cancel()
        heartbeatJob = null
    }

    fun sendHeartbeat(context: Context) {
        try {
            val installId = getInstallId(context)
            val json = JSONObject().apply {
                put("kind", "heartbeat")
                put("app_version", "1.8.1")
                put("platform", "android")
                put("install_id", installId)
                put("ts", System.currentTimeMillis() / 1000)
            }

            val connection = (URL(TELEMETRY_URL).openConnection() as HttpURLConnection).apply {
                requestMethod = "POST"
                setRequestProperty("Content-Type", "application/json; charset=utf-8")
                setRequestProperty("User-Agent", "Lumen-Android/1.8.1")
                connectTimeout = 10000
                readTimeout = 10000
                doOutput = true
            }

            connection.outputStream.use { os ->
                os.write(json.toString().toByteArray(Charsets.UTF_8))
            }

            val responseCode = connection.responseCode
            connection.disconnect()
        } catch (_: Exception) {
            // Best effort anonymous ping: ignore network or server exceptions
        }
    }
}
