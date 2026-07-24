package com.lumen.core.engine

import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class FakeEngineDriver(
    override val type: EngineType,
    private val statsToReturn: TrafficStats = TrafficStats()
) : IEngineDriver {
    private var _isRunning = false
    override val isRunning: Boolean get() = _isRunning

    var startCount = 0
    var stopCount = 0
    var lastConfig: String? = null
    var lastTunFd: Int? = null
    var shouldFailStart = false

    override suspend fun start(configJson: String, tunFd: Int?) {
        if (shouldFailStart) {
            throw IllegalStateException("Simulated engine start failure")
        }
        startCount++
        lastConfig = configJson
        lastTunFd = tunFd
        _isRunning = true
    }

    override suspend fun stop() {
        stopCount++
        _isRunning = false
    }

    override fun getTrafficStats(): TrafficStats = statsToReturn
}

class EngineManagerTest {

    private lateinit var fakeSingbox: FakeEngineDriver
    private lateinit var engineManager: EngineManager

    @Before
    fun setUp() {
        fakeSingbox = FakeEngineDriver(EngineType.SINGBOX, TrafficStats(100L, 200L, 1000L, 2000L))
        engineManager = EngineManager(fakeSingbox)
    }

    @Test
    fun testInitialStateIsIdle() {
        assertEquals(EngineState.Idle, engineManager.state.value)
        assertNull(engineManager.activeEngineType)
    }

    @Test
    fun testStartSingboxEngine() = runTest {
        val config = "{\"inbounds\":[]}"
        engineManager.startEngine(EngineType.SINGBOX, config, 10)

        val currentState = engineManager.state.value
        assertTrue(currentState is EngineState.Running)
        val runningState = currentState as EngineState.Running
        assertEquals(EngineType.SINGBOX, runningState.engineType)
        assertEquals(100L, runningState.stats.uploadSpeed)
        assertEquals(200L, runningState.stats.downloadSpeed)

        assertEquals(1, fakeSingbox.startCount)
        assertEquals(config, fakeSingbox.lastConfig)
        assertEquals(10, fakeSingbox.lastTunFd)
        assertEquals(EngineType.SINGBOX, engineManager.activeEngineType)
    }

    @Test
    fun testStopEngine() = runTest {
        engineManager.startEngine(EngineType.SINGBOX, "{}")
        assertTrue(engineManager.state.value is EngineState.Running)

        engineManager.stopEngine()
        assertEquals(EngineState.Idle, engineManager.state.value)
        assertTrue(fakeSingbox.stopCount >= 1)
        assertEquals(false, fakeSingbox.isRunning)
    }


    @Test
    fun testEmptyConfigTriggersErrorState() = runTest {
        engineManager.startEngine(EngineType.SINGBOX, "   ")

        val currentState = engineManager.state.value
        assertTrue(currentState is EngineState.Error)
        val errorState = currentState as EngineState.Error
        assertTrue(errorState.message.contains("empty", ignoreCase = true))
    }

    @Test
    fun testEngineDriverStartFailureTriggersErrorState() = runTest {
        fakeSingbox.shouldFailStart = true
        engineManager.startEngine(EngineType.SINGBOX, "{\"test\": 1}")

        val currentState = engineManager.state.value
        assertTrue(currentState is EngineState.Error)
        val errorState = currentState as EngineState.Error
        assertTrue(errorState.message.contains("Simulated engine start failure"))
    }

    @Test
    fun testUpdateTrafficStats() = runTest {
        engineManager.startEngine(EngineType.SINGBOX, "{}")
        val updatedStats = TrafficStats(999L, 888L, 7777L, 6666L)
        engineManager.updateTrafficStats(updatedStats)

        val currentState = engineManager.state.value
        assertTrue(currentState is EngineState.Running)
        val runningState = currentState as EngineState.Running
        assertEquals(999L, runningState.stats.uploadSpeed)
        assertEquals(888L, runningState.stats.downloadSpeed)
    }
}
