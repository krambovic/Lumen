package com.lumen.core.config.builder

import com.lumen.core.config.normalizer.AmneziaWGNormalizer
import com.lumen.core.config.parser.LinkParser
import com.lumen.core.config.parser.ParsedNode
import org.json.JSONArray
import org.json.JSONObject

data class SingboxConfigOptions(
    val tunMode: Boolean = true,
    val tunAddressIPv4: String = "172.19.0.1/30",
    val tunAddressIPv6: String = "fdfe:dcba:9876::1/126",
    val tunMtu: Int = 9000,
    val tunStrictRoute: Boolean = true,
    val tunStack: String = "mixed",
    val localSocksPort: Int = 10808,
    val localHttpPort: Int = 10809,
    val multiplexEnabled: Boolean = false,
    val multiplexConcurrency: Int = 8,
    val enableFinalFragment: Boolean = false,
    val fragmentPackets: String = "tlshello",
    val fragmentLength: String = "50-100",
    val fragmentDelay: String = "10-20",
    val preferIpv6: Boolean = false,
    val blockQuic: Boolean = false,
    val sniffRouteOnly: Boolean = false,
    val proxyDnsServer: String = "cloudflare-dns.com",
    val directDnsServer: String = "1.1.1.1",
    val dnsMode: String = "automatic",
    val dnsDirectServers: List<String> = listOf("1.1.1.1", "8.8.8.8"),
    val dnsProxyServers: List<String> = listOf("cloudflare-dns.com", "dns.google"),
    val dnsDirectType: String = "udp",
    val dnsProxyType: String = "https",
    val dnsDirectStrategy: String = "ipv4_only",
    val dnsProxyStrategy: String = "ipv4_only",
    val dnsHijackEnabled: Boolean = true,
    val dnsFakeIpEnabled: Boolean = false,
    val dnsParallelQuery: Boolean = false,
    val dnsOptimisticCache: Boolean = false,
    val dnsGeoCheck: Boolean = true,
    val dnsProxyIpv4Only: Boolean = true,
    val dnsHosts: Map<String, List<String>> = emptyMap(),
    val dnsOverrideEnabled: Boolean = true,
    val dnsOverrideHostname: String = "ntc.party",
    val dnsOverrideIpv4: String = "130.255.77.28",
    val logLevel: String = "info",
    val urlTestUrl: String = "https://www.gstatic.com/generate_204",
    val urlTestIntervalMinutes: Int = 3,
    val urlTestToleranceMs: Int = 50,
    val directDomains: List<String> = emptyList(),
    val directIpCidrs: List<String> = emptyList()
)

object SingboxConfigBuilder {

    fun buildConfig(
        node: ParsedNode,
        options: SingboxConfigOptions = SingboxConfigOptions()
    ): String {
        return buildConfig(listOf(node), node, options)
    }

    fun buildConfig(
        nodes: List<ParsedNode>,
        selectedNode: ParsedNode?,
        options: SingboxConfigOptions = SingboxConfigOptions()
    ): String {
        val root = mutableMapOf<String, Any?>()

        // 1. Log
        root["log"] = mapOf(
            "level" to (options.logLevel.takeIf { it in setOf("debug", "info", "warning", "error", "none") } ?: "info"),
            "timestamp" to true
        )

        // 2. Inbounds (TUN + optional local SOCKS/HTTP)
        val inbounds = mutableListOf<Map<String, Any?>>()
        if (options.tunMode) {
            inbounds.add(
                mapOf(
                    "type" to "tun",
                    "tag" to "tun-in",
                    "interface_name" to "singbox_tun",
                    "address" to listOf(options.tunAddressIPv4, options.tunAddressIPv6),
                    "mtu" to options.tunMtu,
                    "auto_route" to true,
                    "strict_route" to options.tunStrictRoute,
                    "stack" to options.tunStack
                )
            )
        }

        if (options.localSocksPort > 0) {
            inbounds.add(
                mapOf(
                    "type" to "socks",
                    "tag" to "socks-in",
                    "listen" to "127.0.0.1",
                    "listen_port" to options.localSocksPort,
                    "udp_fragment" to true
                )
            )
        }

        if (options.localHttpPort > 0 && options.localHttpPort != options.localSocksPort) {
            inbounds.add(
                mapOf(
                    "type" to "http",
                    "tag" to "http-in",
                    "listen" to "127.0.0.1",
                    "listen_port" to options.localHttpPort
                )
            )
        }

        root["inbounds"] = inbounds

        // 3. Outbounds
        val outbounds = mutableListOf<Map<String, Any?>>()
        val activeNode = selectedNode ?: nodes.firstOrNull()

        if (activeNode != null) {
            if (activeNode.scheme.equals("auto", true) || activeNode.outbound["type"] == "urltest" || activeNode.outbound["type"] == "selector") {
                // Auto Virtual Node: an imported AUTO group carries its own pool of
                // servers; a manually created one falls back to every other server.
                val poolNodes = LinkParser.autoMembers(activeNode.outbound)
                    .ifEmpty { nodes.filterNot { it.scheme.equals("auto", true) } }
                val poolTags = mutableListOf<String>()

                for (poolNode in poolNodes) {
                    val tag = "proxy-${poolTags.size}"
                    // One broken server must not invalidate the whole auto pool.
                    val ob = try { buildOutboundMap(poolNode, tag, options) } catch (_: Exception) { null }
                    if (ob == null || ob["type"] == null) continue
                    outbounds.add(ob)
                    poolTags.add(tag)
                }

                if (poolTags.isEmpty()) {
                    // sing-box rejects an empty urltest pool: keep the config valid.
                    outbounds.add(mapOf("type" to "direct", "tag" to "proxy"))
                } else {
                    val autoOutbound = mutableMapOf<String, Any?>(
                        "type" to "urltest",
                        "tag" to "proxy",
                        "outbounds" to poolTags,
                        "url" to options.urlTestUrl,
                        "interval" to "${options.urlTestIntervalMinutes.coerceIn(1, 1440)}m",
                        "tolerance" to options.urlTestToleranceMs.coerceIn(0, 5000),
                        "interrupt_exist_connections" to true
                    )
                    outbounds.add(0, autoOutbound)
                }
            } else {
                // Single node
                val ob = buildOutboundMap(activeNode, "proxy", options)
                outbounds.add(ob)
            }
        }

        // Auxiliary Outbounds. Note: the legacy special "block" outbound was
        // removed in sing-box 1.13; blocking is done via `action: reject` rules.
        outbounds.add(mapOf("type" to "direct", "tag" to "direct"))

        root["outbounds"] = outbounds

        // 4. DNS. Managed resolvers are deterministic: bootstrap/direct never
        // recurse into the tunnel, while every remote resolver dials via proxy.
        val dohHostByIp = mapOf(
            "1.1.1.1" to "cloudflare-dns.com", "1.0.0.1" to "cloudflare-dns.com",
            "8.8.8.8" to "dns.google", "8.8.4.4" to "dns.google",
            "9.9.9.9" to "dns.quad9.net", "149.112.112.112" to "dns.quad9.net"
        )
        val directServers = options.dnsDirectServers.map(String::trim).filter(String::isNotEmpty)
            .ifEmpty { listOf(options.directDnsServer.ifBlank { "1.1.1.1" }) }
        val proxyServers = options.dnsProxyServers.map(String::trim).filter(String::isNotEmpty)
            .ifEmpty { listOf(options.proxyDnsServer.ifBlank { "cloudflare-dns.com" }) }
        fun dnsServer(tag: String, raw: String, type: String, detour: String, strategy: String): Map<String, Any?> {
            val normalizedType = type.lowercase().takeIf { it in setOf("udp", "tcp", "tls", "https") } ?: "udp"
            val server = if (normalizedType in setOf("tls", "https")) dohHostByIp[raw] ?: raw else raw
            // Dial-field "domain_strategy" is legacy since 1.12: strategy now
            // lives inside the domain_resolver object.
            return mutableMapOf<String, Any?>(
                "type" to normalizedType, "tag" to tag, "server" to server,
                "detour" to detour
            ).apply {
                if (normalizedType == "https") put("path", "/dns-query")
                if (server.any { it.isLetter() } && tag != "dns-bootstrap") {
                    put("domain_resolver", mapOf("server" to "dns-bootstrap", "strategy" to strategy))
                }
            }
        }
        val dnsServers = mutableListOf<Map<String, Any?>>()
        dnsServers += dnsServer("dns-bootstrap", directServers.first(), options.dnsDirectType, "direct", options.dnsDirectStrategy)
        directServers.forEachIndexed { index, server ->
            dnsServers += dnsServer("dns-direct-${index + 1}", server, options.dnsDirectType, "direct", options.dnsDirectStrategy)
        }
        proxyServers.forEachIndexed { index, server ->
            dnsServers += dnsServer("dns-proxy-${index + 1}", server, options.dnsProxyType, "proxy", options.dnsProxyStrategy)
        }
        val hosts = options.dnsHosts.toMutableMap()
        if (options.dnsOverrideEnabled && options.dnsOverrideHostname.isNotBlank() && options.dnsOverrideIpv4.isNotBlank()) {
            hosts[options.dnsOverrideHostname.trim().trimEnd('.').lowercase()] = listOf(options.dnsOverrideIpv4.trim())
        }
        if (hosts.isNotEmpty()) {
            dnsServers.add(0, mapOf("type" to "hosts", "tag" to "dns-hosts", "predefined" to hosts))
        }
        if (options.dnsFakeIpEnabled) {
            dnsServers += mapOf(
                "type" to "fakeip", "tag" to "dns-fake",
                "inet4_range" to "198.18.0.0/15", "inet6_range" to "fc00::/18"
            )
        }
        val dnsRules = mutableListOf<Map<String, Any?>>(
            mapOf("query_type" to listOf("HTTPS", "SVCB"), "action" to "reject"),
            mapOf("domain_suffix" to listOf("dns.google", "cloudflare-dns.com", "mozilla.cloudflare-dns.com", "doh.opendns.com"), "action" to "reject")
        )
        if (hosts.isNotEmpty()) dnsRules.add(0, mapOf("ip_accept_any" to true, "server" to "dns-hosts"))
        if (options.dnsProxyIpv4Only && options.dnsMode != "json") {
            dnsRules += mapOf("query_type" to listOf("AAAA"), "action" to "reject")
        }
        // Direct-listed domains must resolve through direct DNS: proxy-resolved
        // IPs would otherwise be routed direct and break geo-based access.
        val directDnsDomains = options.directDomains
            .map { it.substringAfter(':').trim().trimEnd('.').lowercase() }
            .filter { it.isNotEmpty() && !it.contains('/') && it.any { c -> c.isLetter() } }
        if (options.dnsGeoCheck && directDnsDomains.isNotEmpty()) {
            dnsRules += mapOf("domain_suffix" to directDnsDomains, "server" to "dns-direct-1")
        }
        // DNS pushed by the profile itself (OpenVPN dhcp-option DNS, WireGuard DNS=).
        val profileDns = (activeNode?.outbound?.get("_dns") as? List<*>)
            ?.mapNotNull { it?.toString()?.trim() }?.filter { it.isNotEmpty() } ?: emptyList()
        profileDns.forEachIndexed { index, server ->
            dnsServers += dnsServer("dns-vpn-${index + 1}", server, "udp", "proxy", options.dnsProxyStrategy)
        }
        val useAndroidDns = options.dnsMode.lowercase() in setOf("android", "system")
        root["dns"] = mapOf(
            "servers" to dnsServers,
            "rules" to dnsRules,
            "final" to if (useAndroidDns) "dns-direct-1" else if (options.dnsFakeIpEnabled) "dns-fake" else if (profileDns.isNotEmpty()) "dns-vpn-1" else "dns-proxy-1",
            "strategy" to if (useAndroidDns) options.dnsDirectStrategy else options.dnsProxyStrategy,
            "independent_cache" to true,
            "reverse_mapping" to true,
            // Optimistic cache = serve stale answers while refreshing.
            "disable_expire" to options.dnsOptimisticCache,
            "cache_capacity" to if (options.dnsParallelQuery) 4096 else 2048
        )

        // 5. Route (sing-box 1.13 rule actions)
        val routeRules = mutableListOf<Map<String, Any?>>(
            mapOf("ip_cidr" to listOf("224.0.0.0/3", "ff00::/8"), "action" to "reject")
        )
        // User Routing Rules (Domains, Geosite, IP CIDRs)
        val directDomains = mutableListOf<String>()
        val proxyDomains = mutableListOf<String>()
        val blockDomains = mutableListOf<String>()
        val directIpCidrs = mutableListOf<String>()
        val proxyIpCidrs = mutableListOf<String>()
        val blockIpCidrs = mutableListOf<String>()
        val geositeRules = mutableListOf<Pair<String, String>>() // code to action
        val geoipRules = mutableListOf<Pair<String, String>>() // country code to action

        fun processRuleItem(item: String) {
            val trimmed = item.trim()
            if (trimmed.isEmpty()) return
            val action = when {
                trimmed.startsWith("proxy:", true) -> "proxy"
                trimmed.startsWith("block:", true) || trimmed.startsWith("reject:", true) -> "block"
                trimmed.startsWith("direct:", true) -> "direct"
                else -> "direct"
            }
            // Only strip a leading action prefix, so bare "geosite:ru" keeps its kind.
            val rawPattern = listOf("proxy:", "block:", "reject:", "direct:")
                .firstOrNull { trimmed.startsWith(it, true) }
                ?.let { trimmed.substring(it.length).trim() }
                ?.ifEmpty { trimmed }
                ?: trimmed

            val isGeosite = rawPattern.startsWith("geosite:", true)
            val isGeoip = rawPattern.startsWith("geoip:", true)
            val isIp = !isGeosite && !isGeoip &&
                (rawPattern.contains("/") || rawPattern.all { it.isDigit() || it == '.' || it == ':' })

            when {
                isGeoip -> geoipRules.add(rawPattern.substring("geoip:".length).trim().lowercase() to action)
                isGeosite -> geositeRules.add(rawPattern.substring("geosite:".length).trim().lowercase() to action)
                isIp -> when (action) {
                    "proxy" -> proxyIpCidrs.add(rawPattern)
                    "block" -> blockIpCidrs.add(rawPattern)
                    else -> directIpCidrs.add(rawPattern)
                }
                else -> when (action) {
                    "proxy" -> proxyDomains.add(rawPattern)
                    "block" -> blockDomains.add(rawPattern)
                    else -> directDomains.add(rawPattern)
                }
            }
        }

        options.directIpCidrs.forEach { processRuleItem(it) }
        options.directDomains.forEach { processRuleItem(it) }

        if (blockIpCidrs.isNotEmpty()) routeRules.add(0, mapOf("ip_cidr" to blockIpCidrs, "action" to "reject"))
        if (proxyIpCidrs.isNotEmpty()) routeRules.add(0, mapOf("ip_cidr" to proxyIpCidrs, "outbound" to "proxy"))
        if (directIpCidrs.isNotEmpty()) routeRules.add(0, mapOf("ip_cidr" to directIpCidrs, "outbound" to "direct"))

        if (blockDomains.isNotEmpty()) routeRules.add(0, mapOf("domain" to blockDomains, "action" to "reject"))
        if (proxyDomains.isNotEmpty()) routeRules.add(0, mapOf("domain" to proxyDomains, "outbound" to "proxy"))
        if (directDomains.isNotEmpty()) routeRules.add(0, mapOf("domain" to directDomains, "outbound" to "direct"))

        // sing-box 1.12+ removed the legacy `geosite` rule key; use remote
        // rule-sets instead (same data, .srs format, cached by the core).
        val ruleSets = mutableListOf<Map<String, Any?>>()
        geositeRules.forEach { (code, action) ->
            val tag = "geosite-$code"
            if (ruleSets.none { it["tag"] == tag }) {
                ruleSets.add(
                    mapOf(
                        "type" to "remote",
                        "tag" to tag,
                        "format" to "binary",
                        "url" to "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/$tag.srs",
                        "download_detour" to "direct"
                    )
                )
            }
            if (action == "block") {
                routeRules.add(0, mapOf("rule_set" to listOf(tag), "action" to "reject"))
            } else {
                routeRules.add(0, mapOf("rule_set" to listOf(tag), "outbound" to action))
            }
        }
        // geoip:<code> maps to the sing-geoip remote rule-sets.
        geoipRules.forEach { (code, action) ->
            val tag = "geoip-$code"
            if (ruleSets.none { it["tag"] == tag }) {
                ruleSets.add(
                    mapOf(
                        "type" to "remote",
                        "tag" to tag,
                        "format" to "binary",
                        "url" to "https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/$tag.srs",
                        "download_detour" to "direct"
                    )
                )
            }
            if (action == "block") {
                routeRules.add(0, mapOf("rule_set" to listOf(tag), "action" to "reject"))
            } else {
                routeRules.add(0, mapOf("rule_set" to listOf(tag), "outbound" to action))
            }
        }
        if (options.blockQuic) {
            routeRules.add(0, mapOf("network" to "udp", "port" to 443, "action" to "reject"))
        }
        // sing-box 1.12+: the `protocol: dns` matcher only works after traffic
        // has been sniffed, and the legacy inbound-level sniff fields were
        // removed. Without these rules DNS from apps leaks as raw UDP:53
        // through the proxy (which many vless/vmess/trojan servers do not
        // relay), so nothing resolves even though the tunnel is up. Keep them
        // strictly first so DNS is always answered by the built-in resolver.
        val managedDns = options.dnsHijackEnabled && options.dnsMode.lowercase() !in setOf("android", "system")
        if (managedDns) {
            routeRules.add(0, mapOf("port" to 53, "action" to "hijack-dns"))
            routeRules.add(0, mapOf("protocol" to "dns", "action" to "hijack-dns"))
        } else {
            routeRules.add(0, mapOf("port" to 53, "outbound" to "direct"))
        }
        routeRules.add(0, mapOf(
            "domain_suffix" to listOf("dns.google", "cloudflare-dns.com", "mozilla.cloudflare-dns.com", "doh.opendns.com"),
            "action" to "reject"
        ))
        routeRules.add(0, mapOf("action" to "sniff"))
        val routeMap = mutableMapOf<String, Any?>(
            // Android's app sandbox forbids sing-box's netlink network monitor.
            // SOCKS-only mode relies on VpnService + tun2socks and the OS default route.
            "auto_detect_interface" to options.tunMode,
            // Replaces legacy per-outbound domain_strategy (removed in 1.14).
            "default_domain_resolver" to mapOf(
                "server" to "dns-bootstrap",
                "strategy" to options.dnsDirectStrategy
            ),
            "rules" to routeRules,
            "final" to "proxy"
        )
        if (ruleSets.isNotEmpty()) {
            routeMap["rule_set"] = ruleSets
        }
        root["route"] = routeMap

        // 6. Normalize AmneziaWG and WireGuard outbounds to extended endpoints format
        AmneziaWGNormalizer.normalizeSingboxWireguardEndpoints(root)

        // Clean non-singbox metadata keys from outbounds and endpoints
        (root["outbounds"] as? List<*>)?.let { obs ->
            root["outbounds"] = obs.mapNotNull { if (it is Map<*, *>) cleanSingboxOutbound(it as Map<String, Any?>) else it }
        }
        (root["endpoints"] as? List<*>)?.let { eps ->
            root["endpoints"] = eps.mapNotNull { if (it is Map<*, *>) cleanSingboxOutbound(it as Map<String, Any?>) else it }
        }

        return mapToJsonString(root)
    }

    private fun cleanSingboxOutbound(raw: Map<String, Any?>): Map<String, Any?> {
        val nonSingboxKeys = mutableSetOf(
            "config_str", "original_link", "happ_id", "happ_token", "display_protocol",
            "protocol", "singbox", "streamSettings", "settings", "v", "ps", "add", "scy",
            "net", "host", "path", "user", "pass", "clash", "_dns",
            // sing-box extended 2.5.1+ rejects "reserved" on every outbound/endpoint type:
            // WARP endpoints derive the reserved bytes from their profile instead.
            "reserved"
        )
        val result = mutableMapOf<String, Any?>()
        for ((k, v) in raw) {
            if (k !in nonSingboxKeys && v != null) {
                result[k] = v
            }
        }
        // Imported configs often carry legacy dial fields; sing-box 1.12+ aborts
        // on them, so migrate domain_strategy into a domain_resolver object.
        val legacyStrategy = result.remove("domain_strategy") as? String
        if (!legacyStrategy.isNullOrBlank() && result["domain_resolver"] == null) {
            result["domain_resolver"] = mapOf("server" to "dns-bootstrap", "strategy" to legacyStrategy)
        }
        result.remove("address_strategy")
        return result
    }

    fun buildOutbound(
        node: ParsedNode,
        tag: String = "proxy",
        options: SingboxConfigOptions = SingboxConfigOptions()
    ): Map<String, Any?> {
        return buildOutboundMap(node, tag, options)
    }

    fun buildAutoSelectorOutbound(
        tag: String = "auto",
        nodeTags: List<String>,
        testUrl: String = "https://www.gstatic.com/generate_204",
        interval: String = "3m",
        tolerance: Int = 50
    ): Map<String, Any?> {
        return mapOf(
            "type" to "urltest",
            "tag" to tag,
            "outbounds" to nodeTags,
            "url" to testUrl,
            "interval" to interval,
            "tolerance" to tolerance
        )
    }

    private fun buildOutboundMap(
        node: ParsedNode,
        tag: String,
        options: SingboxConfigOptions
    ): Map<String, Any?> {
        val outbound = node.outbound.toMutableMap()
        val scheme = node.scheme.lowercase()

        val sbNative = outbound["singbox"]
        if (sbNative is Map<*, *>) {
            @Suppress("UNCHECKED_CAST")
            val nativeMap = (sbNative as Map<String, Any?>).toMutableMap()
            nativeMap["tag"] = tag
            val normalized = AmneziaWGNormalizer.normalizeWireGuardEndpoint(nativeMap).toMutableMap()
            if (normalized["type"] == "masque") {
                sanitizeMasqueOutbound(normalized)
            }
            return normalized
        }

        val result = mutableMapOf<String, Any?>()
        result["tag"] = tag

        when (scheme) {
            "wireguard", "awg" -> {
                val wireguardMap = outbound.toMutableMap()
                wireguardMap["type"] = "wireguard"
                wireguardMap["tag"] = tag
                if (!wireguardMap.containsKey("server") && node.server.isNotEmpty()) {
                    wireguardMap["server"] = node.server
                }
                if (!wireguardMap.containsKey("server_port") && node.port > 0) {
                    wireguardMap["server_port"] = node.port
                }
                return AmneziaWGNormalizer.normalizeWireGuardEndpoint(wireguardMap)
            }
            "vless", "vmess" -> {
                result["type"] = scheme
                result["server"] = node.server
                result["server_port"] = node.port

                val settings = outbound["settings"] as? Map<*, *>
                val vnext = (settings?.get("vnext") as? List<*>)?.firstOrNull() as? Map<*, *>
                val users = (vnext?.get("users") as? List<*>)?.firstOrNull() as? Map<*, *>

                val uuid = users?.get("id")?.toString() ?: outbound["uuid"]?.toString() ?: ""
                result["uuid"] = uuid

                if (scheme == "vless") {
                    val flow = users?.get("flow")?.toString() ?: outbound["flow"]?.toString() ?: ""
                    if (flow.isNotEmpty()) {
                        result["flow"] = flow
                    }
                } else {
                    val alterId = (users?.get("alterId") as? Number)?.toInt() ?: (outbound["alterId"] as? Number)?.toInt() ?: 0
                    val security = users?.get("security")?.toString() ?: outbound["security"]?.toString() ?: "auto"
                    result["alter_id"] = alterId
                    result["security"] = security
                }

                val streamSettings = outbound["streamSettings"] as? Map<*, *> ?: emptyMap<String, Any?>()
                applyTlsAndTransport(result, streamSettings, node.server)
            }
            "trojan" -> {
                result["type"] = "trojan"
                result["server"] = node.server
                result["server_port"] = node.port

                val settings = outbound["settings"] as? Map<*, *>
                val servers = (settings?.get("servers") as? List<*>)?.firstOrNull() as? Map<*, *>
                val password = servers?.get("password")?.toString() ?: outbound["password"]?.toString() ?: ""
                result["password"] = password

                val streamSettings = outbound["streamSettings"] as? Map<*, *> ?: emptyMap<String, Any?>()
                applyTlsAndTransport(result, streamSettings, node.server)
            }
            "ss", "shadowsocks" -> {
                result["type"] = "shadowsocks"
                result["server"] = node.server
                result["server_port"] = node.port

                val settings = outbound["settings"] as? Map<*, *>
                val servers = (settings?.get("servers") as? List<*>)?.firstOrNull() as? Map<*, *>
                val method = servers?.get("method")?.toString() ?: outbound["method"]?.toString() ?: ""
                val password = servers?.get("password")?.toString() ?: outbound["password"]?.toString() ?: ""
                result["method"] = method
                result["password"] = password
            }
            "hysteria2", "hy2" -> {
                result["type"] = "hysteria2"
                result["server"] = node.server
                result["server_port"] = node.port
                val password = outbound["password"]?.toString() ?: outbound["auth"]?.toString() ?: ""
                if (password.isNotEmpty()) {
                    result["password"] = password
                }
                val upMbps = (outbound["up_mbps"] as? Number)?.toInt() ?: 50
                val downMbps = (outbound["down_mbps"] as? Number)?.toInt() ?: 200
                result["up_mbps"] = upMbps
                result["down_mbps"] = downMbps

                val obfs = outbound["obfs"]
                if (obfs is Map<*, *>) {
                    result["obfs"] = obfs
                }
                // hysteria2 always requires TLS in sing-box; without it the core
                // rejects the config (legacy stored nodes may lack a native block).
                if (result["tls"] == null) {
                    val tls = mutableMapOf<String, Any?>("enabled" to true)
                    tls["server_name"] = outbound["sni"]?.toString()?.takeIf { it.isNotEmpty() }
                        ?: outbound["server_name"]?.toString()?.takeIf { it.isNotEmpty() }
                        ?: node.server
                    if (outbound["insecure"] == true || outbound["allowInsecure"] == true) {
                        tls["insecure"] = true
                    }
                    result["tls"] = tls
                }
            }
            "tuic" -> {
                result["type"] = "tuic"
                result["server"] = node.server
                result["server_port"] = node.port
                val uuid = outbound["uuid"]?.toString() ?: ""
                val password = outbound["password"]?.toString() ?: ""
                if (uuid.isNotEmpty()) result["uuid"] = uuid
                if (password.isNotEmpty()) result["password"] = password
                result["congestion_control"] = outbound["congestion_control"]?.toString() ?: "cubic"
                result["zero_rtt_handshake"] = outbound["zero_rtt_handshake"] == true
                // TUIC always requires TLS in sing-box.
                if (result["tls"] == null) {
                    val tls = mutableMapOf<String, Any?>("enabled" to true)
                    tls["server_name"] = outbound["sni"]?.toString()?.takeIf { it.isNotEmpty() } ?: node.server
                    (outbound["alpn"] as? List<*>)?.let { tls["alpn"] = it }
                    if (outbound["insecure"] == true || outbound["allowInsecure"] == true) {
                        tls["insecure"] = true
                    }
                    result["tls"] = tls
                }
            }
            "openvpn" -> {
                // Legacy nodes without a native "singbox" block: re-derive the whole
                // profile from the stored config, otherwise ca/tls/cipher are lost
                // and the core rejects the outbound.
                val reparsed = runCatching { LinkParser.parseOpenVpnConfig(node.link) }
                    .getOrNull()?.outbound?.get("singbox") as? Map<*, *>
                if (reparsed != null) {
                    @Suppress("UNCHECKED_CAST")
                    val nativeMap = (reparsed as Map<String, Any?>).toMutableMap()
                    nativeMap["tag"] = tag
                    return nativeMap
                }
                result["type"] = "openvpn"
                result["system"] = false
                result["name"] = "openvpn0"
                result["servers"] = listOf(mapOf("server" to node.server, "server_port" to node.port))
                for (key in listOf(
                    "proto", "cipher", "auth", "tls", "tls_auth", "tls_crypt", "tls_crypt_v2",
                    "key_direction", "username", "password", "reconnect_delay",
                    "ping_interval", "ping_restart"
                )) {
                    outbound[key]?.let { result[key] = it }
                }
            }
            "socks", "http" -> {
                result["type"] = scheme
                result["server"] = node.server
                result["server_port"] = node.port
                outbound["username"]?.let { result["username"] = it.toString() }
                outbound["password"]?.let { result["password"] = it.toString() }
            }
            else -> {
                // Fallback copying outbound map
                for ((k, v) in outbound) {
                    result[k] = v
                }
                result["type"] = scheme.ifEmpty { "direct" }
                result["server"] = node.server
                result["server_port"] = node.port
            }
        }

        // smux is only supported by these protocols in sing-box; attaching it to
        // e.g. socks/http/hysteria2/tuic makes the core reject the whole config.
        if (options.multiplexEnabled && result["type"] in setOf("vless", "vmess", "trojan", "shadowsocks")) {
            val multiplexMap = mutableMapOf<String, Any?>(
                "enabled" to true,
                "protocol" to "smux",
                "max_connections" to options.multiplexConcurrency,
                "min_streams" to 4,
                "padding" to true
            )
            result["multiplex"] = multiplexMap
        }

        if (result["type"] == "masque") {
            sanitizeMasqueOutbound(result)
        }

        return result
    }

    private fun sanitizeMasqueOutbound(result: MutableMap<String, Any?>) {
        val rawProfile = result["profile"] as? Map<*, *>
        val profile = mutableMapOf<String, Any?>("detour" to "direct")
        if (rawProfile != null) {
            for ((k, v) in rawProfile) {
                profile[k.toString()] = v
            }
        }

        val server = result.remove("server")?.toString()?.trim()?.takeIf { it.isNotEmpty() }
        val serverPort = (result.remove("server_port") as? Number)?.toInt()
            ?: (result.remove("port") as? Number)?.toInt()
        val privateKey = result.remove("private_key")?.toString()?.trim()?.takeIf { it.isNotEmpty() }
        val publicKey = result.remove("public_key")?.toString()?.trim()?.takeIf { it.isNotEmpty() }
        val address = result.remove("address")
        val mtu = result.remove("mtu")

        if (server != null) profile["server"] = server
        if (serverPort != null && serverPort > 0) profile["server_port"] = serverPort
        if (privateKey != null) profile["private_key"] = privateKey
        if (publicKey != null) profile["public_key"] = publicKey
        if (address != null) profile["address"] = address
        if (mtu != null) profile["mtu"] = mtu

        result["profile"] = profile
    }

    private fun applyTlsAndTransport(
        result: MutableMap<String, Any?>,
        stream: Map<*, *>,
        server: String
    ) {
        val security = stream["security"]?.toString()?.lowercase() ?: ""
        if (security == "tls" || security == "reality") {
            val tls = mutableMapOf<String, Any?>("enabled" to true)
            if (security == "reality") {
                val realitySettings = stream["realitySettings"] as? Map<*, *> ?: emptyMap<Any, Any>()
                val serverName = realitySettings["serverName"]?.toString() ?: server
                tls["server_name"] = serverName
                realitySettings["publicKey"]?.let {
                    tls["reality"] = mapOf("enabled" to true, "public_key" to it.toString(), "short_id" to (realitySettings["shortId"]?.toString() ?: ""))
                }
                realitySettings["fingerprint"]?.let {
                    tls["utls"] = mapOf("enabled" to true, "fingerprint" to it.toString())
                }
            } else {
                val tlsSettings = stream["tlsSettings"] as? Map<*, *> ?: emptyMap<Any, Any>()
                val serverName = tlsSettings["serverName"]?.toString() ?: server
                if (serverName.isNotEmpty()) {
                    tls["server_name"] = serverName
                }
                if (tlsSettings["allowInsecure"] == true) {
                    tls["insecure"] = true
                }
                tlsSettings["fingerprint"]?.let {
                    tls["utls"] = mapOf("enabled" to true, "fingerprint" to it.toString())
                }
                val alpn = tlsSettings["alpn"]
                if (alpn is List<*>) {
                    tls["alpn"] = alpn
                }
            }
            result["tls"] = tls
        }

        val network = stream["network"]?.toString()?.lowercase() ?: "tcp"
        if (network != "tcp") {
            when (network) {
                "ws" -> {
                    val wsSettings = stream["wsSettings"] as? Map<*, *> ?: emptyMap<Any, Any>()
                    val transport = mutableMapOf<String, Any?>("type" to "ws")
                    wsSettings["path"]?.let { transport["path"] = it.toString() }
                    wsSettings["headers"]?.let { transport["headers"] = it }
                    result["transport"] = transport
                }
                "http", "h2" -> {
                    val httpSettings = stream["httpSettings"] as? Map<*, *> ?: stream["h2Settings"] as? Map<*, *> ?: emptyMap<Any, Any>()
                    val transport = mutableMapOf<String, Any?>("type" to "http")
                    httpSettings["host"]?.let { transport["host"] = if (it is List<*>) it else listOf(it.toString()) }
                    httpSettings["path"]?.let { transport["path"] = it.toString() }
                    result["transport"] = transport
                }
                "grpc" -> {
                    val grpcSettings = stream["grpcSettings"] as? Map<*, *> ?: emptyMap<Any, Any>()
                    val transport = mutableMapOf<String, Any?>("type" to "grpc")
                    grpcSettings["serviceName"]?.let { transport["service_name"] = it.toString() }
                    result["transport"] = transport
                }
                "xhttp" -> {
                    val xhttpSettings = stream["xhttpSettings"] as? Map<*, *> ?: emptyMap<Any, Any>()
                    val transport = mutableMapOf<String, Any?>("type" to "xhttp")
                    xhttpSettings["mode"]?.let { transport["mode"] = it.toString() }
                    xhttpSettings["path"]?.let { transport["path"] = it.toString() }
                    xhttpSettings["host"]?.let { transport["host"] = it.toString() }
                    xhttpSettings["headers"]?.let { transport["headers"] = it }
                    xhttpSettings["scMaxEachPostBytes"]?.let { transport["sc_max_each_post_bytes"] = (it as? Number)?.toInt() ?: it.toString().toIntOrNull() }
                    xhttpSettings["xPaddingBytes"]?.let { transport["x_padding_bytes"] = it.toString() }
                    xhttpSettings["downloadSettings"]?.let { transport["download_settings"] = it }
                    transport.putIfAbsent("x_padding_bytes", "100-1000")
                    result["transport"] = transport
                }
                "httpupgrade" -> {
                    val settings = stream["httpupgradeSettings"] as? Map<*, *> ?: emptyMap<Any, Any>()
                    val transport = mutableMapOf<String, Any?>("type" to "httpupgrade")
                    settings["path"]?.let { transport["path"] = it.toString() }
                    settings["host"]?.let { transport["host"] = it.toString() }
                    settings["headers"]?.let { transport["headers"] = it }
                    result["transport"] = transport
                }
            }
        }
    }

    private fun mapToJsonString(map: Map<String, Any?>): String {
        val obj = toJsonSafe(map)
        return if (obj is JSONObject) obj.toString(2) else JSONObject().toString(2)
    }

    private fun toJsonSafe(value: Any?): Any? {
        if (value == null) return null
        return when (value) {
            is Number -> {
                val d = value.toDouble()
                if (d.isInfinite() || d.isNaN()) null else value
            }
            is Map<*, *> -> {
                val jsonObj = JSONObject()
                for ((k, v) in value) {
                    if (k != null) {
                        val safeV = toJsonSafe(v)
                        if (safeV != null && safeV != JSONObject.NULL) {
                            jsonObj.put(k.toString(), safeV)
                        }
                    }
                }
                jsonObj
            }
            is List<*> -> {
                val jsonArr = JSONArray()
                for (item in value) {
                    val safeItem = toJsonSafe(item)
                    if (safeItem != null && safeItem != JSONObject.NULL) {
                        jsonArr.put(safeItem)
                    }
                }
                jsonArr
            }
            is Array<*> -> {
                val jsonArr = JSONArray()
                for (item in value) {
                    val safeItem = toJsonSafe(item)
                    if (safeItem != null && safeItem != JSONObject.NULL) {
                        jsonArr.put(safeItem)
                    }
                }
                jsonArr
            }
            is String -> {
                if (value.equals("Infinity", ignoreCase = true) || value.equals("-Infinity", ignoreCase = true) || value.equals("NaN", ignoreCase = true)) {
                    null
                } else value
            }
            is Boolean -> value
            else -> value.toString()
        }
    }
}
