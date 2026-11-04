package com.lumen.app

import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import com.lumen.app.vm.MainViewModel
import com.lumen.ui.screens.NodeDraft
import com.lumen.ui.screens.NodeUiModel
import java.net.ServerSocket
import java.net.Socket
import java.util.UUID
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test

/** Real temporary cores, local SOCKS peer: no provider or Internet variability. */
class PingLifecycleDeviceTest {
    @Test fun sixteenHttpWorkersCancelAndRestartWithoutStuckRows() = runBlocking {
        ActivityScenario.launch(MotionTestActivity::class.java).use { scenario ->
            lateinit var vm: MainViewModel
            scenario.onActivity { vm = ViewModelProvider(it)[MainViewModel::class.java] }
            val original = vm.settings.value
            val originalFilter = vm.selectedServerFilter.value
            val prefix = "lumen-probe-test-${UUID.randomUUID()}-"
            val server = ServerSocket(0)
            val executor = Executors.newCachedThreadPool { Thread(it, "lumen-test-peer").apply { isDaemon = true } }
            val sockets = ConcurrentLinkedQueue<Socket>()
            val allStarted = CountDownLatch(16)
            val release = CountDownLatch(1)
            executor.submit {
                while (!server.isClosed) {
                    val socket = try { server.accept() } catch (_: Exception) { break }
                    sockets.add(socket)
                    executor.submit {
                        try { socket.use {
                            socket.soTimeout = 60_000
                            val input = socket.getInputStream(); val output = socket.getOutputStream()
                            fun read(count: Int) { repeat(count) { check(input.read() >= 0) } }
                            check(input.read() == 5)
                            val methods = input.read(); read(methods)
                            output.write(byteArrayOf(5, 0)); output.flush()
                            read(3)
                            when (input.read()) { 1 -> read(4); 3 -> read(input.read()); 4 -> read(16); else -> error("address") }
                            read(2)
                            output.write(byteArrayOf(5, 0, 0, 1, 127, 0, 0, 1, 0, 0)); output.flush()
                            // Wait for the HTTP request, proving this core is doing a live probe.
                            check(input.read() >= 0)
                            allStarted.countDown()
                            release.await(60, TimeUnit.SECONDS)
                            output.write("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".toByteArray())
                            output.flush()
                        } } catch (_: Exception) { }
                    }
                }
            }
            try {
                scenario.onActivity { vm.updateSettings(original.copy(pingType = "http", pingConcurrency = 16,
                    pingTimeoutMs = 5_000, pingAttempts = 1, pingAutoDeleteUnreachable = false,
                    pingUrl = "http://198.18.0.1/check")) }
                repeat(16) { index ->
                    val saved = CompletableDeferred<String?>()
                    scenario.onActivity { vm.saveDraft(NodeDraft(name = "$prefix$index", protocol = "socks",
                        server = "127.0.0.1", port = server.localPort.toString())) { saved.complete(it) } }
                    assertNull(withTimeout(10_000) { saved.await() })
                }
                val targets = withTimeout(10_000) {
                    while (vm.nodes.value.count { it.name.startsWith(prefix) } < 16) delay(50)
                    vm.nodes.value.filter { it.name.startsWith(prefix) }
                }
                scenario.onActivity { vm.pingNodes(targets) }
                assertTrue("All 16 HTTP cores must issue requests simultaneously", allStarted.await(30, TimeUnit.SECONDS))
                assertEquals(0 to 16, vm.bulkPingProgress.value)
                scenario.onActivity { vm.stopPing() }
                withTimeout(3_000) { while (vm.isPinging.value || vm.pingingNodeIds.value.isNotEmpty()) delay(20) }
                assertNull(vm.bulkPingProgress.value)
                release.countDown()
                scenario.onActivity { vm.pingNodes(targets) }
                withTimeout(45_000) { while (vm.isPinging.value) delay(100) }
                withTimeout(5_000) {
                    while (vm.nodes.value.filter { it.name.startsWith(prefix) }.any { it.pingMs == null || it.pingMs!! <= 0 }) delay(50)
                }
                assertTrue(vm.pingingNodeIds.value.isEmpty())
                assertEquals(16 to 16, vm.bulkPingProgress.value)
                delay(650)
                assertNull(vm.bulkPingProgress.value)
                // A connection transition must also cancel an individual check.
                scenario.onActivity { vm.pingNode(targets.first()); vm.markConnecting() }
                withTimeout(3_000) { while (vm.pingingNodeIds.value.isNotEmpty()) delay(20) }
            } finally {
                release.countDown()
                scenario.onActivity {
                    vm.stopPing()
                    vm.nodes.value.filter { it.name.startsWith(prefix) }.forEach(vm::deleteNode)
                    vm.updateSettings(original)
                    vm.selectServerFilter(originalFilter)
                }
                withTimeout(10_000) { while (vm.nodes.value.any { it.name.startsWith(prefix) }) delay(50) }
                server.close()
                sockets.forEach { runCatching { it.close() } }
                executor.shutdownNow()
            }
        }
    }
}
