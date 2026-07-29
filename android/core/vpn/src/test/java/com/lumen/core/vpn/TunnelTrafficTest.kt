package com.lumen.core.vpn

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class TunnelTrafficTest {
    @Test
    fun `hev counters map tun tx to upload and tun rx to download`() {
        val counters = countersFromHev(longArrayOf(7L, 1_200L, 9L, 3_400L))!!

        assertEquals(TrafficCounterSource.TUNNEL, counters.source)
        assertEquals(1_200L, counters.uploaded)
        assertEquals(3_400L, counters.downloaded)
        assertNull(countersFromHev(longArrayOf(1L, 2L, 3L)))
    }

    @Test
    fun `rate uses actual elapsed time rather than assuming one second`() {
        val previous = TrafficCounters(TrafficCounterSource.TUNNEL, 100L, 200L)
        val current = TrafficCounters(TrafficCounterSource.TUNNEL, 1_100L, 2_200L)

        val rate = trafficRate(previous, current, 2_000_000_000L)

        assertEquals(1_000L, rate.uploadedDelta)
        assertEquals(2_000L, rate.downloadedDelta)
        assertEquals(500L, rate.uploadBytesPerSecond)
        assertEquals(1_000L, rate.downloadBytesPerSecond)
    }

    @Test
    fun `counter source changes reset the rate baseline`() {
        val previous = TrafficCounters(TrafficCounterSource.APP_UID, 100L, 200L)
        val current = TrafficCounters(TrafficCounterSource.TUNNEL, 10_000L, 20_000L)

        assertEquals(TrafficRate(0L, 0L, 0L, 0L), trafficRate(previous, current, 1_000_000_000L))
    }
}
