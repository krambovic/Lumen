package com.lumen.core.config.builder

import com.lumen.core.config.normalizer.AmneziaWGNormalizer
import com.lumen.core.config.parser.LinkParser
import com.lumen.core.config.parser.ParsedNode
import org.json.JSONArray
import org.json.JSONObject
import java.util.Base64

data class SingboxConfigOptions(
    val tunMode: Boolean = true,
    val tunAddressIPv4: String = "172.19.0.1/30",
    val tunAddressIPv6: String = "fdfe:dcba:9876::1/126",
    val tunMtu: Int = 9000,
    val tunStrictRoute: Boolean = true,
    val tunStack: String = "mixed",
    val localSocksPort: Int = 10808,
    val localHttpPort: Int = 10809,
    val allowLanConnections: Boolean = false,
    val inboundUdpTimeoutSeconds: Int = 0,
    val multiplexEnabled: Boolean = false,
    val multiplexConcurrency: Int = 8,
    val multiplexProtocol: String = "smux",
    val multiplexMinStreams: Int = 4,
    val multiplexPadding: Boolean = true,
    val multiplexBrutalEnabled: Boolean = false,
    val multiplexBrutalUpMbps: Int = 0,
    val multiplexBrutalDownMbps: Int = 0,
    val outboundTcpFastOpen: Boolean = false,
    val outboundTcpMultiPath: Boolean = false,
    val outboundUdpFragment: Boolean = false,
    val outboundConnectTimeoutSeconds: Int = 0,
    val udpOverTcp: Boolean = false,
    val enableFinalFragment: Boolean = false,
    val tlsFragmentFallbackDelayMs: Int = 500,
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
    /**
     * Resolvers of the physical network, as the OS reports them. Used only for
     * `dns-system`, the literal bootstrap resolver every other DNS server is
     * resolved through. Empty keeps the hard-coded public fallback.
     */
    val systemDnsServers: List<String> = emptyList(),
    val dnsProxyServers: List<String> = listOf("cloudflare-dns.com", "dns.google"),
    val dnsDirectType: String = "udp",
    val dnsProxyType: String = "https",
    val dnsDirectStrategy: String = "ipv4_only",
    val dnsProxyStrategy: String = "ipv4_only",
    val dnsHijackEnabled: Boolean = true,
    val dnsFakeIpEnabled: Boolean = false,
    val dnsFakeIpRangeIPv4: String = "198.18.0.0/15",
    val dnsFakeIpRangeIPv6: String = "fc00::/18",
    val dnsParallelQuery: Boolean = false,
    val dnsOptimisticCache: Boolean = false,
    val dnsIndependentCache: Boolean = true,
    val dnsDisableCache: Boolean = false,
    val dnsCacheCapacity: Int = 0,
    val dnsClientSubnet: String = "",
    val dnsDirectRuleStrategy: String = "",
    val dnsGeoCheck: Boolean = true,
    val dnsProxyIpv4Only: Boolean = true,
    val dnsHosts: Map<String, List<String>> = emptyMap(),
    val dnsOverrideEnabled: Boolean = true,
    val dnsOverrideHostname: String = "ntc.party",
    val dnsOverrideIpv4: String = "130.255.77.28",
    val domainResolverStrategy: String = "",
    val sniffTimeoutMs: Int = 0,
    val sniffers: List<String> = emptyList(),
    val logLevel: String = "info",
    val urlTestUrl: String = "https://www.gstatic.com/generate_204",
    val urlTestIntervalMinutes: Int = 3,
    val urlTestToleranceMs: Int = 50,
    val urlTestIdleTimeoutMinutes: Int = 0,
    val urlTestInterruptExistConnections: Boolean = true,
    val bypassLan: Boolean = false,
    val cacheFileEnabled: Boolean = false,
    val cacheFilePath: String = "",
    val ntpEnabled: Boolean = false,
    val ntpServer: String = "time.apple.com",
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

    /**
     * Loopback port of the in-app obfs2/obfs3 pluggable-transport relay. The
     * relay speaks SOCKS5 to the core and obfs to the bridge, so no external
     * obfsproxy binary has to be shipped.
     */
    const val OBFS_LOCAL_PORT = 10871

    fun buildConfig(
        nodes: List<ParsedNode>,
        selectedNode: ParsedNode?,
        options: SingboxConfigOptions = SingboxConfigOptions()
    ): String {
        val root = mutableMapOf<String, Any?>()

        // 1. Log
        root["log"] = mapOf(
            "level" to (options.logLevel.takeIf { it in LOG_LEVELS } ?: "info"),
            "timestamp" to true
        )

        // 2. Inbounds (TUN + optional local SOCKS/HTTP). The shipped app always
        // passes tunMode = false: VpnService owns the tun device and hev
        // tun2socks feeds the local SOCKS inbound. The tun branch (and tunMtu)
        // only exists for tests and desktop-shaped configs.
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

        // "Allow connections from LAN" only widens the bind address; tun2socks
        // keeps reaching the same inbound over loopback either way.
        val listenAddress = if (options.allowLanConnections) "0.0.0.0" else "127.0.0.1"

        if (options.localSocksPort > 0) {
            val socksIn = mutableMapOf<String, Any?>(
                "type" to "socks",
                "tag" to "socks-in",
                "listen" to listenAddress,
                "listen_port" to options.localSocksPort,
                "udp_fragment" to true
            )
            // How long an idle UDP association is kept. The core's own default
            // (5m) is left untouched while the user has not chosen a value.
            if (options.inboundUdpTimeoutSeconds > 0) {
                socksIn["udp_timeout"] = "${options.inboundUdpTimeoutSeconds.coerceIn(1, 3600)}s"
            }
            inbounds.add(socksIn)
        }

        if (options.localHttpPort > 0 && options.localHttpPort != options.localSocksPort) {
            inbounds.add(
                mapOf(
                    "type" to "http",
                    "tag" to "http-in",
                    "listen" to listenAddress,
                    "listen_port" to options.localHttpPort
                )
            )
        }

        root["inbounds"] = inbounds

        // 3. Outbounds
        val outbounds = mutableListOf<Map<String, Any?>>()
        val activeNode = selectedNode ?: nodes.firstOrNull()
            ?: throw IllegalArgumentException("No server selected")

        run {
            val activeType = activeNode.outbound["type"]?.toString()?.lowercase()
            if (activeNode.scheme.equals("auto", true) || activeType == "urltest" || activeType == "selector") {
                // Auto Virtual Node: an imported AUTO group carries its own pool of
                // servers; a manually created one falls back to every other server.
                val poolNodes = LinkParser.autoMembers(activeNode.outbound)
                    .ifEmpty { nodes.filterNot { it.scheme.equals("auto", true) } }
                val poolTags = mutableListOf<String>()
                val failures = mutableListOf<String>()

                for (poolNode in poolNodes) {
                    val tag = "proxy-${poolTags.size}"
                    // One broken server must not invalidate the whole auto pool.
                    val ob = try {
                        buildOutboundMap(poolNode, tag, options)
                    } catch (e: Exception) {
                        failures += "${poolNode.name.ifBlank { poolNode.server }}: ${e.message ?: "invalid config"}"
                        null
                    }
                    if (ob == null || ob["type"] == null) continue
                    outbounds.add(ob)
                    poolTags.add(tag)
                }

                if (poolTags.isEmpty()) {
                    val details = failures.take(3).joinToString("; ")
                    throw IllegalArgumentException(
                        "AUTO `${activeNode.name}` has no usable servers" +
                            if (details.isBlank()) "" else ": $details"
                    )
                }
                val autoOutbound = mutableMapOf<String, Any?>(
                    "type" to "urltest",
                    "tag" to "proxy",
                    "outbounds" to poolTags,
                    "url" to options.urlTestUrl,
                    "interval" to "${options.urlTestIntervalMinutes.coerceIn(1, 1440)}m",
                    "tolerance" to options.urlTestToleranceMs.coerceIn(0, 5000),
                    "interrupt_exist_connections" to options.urlTestInterruptExistConnections
                )
                // How long an unused member keeps being probed. On a 300-member
                // pool the periodic sweep is the dominant cost, so a short idle
                // timeout is the knob that actually helps.
                if (options.urlTestIdleTimeoutMinutes > 0) {
                    autoOutbound["idle_timeout"] = "${options.urlTestIdleTimeoutMinutes.coerceIn(1, 1440)}m"
                }
                outbounds.add(0, autoOutbound)
            } else {
                // Single node
                val ob = buildOutboundMap(activeNode, "proxy", options)
                outbounds.add(ob)
            }
        }

        // Auxiliary Outbounds. Note: the legacy special "block" outbound was
        // removed in sing-box 1.13; blocking is done via `action: reject` rules.
        outbounds.add(mapOf("type" to "direct", "tag" to "direct"))

        // "Use proxy" for OpenVPN: the parser stores the selection under
        // `lumen_proxy`, which the core does not know. Turn it into a real detour
        // outbound and strip the marker key. obfs2/obfs3 are terminated by the
        // in-app relay, so their detour points at loopback.
        val detourOutbounds = mutableListOf<Map<String, Any?>>()
        outbounds.forEachIndexed { index, ob ->
            val proxy = ob["lumen_proxy"] as? Map<*, *> ?: return@forEachIndexed
            val cleaned = ob.toMutableMap()
            cleaned.remove("lumen_proxy")
            val detourTag = "ovpn-proxy-$index"
            cleaned["detour"] = detourTag
            outbounds[index] = cleaned
            val type = proxy["type"]?.toString().orEmpty()
            val detour = mutableMapOf<String, Any?>("tag" to detourTag)
            if (type == "http" || type == "socks") {
                detour["type"] = type
                detour["server"] = proxy["server"]?.toString().orEmpty()
                detour["server_port"] = (proxy["server_port"] as? Number)?.toInt()
                    ?: proxy["server_port"]?.toString()?.toIntOrNull() ?: 0
                proxy["username"]?.toString()?.takeIf { it.isNotBlank() }?.let { detour["username"] = it }
                proxy["password"]?.toString()?.takeIf { it.isNotBlank() }?.let { detour["password"] = it }
            } else {
                detour["type"] = "socks"
                detour["server"] = "127.0.0.1"
                detour["server_port"] = OBFS_LOCAL_PORT
            }
            detourOutbounds.add(detour)
        }
        outbounds.addAll(detourOutbounds)

        // Final masque sweep. buildOutboundMap only sanitizes the outbounds it
        // builds itself, so masque nodes that arrive from the database, from an
        // AUTO pool member, or from a raw imported config could still carry
        // `profile.server` and abort the whole config with
        // `outbounds[N].profile.server: json: unknown field "server"`.
        for (index in outbounds.indices) {
            if (outbounds[index]["type"]?.toString()?.lowercase() != "masque") continue
            val sanitized = outbounds[index].toMutableMap()
            sanitizeMasqueOutbound(sanitized)
            outbounds[index] = sanitized
        }

        root["outbounds"] = outbounds

        // 4. User Routing Rules (Domains, Geosite, IP CIDRs). Classified before
        // the DNS section because the direct-DNS rule is built from the result:
        // a `proxy:`/`block:` entry or a geosite:/geoip: code must never end up
        // in the direct resolver rule.
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

        // 5. DNS. Keep the same three-stage contract as desktop Lumen:
        // system -> bootstrap/direct -> proxied resolver. In particular, a
        // hostname-based DoH resolver must never try to resolve itself through
        // the proxy resolver, otherwise sing-box repeatedly reports
        // `dns exchange failed` even though the transport connected.
        val dohHostByIp = mapOf(
            "1.1.1.1" to "cloudflare-dns.com", "1.0.0.1" to "cloudflare-dns.com",
            "8.8.8.8" to "dns.google", "8.8.4.4" to "dns.google",
            "9.9.9.9" to "dns.quad9.net", "149.112.112.112" to "dns.quad9.net"
        )
        val directServers = options.dnsDirectServers.map(String::trim).filter(String::isNotEmpty)
            .ifEmpty { listOf(options.directDnsServer.ifBlank { "1.1.1.1" }) }
        val proxyServers = options.dnsProxyServers.map(String::trim).filter(String::isNotEmpty)
            .ifEmpty { listOf(options.proxyDnsServer.ifBlank { "cloudflare-dns.com" }) }
        fun dnsServer(
            tag: String,
            raw: String,
            type: String,
            detour: String,
            resolver: String
        ): Map<String, Any?> {
            val normalizedType = type.lowercase().takeIf { it in setOf("udp", "tcp", "tls", "https") } ?: "udp"
            val server = if (normalizedType in setOf("tls", "https")) dohHostByIp[raw] ?: raw else raw
            return mutableMapOf<String, Any?>(
                "type" to normalizedType, "tag" to tag, "server" to server,
                "detour" to detour
            ).apply {
                if (normalizedType == "https") put("path", "/dns-query")
                if (normalizedType == "https") put("server_port", 443)
                if (normalizedType == "tls") put("server_port", 853)
                if (normalizedType in setOf("udp", "tcp")) put("server_port", 53)
                // A hex IPv6 literal contains letters too, so the letter test alone
                // would ask the core to resolve an address. dns-system is the one
                // server with no resolver behind it, and an empty domain_resolver is
                // a field the core ignores, so it is dropped rather than emitted.
                if (resolver.isNotEmpty() && server.any { it.isLetter() }) {
                    put("domain_resolver", resolver)
                }
            }
        }
        val dnsServers = mutableListOf<Map<String, Any?>>()
        // A literal emergency resolver guarantees that even a custom
        // hostname-based bootstrap server can be reached without recursion.
        // Desktop Lumen feeds the physical adapter's own resolvers in here
        // (runtime_planner `system_dns_servers`); match that, because a carrier
        // that blocks or hijacks UDP/53 to 1.1.1.1 otherwise leaves this server
        // unable to resolve anything - including the tunnel endpoint's hostname.
        // Only a usable IP literal qualifies: dns-system has no resolver of its
        // own, so a hostname here would have nothing to resolve it, and a
        // link-local address arrives without the scope id that would make it
        // dialable.
        val systemServer = options.systemDnsServers
            .map(String::trim)
            .firstOrNull(::isBootstrapCapableAddress)
            ?: "1.1.1.1"
        dnsServers += dnsServer("dns-system", systemServer, "udp", "direct", "")
        dnsServers += dnsServer("dns-bootstrap", directServers.first(), options.dnsDirectType, "direct", "dns-system")
        directServers.forEachIndexed { index, server ->
            dnsServers += dnsServer("dns-direct-${index + 1}", server, options.dnsDirectType, "direct", "dns-system")
        }
        proxyServers.forEachIndexed { index, server ->
            dnsServers += dnsServer("dns-proxy-${index + 1}", server, options.dnsProxyType, "proxy", "dns-bootstrap")
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
                "inet4_range" to cidrOrDefault(options.dnsFakeIpRangeIPv4, "198.18.0.0/15"),
                "inet6_range" to cidrOrDefault(options.dnsFakeIpRangeIPv6, "fc00::/18")
            )
        }
        val dnsRules = mutableListOf<Map<String, Any?>>(
            mapOf("query_type" to listOf("HTTPS", "SVCB"), "action" to "reject")
        )
        if (hosts.isNotEmpty()) dnsRules.add(0, mapOf("ip_accept_any" to true, "server" to "dns-hosts"))
        // Rejecting AAAA would make "prefer IPv6" unsatisfiable, so the user's
        // explicit IPv6 preference wins over the IPv4-only shortcut.
        if (options.dnsProxyIpv4Only && !options.preferIpv6 && options.dnsMode != "json") {
            dnsRules += mapOf("query_type" to listOf("AAAA"), "action" to "reject")
        }
        // Direct-listed domains must resolve through direct DNS: proxy-resolved
        // IPs would otherwise be routed direct and break geo-based access.
        val directDnsDomains = directDomains
            .map { it.trim().trimEnd('.').lowercase() }
            .filter { it.isNotEmpty() && !it.contains('/') && it.any { c -> c.isLetter() } }
        if (options.dnsGeoCheck && directDnsDomains.isNotEmpty()) {
            val directRule = mutableMapOf<String, Any?>(
                "domain_suffix" to directDnsDomains, "server" to "dns-direct-1"
            )
            // Per-rule query strategy. `unknown domain strategy` aborts the whole
            // config, so an unrecognised value is dropped rather than emitted.
            options.dnsDirectRuleStrategy.trim().lowercase().takeIf { it in DOMAIN_STRATEGIES }
                ?.let { directRule["strategy"] = it }
            dnsRules += directRule
        }
        // FakeIP must be reached through a rule: the core refuses to start with
        // `default server cannot be fakeip` when dns.final points at it.
        if (options.dnsFakeIpEnabled) {
            dnsRules += mapOf("query_type" to listOf("A", "AAAA"), "server" to "dns-fake")
        }
        // DNS pushed by the profile itself (OpenVPN dhcp-option DNS, WireGuard DNS=).
        val profileDns = (activeNode.outbound["_dns"] as? List<*>)
            ?.mapNotNull { it?.toString()?.trim() }?.filter { it.isNotEmpty() } ?: emptyList()
        // Provider profiles often contain public UDP resolvers. Many VLESS,
        // VMess and Trojan transports do not relay UDP/53, so replacing a
        // working DoH resolver with that hint makes a connected VPN unusable.
        // Desktop Lumen only lets a private, tunnel-internal DNS override the
        // configured proxy resolver; preserve that policy on Android.
        val privateProfileDns = profileDns.filter(::isPrivateDnsAddress)
        privateProfileDns.forEachIndexed { index, server ->
            dnsServers += dnsServer("dns-vpn-${index + 1}", server, "udp", "proxy", "dns-bootstrap")
        }
        // The `DNS =` line of a WireGuard/AmneziaWG profile (and OpenVPN's
        // dhcp-option DNS) is frequently a placeholder that nothing actually
        // answers on inside the tunnel. As a bare dns.final it has no fallback,
        // so every single lookup burns the exchange deadline and the core spams
        // `dns: exchange failed ... context deadline exceeded` while the tunnel
        // itself is perfectly healthy. Race it against the configured proxy
        // resolver instead - both run through `proxy`, so nothing leaks and the
        // exit stays the same. Verified against core/sing-box-lumen.exe: with a
        // reachable profile DNS the answer comes from it in 2ms, with a dead one
        // the query completes in ~1s, where dns.final=dns-vpn-1 failed after 10s.
        // The fallback server resolves its members by tag as the list is
        // initialized, so it must stay after every server it names.
        if (privateProfileDns.isNotEmpty()) {
            dnsServers += mapOf(
                "type" to "fallback",
                "tag" to "dns-vpn-final",
                "servers" to (privateProfileDns.indices.map { "dns-vpn-${it + 1}" } + "dns-proxy-1"),
                "strategy" to "parallel"
            )
        }
        val useAndroidDns = options.dnsMode.lowercase() in setOf("android", "system")
        val resolveStrategy = if (options.preferIpv6) {
            "prefer_ipv6"
        } else if (useAndroidDns) options.dnsDirectStrategy else options.dnsProxyStrategy
        val dnsMap = mutableMapOf<String, Any?>(
            "servers" to dnsServers,
            "rules" to dnsRules,
            "final" to if (useAndroidDns) "dns-direct-1" else if (privateProfileDns.isNotEmpty()) "dns-vpn-final" else "dns-proxy-1",
            "strategy" to resolveStrategy,
            "independent_cache" to options.dnsIndependentCache,
            "reverse_mapping" to true,
            // Optimistic cache = serve stale answers while refreshing.
            "disable_expire" to options.dnsOptimisticCache,
            "cache_capacity" to (options.dnsCacheCapacity.takeIf { it > 0 }?.coerceIn(64, 65536)
                ?: if (options.dnsParallelQuery) 4096 else 2048)
        )
        if (options.dnsDisableCache) dnsMap["disable_cache"] = true
        // EDNS Client Subnet. The core parses this with netip and aborts the whole
        // config on anything it cannot read, so an unusable value is dropped here.
        clientSubnetOrNull(options.dnsClientSubnet)?.let { dnsMap["client_subnet"] = it }
        root["dns"] = dnsMap

        // 6. Route (sing-box 1.13 rule actions)
        val routeRules = mutableListOf<Map<String, Any?>>(
            mapOf("ip_cidr" to listOf("224.0.0.0/3", "ff00::/8"), "action" to "reject")
        )
        // "Bypass LAN": appended, not inserted, so every explicit user rule above
        // still wins over the blanket private-range shortcut.
        if (options.bypassLan) {
            routeRules += mapOf("ip_is_private" to true, "outbound" to "direct")
        }
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
        // "Block QUIC" must reject QUIC, not UDP/443. Without the protocol matcher
        // the rule kills every datagram on 443 - WhatsApp/Signal/FaceTime media,
        // DTLS/WebRTC and DNS-over-QUIC all die while TCP keeps working, which is
        // what gets reported as "UDP does not work". The sniff rule above runs
        // first, so `protocol: quic` is already resolved here (verified against
        // core/sing-box-lumen.exe: `match[1] network=udp protocol=quic port=443
        // => reject` for a real QUIC client hello, plain UDP/443 passes).
        if (options.blockQuic) {
            routeRules.add(
                0,
                mapOf("network" to "udp", "protocol" to "quic", "port" to 443, "action" to "reject")
            )
        }
        // TLS fragmentation. The extended core splits the ClientHello itself via
        // a non-terminal route-options rule; it has no equivalent of Xray's
        // packets/length knobs (the route-options decoder is lenient and would
        // swallow them silently). Only the fallback delay is a real field.
        if (options.enableFinalFragment) {
            routeRules.add(0, mapOf(
                "protocol" to listOf("tls"),
                "action" to "route-options",
                "tls_fragment" to true,
                "tls_fragment_fallback_delay" to
                    "${options.tlsFragmentFallbackDelayMs.coerceIn(0, 10000)}ms"
            ))
        }
        // sing-box 1.12+: the `protocol: dns` matcher only works after traffic
        // has been sniffed, and the legacy inbound-level sniff fields were
        // removed. Without these rules DNS from apps leaks as raw UDP:53
        // through the proxy (which many vless/vmess/trojan servers do not
        // relay), so nothing resolves even though the tunnel is up. Keep them
        // strictly first so DNS is always answered by the built-in resolver.
        val managedDns = options.dnsHijackEnabled && options.dnsMode.lowercase() !in setOf("android", "system")
        if (managedDns) {
            routeRules.add(0, mapOf(
                "type" to "logical",
                "mode" to "or",
                "rules" to listOf(
                    mapOf("protocol" to "dns"),
                    mapOf("port" to 53)
                ),
                "action" to "hijack-dns"
            ))
        } else {
            routeRules.add(0, mapOf("port" to 53, "outbound" to "direct"))
        }
        val sniffRule = mutableMapOf<String, Any?>("action" to "sniff")
        // An unknown sniffer name aborts the router, so only known ones survive.
        options.sniffers.map { it.trim().lowercase() }.filter { it in SNIFFERS }.distinct()
            .takeIf { it.isNotEmpty() }
            ?.let { sniffRule["sniffer"] = it }
        if (options.sniffTimeoutMs > 0) {
            sniffRule["timeout"] = "${options.sniffTimeoutMs.coerceIn(1, 60000)}ms"
        }
        routeRules.add(0, sniffRule)
        val routeMap = mutableMapOf<String, Any?>(
            // Android's app sandbox forbids sing-box's netlink network monitor.
            // SOCKS-only mode relies on VpnService + tun2socks and the OS default route.
            "auto_detect_interface" to options.tunMode,
            // Replaces legacy per-outbound domain_strategy (removed in 1.14).
            "default_domain_resolver" to mapOf(
                "server" to "dns-bootstrap",
                "strategy" to (options.domainResolverStrategy.trim().lowercase()
                    .takeIf { it in DOMAIN_STRATEGIES }
                    ?: if (options.preferIpv6) "prefer_ipv6" else options.dnsDirectStrategy)
            ),
            "rules" to routeRules,
            "final" to "proxy"
        )
        if (ruleSets.isNotEmpty()) {
            routeMap["rule_set"] = ruleSets
        }
        root["route"] = routeMap

        // 7. Persistent cache. Keeps the urltest verdicts and the downloaded
        // geosite/geoip rule-sets across restarts, which is what makes a
        // 300-member AUTO pool usable on the second connect.
        if (options.cacheFileEnabled) {
            val cacheFile = mutableMapOf<String, Any?>("enabled" to true)
            options.cacheFilePath.trim().takeIf(String::isNotEmpty)?.let { cacheFile["path"] = it }
            if (options.dnsFakeIpEnabled) cacheFile["store_fakeip"] = true
            root["experimental"] = mapOf("cache_file" to cacheFile)
        }

        // 8. NTP. A skewed device clock makes every TLS/REALITY handshake fail;
        // the core then uses this time source instead of the system one.
        if (options.ntpEnabled) {
            val ntpServer = options.ntpServer.trim().ifEmpty { "time.apple.com" }
            root["ntp"] = mutableMapOf<String, Any?>(
                "enabled" to true,
                "server" to ntpServer,
                "server_port" to 123,
                "interval" to "30m",
                "detour" to "direct"
            ).apply {
                if (ntpServer.any(Char::isLetter)) put("domain_resolver", "dns-bootstrap")
            }
        }

        // 9. Normalize AmneziaWG and WireGuard outbounds to extended endpoints format
        AmneziaWGNormalizer.normalizeSingboxWireguardEndpoints(root)
        applyBootstrapResolvers(root)
        applyDialOptions(root, options)

        // Clean non-singbox metadata keys from outbounds and endpoints
        (root["outbounds"] as? List<*>)?.let { obs ->
            root["outbounds"] = obs.mapNotNull { if (it is Map<*, *>) cleanSingboxOutbound(it as Map<String, Any?>) else it }
        }
        (root["endpoints"] as? List<*>)?.let { eps ->
            root["endpoints"] = eps.mapNotNull { if (it is Map<*, *>) cleanSingboxOutbound(it as Map<String, Any?>) else it }
        }

        return mapToJsonString(root)
    }

    private val LOG_LEVELS =
        setOf("trace", "debug", "info", "warning", "error", "fatal", "panic", "none")

    private val DOMAIN_STRATEGIES =
        setOf("prefer_ipv4", "prefer_ipv6", "ipv4_only", "ipv6_only")

    private val MULTIPLEX_PROTOCOLS = setOf("smux", "yamux", "h2mux")

    private val SNIFFERS = setOf(
        "http", "tls", "quic", "dns", "stun", "bittorrent", "dtls", "ssh", "rdp", "ntp"
    )

    // Dial options are meaningless on the built-in outbounds and rejected on
    // group outbounds, so they are only written to real transports.
    private val DIAL_EXCLUDED_TYPES = setOf("direct", "block", "selector", "urltest")

    /**
     * The core parses these with netip and aborts the whole config on anything it
     * cannot read, so a user-supplied prefix is validated before it is emitted.
     */
    private fun isIpPrefix(value: String): Boolean {
        val host = value.substringBefore('/')
        val length = value.substringAfter('/', "")
        if (host.isEmpty()) return false
        val bits = if (host.contains(':')) 128 else 32
        if (length.isNotEmpty() && (length.toIntOrNull() ?: -1) !in 0..bits) return false
        if (bits == 128) {
            // Only hex and colons, so InetAddress parses the literal without DNS.
            if (!host.all { it.isDigit() || it in "abcdefABCDEF:" }) return false
            return runCatching { java.net.InetAddress.getByName(host) }.isSuccess
        }
        val parts = host.split('.')
        return parts.size == 4 && parts.all { (it.toIntOrNull() ?: -1) in 0..255 }
    }

    private fun cidrOrDefault(value: String, fallback: String): String {
        val trimmed = value.trim()
        return if (trimmed.contains('/') && isIpPrefix(trimmed)) trimmed else fallback
    }

    private fun clientSubnetOrNull(value: String): String? =
        value.trim().takeIf { it.isNotEmpty() && isIpPrefix(it) }

    /**
     * Per-outbound dial tuning. Applied as a sweep over the finished document so
     * natively imported outbounds and AUTO pool members are covered the same way
     * as the ones this builder assembles itself.
     */
    private fun applyDialOptions(root: MutableMap<String, Any?>, options: SingboxConfigOptions) {
        val connectTimeout = options.outboundConnectTimeoutSeconds.takeIf { it > 0 }?.coerceAtMost(600)
        if (!options.outboundTcpFastOpen && !options.outboundTcpMultiPath &&
            !options.outboundUdpFragment && connectTimeout == null && !options.udpOverTcp
        ) {
            return
        }

        fun withDial(raw: Map<*, *>): Map<String, Any?> {
            @Suppress("UNCHECKED_CAST")
            val result = (raw as Map<String, Any?>).toMutableMap()
            val type = result["type"]?.toString()?.trim()?.lowercase().orEmpty()
            if (type in DIAL_EXCLUDED_TYPES) return result
            // `tcp_fast_open is not supported with anytls outbound` is fatal.
            if (options.outboundTcpFastOpen && type != "anytls") result["tcp_fast_open"] = true
            if (options.outboundTcpMultiPath) result["tcp_multi_path"] = true
            if (options.outboundUdpFragment) result["udp_fragment"] = true
            connectTimeout?.let { result["connect_timeout"] = "${it}s" }
            // shadowsocks is the only type carrying udp_over_tcp in this core.
            if (options.udpOverTcp && type == "shadowsocks") {
                result["udp_over_tcp"] = mapOf("enabled" to true, "version" to 2)
            }
            return result
        }

        for (key in listOf("outbounds", "endpoints")) {
            (root[key] as? List<*>)?.let { values ->
                root[key] = values.map { value -> if (value is Map<*, *>) withDial(value) else value }
            }
        }
    }

    private fun applyBootstrapResolvers(root: MutableMap<String, Any?>) {
        fun isDomain(value: String): Boolean {
            val host = value.trim().removePrefix("[").removeSuffix("]")
            if (host.isEmpty() || host.contains(':')) return false
            return host.any(Char::isLetter)
        }

        fun withResolver(raw: Map<*, *>): Map<String, Any?> {
            @Suppress("UNCHECKED_CAST")
            val result = (raw as Map<String, Any?>).toMutableMap()
            val type = result["type"]?.toString()?.trim()?.lowercase().orEmpty()
            if (type in setOf("direct", "block", "selector", "urltest")) return result
            val server = result["server"]?.toString().orEmpty()
            val peerDomain = (result["peers"] as? List<*>)
                ?.asSequence()
                ?.filterIsInstance<Map<*, *>>()
                ?.mapNotNull { it["address"]?.toString() }
                ?.firstOrNull(::isDomain)
            if ((isDomain(server) || peerDomain != null) && result["domain_resolver"] == null) {
                result["domain_resolver"] = "dns-bootstrap"
            }
            return result
        }

        (root["outbounds"] as? List<*>)?.let { values ->
            root["outbounds"] = values.map { value ->
                if (value is Map<*, *>) withResolver(value) else value
            }
        }
        (root["endpoints"] as? List<*>)?.let { values ->
            root["endpoints"] = values.map { value ->
                if (value is Map<*, *>) withResolver(value) else value
            }
        }
    }

    /**
     * Whether an OS-reported resolver can serve as `dns-system`, the one server in
     * the chain that is resolved by nothing. It must therefore be an IP literal, and
     * it must be dialable on its own: a link-local address reaches us with its scope
     * id already stripped, and an unspecified/loopback address points nowhere useful.
     */
    private fun isBootstrapCapableAddress(value: String): Boolean {
        val host = value.trim().removePrefix("[").removeSuffix("]")
        if (host.isEmpty() || host.contains('/')) return false
        if (!isIpPrefix(host)) return false
        val ipv4 = host.split('.').mapNotNull(String::toIntOrNull)
        if (ipv4.size == 4) {
            return ipv4[0] != 0 && ipv4[0] != 127 && !(ipv4[0] == 169 && ipv4[1] == 254)
        }
        val lower = host.lowercase()
        return lower != "::" && lower != "::1" &&
            !lower.startsWith("fe8") && !lower.startsWith("fe9") &&
            !lower.startsWith("fea") && !lower.startsWith("feb")
    }

    private fun isPrivateDnsAddress(value: String): Boolean {
        val host = value.trim().removePrefix("[").removeSuffix("]")
        val ipv4 = host.split('.').mapNotNull(String::toIntOrNull)
        if (ipv4.size == 4 && ipv4.all { it in 0..255 }) {
            return ipv4[0] == 10 ||
                (ipv4[0] == 172 && ipv4[1] in 16..31) ||
                (ipv4[0] == 192 && ipv4[1] == 168) ||
                ipv4[0] == 127 ||
                (ipv4[0] == 169 && ipv4[1] == 254)
        }
        val lower = host.lowercase()
        return lower == "::1" ||
            lower.startsWith("fc") ||
            lower.startsWith("fd") ||
            lower.startsWith("fe8") ||
            lower.startsWith("fe9") ||
            lower.startsWith("fea") ||
            lower.startsWith("feb")
    }

    private fun cleanSingboxOutbound(raw: Map<String, Any?>): Map<String, Any?> {
        val nonSingboxKeys = mutableSetOf(
            "config_str", "original_link", "happ_id", "happ_token", "display_protocol",
            "protocol", "singbox", "streamSettings", "settings", "v", "ps", "add", "scy",
            "net", "host", "path", "user", "pass", "clash", "_dns", "warp",
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

        // Subscription generators commonly export uTLS-Go enum names such as
        // HelloChrome_120. sing-box accepts only stable family names and rejects
        // the whole AUTO pool when even one member keeps a legacy value.
        val type = result["type"]?.toString()?.trim()?.lowercase().orEmpty()
        if (type == "hysteria") {
            // Desktop Lumen applies the same compatibility defaults. Hysteria
            // v1 refuses to initialize when both the numeric and string speed
            // forms are absent.
            if (missingHysteriaSpeed(result, "up_mbps", "up")) {
                result["up_mbps"] = 50
            }
            if (missingHysteriaSpeed(result, "down_mbps", "down")) {
                result["down_mbps"] = 200
            }
        }
        val requiredTlsTypes = setOf(
            "trojan", "hysteria", "hysteria2", "tuic", "anytls", "naive", "shadowtls"
        )
        val flowRequiresTls = type == "vless" &&
            !result["flow"]?.toString()?.trim().isNullOrEmpty()
        val tlsRequired = type in requiredTlsTypes || flowRequiresTls
        val rawTls = mapValue(result["tls"])
        if (rawTls != null || tlsRequired) {
            val tls = (rawTls ?: emptyMap()).toMutableMap()
            if (tlsRequired) tls["enabled"] = true
            if (tls["server_name"]?.toString().isNullOrBlank()) {
                result["server"]?.toString()?.trim()?.takeIf(String::isNotEmpty)?.let {
                    tls["server_name"] = it
                }
            }
            // Natively imported reality outbounds often ship without uTLS; the
            // core refuses to initialize them ("uTLS is required by reality client").
            if (mapValue(tls["reality"]) != null && tls["utls"] == null) {
                tls["utls"] = mapOf("enabled" to true, "fingerprint" to "chrome")
            }
            mapValue(tls["utls"])?.let { rawUtls ->
                val utls = rawUtls.toMutableMap()
                val normalized = normalizeUtlsFingerprint(utls["fingerprint"]?.toString())
                if (normalized.isNotEmpty()) {
                    utls["enabled"] = true
                    utls["fingerprint"] = normalized
                } else {
                    // Empty fingerprint already means Chrome in sing-box. Keeping
                    // uTLS enabled preserves the imported profile's intent.
                    utls.remove("fingerprint")
                }
                tls["utls"] = utls
            }
            result["tls"] = tls
        }
        return result
    }

    private fun missingHysteriaSpeed(
        outbound: Map<String, Any?>,
        mbpsKey: String,
        textKey: String
    ): Boolean {
        val numeric = when (val value = outbound[mbpsKey]) {
            is Number -> value.toDouble()
            is String -> value.toDoubleOrNull() ?: 0.0
            else -> 0.0
        }
        return numeric <= 0.0 && outbound[textKey]?.toString()?.trim().isNullOrEmpty()
    }

    private fun mapValue(value: Any?): Map<String, Any?>? = when (value) {
        is Map<*, *> -> {
            @Suppress("UNCHECKED_CAST")
            value as Map<String, Any?>
        }
        is JSONObject -> LinkParser.jsonToMap(value)
        is String -> runCatching { LinkParser.jsonToMap(JSONObject(value)) }.getOrNull()
        else -> null
    }

    private fun normalizeUtlsFingerprint(value: String?): String {
        val compact = value.orEmpty()
            .trim()
            .lowercase()
            .replace(Regex("[^a-z0-9]+"), "")
            .removePrefix("hello")
        return when {
            compact.isEmpty() -> ""
            compact.startsWith("chrome") -> "chrome"
            compact.startsWith("firefox") -> "firefox"
            compact.startsWith("edge") -> "edge"
            compact.startsWith("safari") -> "safari"
            compact.startsWith("ios") -> "ios"
            compact.startsWith("android") -> "android"
            compact.startsWith("qq") -> "qq"
            compact.startsWith("360") -> "360"
            compact.startsWith("randomized") -> "randomized"
            compact.startsWith("random") -> "random"
            // Unknown aliases are safer as the documented Chrome default than
            // as a fatal value that prevents every AUTO member from starting.
            else -> "chrome"
        }
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
            normalizeShadowsocks2022(normalized)
            requireOpenVpnCredentials(normalized, node)
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
                // The core aborts the whole document on `unknown method:`, which
                // would take every healthy AUTO member down with this one node.
                // Throwing lets the per-member catch drop just this server.
                if (method.isBlank()) {
                    throw IllegalArgumentException("Shadowsocks node has no encryption method")
                }
                if (password.isBlank() && method.trim().lowercase() != "none") {
                    throw IllegalArgumentException("Shadowsocks node has no password")
                }
                result["method"] = method
                result["password"] = password
                // SIP003 obfuscation, when the source profile carries it.
                (servers?.get("plugin") ?: outbound["plugin"])?.toString()
                    ?.takeIf(String::isNotBlank)?.let { result["plugin"] = it }
                (servers?.get("plugin_opts") ?: outbound["plugin_opts"])?.toString()
                    ?.takeIf(String::isNotBlank)?.let { result["plugin_opts"] = it }
            }
            "hysteria2", "hy2" -> {
                result["type"] = "hysteria2"
                result["server"] = node.server
                result["server_port"] = node.port
                val password = outbound["password"]?.toString() ?: outbound["auth"]?.toString() ?: ""
                if (password.isNotEmpty()) {
                    result["password"] = password
                }
                // Hysteria v2 uses BBR while no speed is configured; inventing
                // up/down would silently pin it to Brutal at a fabricated rate.
                intValue(outbound["up_mbps"])?.let { result["up_mbps"] = it }
                intValue(outbound["down_mbps"])?.let { result["down_mbps"] = it }

                val obfs = outbound["obfs"]
                if (obfs is Map<*, *>) {
                    result["obfs"] = obfs
                }
                // hysteria2 always requires TLS in sing-box; without it the core
                // rejects the config (legacy stored nodes may lack a native block).
                // Seed it from the node so alpn/insecure/ca survive the rebuild.
                val tls = (mapValue(outbound["tls"]) ?: emptyMap()).toMutableMap()
                tls["enabled"] = true
                if (tls["server_name"]?.toString().isNullOrBlank()) {
                    tls["server_name"] = outbound["sni"]?.toString()?.takeIf { it.isNotEmpty() }
                        ?: outbound["server_name"]?.toString()?.takeIf { it.isNotEmpty() }
                        ?: node.server
                }
                (outbound["alpn"] as? List<*>)?.let { tls.putIfAbsent("alpn", it) }
                if (outbound["insecure"] == true || outbound["allowInsecure"] == true) {
                    tls["insecure"] = true
                }
                result["tls"] = tls
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
                // TUIC always requires TLS in sing-box; seed it from the node so
                // alpn/insecure/ca survive the rebuild.
                val tls = (mapValue(outbound["tls"]) ?: emptyMap()).toMutableMap()
                tls["enabled"] = true
                if (tls["server_name"]?.toString().isNullOrBlank()) {
                    tls["server_name"] = outbound["sni"]?.toString()?.takeIf { it.isNotEmpty() }
                        ?: outbound["server_name"]?.toString()?.takeIf { it.isNotEmpty() }
                        ?: node.server
                }
                (outbound["alpn"] as? List<*>)?.let { tls.putIfAbsent("alpn", it) }
                if (outbound["insecure"] == true || outbound["allowInsecure"] == true) {
                    tls["insecure"] = true
                }
                result["tls"] = tls
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
                    // Keep the "Use proxy" marker; the detour sweep consumes it later.
                    if (nativeMap["lumen_proxy"] == null) {
                        outbound["lumen_proxy"]?.let { nativeMap["lumen_proxy"] = it }
                    }
                    requireOpenVpnCredentials(nativeMap, node)
                    return nativeMap
                }
                result["type"] = "openvpn"
                result["system"] = false
                result["name"] = "openvpn0"
                result["servers"] = listOf(mapOf("server" to node.server, "server_port" to node.port))
                for (key in listOf(
                    "proto", "cipher", "auth", "tls", "tls_auth", "tls_crypt", "tls_crypt_v2",
                    "key_direction", "username", "password", "reconnect_delay",
                    "ping_interval", "ping_restart", "lumen_proxy"
                )) {
                    outbound[key]?.let { result[key] = it }
                }
            }
            "socks", "http" -> {
                result["type"] = scheme
                result["server"] = node.server
                result["server_port"] = node.port
                val settings = outbound["settings"] as? Map<*, *>
                val server = (settings?.get("servers") as? List<*>)
                    ?.firstOrNull() as? Map<*, *>
                val user = (server?.get("users") as? List<*>)
                    ?.firstOrNull() as? Map<*, *>
                val username = outbound["username"]?.toString()
                    ?: user?.get("username")?.toString()
                    ?: user?.get("user")?.toString()
                val password = outbound["password"]?.toString()
                    ?: user?.get("password")?.toString()
                    ?: user?.get("pass")?.toString()
                username?.takeIf(String::isNotEmpty)?.let { result["username"] = it }
                password?.takeIf(String::isNotEmpty)?.let { result["password"] = it }
                (outbound["tls"] as? Map<*, *>)?.let { result["tls"] = it }
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

        // The fallback branch above copies the provider's own "tag" over ours, which
        // leaves the AUTO pool referencing a member that does not exist and makes the
        // core abort with `dependency[proxy-N] not found for outbound[proxy]`.
        result["tag"] = tag

        requireOpenVpnCredentials(result, node)

        // smux is only supported by these protocols in sing-box; attaching it to
        // e.g. socks/http/hysteria2/tuic makes the core reject the whole config.
        if (options.multiplexEnabled && result["type"] in setOf("vless", "vmess", "trojan", "shadowsocks")) {
            val multiplexMap = mutableMapOf<String, Any?>(
                "enabled" to true,
                "protocol" to (options.multiplexProtocol.trim().lowercase()
                    .takeIf { it in MULTIPLEX_PROTOCOLS } ?: "smux"),
                "max_connections" to options.multiplexConcurrency,
                "min_streams" to options.multiplexMinStreams.coerceIn(0, 1024),
                "padding" to options.multiplexPadding
            )
            // TCP Brutal needs both directions; `brutal: invalid download speed`
            // aborts the config when only one of them is set.
            if (options.multiplexBrutalEnabled &&
                options.multiplexBrutalUpMbps > 0 && options.multiplexBrutalDownMbps > 0
            ) {
                multiplexMap["brutal"] = mapOf(
                    "enabled" to true,
                    "up_mbps" to options.multiplexBrutalUpMbps,
                    "down_mbps" to options.multiplexBrutalDownMbps
                )
            }
            result["multiplex"] = multiplexMap
        }

        if (result["type"] == "masque") {
            sanitizeMasqueOutbound(result)
        }
        normalizeShadowsocks2022(result)

        return result
    }

    /**
     * An OpenVPN profile carrying `auth-user-pass` cannot authenticate on its
     * certificates alone, so a node saved without a login only fails once the
     * core has already dialed the tunnel. Reject the member here instead, the
     * same way a shadowsocks node without an encryption method is rejected: an
     * AUTO pool then drops just this server rather than losing the whole config.
     * Certificate-only profiles ask for no credentials and are left alone.
     */
    private fun requireOpenVpnCredentials(outbound: Map<String, Any?>, node: ParsedNode) {
        if (outbound["type"]?.toString()?.lowercase() != "openvpn") return
        if (!LinkParser.openVpnRequiresUserAuth(node.link)) return
        val username = outbound["username"]?.toString()?.trim().orEmpty()
        val password = outbound["password"]?.toString()?.trim().orEmpty()
        if (username.isEmpty() || password.isEmpty()) {
            throw IllegalArgumentException("OpenVPN node requires a username and password")
        }
    }

    // Endpoint fields for Lumen's direct Clash/usque MASQUE mode. The bundled
    // core accepts these at the top level of the outbound and only there: inside
    // `profile` they abort decoding with `json: unknown field`. Dropping them
    // instead of hoisting leaves a direct node without an endpoint.
    private val MASQUE_DIRECT_ENDPOINT_KEYS =
        setOf("server", "server_port", "private_key", "public_key", "address", "mtu")

    // The only fields sing-box accepts inside `masque.profile`. Everything else
    // aborts config decoding with `json: unknown field "..."`.
    private val MASQUE_PROFILE_KEYS =
        setOf("detour", "id", "auth_token", "private_key", "recreate")

    // A node restored from the database carries placeholder `server=""` / `port=0`
    // columns. They must not shadow a real endpoint stored inside `profile`.
    private fun MutableMap<String, Any?>.hasMasqueValue(key: String): Boolean =
        when (val v = this[key]) {
            null -> false
            is String -> v.isNotBlank()
            is Number -> v.toInt() != 0
            is Collection<*> -> v.isNotEmpty()
            else -> true
        }

    private fun sanitizeMasqueOutbound(result: MutableMap<String, Any?>) {
        // `profile` may arrive as a Map (freshly parsed link), or as a JSONObject /
        // JSON string when the node was restored from the database. Normalize all
        // three shapes: silently skipping the non-Map ones used to leave legacy
        // endpoint fields in place, which made the core reject the whole config
        // with `outbounds[0].profile.server: json: unknown field "server"`.
        val rawProfile: Map<*, *>? = when (val raw = result["profile"]) {
            is Map<*, *> -> raw
            is JSONObject -> LinkParser.jsonToMap(raw)
            is String -> runCatching { LinkParser.jsonToMap(JSONObject(raw)) }.getOrNull()
            else -> null
        }
        val profile = mutableMapOf<String, Any?>("detour" to "direct")
        if (rawProfile != null) {
            for ((k, v) in rawProfile) {
                val key = k.toString()
                when {
                    // `private_key` is valid in both places; keep it where it arrived.
                    key in MASQUE_PROFILE_KEYS -> profile[key] = v
                    // Legacy nodes carry the endpoint inside `profile`. Hoist it back
                    // to the top level rather than losing it.
                    key == "port" -> if (!result.hasMasqueValue("server_port")) {
                        result["server_port"] = (v as? Number)?.toInt() ?: v
                    }
                    key in MASQUE_DIRECT_ENDPOINT_KEYS -> if (!result.hasMasqueValue(key)) {
                        result[key] = v
                    }
                    // Unknown keys are dropped on purpose: the core fails hard on them.
                }
            }
        }

        // `port` is not part of the MASQUE schema; the core only knows `server_port`.
        val portNum = (result.remove("port") as? Number)?.toInt()
        if (portNum != null && portNum > 0 && !result.hasMasqueValue("server_port")) {
            result["server_port"] = portNum
        }

        // Leftover placeholders would be decoded as a real (empty) endpoint.
        MASQUE_DIRECT_ENDPOINT_KEYS.filterNot { result.hasMasqueValue(it) }.forEach(result::remove)

        result["profile"] = profile
    }

    /**
     * Shadowsocks 2022 AES methods encode each PSK as Base64 and require an exact
     * 16/32-byte size. Some Xray subscriptions label 32-byte PSKs as AES-128.
     * Switch only when every key segment unambiguously matches the other AES size;
     * otherwise reject the member so a malformed node cannot abort an AUTO pool.
     */
    private fun normalizeShadowsocks2022(result: MutableMap<String, Any?>) {
        if (result["type"]?.toString()?.lowercase() != "shadowsocks") return
        val method = result["method"]?.toString()?.lowercase().orEmpty()
        val expected = when (method) {
            "2022-blake3-aes-128-gcm" -> 16
            "2022-blake3-aes-256-gcm", "2022-blake3-chacha20-poly1305" -> 32
            else -> return
        }
        val password = result["password"]?.toString().orEmpty()
        val sizes = password.split(':').map { part ->
            runCatching {
                val padded = part.trim().replace('-', '+').replace('_', '/') +
                    "=".repeat((4 - part.trim().length % 4) % 4)
                Base64.getDecoder().decode(padded).size
            }.getOrElse {
                throw IllegalArgumentException("invalid Base64 key for Shadowsocks 2022")
            }
        }
        if (sizes.isNotEmpty() && sizes.all { it == expected }) return
        val correctedMethod = when {
            method == "2022-blake3-aes-128-gcm" && sizes.all { it == 32 } ->
                "2022-blake3-aes-256-gcm"
            method == "2022-blake3-aes-256-gcm" && sizes.all { it == 16 } ->
                "2022-blake3-aes-128-gcm"
            else -> null
        }
        if (correctedMethod != null) {
            result["method"] = correctedMethod
            return
        }
        throw IllegalArgumentException(
            "invalid key length for $method: required $expected, got ${sizes.joinToString("/")}"
        )
    }

    /**
     * Xray/v2ray share links fold WebSocket early data into the path query
     * (`path=/proxy?ed=2048`). sing-box wants a clean `path` plus
     * `max_early_data`, and percent-encodes a literal '?' into the request
     * target (`GET /proxy%3Fed=2048`), which no server can match.
     */
    private fun applyTransportPath(
        transport: MutableMap<String, Any?>,
        rawPath: String,
        edSetting: Any?,
        supportsEarlyData: Boolean
    ) {
        transport["path"] = rawPath.substringBefore('?')
        if (!supportsEarlyData) return
        val fromQuery = rawPath.substringAfter('?', "")
            .split('&')
            .firstOrNull { it.startsWith("ed=", true) }
            ?.substringAfter('=')
        val earlyData = intValue(edSetting) ?: intValue(fromQuery) ?: return
        transport["max_early_data"] = earlyData
        transport["early_data_header_name"] = "Sec-WebSocket-Protocol"
    }

    private val KCP_INT_FIELDS = listOf(
        "mtu" to "mtu",
        "tti" to "tti",
        "uplinkCapacity" to "uplink_capacity",
        "downlinkCapacity" to "downlink_capacity",
        "readBufferSize" to "read_buffer_size",
        "writeBufferSize" to "write_buffer_size"
    )

    private fun toBoolValue(value: Any?): Boolean = when (value) {
        is Boolean -> value
        is Number -> value.toInt() != 0
        else -> value.toString().trim().lowercase() in setOf("1", "true", "yes", "on")
    }

    private fun intValue(value: Any?): Int? {
        val parsed = when (value) {
            is Number -> value.toInt()
            is String -> value.trim().toIntOrNull()
            else -> null
        }
        return parsed?.takeIf { it > 0 }
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
                // fp= is optional in vless share links, but the core aborts with
                // `uTLS is required by reality client` without a uTLS block.
                tls["utls"] = mapOf(
                    "enabled" to true,
                    "fingerprint" to normalizeUtlsFingerprint(realitySettings["fingerprint"]?.toString())
                        .ifEmpty { "chrome" }
                )
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
                    wsSettings["path"]?.let {
                        applyTransportPath(transport, it.toString(), wsSettings["ed"], true)
                    }
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
                "kcp", "mkcp" -> {
                    val kcpSettings = stream["kcpSettings"] as? Map<*, *> ?: emptyMap<Any, Any>()
                    val transport = mutableMapOf<String, Any?>("type" to "mkcp")
                    for ((source, target) in KCP_INT_FIELDS) {
                        intValue(kcpSettings[source])?.let { transport[target] = it }
                    }
                    kcpSettings["seed"]?.toString()?.takeIf { it.isNotEmpty() }?.let { transport["seed"] = it }
                    kcpSettings["congestion"]?.let { transport["congestion"] = toBoolValue(it) }
                    (kcpSettings["header"] as? Map<*, *>)?.get("type")?.toString()
                        ?.takeIf { it.isNotEmpty() }
                        ?.let { transport["header_type"] = it }
                    result["transport"] = transport
                }
                "httpupgrade" -> {
                    val settings = stream["httpupgradeSettings"] as? Map<*, *> ?: emptyMap<Any, Any>()
                    val transport = mutableMapOf<String, Any?>("type" to "httpupgrade")
                    // The httpupgrade transport has no early-data fields, so the
                    // query is only stripped, never translated.
                    settings["path"]?.let {
                        applyTransportPath(transport, it.toString(), settings["ed"], false)
                    }
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
