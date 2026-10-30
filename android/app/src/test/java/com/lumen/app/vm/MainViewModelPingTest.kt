package com.lumen.app.vm

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MainViewModelPingTest {
    @Test fun iranCheckBudgetsColdDnsAndTlsAndHasIndependentDestinations() {
        assertEquals(10_000, PingBudget.setupMs(2_000, iranNetwork = true))
        assertTrue(PingBudget.attemptsMs(5_000, 2, 200, true, iranNetwork = true) >= 160_000L)
        val urls = httpPingTargets("", iranNetwork = true)
        assertEquals(3, urls.distinct().size)
        assertEquals("https://www.google.com/generate_204", urls.first())
        assertEquals(listOf("https://example.org/check", "https://example.org/check"),
            httpPingTargets("https://example.org/check", iranNetwork = true))
    }
    @Test
    fun httpGetAcceptsAnyValidHttpResponse() {
        assertTrue(isSuccessfulHttpPingStatusLine("HTTP/1.1 204 No Content"))
        assertTrue(isSuccessfulHttpPingStatusLine("HTTP/1.0 301 Moved Permanently"))
        assertTrue(isSuccessfulHttpPingStatusLine("HTTP/2 399 Edge Response"))
        assertTrue(isSuccessfulHttpPingStatusLine("HTTP/1.1 404 Not Found"))
        assertTrue(isSuccessfulHttpPingStatusLine("HTTP/1.1 429 Too Many Requests"))
        assertTrue(isSuccessfulHttpPingStatusLine("HTTP/1.1 503 Service Unavailable"))

        assertFalse(isSuccessfulHttpPingStatusLine("HTTP/1.1 099 Invalid"))
        assertFalse(isSuccessfulHttpPingStatusLine("HTTP/1.1 600 Invalid"))
        assertFalse(isSuccessfulHttpPingStatusLine("connected"))
        assertFalse(isSuccessfulHttpPingStatusLine(null))
    }

    @Test
    fun coreBackedAttemptsBudgetStartupAndRequestSeparately() {
        assertEquals(
            145_500L,
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

    @Test
    fun defaultTargetHasIndependentFallbackAndColdStartRetry() {
        val targets = httpPingTargets("")
        assertEquals(3, targets.size)
        assertEquals(targets.first(), targets.last())
        assertFalse(targets[0] == targets[1])
        assertEquals(listOf("https://example.org/check", "https://example.org/check"),
            httpPingTargets(" https://example.org/check "))
    }

    @Test
    fun shortResponseTimeoutDoesNotTruncateColdProxySetup() {
        assertEquals(5_000, PingBudget.setupMs(2_000))
        assertEquals(15_000, PingBudget.setupMs(15_000))
        assertTrue(PingBudget.attemptsMs(2_000, 1, 0, true) >= 3L * (5_000 + 5_000 + 2_000))
    }

    @Test
    fun autoPoolBudgetUsesActualCoreConcurrency() {
        assertEquals(120_000L, PingBudget.nodeMs(30_000L, 8, PingBudget.CORE_PING_CONCURRENCY))
        assertEquals(30_000L, PingBudget.nodeMs(30_000L, 8, PingBudget.AUTO_MEMBER_CONCURRENCY))
    }
}
