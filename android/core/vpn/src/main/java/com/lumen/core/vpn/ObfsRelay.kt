package com.lumen.core.vpn

import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.Collections
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread

/**
 * Loopback SOCKS5 front-end for the in-app obfs2/obfs3 transports.
 *
 * sing-box dials this relay through a `socks` detour outbound; every accepted
 * connection is forwarded to the configured obfs bridge, which unwraps the
 * transport and hands the traffic to the OpenVPN server behind it. The upstream
 * socket is created here, so `VpnService.protect()` keeps it outside the tunnel.
 */
class ObfsRelay(
    private val localPort: Int,
    private val type: String,
    private val bridgeHost: String,
    private val bridgePort: Int,
    private val protect: (Socket) -> Boolean
) {
    private companion object {
        const val TAG = "OBFS"
        const val BUFFER = 32 * 1024
        const val CONNECT_TIMEOUT_MS = 15_000
        // A bridge that accepts TCP but never answers would otherwise pin a thread
        // and two file descriptors for the lifetime of the process.
        const val HANDSHAKE_TIMEOUT_MS = 30_000
        const val IDLE_TIMEOUT_MS = 10 * 60 * 1000
        const val MAX_CONNECTIONS = 64
    }

    @Volatile
    private var running = false
    private var server: ServerSocket? = null
    // Every accepted connection uses two pooled threads: its handler (which also
    // runs the client -> bridge direction) and the reverse pipe.
    private var workers: ExecutorService? = null
    private val liveSockets: MutableSet<Socket> = Collections.synchronizedSet(mutableSetOf())
    private val active = AtomicInteger(0)

    val activeConnections: Int
        get() = active.get()

    fun start() {
        if (running) return
        val socket = ServerSocket()
        socket.reuseAddress = true
        socket.bind(InetSocketAddress("127.0.0.1", localPort))
        server = socket
        workers = Executors.newFixedThreadPool(MAX_CONNECTIONS * 2) { runnable ->
            Thread(runnable, "obfs-relay-worker").apply { isDaemon = true }
        }
        running = true
        VpnLogBus.info(TAG, "$type relay on 127.0.0.1:$localPort -> $bridgeHost:$bridgePort")
        thread(name = "obfs-relay", isDaemon = true) {
            while (running) {
                val client = try {
                    socket.accept()
                } catch (e: IOException) {
                    if (running) VpnLogBus.error(TAG, "relay stopped accepting connections", e)
                    break
                }
                val pool = workers
                if (pool == null || active.get() >= MAX_CONNECTIONS) {
                    VpnLogBus.warning(TAG, "relay connection limit reached; dropping a connection")
                    closeQuietly(client)
                    continue
                }
                active.incrementAndGet()
                liveSockets.add(client)
                runCatching { pool.execute { handle(client) } }
                    .onFailure { release(client, null) }
            }
        }
    }

    fun stop() {
        running = false
        try { server?.close() } catch (_: IOException) {}
        server = null
        // Closing only the listening socket left every in-flight connection
        // relaying to a bridge the core has already abandoned.
        synchronized(liveSockets) {
            liveSockets.forEach { closeQuietly(it) }
            liveSockets.clear()
        }
        active.set(0)
        workers?.shutdownNow()
        workers = null
    }

    /**
     * Minimal SOCKS5 CONNECT server. The requested address is read and
     * discarded: the bridge itself decides where the unwrapped traffic goes.
     */
    private fun acceptSocks(input: InputStream, output: OutputStream): Boolean {
        val greeting = readExactly(input, 2)
        if (greeting[0].toInt() != 0x05) return false
        val methods = greeting[1].toInt() and 0xFF
        if (methods > 0) readExactly(input, methods)
        output.write(byteArrayOf(0x05, 0x00))
        output.flush()

        val head = readExactly(input, 4)
        if (head[0].toInt() != 0x05 || head[1].toInt() != 0x01) {
            output.write(byteArrayOf(0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0))
            output.flush()
            return false
        }
        when (val atyp = head[3].toInt() and 0xFF) {
            0x01 -> readExactly(input, 4)
            0x03 -> readExactly(input, readExactly(input, 1)[0].toInt() and 0xFF)
            0x04 -> readExactly(input, 16)
            else -> throw IOException("unsupported SOCKS5 address type $atyp")
        }
        readExactly(input, 2)
        output.write(byteArrayOf(0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0))
        output.flush()
        return true
    }

    private fun handle(client: Socket) {
        var bridge: Socket? = null
        try {
            client.tcpNoDelay = true
            client.soTimeout = HANDSHAKE_TIMEOUT_MS
            val clientIn = client.getInputStream()
            val clientOut = client.getOutputStream()
            if (!acceptSocks(clientIn, clientOut)) {
                release(client, null)
                return
            }
            val upstream = Socket()
            bridge = upstream
            liveSockets.add(upstream)
            upstream.tcpNoDelay = true
            upstream.soTimeout = HANDSHAKE_TIMEOUT_MS
            // Protect before connecting: the bridge must stay outside the tunnel.
            if (!protect(upstream)) VpnLogBus.warning(TAG, "relay socket was not protected")
            upstream.connect(InetSocketAddress(bridgeHost, bridgePort), CONNECT_TIMEOUT_MS)
            val bridgeIn = upstream.getInputStream()
            val bridgeOut = upstream.getOutputStream()
            val stream = when (type) {
                "obfs3" -> Obfs3.handshake(bridgeIn, bridgeOut)
                "obfs2-legacy" -> Obfs2.handshake(bridgeIn, bridgeOut, legacy = true)
                else -> Obfs2.handshake(bridgeIn, bridgeOut, legacy = false)
            }
            client.soTimeout = IDLE_TIMEOUT_MS
            upstream.soTimeout = IDLE_TIMEOUT_MS
            val pool = workers ?: throw IOException("relay was stopped")
            pool.execute { pipe(bridgeIn, clientOut, stream.decrypt, client, upstream) }
            pipe(clientIn, bridgeOut, stream.encrypt, client, upstream)
        } catch (e: Exception) {
            VpnLogBus.error(TAG, "$type connection to $bridgeHost:$bridgePort failed", e)
            release(client, bridge)
        }
    }

    private fun pipe(
        source: InputStream,
        sink: OutputStream,
        transform: (ByteArray, Int) -> ByteArray,
        client: Socket,
        upstream: Socket
    ) {
        val buffer = ByteArray(BUFFER)
        try {
            while (true) {
                val read = source.read(buffer)
                if (read < 0) break
                sink.write(transform(buffer, read))
                sink.flush()
            }
        } catch (_: IOException) {
            // Either side closing, or an idle timeout, simply ends this connection.
        } finally {
            release(client, upstream)
        }
    }

    /** Idempotent: both pipe directions end up here, only the first one counts. */
    private fun release(client: Socket, upstream: Socket?) {
        if (liveSockets.remove(client)) active.decrementAndGet()
        closeQuietly(client)
        upstream?.let {
            liveSockets.remove(it)
            closeQuietly(it)
        }
    }

    private fun readExactly(input: InputStream, count: Int): ByteArray {
        val buffer = ByteArray(count)
        var read = 0
        while (read < count) {
            val next = input.read(buffer, read, count - read)
            if (next < 0) throw IOException("SOCKS5 client closed the connection")
            read += next
        }
        return buffer
    }

    private fun closeQuietly(socket: Socket?) {
        try { socket?.close() } catch (_: IOException) {}
    }
}
