package com.lumen.core.vpn

import org.json.JSONObject

/**
 * Refreshes only Lumen's generated physical-network resolver. The persisted
 * config remains untouched, so another reconnect can patch it for the next
 * Wi-Fi/mobile network without accumulating mutations.
 */
internal object ManagedDnsConfigPatcher {
    data class Result(val json: String, val changed: Boolean)

    fun refreshSystemDns(configJson: String, dnsServers: List<String>): Result {
        val replacement = dnsServers.firstOrNull { it.isNotBlank() }
            ?: return Result(configJson, false)
        return runCatching {
            val root = JSONObject(configJson)
            val servers = root.optJSONObject("dns")?.optJSONArray("servers")
                ?: return Result(configJson, false)
            var changed = false
            for (index in 0 until servers.length()) {
                val server = servers.optJSONObject(index) ?: continue
                if (server.optString("tag") != SYSTEM_DNS_TAG || !server.has("server")) continue
                if (server.optString("server") != replacement) {
                    server.put("server", replacement)
                    changed = true
                }
            }
            Result(if (changed) root.toString() else configJson, changed)
        }.getOrElse { Result(configJson, false) }
    }

    private const val SYSTEM_DNS_TAG = "dns-system"
}
