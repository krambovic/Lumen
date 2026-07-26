package com.lumen.core.vpn

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.InputStream
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.SocketException
import java.util.Collections
import kotlin.concurrent.thread

class ObfsRelayTest {

    private fun freePort(): Int = ServerSocket(0, 1, InetAddress.getByName("127.0.0.1")).use {
        it.localPort
    }

    private fun readExactly(input: InputStream, count: Int): ByteArray {
        val buffer = ByteArray(count)
        var read = 0
        while (read < count) {
            val next = input.read(buffer, read, count - read)
            if (next < 0) throw SocketException("closed after $read bytes")
            read += next
        }
        return buffer
    }

    /** Completes the loopback SOCKS5 CONNECT the relay serves before it dials the bridge. */
    private fun socksConnect(port: Int): Socket {
        val client = Socket()
        client.connect(InetSocketAddress("127.0.0.1", port), 5_000)
        client.soTimeout = 5_000
        val output = client.getOutputStream()
        val input = client.getInputStream()
        output.write(byteArrayOf(0x05, 0x01, 0x00))
        output.flush()
        assertEquals(0x05, readExactly(input, 2)[0].toInt())
        output.write(byteArrayOf(0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1, 0x04, 0x38))
        output.flush()
        assertEquals(0x05, readExactly(input, 10)[0].toInt())
        return client
    }

    private fun waitFor(timeoutMs: Long = 5_000, condition: () -> Boolean) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline && !condition()) Thread.sleep(20)
    }

    /**
     * A bridge that accepts TCP but never answers used to pin the handler thread and
     * both file descriptors forever: stop() only closed the listening socket.
     */
    @Test
    fun `stop closes connections parked in the bridge handshake`() {
        val bridge = ServerSocket(0, 50, InetAddress.getByName("127.0.0.1"))
        val bridgeSockets = Collections.synchronizedList(mutableListOf<Socket>())
        thread(isDaemon = true) {
            runCatching { while (true) bridgeSockets.add(bridge.accept()) }
        }
        val localPort = freePort()
        val relay = ObfsRelay(localPort, "obfs3", "127.0.0.1", bridge.localPort) { true }
        relay.start()

        val client = socksConnect(localPort)
        waitFor { relay.activeConnections == 1 && bridgeSockets.isNotEmpty() }
        assertEquals(1, relay.activeConnections)

        relay.stop()

        assertEquals(0, relay.activeConnections)
        val ended = runCatching { client.getInputStream().read() }
        assertTrue(
            "the relay left an in-flight client socket open",
            ended.getOrNull() == -1 || ended.exceptionOrNull() is SocketException
        )
        client.close()
        bridge.close()
    }

    @Test
    fun `stop releases the listening port so a reconnect can rebind it`() {
        val localPort = freePort()
        val relay = ObfsRelay(localPort, "obfs2", "127.0.0.1", freePort()) { true }
        relay.start()
        relay.stop()

        val restarted = ObfsRelay(localPort, "obfs2", "127.0.0.1", freePort()) { true }
        restarted.start()
        restarted.stop()
    }
}
