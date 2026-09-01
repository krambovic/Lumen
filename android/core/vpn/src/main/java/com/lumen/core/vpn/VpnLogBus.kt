package com.lumen.core.vpn

import android.content.Context
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update

enum class VpnLogLevel { DEBUG, INFO, WARNING, ERROR }

data class VpnLogEntry(
    val timestamp: Long,
    val formattedTime: String,
    val level: VpnLogLevel,
    val component: String,
    val message: String
)

/**
 * Everything the user can change about the log.
 *
 * [level] is the lowest level that is kept at all: it filters both the live view
 * and the file, so choosing WARNING really does make the log quieter instead of
 * only hiding lines that were written anyway.
 */
data class VpnLogSettings(
    /** Master switch. Off records nothing at all, live view included. */
    val enabled: Boolean = true,
    val persist: Boolean = true,
    val level: VpnLogLevel = VpnLogLevel.DEBUG,
    val retention: VpnLogRetention = VpnLogRetention()
) {
    fun sanitized(): VpnLogSettings = copy(retention = retention.sanitized())
}

/**
 * Process-wide structured log stream shared by the UI, VPN service and cores.
 *
 * The live [entries] flow stays a small in-memory ring for the real-time view; the
 * durable copy goes to [VpnLogStore] on a single writer thread, so no caller and
 * no main thread ever waits for the disk. Entries are appended and flushed one by
 * one, which is what makes the tail survive a crash or a kill.
 */
object VpnLogBus {
    private const val MAX_ENTRIES = 200
    private const val MAX_MESSAGE_CHARS = 2_000
    private const val MAX_STACK_FRAMES = 12
    private const val MAX_PENDING_WRITES = 512
    private const val FLUSH_TIMEOUT_MS = 2_000L
    private const val DIR_NAME = "logs"
    private const val EXPORT_NAME = "lumen-log.txt"

    /**
     * Preference keys; the settings screen writes them through [updateSettings].
     * Deliberately prefixed: "log_level" already belongs to the core's own
     * verbosity, which is a different setting with different values.
     */
    const val KEY_LOGGING_ENABLED = "app_log_enabled"
    const val KEY_PERSIST_LOGS = "app_log_persist"
    const val KEY_LOG_LEVEL = "app_log_level"
    const val KEY_LOG_MAX_BYTES = "app_log_max_bytes"
    const val KEY_LOG_MAX_ENTRIES = "app_log_max_entries"

    /** One screenful; the log screen pages further back with `skipFromEnd`. */
    const val DEFAULT_PAGE_SIZE = 200

    /** An export has to fit into an Intent extra, so it is bounded too. */
    const val DEFAULT_EXPORT_ENTRIES = 2_000

    private val timeFormatter = SimpleDateFormat("HH:mm:ss.SSS", Locale.US)

    // Rejecting instead of blocking: a caller that logs must never wait for the
    // disk, and losing a line under a burst beats stalling the VPN service.
    private val writer = ThreadPoolExecutor(
        1,
        1,
        0L,
        TimeUnit.MILLISECONDS,
        ArrayBlockingQueue<Runnable>(MAX_PENDING_WRITES),
        { runnable -> Thread(runnable, "lumen-log-writer").apply { isDaemon = true } },
        ThreadPoolExecutor.DiscardPolicy()
    )

    @Volatile
    private var logStore: VpnLogStore? = null

    @Volatile
    private var initialized = false

    private val _entries = MutableStateFlow<List<VpnLogEntry>>(emptyList())
    val entries: StateFlow<List<VpnLogEntry>> = _entries.asStateFlow()

    // The diagnostics uploader consumes only new WARNING/ERROR entries. This is
    // deliberately separate from the live ring and persisted log: restoring an
    // old log after process start must not resend the same events forever.
    private val diagnosticEntries = Channel<VpnLogEntry>(
        capacity = MAX_PENDING_WRITES,
        onBufferOverflow = BufferOverflow.DROP_OLDEST
    )
    internal val diagnosticEntriesFlow: Flow<VpnLogEntry> = diagnosticEntries.receiveAsFlow()

    private val _lastError = MutableStateFlow<String?>(null)
    val lastError: StateFlow<String?> = _lastError.asStateFlow()

    private val _settings = MutableStateFlow(VpnLogSettings())
    val settings: StateFlow<VpnLogSettings> = _settings.asStateFlow()

    @Volatile
    private var lastEmittedTime = 0L
    private val pendingEntries = mutableListOf<VpnLogEntry>()

    /**
     * Attaches the persistent log and restores what survived the last run into the
     * live view. Idempotent, cheap and safe to call from the main thread: the
     * restore itself runs on the writer thread.
     *
     * Both the app and the service call it, because a tile or widget start brings
     * the service up without the app ever being created.
     */
    fun init(context: Context) {
        if (initialized) return
        val app = context.applicationContext ?: context
        initialized = true
        _settings.value = loadSettings(app)
        val directory = runCatching { logDirectory(app) }.getOrNull() ?: return
        val store = attach(directory) ?: return
        submit {
            val restored = runCatching { store.readTail(MAX_ENTRIES) }.getOrDefault(emptyList())
            if (restored.isNotEmpty()) {
                _entries.update { (restored + it).takeLast(MAX_ENTRIES) }
            }
        }
    }

    /** Points the persistent log at [directory]. Opens nothing until the first write. */
    fun attach(directory: File): VpnLogStore? {
        val store = runCatching { VpnLogStore(directory, _settings.value.retention) }.getOrNull()
        logStore = store
        return store
    }

    fun logDirectory(context: Context): File = File(context.noBackupFilesDir, DIR_NAME)

    fun loadSettings(context: Context): VpnLogSettings = runCatching {
        val prefs = context.getSharedPreferences(
            VpnStartIntentFactory.PREFS_NAME,
            Context.MODE_PRIVATE
        )
        val defaults = VpnLogSettings()
        VpnLogSettings(
            enabled = prefs.getBoolean(KEY_LOGGING_ENABLED, defaults.enabled),
            persist = prefs.getBoolean(KEY_PERSIST_LOGS, defaults.persist),
            level = runCatching {
                VpnLogLevel.valueOf(prefs.getString(KEY_LOG_LEVEL, null) ?: defaults.level.name)
            }.getOrDefault(defaults.level),
            retention = VpnLogRetention(
                maxBytes = prefs.getLong(KEY_LOG_MAX_BYTES, VpnLogRetention.DEFAULT_MAX_BYTES),
                maxEntries = prefs.getInt(KEY_LOG_MAX_ENTRIES, VpnLogRetention.DEFAULT_MAX_ENTRIES)
            )
        ).sanitized()
    }.getOrDefault(VpnLogSettings())

    /** Applies the settings to the running bus without storing them. */
    fun updateSettings(settings: VpnLogSettings) {
        val sanitized = settings.sanitized()
        _settings.value = sanitized
        val store = logStore ?: return
        submit { store.setRetention(sanitized.retention) }
    }

    /** Stores the settings in the shared prefs and applies them. */
    fun updateSettings(context: Context, settings: VpnLogSettings) {
        val sanitized = settings.sanitized()
        runCatching {
            context.getSharedPreferences(VpnStartIntentFactory.PREFS_NAME, Context.MODE_PRIVATE)
                .edit()
                .putBoolean(KEY_LOGGING_ENABLED, sanitized.enabled)
                .putBoolean(KEY_PERSIST_LOGS, sanitized.persist)
                .putString(KEY_LOG_LEVEL, sanitized.level.name)
                .putLong(KEY_LOG_MAX_BYTES, sanitized.retention.maxBytes)
                .putInt(KEY_LOG_MAX_ENTRIES, sanitized.retention.maxEntries)
                .apply()
        }
        updateSettings(sanitized)
    }

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
        // Dialogs already use "Connection failed" as their title. Prefer the actual
        // exception reason, strip the core's colour escapes, and remove wrappers left
        // by an older service so the body never reads "Connection failed" twice.
        _lastError.value = userFacingError(message, cause)
        append(VpnLogLevel.ERROR, component, details)
    }

    internal fun userFacingError(message: String, cause: Throwable?): String {
        val raw = cause?.message?.takeIf(String::isNotBlank) ?: message
        return raw
            .replace(Regex("\\u001B\\[[0-9;]*[A-Za-z]"), "")
            .trim()
            .removeRepeatedPrefix("Connection failed:")
            .ifBlank { "Connection failed" }
    }

    private fun String.removeRepeatedPrefix(prefix: String): String {
        var value = this
        while (value.startsWith(prefix, ignoreCase = true)) {
            value = value.substring(prefix.length).trimStart()
        }
        return value
    }

    /** Drops the live view and the persisted log; the log screen's "clear" button. */
    fun clear() {
        _entries.value = emptyList()
        _lastError.value = null
        discardDiagnosticEntries()
        synchronized(pendingEntries) { pendingEntries.clear() }
        val store = logStore ?: return
        submit { store.clear() }
    }

    /** Drops events that were generated while telemetry was disabled or logs were cleared. */
    internal fun discardDiagnosticEntries() {
        while (diagnosticEntries.tryReceive().isSuccess) {
            // Drain the bounded channel without ever blocking the VPN thread.
        }
    }

    /**
     * The persisted tail, oldest first. [skipFromEnd] pages further back, so the log
     * screen loads a screenful at a time instead of the whole file.
     *
     * Blocking IO: call it from a background dispatcher.
     */
    fun readPersisted(limit: Int = DEFAULT_PAGE_SIZE, skipFromEnd: Int = 0): List<VpnLogEntry> {
        val store = logStore ?: return emptyList()
        flushPersisted()
        return runCatching { store.readTail(limit, skipFromEnd) }.getOrDefault(emptyList())
    }

    /** Bytes the persisted log currently occupies; for the retention setting's summary. */
    fun persistedSize(): Long = logStore?.let { runCatching { it.size() }.getOrDefault(0L) } ?: 0L

    /**
     * The persisted tail rendered for sharing as text. Bounded, because the share
     * intent carries it in a Binder transaction.
     *
     * Blocking IO: call it from a background dispatcher.
     */
    fun exportText(limit: Int = DEFAULT_EXPORT_ENTRIES): String {
        val persisted = readPersisted(limit)
        val source = persisted.ifEmpty { _entries.value }
        return source.joinToString("\n") { VpnLogStore.format(it) }
    }

    /**
     * Writes the whole persisted log to a file in the cache directory and returns
     * it, or null when there is nothing to export. Meant for a FileProvider share.
     *
     * Blocking IO: call it from a background dispatcher.
     */
    fun exportToFile(context: Context): File? {
        val store = logStore ?: return null
        flushPersisted()
        val target = File(File(context.cacheDir, DIR_NAME), EXPORT_NAME)
        val written = runCatching { store.exportTo(target) }.getOrDefault(0L)
        return if (written > 0L) target else null
    }

    /** Waits for the queued writes to reach the disk. Returns false on timeout. */
    fun flushPersisted(timeoutMs: Long = FLUSH_TIMEOUT_MS): Boolean {
        val done = CountDownLatch(1)
        if (!submit { done.countDown() }) return false
        return runCatching { done.await(timeoutMs, TimeUnit.MILLISECONDS) }.getOrDefault(false)
    }

    fun formatTime(timestamp: Long): String =
        synchronized(timeFormatter) { timeFormatter.format(Date(timestamp)) }

    private fun submit(task: () -> Unit): Boolean = runCatching {
        writer.execute { runCatching { task() } }
        true
    }.getOrDefault(false)

    private fun append(level: VpnLogLevel, component: String, message: String) {
        val current = _settings.value
        // Logging off means nothing is recorded anywhere - not the live ring either.
        if (!current.enabled) return
        // The chosen level decides what exists at all, live view included.
        if (level.ordinal < current.level.ordinal) return
        val normalized = message.trimEnd().take(MAX_MESSAGE_CHARS).ifEmpty { return }
        val now = System.currentTimeMillis()
        val entry = VpnLogEntry(
            timestamp = now,
            formattedTime = formatTime(now),
            level = level,
            component = component,
            message = normalized
        )
        if (level.ordinal >= VpnLogLevel.WARNING.ordinal) {
            diagnosticEntries.trySend(entry)
        }
        persist(entry)
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

    /** Never touches the disk on the calling thread; the writer does. */
    private fun persist(entry: VpnLogEntry) {
        if (!_settings.value.persist) return
        val store = logStore ?: return
        submit { store.append(entry) }
    }
}
