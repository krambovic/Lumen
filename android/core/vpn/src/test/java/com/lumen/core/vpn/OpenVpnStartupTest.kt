package com.lumen.core.vpn

import com.lumen.core.engine.EngineType
import com.lumen.core.engine.IEngineDriver
import com.lumen.core.engine.TrafficStats
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class OpenVpnStartupTest {
    private class Driver : IEngineDriver {
        override val type = EngineType.SINGBOX
        override var isRunning = true
        override var endpointFailure = ""
        override var lastFatalLine = ""
        override var readyOpenVpnTags = emptySet<String>()
        override suspend fun start(configJson: String, tunFd: Int?) = Unit
        override suspend fun stop() = Unit
        override fun getTrafficStats() = TrafficStats()
    }

    @Test fun stoppedCoreKeepsItsActualReason() = runTest {
        val driver = Driver().apply { isRunning = false; lastFatalLine = "FATAL: core stopped unexpectedly" }
        val error = runCatching { awaitOpenVpnDataChannel(driver, "proxy") }.exceptionOrNull()
        assertEquals("OpenVPN core stopped during startup: FATAL: core stopped unexpectedly", error?.message)
    }

    @Test fun failureFromUnusedEndpointCannotBecomeSelectedEndpointTimeout() = runTest {
        val driver = Driver().apply { endpointFailure = "ERROR endpoint/openvpn-client[unused]: client terminated: AUTH_FAILED" }
        val error = runCatching { awaitOpenVpnDataChannel(driver, "proxy", 1_000L) }.exceptionOrNull()
        assertEquals("OpenVPN handshake did not complete within 1000 ms", error?.message)
    }

    @Test fun selectedEndpointRefusalIsPreserved() = runTest {
        val driver = Driver().apply { endpointFailure = "ERROR endpoint/openvpn-client[proxy]: client terminated: AUTH_FAILED" }
        val error = runCatching { awaitOpenVpnDataChannel(driver, "proxy") }.exceptionOrNull()
        assertEquals(driver.endpointFailure, error?.message)
        assertEquals(0L, testScheduler.currentTime)
    }

    @Test fun parentDeadlineRemainsCancellation() = runTest {
        val error = runCatching { withTimeout(50L) { awaitOpenVpnDataChannel(Driver(), "proxy", 1_000L) } }.exceptionOrNull()
        assertTrue(error is TimeoutCancellationException)
    }

    @Test fun authenticatedSelectedEndpointIsReadyWithoutNetworkProbe() = runTest {
        awaitOpenVpnDataChannel(Driver().apply { readyOpenVpnTags = setOf("proxy") }, "proxy")
        assertEquals(0L, testScheduler.currentTime)
    }
}
