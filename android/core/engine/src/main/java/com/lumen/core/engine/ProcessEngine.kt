package com.lumen.core.engine

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.io.File
import java.util.Collections
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit

/** The core is missing for this device's CPU architecture, so no tunnel can ever be built. */
class MissingCoreBinaryException(message: String) : IllegalStateException(message)

/**
 * The core process died while it was still starting up. Its own last line is the
 * only place the real reason is ever stated (a bind conflict, a rejected
 * outbound), so it travels with the exception instead of being reduced to an
 * exit code.
 */
class CoreStartupException(
    message: String,
    val exitCode: Int,
    val output: String
) : IllegalStateException(message)

/** Runs a bundled command-line core as a child process. */
class ProcessEngine(
    override val type: EngineType,
    private val binary: File,
    private val workDir: File,
    private val onLog: (String) -> Unit = {},
    private val onUnexpectedExit: (Int) -> Unit = {}
) : IEngineDriver {
    @Volatile
    private var process: Process? = null

    @Volatile
    private var fatalLine: String = ""
    private val recentLines = ArrayDeque<String>()

    override val isRunning: Boolean
        get() = process?.isAlive == true

    override val lastFatalLine: String
        get() = fatalLine.ifEmpty { synchronized(recentLines) { recentLines.lastOrNull().orEmpty() } }

    override suspend fun start(configJson: String, tunFd: Int?) = withContext(Dispatchers.IO) {
        require(configJson.isNotBlank()) { "Engine configuration cannot be empty" }
        if (isRunning) stop()
        // Android destroys and recreates the VPN service on a reconnect, and every
        // instance builds its own ProcessEngine, so instance state cannot see the
        // core the previous instance spawned. Without this the old process is still
        // holding the local SOCKS port when the new one tries to bind it.
        reapLeftoverCores()
        if (!binary.isFile) {
            throw MissingCoreBinaryException(
                "This Lumen build does not contain the VPN core for your device's CPU " +
                    "architecture. Install the arm64-v8a, x86_64 or universal APK."
            )
        }

        workDir.mkdirs()
        val configFile = File(workDir, "${type.name.lowercase()}-config.json")
        configFile.writeText(configJson, Charsets.UTF_8)
        onLog("${type.name}: validating generated configuration")
        validateConfig(configFile)
        onLog("${type.name}: configuration is valid; starting core")

        fatalLine = ""
        synchronized(recentLines) { recentLines.clear() }

        val started = ProcessBuilder(
            binary.absolutePath,
            "run",
            "-c",
            configFile.absolutePath
        )
            .directory(workDir)
            .redirectErrorStream(true)
            .start()

        process = started
        liveProcesses.add(started)
        Thread({
            runCatching {
                started.inputStream.bufferedReader().useLines { lines ->
                    lines.forEach { line ->
                        // The core colours its own output even through a pipe; the
                        // escapes only make the log and the reported reason unreadable.
                        val trimmed = stripAnsi(line).trim()
                        // Keep all operational core output. Outbound VLESS/Reality
                        // lines contain the handshake and dial errors needed to diagnose
                        // real connection failures; only verbose DEBUG is omitted.
                        if (trimmed.isNotEmpty() && !trimmed.contains(" [DEBUG] ")) {
                            record(trimmed)
                            onLog("${type.name}: $trimmed")
                        }
                    }
                }
                val exitCode = started.waitFor()
                liveProcesses.remove(started)
                if (process === started) {
                    process = null
                    onLog("${type.name}: core exited unexpectedly with code $exitCode")
                    onUnexpectedExit(exitCode)
                }
            }.onFailure { onLog("${type.name}: log reader failed: ${it.message}") }
        }, "lumen-${type.name.lowercase()}-log").apply {
            isDaemon = true
            start()
        }

        // A bind conflict or a rejected config kills the core within milliseconds,
        // so poll instead of sleeping through the whole grace period.
        val deadline = System.nanoTime() + STARTUP_GRACE_MS * 1_000_000L
        while (started.isAlive && System.nanoTime() < deadline) delay(25)
        if (!started.isAlive) {
            val code = runCatching { started.exitValue() }.getOrDefault(-1)
            process = null
            liveProcesses.remove(started)
            // Let the reader thread drain the now closed pipe, otherwise the reason
            // is lost and the caller can only report a bare exit code.
            delay(STARTUP_DRAIN_MS)
            val reason = lastFatalLine
            throw CoreStartupException(
                if (reason.isBlank()) {
                    "${type.name} exited during startup with code $code"
                } else {
                    "${type.name} exited during startup: $reason"
                },
                code,
                reason
            )
        }
    }

    private fun record(line: String) {
        if (fatalLine.isEmpty() &&
            (line.contains("FATAL", ignoreCase = true) || line.contains("panic", ignoreCase = true))
        ) {
            fatalLine = line
        }
        synchronized(recentLines) {
            recentLines.addLast(line)
            while (recentLines.size > MAX_RECENT_LINES) recentLines.removeFirst()
        }
    }

    private fun validateConfig(configFile: File) {
        val validationOutput = File(workDir, "${type.name.lowercase()}-validation.log")
        val command = listOf(binary.absolutePath, "check", "-c", configFile.absolutePath)
        val validator = ProcessBuilder(command)
            .directory(workDir)
            .redirectErrorStream(true)
            .redirectOutput(validationOutput)
            .start()
        if (!validator.waitFor(10, TimeUnit.SECONDS)) {
            validator.destroyForcibly()
            error("${type.name} configuration validation timed out")
        }
        val output = validationOutput.takeIf(File::isFile)?.readText()?.trim().orEmpty()
        output.lineSequence().filter(String::isNotBlank).forEach { onLog("${type.name} CHECK: $it") }
        check(validator.exitValue() == 0) {
            "${type.name} rejected generated configuration (exit ${validator.exitValue()}): ${output.takeLast(2_000)}"
        }
    }

    override suspend fun stop(): Unit = withContext(Dispatchers.IO) {
        val runningProcess = process
        process = null
        if (runningProcess != null && !terminate(runningProcess)) {
            onLog("${type.name}: core did not exit within $STOP_TIMEOUT_MS ms")
        }
        // Whatever an earlier service instance left behind holds the same port.
        reapLeftoverCores()
    }

    /**
     * Kills every core this app process still owns and waits for it to be reaped,
     * so the listening socket is released before the next core binds it. Blocking:
     * every caller already runs on [Dispatchers.IO].
     */
    private fun reapLeftoverCores() {
        liveProcesses.toList().forEach { leftover ->
            if (leftover === process) return@forEach
            if (!leftover.isAlive) {
                liveProcesses.remove(leftover)
                return@forEach
            }
            onLog("${type.name}: a previous core is still running; stopping it first")
            terminate(leftover)
        }
    }

    /** destroy -> destroyForcibly with a bounded wait; true when the process is gone. */
    private fun terminate(target: Process): Boolean {
        if (target.isAlive) {
            target.destroy()
            if (!target.waitFor(STOP_GRACEFUL_MS, TimeUnit.MILLISECONDS)) {
                target.destroyForcibly()
                target.waitFor(STOP_TIMEOUT_MS - STOP_GRACEFUL_MS, TimeUnit.MILLISECONDS)
            }
        }
        val gone = !target.isAlive
        if (gone) liveProcesses.remove(target)
        return gone
    }

    override fun getTrafficStats(): TrafficStats = TrafficStats()

    companion object {
        private const val MAX_RECENT_LINES = 40
        private const val STARTUP_GRACE_MS = 300L
        private const val STARTUP_DRAIN_MS = 150L
        private const val STOP_GRACEFUL_MS = 1_500L
        private const val STOP_TIMEOUT_MS = 4_000L
        private val ANSI_ESCAPE = Regex("\\u001B\\[[0-9;]*[A-Za-z]")

        // Every core this app process spawned and has not reaped yet. The VPN service
        // is destroyed and recreated on a reconnect, so this cannot live on an instance.
        private val liveProcesses: MutableSet<Process> =
            Collections.newSetFromMap(ConcurrentHashMap<Process, Boolean>())

        fun stripAnsi(line: String): String = ANSI_ESCAPE.replace(line, "")

        /**
         * True when the core refused to bind its local inbound. Both wordings mean
         * the same errno (Linux first, Windows second) and both mean the previous
         * core has not released the port yet, which a retry fixes.
         */
        fun isAddressInUse(text: String?): Boolean {
            val lower = text?.lowercase() ?: return false
            return lower.contains("address already in use") ||
                lower.contains("address in use") ||
                lower.contains("only one usage of each socket address")
        }
    }
}
