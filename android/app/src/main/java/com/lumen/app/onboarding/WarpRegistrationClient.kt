package com.lumen.app.onboarding

import com.lumen.core.config.parser.ParsedNode
import com.lumen.core.config.warp.WarpProfiles
import kotlinx.coroutines.*
import java.net.HttpURLConnection
import java.net.URL

internal data class WarpGeneration(val nodes: List<ParsedNode>, val incomplete: Boolean)

/** Same explicit public registration helpers as Desktop. Never logs or persists their response. */
internal class WarpRegistrationClient(private val version: String) {
    suspend fun generate(includeAwg: Boolean = true, includeMasque: Boolean = true): WarpGeneration = coroutineScope {
        val awg = if (includeAwg) async(Dispatchers.IO) { fetch(false) } else null
        val masque = if (includeMasque) async(Dispatchers.IO) { fetch(true) } else null
        val awgNodes = awg?.await().orEmpty()
        val masqueNodes = masque?.await().orEmpty()
        val incomplete = (includeAwg && awgNodes.isEmpty()) || (includeMasque && masqueNodes.isEmpty())
        WarpGeneration(awgNodes + masqueNodes, incomplete)
    }

    private suspend fun fetch(masque: Boolean): List<ParsedNode> {
        for (endpoint in WarpProfiles.endpoints(masque)) {
            currentCoroutineContext().ensureActive()
            val connection = URL(endpoint).openConnection() as HttpURLConnection
            try {
                connection.connectTimeout = 8_000
                connection.readTimeout = 8_000
                // Desktop's urllib follows helper redirects; these public
                // providers use redirects to their active registration workers.
                connection.instanceFollowRedirects = true
                connection.setRequestProperty("User-Agent", "Lumen/$version")
                connection.setRequestProperty("Accept", "application/json")
                if (!masque) connection.setRequestProperty("X-Client", "WARP")
                check(connection.responseCode in 200..299)
                val bytes = connection.inputStream.use { stream ->
                    val buffer = ByteArray(65_537)
                    var length = 0
                    while (length < buffer.size) {
                        currentCoroutineContext().ensureActive()
                        val read = stream.read(buffer, length, buffer.size - length)
                        if (read < 0) break
                        length += read
                    }
                    buffer.copyOf(length)
                }
                check(bytes.size <= 65_536)
                val registration = WarpProfiles.registration(bytes.toString(Charsets.UTF_8), masque = masque)
                return if (masque) WarpProfiles.masque(registration) else WarpProfiles.awg(registration)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (_: Exception) {
                // Try Desktop's next helper without leaking generated private keys to logs.
            } finally {
                connection.disconnect()
            }
        }
        return emptyList()
    }
}
