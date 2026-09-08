package com.lumen.app.subscription

import com.lumen.core.config.parser.LinkParser
import java.net.Authenticator
import java.net.ServerSocket
import java.net.Socket
import java.net.URL
import java.util.concurrent.CancellationException
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class SubscriptionTransportRegressionTest {
    private val uri = "vless://11111111-1111-1111-1111-111111111111@one.example:443?security=tls#one"

    @Test fun normalizationKeepsObjectNodesAndSiblingContainers() {
        val body = JSONObject().put("links", JSONArray().put(JSONObject().put("url", uri)))
            .put("nodes", JSONArray().put(uri)).put("profileTitle", "Example").toString()
        val payload = SubscriptionClient.normalize("\uFEFF \n" + body, emptyMap())
        val (nodes, errors) = LinkParser.parseLinksText(payload.body)
        assertEquals(2, nodes.size)
        assertTrue(errors.toString(), errors.isEmpty())
        assertEquals("Example", payload.profileTitle)
        assertTrue(JSONObject(payload.body).has("nodes"))
    }
    @Test fun cancellationNeverFallsThroughToAnotherUserAgent() {
        assertThrows(CancellationException::class.java) {
            SubscriptionClient.fetch("https://example.invalid/sub", null, cancelled = { true })
        }
    }
    @Test fun hwidOriginPolicyIncludesSchemeAndPort() {
        assertTrue(SubscriptionClient.sameOrigin(URL("https://one.example/a"), URL("https://one.example:443/b")))
        assertFalse(SubscriptionClient.sameOrigin(URL("https://one.example/a"), URL("https://two.example/b")))
        assertFalse(SubscriptionClient.sameOrigin(URL("https://one.example/a"), URL("http://one.example/a")))
        assertFalse(SubscriptionClient.sameOrigin(URL("https://one.example/a"), URL("https://one.example:8443/b")))
    }
    @Test(timeout = 8000) fun authenticatedSocksUsesExactUtf8CredentialsWithoutGlobalAuthenticator() {
        val before = Authenticator::class.java.getMethod("getDefault").invoke(null)
        val username = " user "
        val password = " пароль 🚀 "
        val pool = Executors.newSingleThreadExecutor()
        ServerSocket(0, 1, java.net.InetAddress.getByName("127.0.0.1")).use { server ->
            val received = pool.submit<Pair<String, String>> {
                server.accept().use { socket ->
                    socket.soTimeout = 3000
                    val input = socket.getInputStream()
                    val output = socket.getOutputStream()
                    fun bytes(n: Int) = input.readNBytes(n).also { assertEquals(n, it.size) }
                    assertArrayEquals(byteArrayOf(5, 1, 2), bytes(3))
                    output.write(byteArrayOf(5, 2)); output.flush()
                    assertEquals(1, input.read())
                    val actualUser = bytes(input.read()).toString(Charsets.UTF_8)
                    val actualPassword = bytes(input.read()).toString(Charsets.UTF_8)
                    output.write(byteArrayOf(1, 0)); output.flush()
                    assertArrayEquals(byteArrayOf(5, 1, 0, 3), bytes(4))
                    val hostname = bytes(input.read()).toString(Charsets.US_ASCII)
                    assertEquals("origin.example", hostname)
                    bytes(2)
                    output.write(byteArrayOf(5, 0, 0, 1, 127, 0, 0, 1, 0, 80)); output.flush()
                    val reader = input.bufferedReader()
                    val request = mutableListOf<String>()
                    while (true) { val line = reader.readLine() ?: break; if (line.isEmpty()) break; request += line }
                    assertTrue(request.first().startsWith("GET /sub HTTP/1.1"))
                    assertTrue(request.none { it.startsWith("Proxy-Authorization", true) || it.contains(password) })
                    val content = uri.toByteArray(Charsets.UTF_8)
                    output.write(("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n" + content.size.toString(16) + "\r\n").toByteArray())
                    output.write(content); output.write("\r\n0\r\n\r\n".toByteArray()); output.flush()
                    actualUser to actualPassword
                }
            }
            try {
                val response = SubscriptionHttp.get(URL("http://origin.example/sub"), emptyMap(), server.localPort, username, password) { false }
                assertEquals(200, response.code)
                assertEquals(uri, response.body.toString(Charsets.UTF_8))
                assertEquals(username to password, received.get(3, TimeUnit.SECONDS))
                assertSame(before, Authenticator::class.java.getMethod("getDefault").invoke(null))
            } finally { pool.shutdownNow() }
        }
    }
}
