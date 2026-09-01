package com.lumen.core.vpn

import android.content.Context
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import java.time.Instant
import java.util.UUID
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec

object TelemetryManager {

    private const val TELEMETRY_URL = "https://diagnostics.lumen-kvn.eu.cc/api/ingest"
    // NOISE FILTER
    private const val DIAGNOSTICS_SECRET = "07f7d005166286e354645dcbce892998987bd8d8d20f296026dbb01ff05a9b8a"
    // Filled by the app layer from BuildConfig so the reported version never drifts.
    var appVersion: String = "0.7.0"
    // Keep the same cadence as desktop Lumen. The diagnostics dashboard defines
    // "online now" as a heartbeat seen in the last 45 minutes.
    private const val HEARTBEAT_INTERVAL_MS = 15 * 60 * 1000L
    private const val RETRY_INTERVAL_MS = 60 * 1000L
    private const val ERROR_BATCH_MAX = 50
    private const val ERROR_FLUSH_INTERVAL_MS = 30 * 1000L
    private const val ERROR_DISABLED_POLL_INTERVAL_MS = 1_000L
    private const val PREF_LAST_HEARTBEAT = "telemetry_last_heartbeat"
    /** User preference owned by App settings. Absent means enabled. */
    const val PREF_TELEMETRY_ENABLED = "telemetry_enabled"

    @Volatile
    private var errorUploadGeneration = 0L

    internal fun signatureHeaders(body: ByteArray, timestampSeconds: Long): Map<String, String> {
        val timestamp = timestampSeconds.toString()
        val bodyDigest = MessageDigest.getInstance("SHA-256").digest(body).toHex()
        val mac = Mac.getInstance("HmacSHA256").apply {
            init(
                SecretKeySpec(
                    DIAGNOSTICS_SECRET.toByteArray(Charsets.UTF_8),
                    "HmacSHA256"
                )
            )
        }
        val signature = mac.doFinal(
            "$timestamp.$bodyDigest".toByteArray(Charsets.UTF_8)
        ).toHex()
        return mapOf(
            "X-Diag-Timestamp" to timestamp,
            "X-Diag-Signature" to signature,
        )
    }

    private fun ByteArray.toHex(): String = joinToString("") {
        (it.toInt() and 0xff).toString(16).padStart(2, '0')
    }

    fun isEnabled(context: Context): Boolean =
        context.getSharedPreferences(VpnStartIntentFactory.PREFS_NAME, Context.MODE_PRIVATE)
            .getBoolean(PREF_TELEMETRY_ENABLED, true)

    fun setEnabled(context: Context, enabled: Boolean) {
        errorUploadGeneration++
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
        if (!enabled) VpnLogBus.discardDiagnosticEntries()
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

    /** Sends new WARNING/ERROR log entries in batches */
    fun startErrorUploadLoop(context: Context, scope: CoroutineScope) {
        val appContext = context.applicationContext
        scope.launch(Dispatchers.IO) {
            val pending = ArrayList<VpnLogEntry>(ERROR_BATCH_MAX)
            var nextFlushAt = System.currentTimeMillis() + ERROR_FLUSH_INTERVAL_MS
            var retryNotBefore = 0L
            var observedGeneration = errorUploadGeneration

            while (isActive) {
                if (observedGeneration != errorUploadGeneration) {
                    pending.clear()
                    VpnLogBus.discardDiagnosticEntries()
                    observedGeneration = errorUploadGeneration
                }
                if (!isEnabled(appContext)) {
                    pending.clear()
                    VpnLogBus.discardDiagnosticEntries()
                    nextFlushAt = System.currentTimeMillis() + ERROR_FLUSH_INTERVAL_MS
                    retryNotBefore = 0L
                    delay(ERROR_DISABLED_POLL_INTERVAL_MS)
                    continue
                }

                val waitMs = (nextFlushAt - System.currentTimeMillis()).coerceAtLeast(1L)
                val entry = withTimeoutOrNull(waitMs) {
                    VpnLogBus.diagnosticEntriesFlow.first()
                }
                if (entry != null) {
                    if (shouldUploadError(entry)) {
                        pending += entry
                    }
                    val now = System.currentTimeMillis()
                    if (pending.size >= ERROR_BATCH_MAX && now >= retryNotBefore) {
                        val sent = sendErrorBatch(appContext, pending)
                        if (sent) pending.clear()
                        val sentAt = System.currentTimeMillis()
                        nextFlushAt = sentAt + if (sent) ERROR_FLUSH_INTERVAL_MS else RETRY_INTERVAL_MS
                        retryNotBefore = if (sent) 0L else sentAt + RETRY_INTERVAL_MS
                    }
                } else {
                    if (pending.isNotEmpty()) {
                        val sent = sendErrorBatch(appContext, pending)
                        if (sent) pending.clear()
                        val sentAt = System.currentTimeMillis()
                        nextFlushAt = sentAt + if (sent) ERROR_FLUSH_INTERVAL_MS else RETRY_INTERVAL_MS
                        retryNotBefore = if (sent) 0L else sentAt + RETRY_INTERVAL_MS
                    } else {
                        nextFlushAt = System.currentTimeMillis() + ERROR_FLUSH_INTERVAL_MS
                        retryNotBefore = 0L
                    }
                }
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

    internal fun shouldUploadError(entry: VpnLogEntry): Boolean {
        if (entry.level.ordinal < VpnLogLevel.WARNING.ordinal) return false

        val message = entry.message
        val low = message.lowercase()
        if (DIAGNOSTIC_NOISE_TOKENS.any(low::contains)) return false
        if (diagnosticDomain(entry.component) == "core") {
            if (isRoutineCoreNoise(message)) return false
            if (ENGINE_LINE_REGEX.containsMatchIn(message)) return false
            if (ENGINE_NOISE_TOKENS.any(low::contains)) return false
        }
        return true
    }

    internal fun diagnosticEventJson(entry: VpnLogEntry): String {
        val logger = "android.${entry.component.trim().lowercase().ifBlank { "app" }}"
        return buildString {
            append('{')
            append("\"ts\":").append(jsonString(Instant.ofEpochMilli(entry.timestamp).toString()))
            append(",\"level\":").append(jsonString(entry.level.name))
            append(",\"domain\":").append(jsonString(diagnosticDomain(entry.component)))
            append(",\"logger\":").append(jsonString(logger))
            append(",\"msg\":").append(jsonString(entry.message))
            append('}')
        }
    }

    internal fun buildErrorBatchBody(
        appVersion: String,
        installId: String,
        entries: List<VpnLogEntry>
    ): ByteArray {
        val events = entries.joinToString(separator = ",", prefix = "[", postfix = "]") {
            diagnosticEventJson(it)
        }
        return buildString {
            append('{')
            append("\"kind\":").append(jsonString("error-batch"))
            append(",\"app_version\":").append(jsonString(appVersion))
            append(",\"platform\":").append(jsonString("android"))
            append(",\"install_id\":").append(jsonString(installId))
            append(",\"events\":").append(events)
            append('}')
        }.toByteArray(Charsets.UTF_8)
    }

    private fun jsonString(value: String): String = buildString(value.length + 2) {
        append('"')
        value.forEach { char ->
            when (char) {
                '\\' -> append("\\\\")
                '"' -> append("\\\"")
                '\b' -> append("\\b")
                '\u000C' -> append("\\f")
                '\n' -> append("\\n")
                '\r' -> append("\\r")
                '\t' -> append("\\t")
                in '\u0000'..'\u001F' -> append("\\u%04x".format(char.code))
                else -> append(char)
            }
        }
        append('"')
    }

    private fun diagnosticDomain(component: String): String = when {
        component.equals("CORE", ignoreCase = true) ||
            component.equals("TUN2SOCKS", ignoreCase = true) ||
            component.equals("OBFS", ignoreCase = true) -> "core"
        else -> "app"
    }

    private fun isRoutineCoreNoise(message: String): Boolean =
        ROUTINE_CORE_NOISE_REGEX.containsMatchIn(message)

    private fun sendErrorBatch(context: Context, entries: List<VpnLogEntry>): Boolean {
        if (entries.isEmpty() || !isEnabled(context)) return false
        var connection: HttpURLConnection? = null
        try {
            val installId = getInstallId(context)
            val requestBody = buildErrorBatchBody(appVersion, installId, entries)
            val timestampSeconds = System.currentTimeMillis() / 1000
            val activeConnection = (URL(TELEMETRY_URL).openConnection() as HttpURLConnection).apply {
                requestMethod = "POST"
                setRequestProperty("Content-Type", "application/json; charset=utf-8")
                setRequestProperty("Accept", "application/json")
                setRequestProperty("User-Agent", "Lumen-Android/$appVersion")
                signatureHeaders(requestBody, timestampSeconds).forEach { (name, value) ->
                    setRequestProperty(name, value)
                }
                connectTimeout = 10000
                readTimeout = 10000
                doOutput = true
            }
            connection = activeConnection
            activeConnection.outputStream.use { it.write(requestBody) }

            val responseCode = activeConnection.responseCode
            if (responseCode in 200..299) return true
            val response = activeConnection.errorStream?.bufferedReader()?.use { it.readText().take(256) }
                .orEmpty()
            VpnLogBus.info(
                "TELEMETRY",
                "error batch rejected, response $responseCode${response.takeIf { it.isNotBlank() }?.let { ": $it" }.orEmpty()}"
            )
        } catch (e: Exception) {
            // Diagnostics are strictly best effort and must not affect VPN work.
            VpnLogBus.info("TELEMETRY", "error batch failed: ${e.message}")
        } finally {
            connection?.disconnect()
        }
        return false
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
            val timestampSeconds = System.currentTimeMillis() / 1000
            val json = JSONObject().apply {
                put("kind", "heartbeat")
                put("app_version", appVersion)
                put("platform", "android")
                put("install_id", installId)
                put("ts", timestampSeconds)
            }
            val requestBody = json.toString().toByteArray(Charsets.UTF_8)

            val activeConnection = (URL(TELEMETRY_URL).openConnection() as HttpURLConnection).apply {
                requestMethod = "POST"
                setRequestProperty("Content-Type", "application/json; charset=utf-8")
                setRequestProperty("Accept", "application/json")
                setRequestProperty("User-Agent", "Lumen-Android/$appVersion")
                signatureHeaders(requestBody, timestampSeconds).forEach { (name, value) ->
                    setRequestProperty(name, value)
                }
                connectTimeout = 10000
                readTimeout = 10000
                doOutput = true
            }
            connection = activeConnection

            activeConnection.outputStream.use { os ->
                os.write(requestBody)
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

    private val ENGINE_LINE_REGEX = Regex("\\[(info|warning|error|debug)]", RegexOption.IGNORE_CASE)
    private val ROUTINE_CORE_NOISE_REGEX = Regex(
        "router:\\s+(?:" +
            "failed to search process:\\s+(?:process not found|access is denied)" +
            "|process dns packet:\\s+unpack request:\\s+bad question name:\\s+dns:" +
            ")",
        RegexOption.IGNORE_CASE
    )
    private val DIAGNOSTIC_NOISE_TOKENS = listOf(
        "connection:",
        "handshake",
        "dial tcp",
        "unexpected http response status",
        "unexpected response status"
    )
    private val ENGINE_NOISE_TOKENS = listOf(
        "common/errors",
        "infra/conf",
        "deprecated",
        "migrate to"
    )
}
