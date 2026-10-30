package com.lumen.app.vm

import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class UiFlowPolicyTest {
    @Test fun burstCoalescingRetainsFinalSnapshotIncludingEmptyResults() = runBlocking {
        val received = flow {
            repeat(1_000) { emit(setOf(it)) }
            emit(emptySet())
        }.coalesceUiUpdates(1).toList()
        assertTrue(received.isNotEmpty())
        assertEquals(emptySet<Int>(), received.last())
        assertTrue(received.size < 1_001)
    }

    @Test fun finiteStreamRetainsItsTrailingValue() = runBlocking {
        val received = flow { repeat(1_000) { emit(it) } }.coalesceUiUpdates(1).toList()
        assertEquals(999, received.last())
    }
}
