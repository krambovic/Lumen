package com.lumen.app.subscription

import java.io.*
import java.net.*
import java.util.Locale
import java.util.concurrent.CancellationException
import java.util.concurrent.TimeUnit
import java.util.zip.GZIPInputStream
import javax.net.ssl.SSLSocket
import javax.net.ssl.SSLSocketFactory

internal data class SubscriptionHttpResponse(val code: Int, val headers: Map<String, String>, val body: ByteArray)

/** Request-local SOCKS authentication: never installs a global java.net.Authenticator. */
internal object SubscriptionHttp {
    private const val MAX_BYTES = 8 * 1024 * 1024

    fun checkCancelled(cancelled: () -> Boolean) {
        if (cancelled() || Thread.currentThread().isInterrupted) throw CancellationException("Subscription refresh cancelled")
    }

    fun get(
        url: URL, headers: Map<String, String>, proxyPort: Int?, username: String?, password: String?,
        cancelled: () -> Boolean
    ): SubscriptionHttpResponse {
        checkCancelled(cancelled)
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(45)
        fun check() {
            checkCancelled(cancelled)
            if (System.nanoTime() >= deadline) throw SocketTimeoutException("Subscription request deadline exceeded")
        }
        require(url.protocol in setOf("http", "https")) { "Unsupported subscription URL scheme" }
        require(url.userInfo == null) { "Userinfo in subscription URLs is unsupported" }
        require(headers.all { (k, v) -> k.none { it == '\r' || it == '\n' } && v.none { it == '\r' || it == '\n' } })
        val response = if (proxyPort == null) {
            val conn = url.openConnection() as HttpURLConnection
            conn.connectTimeout = 15_000
            conn.readTimeout = 20_000
            conn.instanceFollowRedirects = false
            headers.forEach { (k, v) -> conn.setRequestProperty(k, v) }
            try {
                val code = conn.responseCode
                check()
                val responseHeaders = conn.headerFields.filterKeys { it != null }
                    .mapKeys { it.key.lowercase(Locale.US) }.mapValues { it.value.joinToString(",") }
                val bytes = if (code in 200..299) conn.inputStream.use { readLimited(it, ::check) } else ByteArray(0)
                SubscriptionHttpResponse(code, responseHeaders, bytes)
            } finally { conn.disconnect() }
        } else {
            require(proxyPort in 1..65535) { "Invalid local SOCKS port" }
            require((username == null) == (password == null)) { "Incomplete SOCKS credentials" }
            requestViaSocks(url, headers, proxyPort, username, password, ::check)
        }
        check()
        val bytes = if (response.headers["content-encoding"]?.equals("gzip", true) == true && response.body.isNotEmpty()) {
            GZIPInputStream(ByteArrayInputStream(response.body)).use { readLimited(it, ::check) }
        } else response.body
        return response.copy(body = bytes)
    }

    private fun readLimited(input: InputStream, check: () -> Unit): ByteArray {
        val output = ByteArrayOutputStream()
        val buffer = ByteArray(8192)
        while (true) {
            check()
            val count = input.read(buffer)
            if (count < 0) break
            if (output.size() + count > MAX_BYTES) throw IOException("Subscription exceeds 8 MiB")
            output.write(buffer, 0, count)
        }
        return output.toByteArray()
    }

    private fun requestViaSocks(
        url: URL, headers: Map<String, String>, port: Int, username: String?, password: String?, check: () -> Unit
    ): SubscriptionHttpResponse {
        var socket: Socket = Socket(Proxy.NO_PROXY)
        try {
            socket.connect(InetSocketAddress("127.0.0.1", port), 15_000)
            socket.soTimeout = 500
            var input: InputStream = socket.getInputStream()
            var output: OutputStream = socket.getOutputStream()
            fun readSome(buffer: ByteArray, offset: Int, length: Int): Int {
                while (true) {
                    check()
                    try { return input.read(buffer, offset, length) }
                    catch (_: SocketTimeoutException) { check() }
                }
            }
            fun readExact(count: Int): ByteArray {
                val data = ByteArray(count)
                var offset = 0
                while (offset < count) {
                    val received = readSome(data, offset, count - offset)
                    if (received < 0) throw EOFException("Truncated SOCKS/HTTP response")
                    offset += received
                }
                return data
            }
            fun byte(): Int = readExact(1)[0].toInt() and 255
            val method = if (username == null) 0 else 2
            output.write(byteArrayOf(5, 1, method.toByte())); output.flush()
            if (byte() != 5 || byte() != method) throw IOException("SOCKS authentication method rejected")
            if (username != null && password != null) {
                val userBytes = username.toByteArray(Charsets.UTF_8)
                val passwordBytes = password.toByteArray(Charsets.UTF_8)
                require(userBytes.size in 1..255 && passwordBytes.size in 1..255) { "Invalid SOCKS credential length" }
                output.write(byteArrayOf(1, userBytes.size.toByte())); output.write(userBytes)
                output.write(passwordBytes.size); output.write(passwordBytes); output.flush()
                if (byte() != 1 || byte() != 0) throw IOException("SOCKS credentials rejected")
            }
            val host = url.host.removeSurrounding("[", "]")
            val targetPort = if (url.port > 0) url.port else if (url.protocol == "https") 443 else 80
            val address = if (':' in host) {
                byteArrayOf(4) + InetAddress.getByName(host).address
            } else {
                val domain = IDN.toASCII(host).toByteArray(Charsets.US_ASCII)
                require(domain.size in 1..255) { "Invalid subscription hostname" }
                byteArrayOf(3, domain.size.toByte()) + domain
            }
            output.write(byteArrayOf(5, 1, 0) + address + byteArrayOf((targetPort shr 8).toByte(), targetPort.toByte()))
            output.flush()
            if (byte() != 5 || byte() != 0 || byte() != 0) throw IOException("SOCKS connection rejected")
            when (byte()) { 1 -> readExact(4); 4 -> readExact(16); 3 -> readExact(byte()); else -> throw IOException("Invalid SOCKS address") }
            readExact(2)
            check()
            if (url.protocol == "https") {
                val tls = (SSLSocketFactory.getDefault() as SSLSocketFactory).createSocket(socket, host, targetPort, true) as SSLSocket
                socket = tls
                tls.soTimeout = 15_000
                tls.sslParameters = tls.sslParameters.apply { endpointIdentificationAlgorithm = "HTTPS" }
                tls.startHandshake()
                tls.soTimeout = 500
                input = tls.inputStream
                output = tls.outputStream
            }
            input = BufferedInputStream(input)
            val path = url.file.ifEmpty { "/" }
            require(path.none { it == '\r' || it == '\n' || it == ' ' }) { "Invalid subscription request path" }
            val authority = (if (':' in host) "[$host]" else IDN.toASCII(host)) + if (url.port > 0) ":${url.port}" else ""
            val request = buildString {
                append("GET ").append(path).append(" HTTP/1.1\r\nHost: ").append(authority)
                append("\r\nConnection: close\r\n")
                headers.forEach { (key, value) -> append(key).append(": ").append(value).append("\r\n") }
                append("\r\n")
            }
            output.write(request.toByteArray(Charsets.UTF_8)); output.flush()
            var headerBytes = 0
            fun line(): String {
                val bytes = ByteArrayOutputStream()
                while (true) {
                    val value = byte()
                    headerBytes++
                    if (headerBytes > 65536 || bytes.size() > 8192) throw IOException("HTTP headers exceed limit")
                    if (value == 13) {
                        if (byte() != 10) throw IOException("Invalid HTTP line ending")
                        return bytes.toString(Charsets.ISO_8859_1.name())
                    }
                    bytes.write(value)
                }
            }
            var status: Int
            var responseHeaders: MutableMap<String, String>
            var interim = 0
            do {
                val statusLine = line()
                status = Regex("HTTP/1\\.[01] ([0-9]{3})(?: .*)?").matchEntire(statusLine)?.groupValues?.get(1)?.toInt()
                    ?: throw IOException("Invalid HTTP status line")
                responseHeaders = linkedMapOf()
                while (true) {
                    val field = line()
                    if (field.isEmpty()) break
                    val separator = field.indexOf(':')
                    if (separator <= 0) throw IOException("Invalid HTTP header")
                    val name = field.substring(0, separator).lowercase(Locale.US)
                    val value = field.substring(separator + 1).trim()
                    responseHeaders[name] = responseHeaders[name]?.let { "$it,$value" } ?: value
                }
                if (++interim > 5) throw IOException("Too many interim HTTP responses")
            } while (status in 100..199 && status != 101)
            if (status !in 200..299) return SubscriptionHttpResponse(status, responseHeaders, ByteArray(0))
            val body = ByteArrayOutputStream()
            val buffer = ByteArray(8192)
            fun readBody(length: Long?) {
                var remaining = length
                while (remaining == null || remaining > 0) {
                    val limit = minOf(buffer.size.toLong(), remaining ?: buffer.size.toLong()).toInt()
                    val received = readSome(buffer, 0, limit)
                    if (received < 0) {
                        if (remaining != null && remaining > 0) throw EOFException("Truncated HTTP body")
                        break
                    }
                    if (body.size() + received > MAX_BYTES) throw IOException("Subscription exceeds 8 MiB")
                    body.write(buffer, 0, received)
                    if (remaining != null) remaining -= received
                }
            }
            val transfer = responseHeaders["transfer-encoding"]?.lowercase(Locale.US)
            if (transfer != null) {
                if (transfer != "chunked") throw IOException("Unsupported transfer encoding")
                while (true) {
                    val length = line().substringBefore(';').trim().toLongOrNull(16)
                        ?: throw IOException("Invalid chunk size")
                    if (length < 0 || length > MAX_BYTES) throw IOException("Invalid chunk size")
                    if (length == 0L) { while (line().isNotEmpty()) { /* bounded trailers */ }; break }
                    readBody(length)
                    if (byte() != 13 || byte() != 10) throw IOException("Invalid chunk terminator")
                }
            } else {
                val rawLength = responseHeaders["content-length"]
                val length = rawLength?.toLongOrNull()
                if (rawLength != null && (length == null || length !in 0..MAX_BYTES.toLong())) throw IOException("Invalid Content-Length")
                readBody(length)
            }
            return SubscriptionHttpResponse(status, responseHeaders, body.toByteArray())
        } finally { runCatching { socket.close() } }
    }
}
