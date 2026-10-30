package com.lumen.app.vm

import java.io.IOException
import java.net.InetAddress
import java.net.Proxy
import java.net.URL
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ExecutorCompletionService
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import javax.net.ssl.HttpsURLConnection
import kotlin.random.Random

internal class TrustedNameNotFound : IOException("Trusted DNS returned NXDOMAIN")

/**
 * The desktop bootstrap policy: authenticated DoH, vetted system DNS, remembered
 * addresses. Only endpoint names use this path; application DNS stays in the VPN.
 * IP URLs avoid a circular dependency on the very resolver being repaired. TLS
 * certificate and hostname verification are left enabled, including IP SAN checks.
 */
internal object TrustedEndpointResolver {
    private const val TIMEOUT_MS = 1_500
    private const val LOOKUP_BUDGET_MS = 3_500L
    private const val FRESH_MS = 5 * 60_000L
    private const val REMEMBER_MS = 24 * 60 * 60_000L
    private data class Cached(val addresses: List<InetAddress>, val at: Long)
    private val cache = ConcurrentHashMap<String, Cached>()
    private val locks = Array(32) { Any() }
    private val workers = Executors.newFixedThreadPool(8) { task ->
        Thread(task, "lumen-bootstrap-doh").apply { isDaemon = true }
    }

    fun resolve(host: String, preferSystem: Boolean = false): List<InetAddress> {
        val name = host.trim().removePrefix("[").removeSuffix("]").trimEnd('.').lowercase()
        if (name.isBlank()) return emptyList()
        if (isLiteral(name)) return runCatching { listOf(InetAddress.getByName(name)) }.getOrDefault(emptyList())
        synchronized(locks[(name.hashCode() and Int.MAX_VALUE) % locks.size]) {
            val now = TimeUnit.NANOSECONDS.toMillis(System.nanoTime())
            // Split-horizon/system answers and global DoH answers are different
            // policies; a lookup from a probe must not seed the other's cache.
            val cacheKey = "${if (preferSystem) "system" else "trusted"}:$name"
            val saved = cache[cacheKey]
            if (saved != null && now - saved.at < FRESH_MS) return saved.addresses
            var trustedSucceeded = false
            var nameNotFound = false
            val addresses = selectAddresses(name, trusted = {
                try {
                    lookupDoh(it).also { result -> trustedSucceeded = result.isNotEmpty() }
                } catch (e: TrustedNameNotFound) {
                    nameNotFound = true
                    throw e
                }
            }, system = { InetAddress.getAllByName(it).toList() },
                remembered = saved?.takeIf { now - it.at < REMEMBER_MS }?.addresses.orEmpty(),
                preferSystem = preferSystem)
            if (nameNotFound) cache.remove(cacheKey)
            if (trustedSucceeded) {
                if (cache.size >= 256) cache.entries.removeIf { now - it.value.at >= FRESH_MS }
                if (cache.size < 256 || cache.containsKey(cacheKey)) cache[cacheKey] = Cached(addresses, now)
            }
            return addresses
        }
    }

    internal fun selectAddresses(
        host: String,
        trusted: (String) -> List<InetAddress>,
        system: (String) -> List<InetAddress>,
        remembered: List<InetAddress> = emptyList(),
        preferSystem: Boolean = false
    ): List<InetAddress> {
        if (isLiteral(host)) return runCatching { listOf(InetAddress.getByName(host)) }.getOrDefault(emptyList())
        fun vetted(values: List<InetAddress>) = values.filterNot(ServerAddressPinner::isBogusEndpointAddress).distinct()
        // A global resolver can select an unreachable CDN edge or return NXDOMAIN
        // for a split-horizon endpoint that resolves correctly on this carrier.
        // Reject known sinkholes, then retain the physical network's usable answer.
        if (preferSystem) {
            val local = try { vetted(system(host)) } catch (_: IOException) { emptyList() }
            if (local.isNotEmpty()) return local
        }
        val secure = try { vetted(trusted(host)) }
            catch (_: TrustedNameNotFound) { return emptyList() }
            catch (_: IOException) { emptyList() }
        if (secure.isNotEmpty()) return secure
        val local = if (preferSystem) emptyList() else
            try { vetted(system(host)) } catch (_: IOException) { emptyList() }
        return local.ifEmpty { vetted(remembered) }
    }

    private fun isLiteral(host: String): Boolean = host.contains(':') ||
        (host.split('.').let { it.size == 4 && it.all { part -> part.toIntOrNull() in 0..255 } })

    private fun lookupDoh(host: String): List<InetAddress> {
        val completed = ExecutorCompletionService<List<InetAddress>>(workers)
        val futures = listOf("8.8.8.8", "1.1.1.1").map { resolver ->
            completed.submit(java.util.concurrent.Callable { queryDoh(host, resolver) })
        }
        val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(LOOKUP_BUDGET_MS)
        var notFound = false
        try {
            repeat(futures.size) {
                val remaining = deadline - System.nanoTime()
                if (remaining <= 0) return@repeat
                val ready = completed.poll(remaining, TimeUnit.NANOSECONDS) ?: return@repeat
                try {
                    val addresses = ready.get().filterNot(ServerAddressPinner::isBogusEndpointAddress)
                    if (addresses.isNotEmpty()) return addresses
                } catch (e: java.util.concurrent.ExecutionException) {
                    if (e.cause is TrustedNameNotFound) notFound = true
                }
            }
            if (notFound) throw TrustedNameNotFound()
            return emptyList()
        } finally {
            futures.forEach { it.cancel(true) }
        }
    }

    private fun queryDoh(host: String, resolver: String): List<InetAddress> {
        val id = Random.nextInt(0x10000)
        val request = ServerAddressPinner.buildQuery(id, host)
        val connection = URL("https://$resolver/dns-query").openConnection(Proxy.NO_PROXY) as HttpsURLConnection
        try {
            connection.connectTimeout = TIMEOUT_MS
            connection.readTimeout = TIMEOUT_MS
            connection.instanceFollowRedirects = false
            connection.requestMethod = "POST"
            connection.doOutput = true
            connection.setFixedLengthStreamingMode(request.size)
            connection.setRequestProperty("Content-Type", "application/dns-message")
            connection.setRequestProperty("Accept", "application/dns-message")
            connection.outputStream.use { it.write(request) }
            if (connection.responseCode != 200) throw IOException("DoH HTTP ${connection.responseCode}")
            val response = connection.inputStream.use { it.readBytesBounded(65_535) }
            return ServerAddressPinner.parseAnswers(response, response.size, id)
        } finally {
            connection.disconnect()
        }
    }

    private fun java.io.InputStream.readBytesBounded(limit: Int): ByteArray {
        val out = java.io.ByteArrayOutputStream()
        val buffer = ByteArray(2048)
        while (true) {
            val read = read(buffer)
            if (read < 0) return out.toByteArray()
            if (out.size() + read > limit) throw IOException("Oversized DNS reply")
            out.write(buffer, 0, read)
        }
    }
}
