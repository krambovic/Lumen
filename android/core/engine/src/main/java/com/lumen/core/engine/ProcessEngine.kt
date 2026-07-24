package com.lumen.core.engine

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.util.concurrent.TimeUnit

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

    override val isRunning: Boolean
        get() = process?.isAlive == true

    override suspend fun start(configJson: String, tunFd: Int?) = withContext(Dispatchers.IO) {
        require(configJson.isNotBlank()) { "Engine configuration cannot be empty" }
        if (isRunning) stop()
        check(binary.isFile) {
            "sing-box extended binary is not bundled for this device ABI (${binary.absolutePath})"
        }

        workDir.mkdirs()
        val configFile = File(workDir, "${type.name.lowercase()}-config.json")
        configFile.writeText(configJson, Charsets.UTF_8)
        onLog("${type.name}: validating generated configuration")
        validateConfig(configFile)
        onLog("${type.name}: configuration is valid; starting core")

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
        Thread({
            runCatching {
                started.inputStream.bufferedReader().useLines { lines ->
                    lines.forEach { line ->
                        val trimmed = line.trim()
                        // Keep all operational core output. Outbound VLESS/Reality
                        // lines contain the handshake and dial errors needed to diagnose
                        // real connection failures; only verbose DEBUG is omitted.
                        if (trimmed.isNotEmpty() && !trimmed.contains(" [DEBUG] ")) {
                            onLog("${type.name}: $trimmed")
                        }
                    }
                }
                val exitCode = started.waitFor()
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

        kotlinx.coroutines.delay(250)
        if (!started.isAlive) {
            val code = started.exitValue()
            process = null
            error("${type.name} exited during startup with code $code")
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

    override suspend fun stop() = withContext(Dispatchers.IO) {
        val runningProcess = process ?: return@withContext
        process = null
        runningProcess.destroy()
        if (!runningProcess.waitFor(2, TimeUnit.SECONDS)) {
            runningProcess.destroyForcibly()
            runningProcess.waitFor(2, TimeUnit.SECONDS)
        }
    }

    override fun getTrafficStats(): TrafficStats = TrafficStats()
}
