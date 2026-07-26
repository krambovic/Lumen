package com.lumen.core.engine

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

enum class EngineType {
    SINGBOX
}

data class TrafficStats(
    val uploadSpeed: Long = 0L,
    val downloadSpeed: Long = 0L,
    val totalUploaded: Long = 0L,
    val totalDownloaded: Long = 0L
)

sealed interface EngineState {
    data object Idle : EngineState
    data object Starting : EngineState
    data class Running(
        val engineType: EngineType,
        val stats: TrafficStats = TrafficStats()
    ) : EngineState
    data class Error(val message: String) : EngineState
}

interface IEngineDriver {
    val type: EngineType
    val isRunning: Boolean

    /**
     * The core's own last FATAL/panic line, or its last line of output. A driver
     * that captures nothing reports an empty string.
     */
    val lastFatalLine: String get() = ""
    suspend fun start(configJson: String, tunFd: Int? = null)
    suspend fun stop()
    fun getTrafficStats(): TrafficStats
}

/**
 * Engine manager managing lifecycle, state transitions, and process safety
 * for the sing-box extended driver.
 */
class EngineManager(
    val singboxEngine: IEngineDriver
) {
    private val _state = MutableStateFlow<EngineState>(EngineState.Idle)
    val state: StateFlow<EngineState> = _state.asStateFlow()

    private val mutex = Mutex()
    private var activeDriver: IEngineDriver? = null

    val activeEngineType: EngineType?
        get() = (state.value as? EngineState.Running)?.engineType

    suspend fun startEngine(
        engineType: EngineType,
        configJson: String,
        tunFd: Int? = null
    ) = mutex.withLock {
        if (configJson.isBlank()) {
            _state.value = EngineState.Error("Engine configuration cannot be empty")
            return@withLock
        }

        _state.value = EngineState.Starting

        try {
            // Process safety: teardown any currently running engine
            stopActiveEngineInternal()

            val targetDriver = getDriverForType(engineType)
            targetDriver.start(configJson, tunFd)
            activeDriver = targetDriver

            _state.value = EngineState.Running(
                engineType = engineType,
                stats = targetDriver.getTrafficStats()
            )
        } catch (e: Throwable) {
            // Teardown must survive cancellation, otherwise a Disconnect during startup leaves
            // the spawned core alive and the state pinned at Starting.
            withContext(NonCancellable) { runCatching { stopActiveEngineInternal() } }
            if (e is CancellationException) {
                _state.value = EngineState.Idle
                throw e
            }
            _state.value = EngineState.Error(e.message ?: "Failed to start engine")
        }
    }

    suspend fun stopEngine() = mutex.withLock {
        try {
            stopActiveEngineInternal()
            _state.value = EngineState.Idle
        } catch (e: Exception) {
            _state.value = EngineState.Error(e.message ?: "Error stopping engine")
        }
    }

    suspend fun switchEngine(
        newEngineType: EngineType,
        configJson: String,
        tunFd: Int? = null
    ) {
        startEngine(newEngineType, configJson, tunFd)
    }

    fun updateTrafficStats(stats: TrafficStats) {
        // A stats tick races stopEngine on another thread; a read-then-write would put
        // Running back after the core was destroyed.
        _state.update { current ->
            if (current is EngineState.Running) current.copy(stats = stats) else current
        }
    }

    fun refreshTrafficStats() {
        if (_state.value !is EngineState.Running) return
        val stats = activeDriver?.getTrafficStats() ?: return
        _state.update { current ->
            if (current is EngineState.Running) current.copy(stats = stats) else current
        }
    }

    private suspend fun stopActiveEngineInternal() {
        singboxEngine.stop()
        activeDriver = null
    }

    private fun getDriverForType(engineType: EngineType): IEngineDriver {
        return when (engineType) {
            EngineType.SINGBOX -> singboxEngine
        }
    }
}
