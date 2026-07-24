package com.lumen.core.vpn

import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

enum class VpnLogLevel { DEBUG, INFO, WARNING, ERROR }

data class VpnLogEntry(
    val timestamp: Long,
    val formattedTime: String,
    val level: VpnLogLevel,
    val component: String,
    val message: String
)

/** Process-wide structured log stream shared by the UI, VPN service and cores. */
object VpnLogBus {
    private const val MAX_ENTRIES = 200
    private const val MAX_MESSAGE_CHARS = 2_000
    private const val MAX_STACK_FRAMES = 12
    private val timeFormatter = SimpleDateFormat("HH:mm:ss.SSS", Locale.US)

    private val _entries = MutableStateFlow<List<VpnLogEntry>>(emptyList())
    val entries: StateFlow<List<VpnLogEntry>> = _entries.asStateFlow()

    private val _lastError = MutableStateFlow<String?>(null)
    val lastError: StateFlow<String?> = _lastError.asStateFlow()

    @Volatile
    private var lastEmittedTime = 0L
    private val pendingEntries = mutableListOf<VpnLogEntry>()

    fun beginSession(engine: String) {
        clearLastError()
        info("VPN", "──────── New connection: $engine ────────")
    }

    fun clearLastError() {
        _lastError.value = null
    }

    fun debug(component: String, message: String) = append(VpnLogLevel.DEBUG, component, message)
    fun info(component: String, message: String) = append(VpnLogLevel.INFO, component, message)
    fun warning(component: String, message: String) = append(VpnLogLevel.WARNING, component, message)
    fun error(component: String, message: String, cause: Throwable? = null) {
        val details = buildString {
            append(message)
            cause?.let {
                append(": ")
                append(it.message ?: it::class.java.simpleName)
                it.stackTrace.take(MAX_STACK_FRAMES).forEach { frame ->
                    append('\n')
                    append("at ")
                    append(frame)
                }
            }
        }
        _lastError.value = message + (cause?.message?.let { ": $it" } ?: "")
        append(VpnLogLevel.ERROR, component, details)
    }

    fun clear() {
        _entries.value = emptyList()
        _lastError.value = null
    }

    private fun append(level: VpnLogLevel, component: String, message: String) {
        val normalized = message.trimEnd().take(MAX_MESSAGE_CHARS).ifEmpty { return }
        val now = System.currentTimeMillis()
        val formatted = synchronized(timeFormatter) { timeFormatter.format(Date(now)) }
        val entry = VpnLogEntry(
            timestamp = now,
            formattedTime = formatted,
            level = level,
            component = component,
            message = normalized
        )
        // Coalesce non-error logs to max 4 emissions/sec: buffer instead of dropping,
        // otherwise bursty core output silently loses lines needed for diagnosis.
        val forceUpdate = level == VpnLogLevel.ERROR || level == VpnLogLevel.WARNING
        val flushed = synchronized(pendingEntries) {
            pendingEntries.add(entry)
            if (!forceUpdate && now - lastEmittedTime <= 250) return
            lastEmittedTime = now
            pendingEntries.toList().also { pendingEntries.clear() }
        }
        _entries.update { (it + flushed).takeLast(MAX_ENTRIES) }
    }
}
