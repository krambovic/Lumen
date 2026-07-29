package com.lumen.app.vm

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MainViewModelPingTest {
    @Test
    fun httpGetAcceptsOnlySuccessfulHttpStatuses() {
        assertTrue(isSuccessfulHttpPingStatusLine("HTTP/1.1 204 No Content"))
        assertTrue(isSuccessfulHttpPingStatusLine("HTTP/1.0 301 Moved Permanently"))
        assertTrue(isSuccessfulHttpPingStatusLine("HTTP/2 399 Edge Response"))

        assertFalse(isSuccessfulHttpPingStatusLine("HTTP/1.1 404 Not Found"))
        assertFalse(isSuccessfulHttpPingStatusLine("HTTP/1.1 503 Service Unavailable"))
        assertFalse(isSuccessfulHttpPingStatusLine("connected"))
        assertFalse(isSuccessfulHttpPingStatusLine(null))
    }

    @Test
    fun coreBackedAttemptsBudgetStartupAndRequestSeparately() {
        assertEquals(
            34_000L,
            PingBudget.attemptsMs(
                timeoutMs = 5_000,
                attempts = 3,
                retryDelayMs = 250,
                realCheck = true
            )
        )
        assertEquals(
            17_500L,
            PingBudget.attemptsMs(
                timeoutMs = 5_000,
                attempts = 3,
                retryDelayMs = 250,
                realCheck = false
            )
        )
    }
}
