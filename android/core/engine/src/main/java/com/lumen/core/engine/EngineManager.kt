package com.lumen.core.engine

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

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
    suspend fun start(configJson: String, tunFd: Int? = null)
    suspend fun stop()
    fun getTrafficStats(): TrafficStats
}

/**
 * Engine manager managing lifecycle, state transitions, and process safety
 * for the sing-box extended driver.
 */
class EngineManager(
    val singboxEngine: IEngineDriver = SingboxEngine()
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
        } catch (e: Exception) {
            stopActiveEngineInternal()
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
        val currentState = _state.value
        if (currentState is EngineState.Running) {
            _state.value = currentState.copy(stats = stats)
        }
    }

    fun refreshTrafficStats() {
        val currentState = _state.value
        if (currentState is EngineState.Running) {
            activeDriver?.let { driver ->
                val stats = driver.getTrafficStats()
                _state.value = currentState.copy(stats = stats)
            }
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
