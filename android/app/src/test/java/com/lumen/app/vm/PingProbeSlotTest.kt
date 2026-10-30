package com.lumen.app.vm

import kotlinx.coroutines.*
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.test.*
import org.junit.Assert.*
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class PingProbeSlotTest {
    @Test fun queuedServerKeepsItsFullNetworkDeadline() = runTest {
        val pool = Semaphore(1, acquiredPermits = 1)
        val result = async { withPingProbeSlot(pool, 100) { delay(80); 42 } }
        advanceTimeBy(500)
        assertFalse(result.isCompleted)
        pool.release()
        assertEquals(42, result.await())
        assertEquals(1, pool.availablePermits)
    }
    @Test fun timeoutAndCancellationReleaseTheirSlots() = runTest {
        val pool = Semaphore(1)
        assertNull(withPingProbeSlot(pool, 100) { delay(200); 42 })
        val job = launch { withPingProbeSlot(pool, 1000) { awaitCancellation() } }
        runCurrent(); job.cancelAndJoin()
        assertEquals(1, pool.availablePermits)
        assertEquals(7, withPingProbeSlot(pool, 100) { 7 })
    }
}
