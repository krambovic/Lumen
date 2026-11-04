package com.lumen.core.vpn

import com.lumen.core.engine.IEngineDriver
import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeoutOrNull
import org.json.JSONObject

/** Wait for the selected endpoint, without testing an unrelated probe website. */
internal suspend fun awaitOpenVpnDataChannel(
    engine: IEngineDriver,
    tag: String,
    timeoutMs: Long = 30_000L
) {
    require(timeoutMs > 0) { "OpenVPN startup timeout must be positive" }
    fun selectedFailure(): String = engine.endpointFailure.takeIf {
        it.contains("endpoint/openvpn-client[$tag]:")
    }.orEmpty()

    // Only this deadline becomes an error. Parent cancellation and failures from
    // the engine propagate unchanged instead of becoming a misleading timeout.
    val ready = withTimeoutOrNull(timeoutMs) {
        while (tag !in engine.readyOpenVpnTags) {
            val failure = selectedFailure()
            check(failure.isBlank()) { failure }
            check(engine.isRunning) {
                val detail = engine.lastFatalLine
                "OpenVPN core stopped during startup" + if (detail.isBlank()) "" else ": $detail"
            }
            delay(100)
        }
        true
    } ?: false
    check(ready) {
        selectedFailure().ifBlank { "OpenVPN handshake did not complete within $timeoutMs ms" }
    }
}

/** Only the selected route may gate startup, never an unused member of an AUTO pool. */
internal fun openVpnStartupTag(config: String): String? = runCatching {
    val root = JSONObject(config)
    val finalTag = root.optJSONObject("route")?.optString("final")?.takeIf { it.isNotBlank() } ?: "proxy"
    listOf("endpoints", "outbounds").firstNotNullOfOrNull { key ->
        val values = root.optJSONArray(key)
        (0 until (values?.length() ?: 0)).firstNotNullOfOrNull { index ->
            values?.optJSONObject(index)?.takeIf {
                it.optString("tag") == finalTag && it.optString("type") in setOf("openvpn", "openvpn-client")
            }?.optString("tag")
        }
    }
}.getOrNull()
