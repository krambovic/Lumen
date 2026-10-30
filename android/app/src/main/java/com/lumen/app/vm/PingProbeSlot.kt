package com.lumen.app.vm

import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import kotlinx.coroutines.withTimeoutOrNull

/** Queue time is not network latency and must never consume a server's probe deadline. */
internal suspend fun <T> withPingProbeSlot(pool: Semaphore, timeoutMs: Long, probe: suspend () -> T): T? =
    pool.withPermit { withTimeoutOrNull(timeoutMs) { probe() } }
