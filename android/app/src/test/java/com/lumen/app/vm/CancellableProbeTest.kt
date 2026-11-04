package com.lumen.app.vm

import java.net.ServerSocket
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.*
import kotlinx.coroutines.sync.Semaphore
import org.junit.Assert.*
import org.junit.Test

class CancellableProbeTest {
    @Test fun cancellationClosesBlockedSocketAndReleasesSlot() = runBlocking {
        ServerSocket(0).use { server ->
            val pool = Semaphore(1)
            val job = launch(Dispatchers.Default) {
                withPingProbeSlot(pool, 60_000) {
                    ProbeIo.socket { socket ->
                        socket.connect(java.net.InetSocketAddress("127.0.0.1", server.localPort))
                        socket.getInputStream().read()
                    }
                }
            }
            val peer = withContext(Dispatchers.IO) { server.accept() }
            peer.use {
                withTimeout(2_000) { job.cancelAndJoin() }
                assertEquals(1, pool.availablePermits)
                peer.soTimeout = 2_000
                assertEquals(-1, peer.getInputStream().read())
            }
        }
    }

    @Test fun sixteenProbesActuallyRunAtOnce() = runBlocking {
        val started = CountDownLatch(16)
        val release = CountDownLatch(1)
        val pool = Semaphore(16)
        try {
            val jobs = (1..16).map {
                async(Dispatchers.Default) {
                    withPingProbeSlot(pool, 10_000) {
                        ProbeIo.run { started.countDown(); release.await(); 1 }
                    }
                }
            }
            assertTrue("All 16 workers must start before any finishes", started.await(5, TimeUnit.SECONDS))
            release.countDown()
            assertEquals(List(16) { 1 }, jobs.awaitAll())
        } finally {
            release.countDown()
        }
    }
}
