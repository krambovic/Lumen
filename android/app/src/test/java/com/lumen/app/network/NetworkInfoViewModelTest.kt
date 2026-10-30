package com.lumen.app.network

import com.lumen.core.vpn.ActiveProxySession
import com.lumen.ui.screens.NetworkInfoSnapshot
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.*
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.Assert.*
import java.io.IOException

@OptIn(ExperimentalCoroutinesApi::class)
class NetworkInfoViewModelTest {
    private val dispatcher = StandardTestDispatcher()
    private val direct = NetworkInfoRequest(false, false, null, null, "wifi")
    private val vpn = NetworkInfoRequest(true, false, ActiveProxySession(12000, null, null, 1), "server", "wifi")
    private val sample = NetworkInfoSnapshot("203.0.113.1", source = "sample", sourceUrl = "https://example.org")
    @Before fun prepare() { Dispatchers.setMain(dispatcher) }
    @After fun cleanup() { Dispatchers.resetMain() }

    @Test fun coldLoadShowsLoadingAndSuppressesOverlappingRequests() = runTest(dispatcher) {
        val gate = CompletableDeferred<NetworkInfoSnapshot>()
        var calls = 0
        val model = NetworkInfoViewModel({ _, _ -> calls++; gate.await() }, dispatcher)
        model.load(direct)
        runCurrent()
        assertTrue(model.state.value.loading)
        assertNull(model.state.value.data)
        model.load(direct, force = true)
        runCurrent()
        assertEquals(1, calls)
        gate.complete(sample)
        advanceUntilIdle()
        assertEquals(sample, model.state.value.data)
        assertFalse(model.state.value.loading)
    }

    @Test fun failedRefreshKeepsDataAndRetryReplacesIt() = runTest(dispatcher) {
        var fail = false
        val model = NetworkInfoViewModel({ _, _ -> if (fail) throw IOException() else sample }, dispatcher)
        model.load(direct); advanceUntilIdle()
        fail = true
        model.load(direct, force = true); advanceUntilIdle()
        assertEquals(sample, model.state.value.data)
        assertTrue(model.state.value.failed)
        fail = false
        model.load(direct, force = true); advanceUntilIdle()
        assertFalse(model.state.value.failed)
    }

    @Test fun switchingRouteClearsOldIpAndNeverFallsBackToDirect() = runTest(dispatcher) {
        val sessions = mutableListOf<ActiveProxySession?>()
        val model = NetworkInfoViewModel({ session, _ ->
            sessions += session
            if (session != null) throw IOException()
            sample
        }, dispatcher)
        model.load(direct); advanceUntilIdle()
        model.load(vpn)
        assertNull(model.state.value.data)
        advanceUntilIdle()
        assertTrue(model.state.value.throughVpn)
        assertTrue(model.state.value.failed)
        assertEquals(listOf(null, vpn.session), sessions)
    }

    @Test fun connectingDoesNotSendALookupOnThePhysicalNetwork() = runTest(dispatcher) {
        val model = NetworkInfoViewModel({ _, _ -> error("must wait") }, dispatcher)
        model.load(direct.copy(waiting = true)); advanceUntilIdle()
        assertTrue(model.state.value.waiting)
        assertTrue(model.state.value.loading)
    }

    @Test fun cancelledOldRouteCannotOverwriteTheNewRoute() = runTest(dispatcher) {
        val gate = CompletableDeferred<NetworkInfoSnapshot>()
        val model = NetworkInfoViewModel({ session, _ -> if (session == null) gate.await() else sample }, dispatcher)
        model.load(direct); runCurrent()
        model.load(vpn); advanceUntilIdle()
        gate.complete(sample.copy(ip = "198.51.100.1")); advanceUntilIdle()
        assertEquals(sample.ip, model.state.value.data?.ip)
        assertTrue(model.state.value.throughVpn)
    }

    @Test fun restartingAfterProcessDeathFetchesCurrentRoute() = runTest(dispatcher) {
        var count = 0
        val model = NetworkInfoViewModel({ _, _ -> count++; sample }, dispatcher)
        model.load(vpn); advanceUntilIdle()
        model.load(vpn); advanceUntilIdle()
        assertEquals(1, count)
        NetworkInfoViewModel({ _, _ -> count++; sample }, dispatcher).load(vpn)
        advanceUntilIdle()
        assertEquals(2, count)
    }
}
