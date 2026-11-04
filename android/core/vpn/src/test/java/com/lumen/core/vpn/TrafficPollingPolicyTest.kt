package com.lumen.core.vpn

import org.junit.Assert.assertEquals
import org.junit.Test

class TrafficPollingPolicyTest {
    @Test fun foregroundKeepsLiveRatesAndBackgroundReducesWakeups() {
        assertEquals(1_000L, trafficPollingIntervalMs(true, true, true))
        assertEquals(5_000L, trafficPollingIntervalMs(false, true, true))
        assertEquals(10_000L, trafficPollingIntervalMs(false, true, false))
        assertEquals(10_000L, trafficPollingIntervalMs(true, false, true))
    }
}
