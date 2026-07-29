package com.lumen.core.config.builder

import com.lumen.core.config.normalizer.AmneziaWGNormalizer
import com.lumen.core.config.normalizer.OpenVpnConfigNormalizer
import com.lumen.core.config.parser.LinkParser
import com.lumen.core.config.parser.ParsedNode
import org.json.JSONArray
import org.json.JSONObject
import java.util.Base64

data class SingboxConfigOptions(
    val tunMode: Boolean = true,
    val tunAddressIPv4: String = "172.19.0.1/30",
    val tunAddressIPv6: String = "fdfe:dcba:9876::1/126",
    // 1500 is what every path between the phone and the server can carry; the old
    // 9000 default only worked for protocols that fragment for themselves and made
    // OpenVPN drop full-size packets.
    val tunMtu: Int = 1500,
    val tunStrictRoute: Boolean = true,
    val tunStack: String = "mixed",
    val localSocksPort: Int = 10808,
    val localHttpPort: Int = 10809,
    /** Per-runtime loopback port used by the in-app OpenVPN obfs relay. */
    val obfsLocalPort: Int = 10871,
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
    /** Exact sing-box-extended DNS section used only when [dnsMode] is `json`. */
    val dnsCustomJson: String = "",
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
    /**
     * Server hostname -> addresses the app resolved before the tunnel existed,
     * for outbounds whose hostname the local network answers with a sinkhole.
     * A hijacking carrier (Iran's 10.10.34.x block page) does not fail a query,
     * it answers it, so no fallback chain inside the core can ever step past a
     * poisoned reply - the core has no way to judge an answer bogus. The app
     * can: it compares resolvers and drops private/reserved addresses, then
     * pins what is left here. Only the matching outbounds are pointed at this
     * table; everything else keeps the normal bootstrap chain.
     */
    val pinnedServerIps: Map<String, List<String>> = emptyMap(),
    val dnsOverrideEnabled: Boolean = false,
    val dnsOverrideHostname: String = "",
    val dnsOverrideIpv4: String = "",
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
    /**
     * Provider selected on the Geo resources screen.  The UI historically downloaded
     * legacy .dat files while this builder silently used SagerNet .srs URLs, so choosing
     * the Iranian/Russian source had no effect on the configuration at all.
     */
    val geoResourceSource: String =
        "https://github.com/runetfreedom/russia-v2ray-rules-dat/",
    val directDomains: List<String> = emptyList(),
    val directIpCidrs: List<String> = emptyList(),
    /**
     * Socks5 authorization for the local inbound. While it is on, the local
     * SOCKS proxy only accepts these credentials and the plain HTTP inbound is
     * not published at all, so a LAN neighbour cannot use the tunnel.
     */
    val socksAuthEnabled: Boolean = false,
    val socksUsername: String = "",
    val socksPassword: String = "",
    /**
     * Directory holding rule sets the app already downloaded (`<tag>.srs`).
     * A tag found here is emitted as a `local` rule set, which removes the
     * remote fetch the core otherwise performs while it starts.
     */
    val geoRuleSetDir: String = "",
    /**
     * Drop a geo rule whose rule set is not in [geoRuleSetDir] instead of
     * emitting a `remote` one. A remote rule set the core cannot download is
     * fatal: it aborts startup, which left users with geo rules unable to
     * connect at all. The app sets this and downloads the sets itself.
     */
    val requireLocalRuleSets: Boolean = false
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

        // Socks5 authorization. Only complete credentials switch it on, so an
        // empty preference can never publish an inbound nothing can reach.
        val socksUsername = options.socksUsername.trim()
        val socksPassword = options.socksPassword.trim()
        val socksAuthActive =
            options.socksAuthEnabled && socksUsername.isNotEmpty() && socksPassword.isNotEmpty()

        if (options.localSocksPort > 0) {
            val socksIn = mutableMapOf<String, Any?>(
                "type" to "socks",
                "tag" to "socks-in",
                "listen" to listenAddress,
                "listen_port" to options.localSocksPort,
                "udp_fragment" to true
            )
            if (socksAuthActive) {
                socksIn["users"] = listOf(
                    mapOf("username" to socksUsername, "password" to socksPassword)
                )
            }
            // How long an idle UDP association is kept. The core's own default
            // (5m) is left untouched while the user has not chosen a value.
            if (options.inboundUdpTimeoutSeconds > 0) {
                socksIn["udp_timeout"] = "${options.inboundUdpTimeoutSeconds.coerceIn(1, 3600)}s"
            }
            inbounds.add(socksIn)
        }

        // The HTTP inbound has no credentials of its own here, so publishing it
        // next to an authenticated SOCKS inbound would reopen the same hole.
        if (!socksAuthActive &&
            options.localHttpPort > 0 && options.localHttpPort != options.localSocksPort
        ) {
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
        val dependencyOutbounds = mutableListOf<Map<String, Any?>>()
        val dependencyEndpoints = mutableListOf<Map<String, Any?>>()
        val activeNode = selectedNode ?: nodes.firstOrNull()
            ?: throw IllegalArgumentException("No server selected")
        val activeType = activeNode.outbound["type"]?.toString()?.lowercase()
        val isAuto = activeNode.scheme.equals("auto", true) ||
            activeType in setOf("urltest", "selector", "fallback", "failover", "bond")
        val activePoolNodes = if (isAuto) {
            LinkParser.autoMembers(activeNode.outbound)
                .ifEmpty { nodes.filterNot { it.scheme.equals("auto", true) } }
        } else {
            emptyList()
        }
        // Native composite configs retain their original dependency tags. Avoid
        // assigning the same proxy-N tag to an AUTO member: duplicate tags are
        // ambiguous in the core and can redirect a detour to the wrong outbound.
        val nativeDependencyTags = activePoolNodes.flatMap { poolNode ->
            val closure = mapValue(poolNode.outbound["_singbox_dependencies"])
            listOf("outbounds", "endpoints").flatMap { key ->
                (closure?.get(key) as? List<*>)
                    ?.mapNotNull(::mapValue)
                    ?.mapNotNull { it["tag"]?.toString()?.trim()?.takeIf(String::isNotEmpty) }
                    .orEmpty()
            }
        }.toSet()

        run {
            if (isAuto) {
                // Auto Virtual Node: an imported AUTO group carries its own pool of
                // servers; a manually created one falls back to every other server.
                val poolTags = mutableListOf<String>()
                val failures = mutableListOf<String>()

                for (poolNode in activePoolNodes) {
                    var tagIndex = poolTags.size
                    var tag = "proxy-$tagIndex"
                    while (tag in nativeDependencyTags || tag in poolTags) {
                        tag = "proxy-${++tagIndex}"
                    }
                    // One broken server must not invalidate the whole auto pool.
                    val ob = try {
                        buildOutboundMap(poolNode, tag, options)
                    } catch (e: Exception) {
                        failures += "${poolNode.name.ifBlank { poolNode.server }}: ${e.message ?: "invalid config"}"
                        null
                    }
                    if (ob == null || ob["type"] == null) continue
                    collectSingboxDependencies(
                        poolNode,
                        dependencyOutbounds,
                        dependencyEndpoints,
                        options
                    )
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
                outbounds.addAll(0, dependencyOutbounds)
                outbounds.add(0, autoOutbound)
            } else {
                // Single node
                val ob = buildOutboundMap(activeNode, "proxy", options)
                collectSingboxDependencies(
                    activeNode,
                    dependencyOutbounds,
                    dependencyEndpoints,
                    options
                )
                outbounds.addAll(dependencyOutbounds)
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
                detour["server_port"] =
                    options.obfsLocalPort.takeIf { it in 1..65535 } ?: OBFS_LOCAL_PORT
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
        if (dependencyEndpoints.isNotEmpty()) {
            root["endpoints"] = dependencyEndpoints
        }

        // 4. User Routing Rules (Domains, Geosite, IP CIDRs). Classified before
        // the DNS section because the direct-DNS rule is built from the result:
        // a `proxy:`/`block:` entry or a geosite:/geoip: code must never end up
        // in the direct resolver rule.
        data class DomainPattern(val field: String, val value: String)
        data class UserRouteRule(
            val kind: String,
            val field: String,
            val value: String,
            val action: String
        )
        val directDomains = mutableListOf<DomainPattern>()
        val orderedUserRules = mutableListOf<UserRouteRule>()

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
                isGeoip -> normalizeGeoCode(
                    "geoip",
                    rawPattern.substring("geoip:".length),
                    options.geoResourceSource
                )?.let { orderedUserRules += UserRouteRule("geoip", "rule_set", it, action) }
                isGeosite -> normalizeGeoCode(
                    "geosite",
                    rawPattern.substring("geosite:".length),
                    options.geoResourceSource
                )?.let {
                    orderedUserRules += UserRouteRule("geosite", "rule_set", it, action)
                }
                isIp -> normalizedIpPrefixOrNull(rawPattern)?.let {
                    orderedUserRules += UserRouteRule("ip", "ip_cidr", it, action)
                }
                else -> {
                    // Match desktop Lumen's imported-rule vocabulary. A plain
                    // hostname is a suffix rule (the domain and its subdomains);
                    // `full:` is the explicit exact-match form.
                    val (field, value) = when {
                        rawPattern.startsWith("full:", true) ->
                            "domain" to rawPattern.substringAfter(':').trim()
                        rawPattern.startsWith("domain:", true) ->
                            "domain_suffix" to rawPattern.substringAfter(':').trim()
                        rawPattern.startsWith("keyword:", true) ->
                            "domain_keyword" to rawPattern.substringAfter(':').trim()
                        rawPattern.startsWith("regexp:", true) || rawPattern.startsWith("regex:", true) ->
                            "domain_regex" to rawPattern.substringAfter(':').trim()
                        else -> "domain_suffix" to rawPattern
                    }
                    val normalized = when (field) {
                        "domain_regex" -> value.takeIf(::isSafeDomainRegex)
                        "domain_keyword" -> value.trim().takeIf {
                            it.isNotEmpty() && it.length <= 253 && it.none(Char::isISOControl)
                        }
                        else -> normalizeDomainPattern(value)
                    } ?: return
                    val pattern = DomainPattern(field, normalized)
                    if (action == "direct") directDomains += pattern
                    orderedUserRules += UserRouteRule("domain", field, normalized, action)
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
        val dnsMode = options.dnsMode.trim().lowercase()
            .takeIf { it in setOf("automatic", "android", "system", "secure", "json") }
            ?: "automatic"
        val directServers = options.dnsDirectServers.map(String::trim).filter(String::isNotEmpty)
            .ifEmpty { listOf(options.directDnsServer.ifBlank { "1.1.1.1" }) }
        val proxyServers = options.dnsProxyServers.map(String::trim).filter(String::isNotEmpty)
            .ifEmpty { listOf(options.proxyDnsServer.ifBlank { "cloudflare-dns.com" }) }
        // "Secure" is a policy, not a label: its final resolvers must use an
        // authenticated encrypted transport even if an old preference still says UDP.
        val proxyDnsType = if (dnsMode == "secure" &&
            options.dnsProxyType.trim().lowercase() !in setOf("tls", "https")
        ) {
            "https"
        } else {
            options.dnsProxyType
        }
        fun dnsServer(
            tag: String,
            raw: String,
            type: String,
            detour: String,
            resolver: String
        ): Map<String, Any?> {
            val rawType = type.trim().lowercase()
            val parsedUri = raw.takeIf { "://" in it }?.let {
                runCatching { java.net.URI(it) }.getOrNull()
            }
            val rawAddress = raw.trim()
            val bracketEnd = rawAddress.takeIf { it.startsWith("[") }?.indexOf(']') ?: -1
            val unschemedHost = when {
                parsedUri != null -> null
                bracketEnd > 0 -> rawAddress.substring(1, bracketEnd)
                rawAddress.count { it == ':' } == 1 &&
                    rawAddress.substringAfterLast(':').toIntOrNull()
                        ?.let { it in 1..65535 } == true ->
                    rawAddress.substringBeforeLast(':')
                else -> rawAddress
            }
            val unschemedPort = when {
                parsedUri != null -> null
                bracketEnd > 0 && rawAddress.drop(bracketEnd + 1).startsWith(":") ->
                    rawAddress.drop(bracketEnd + 2).toIntOrNull()?.takeIf { it in 1..65535 }
                rawAddress.count { it == ':' } == 1 ->
                    rawAddress.substringAfterLast(':').toIntOrNull()?.takeIf { it in 1..65535 }
                else -> null
            }
            val normalizedType = when {
                rawType in setOf("udp", "tcp", "tls", "https") -> rawType
                parsedUri?.scheme?.lowercase() in setOf("udp", "tcp", "tls", "https") ->
                    parsedUri?.scheme?.lowercase().orEmpty()
                else -> "udp"
            }
            val rawHost = (parsedUri?.host ?: unschemedHost.orEmpty())
                .trim()
                .removePrefix("[")
                .removeSuffix("]")
            val server = if (normalizedType in setOf("tls", "https")) {
                dohHostByIp[rawHost] ?: rawHost
            } else {
                rawHost
            }
            require(server.isNotBlank()) { "DNS server `$tag` has no address" }
            // A resolver handed out by DHCP is often a private or CGNAT literal -
            // Iranian mobile carriers commonly hand out 172.19.0.2 and friends.
            // Such a box answers plain UDP/53 and nothing else: 853/443 are closed,
            // and no certificate can ever be valid for an RFC1918 address, so an
            // encrypted transport pointed at it can only ever time out. Every
            // lookup then burns the dial deadline and the tunnel never resolves
            // its own endpoint hostname, which looks exactly like "every server in
            // the subscription is dead" while the subscription itself imports fine
            // over the Android system resolver.
            val downgraded = normalizedType in setOf("tls", "https") &&
                isIpAddress(server) && isUnencryptableDnsAddress(server)
            val effectiveType = if (downgraded) "udp" else normalizedType
            return mutableMapOf<String, Any?>(
                "type" to effectiveType, "tag" to tag, "server" to server,
                "detour" to detour
            ).apply {
                if (effectiveType == "https") put(
                    "path",
                    parsedUri?.rawPath?.takeIf { it.isNotBlank() } ?: "/dns-query"
                )
                val defaultPort = when (effectiveType) {
                    "https" -> 443
                    "tls" -> 853
                    else -> 53
                }
                // An explicit port that only made sense for the encrypted
                // transport must not survive the downgrade to plain UDP.
                val explicitPort = (parsedUri?.port?.takeIf { it in 1..65535 } ?: unschemedPort)
                    ?.takeUnless { downgraded && (it == 853 || it == 443) }
                put("server_port", explicitPort ?: defaultPort)
                // A hex IPv6 literal contains letters too, so the letter test alone
                // would ask the core to resolve an address. dns-system is the one
                // server with no resolver behind it, and an empty domain_resolver is
                // a field the core ignores, so it is dropped rather than emitted.
                if (resolver.isNotEmpty() && !isIpAddress(server)) {
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
        val systemDnsTransport = dnsServer("dns-system", systemServer, "udp", "direct", "")
        dnsServers += systemDnsTransport
        // Well-known DoH/DoT providers are resolved from a static table instead of
        // being bootstrapped over plain UDP/53. The hostname still reaches TLS - a
        // bare IP has no valid certificate identity, which is exactly why desktop
        // Lumen rewrites IP -> hostname - but resolving that hostname no longer
        // depends on the local network allowing outbound port 53. A carrier that
        // blocks or hijacks UDP/53 previously left every proxy resolver unusable
        // while the tunnel itself was perfectly healthy.
        val dohIpsByHost = linkedMapOf(
            "cloudflare-dns.com" to listOf("1.1.1.1", "1.0.0.1"),
            "dns.google" to listOf("8.8.8.8", "8.8.4.4"),
            "dns.quad9.net" to listOf("9.9.9.9", "149.112.112.112")
        )
        // Matches the same value dnsServer() will end up dialing: a bare hostname,
        // a scheme-prefixed URL, or an IP that dohHostByIp rewrites to a hostname.
        fun knownDohHostOf(raw: String): String? {
            val value = raw.trim().lowercase().removeSuffix(".")
            if (value.isEmpty()) return null
            dohHostByIp[value]?.let { return it }
            return dohIpsByHost.keys.firstOrNull { host ->
                value == host || value.contains("//$host") ||
                    value.startsWith("$host:") || value.startsWith("$host/")
            }
        }
        val dohHostsResolverTag = "dns-doh-hosts"
        // The secure bootstrap resolves itself from the same table, so the table is
        // always emitted even when no proxy resolver needs it.
        val usedDohHosts = (proxyServers.mapNotNull(::knownDohHostOf) + SECURE_BOOTSTRAP_HOST).distinct()
        val dohHostsTransport = mapOf<String, Any?>(
            "type" to "hosts",
            "tag" to dohHostsResolverTag,
            "predefined" to usedDohHosts.associateWith { dohIpsByHost.getValue(it) }
        )
        // Plaintext UDP/53 is not merely blocked on hijacking carriers - it is
        // answered with a sinkhole address (Iran's 10.10.34.x block page being the
        // widespread case), and that happens no matter which resolver IP the query
        // is addressed to. Outbounds resolve their server hostname through this
        // bootstrap, so a single poisoned answer points the tunnel at the sinkhole
        // and every connection dies with an i/o timeout. DoH answers cannot be
        // forged, so it is tried first; the previous plaintext transport stays
        // behind it as a fallback, leaving networks that block DoH outright no
        // worse off than before.
        val bootstrapSecureTransport =
            dnsServer("dns-bootstrap-secure", SECURE_BOOTSTRAP_HOST, "https", "direct", dohHostsResolverTag)
        val bootstrapPlainTransport =
            dnsServer("dns-bootstrap-plain", directServers.first(), options.dnsDirectType, "direct", "dns-system")
        val bootstrapFallbackTransport = mapOf<String, Any?>(
            "type" to "fallback",
            "tag" to "dns-bootstrap",
            "servers" to listOf("dns-bootstrap-secure", "dns-bootstrap-plain"),
            "strategy" to "sequential"
        )
        // Ordered so that every tag is defined before it is referenced.
        val bootstrapTransports = listOf(
            dohHostsTransport,
            bootstrapSecureTransport,
            bootstrapPlainTransport,
            bootstrapFallbackTransport
        )
        dnsServers += bootstrapTransports
        // Addresses the app already resolved and vetted for outbound hostnames
        // the local network poisons. Kept as its own table rather than prepended
        // to the bootstrap chain: a `hosts` miss answers NXDOMAIN, which counts
        // as a successful reply, so a chain starting here would leave every
        // hostname it does not know unresolvable.
        val pinnedServerHosts = options.pinnedServerIps
            .mapNotNull { (rawHost, rawAddresses) ->
                val host = rawHost.trim().trimEnd('.').lowercase().takeIf(String::isNotEmpty)
                val addresses = rawAddresses.map(String::trim).filter(::isIpAddress).distinct()
                if (host != null && addresses.isNotEmpty()) host to addresses else null
            }
            .toMap()
        if (pinnedServerHosts.isNotEmpty()) {
            dnsServers += mapOf<String, Any?>(
                "type" to "hosts",
                "tag" to PINNED_SERVER_DNS_TAG,
                "predefined" to pinnedServerHosts
            )
        }
        directServers.forEachIndexed { index, server ->
            dnsServers += dnsServer("dns-direct-${index + 1}", server, options.dnsDirectType, "direct", "dns-system")
        }
        proxyServers.forEachIndexed { index, server ->
            // Anything the table does not know keeps the old bootstrap path.
            val resolver = if (knownDohHostOf(server) != null) dohHostsResolverTag else "dns-bootstrap"
            dnsServers += dnsServer("dns-proxy-${index + 1}", server, proxyDnsType, "proxy", resolver)
        }
        val fallbackStrategy = if (options.dnsParallelQuery) "parallel" else "sequential"
        dnsServers += mapOf(
            "type" to "fallback",
            "tag" to "dns-direct-final",
            "servers" to directServers.indices.map { "dns-direct-${it + 1}" },
            "strategy" to fallbackStrategy
        )
        dnsServers += mapOf(
            "type" to "fallback",
            "tag" to "dns-proxy-final",
            "servers" to proxyServers.indices.map { "dns-proxy-${it + 1}" },
            "strategy" to fallbackStrategy
        )
        val hosts = linkedMapOf<String, List<String>>()
        options.dnsHosts.forEach { (rawHost, rawAddresses) ->
            val host = normalizeDomainPattern(rawHost) ?: return@forEach
            val addresses = rawAddresses.map(String::trim).filter(::isIpAddress).distinct()
            if (addresses.isNotEmpty()) hosts[host] = addresses
        }
        if (options.dnsOverrideEnabled) {
            val host = normalizeDomainPattern(options.dnsOverrideHostname)
            val address = options.dnsOverrideIpv4.trim().takeIf(::isIpAddress)
            if (host != null && address != null) hosts[host] = listOf(address)
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
        // On an IPv6-only carrier (every resolver handed out by the network is an
        // IPv6 literal, typical for DNS64/NAT64 mobile networks) an AAAA reject
        // leaves the device with no usable answers at all. Only such networks are
        // exempted: with no resolver list, or with any IPv4 resolver present, the
        // IPv4-only shortcut behaves exactly as before.
        val networkResolvers = options.systemDnsServers.map { it.trim() }.filter(String::isNotEmpty)
        val ipv6OnlyNetwork = networkResolvers.isNotEmpty() && networkResolvers.all { it.contains(":") }
        // Rejecting AAAA would make "prefer IPv6" unsatisfiable, so the user's
        // explicit IPv6 preference wins over the IPv4-only shortcut.
        if (options.dnsProxyIpv4Only && !options.preferIpv6 && !ipv6OnlyNetwork && dnsMode != "json") {
            dnsRules += mapOf("query_type" to listOf("AAAA"), "action" to "reject")
        }
        // Direct-listed domains must resolve through direct DNS: proxy-resolved
        // IPs would otherwise be routed direct and break geo-based access.
        if (options.dnsGeoCheck && directDomains.isNotEmpty()) {
            directDomains.groupBy(DomainPattern::field).forEach { (field, patterns) ->
                val directRule = mutableMapOf<String, Any?>(
                    field to patterns.map { it.value.trim().trimEnd('.').lowercase() },
                    "server" to "dns-direct-final"
                )
                // Per-rule query strategy. `unknown domain strategy` aborts the whole
                // config, so an unrecognised value is dropped rather than emitted.
                options.dnsDirectRuleStrategy.trim().lowercase().takeIf { it in DOMAIN_STRATEGIES }
                    ?.let { directRule["strategy"] = it }
                dnsRules += directRule
            }
        }
        // FakeIP must be reached through a rule: the core refuses to start with
        // `default server cannot be fakeip` when dns.final points at it.
        if (options.dnsFakeIpEnabled) {
            dnsRules += mapOf("query_type" to listOf("A", "AAAA"), "server" to "dns-fake")
        }
        // DNS pushed by the profile itself (OpenVPN dhcp-option DNS, WireGuard DNS=).
        val profileDnsNodes = if (isAuto) activePoolNodes else listOf(activeNode)
        val profileDns = profileDnsNodes.flatMap { profileNode ->
            (profileNode.outbound["_dns"] as? List<*>)
                ?.mapNotNull { it?.toString()?.trim() }
                ?.filter(String::isNotEmpty)
                .orEmpty()
        }.distinct()
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
                "servers" to (privateProfileDns.indices.map { "dns-vpn-${it + 1}" } + "dns-proxy-final"),
                "strategy" to "parallel"
            )
        }
        val useAndroidDns = dnsMode in setOf("android", "system")
        val resolveStrategy = if (options.preferIpv6) {
            "prefer_ipv6"
        } else if (useAndroidDns) options.dnsDirectStrategy else options.dnsProxyStrategy
        val dnsMap = mutableMapOf<String, Any?>(
            "servers" to dnsServers,
            "rules" to dnsRules,
            "final" to if (useAndroidDns) {
                "dns-direct-final"
            } else if (privateProfileDns.isNotEmpty()) {
                "dns-vpn-final"
            } else {
                "dns-proxy-final"
            },
            "strategy" to resolveStrategy,
            "independent_cache" to options.dnsIndependentCache,
            "reverse_mapping" to true,
            "cache_capacity" to (options.dnsCacheCapacity.takeIf { it > 0 }?.coerceIn(64, 65536) ?: 2048)
        )
        // The exact extended core has no stale-while-revalidate switch.
        // `disable_expire` keeps stale records forever, so the old optimistic-cache
        // mapping was actively harmful and is deliberately not emitted.
        if (options.dnsDisableCache) dnsMap["disable_cache"] = true
        // EDNS Client Subnet. The core parses this with netip and aborts the whole
        // config on anything it cannot read, so an unusable value is dropped here.
        clientSubnetOrNull(options.dnsClientSubnet)?.let { dnsMap["client_subnet"] = it }
        root["dns"] = if (dnsMode == "json") {
            buildCustomDnsMap(
                options.dnsCustomJson,
                systemDnsTransport,
                bootstrapTransports,
                options
            )
        } else {
            dnsMap
        }

        // 6. Route (sing-box 1.13 rule actions). The old implementation repeatedly
        // inserted at index zero, reversing the user's order and making direct rules
        // beat explicit rejects. System rules stay first; user rejects are then
        // fail-closed, while every remaining user rule keeps its original order.
        val routeRules = mutableListOf<Map<String, Any?>>()
        val sniffRule = mutableMapOf<String, Any?>("action" to "sniff")
        // An unknown sniffer name aborts the router, so only known ones survive.
        options.sniffers.map { it.trim().lowercase() }.filter { it in SNIFFERS }.distinct()
            .takeIf { it.isNotEmpty() }
            ?.let { sniffRule["sniffer"] = it }
        if (options.sniffTimeoutMs > 0) {
            sniffRule["timeout"] = "${options.sniffTimeoutMs.coerceIn(1, 60000)}ms"
        }
        routeRules += sniffRule

        // sing-box 1.12+: the `protocol: dns` matcher only works after sniffing.
        val managedDns = options.dnsHijackEnabled && dnsMode !in setOf("android", "system")
        if (managedDns) {
            routeRules += mapOf(
                "type" to "logical",
                "mode" to "or",
                "rules" to listOf(mapOf("protocol" to "dns"), mapOf("port" to 53)),
                "action" to "hijack-dns"
            )
        } else {
            routeRules += mapOf("port" to 53, "outbound" to "direct")
        }

        // The protocol matcher is populated by the sniff rule immediately above.
        if (options.blockQuic) {
            routeRules += mapOf(
                "network" to "udp",
                "protocol" to "quic",
                "port" to 443,
                "action" to "reject"
            )
        }

        // Non-terminal route-options must run before a user terminal rule.
        if (options.enableFinalFragment) {
            routeRules += mapOf(
                "protocol" to listOf("tls"),
                "action" to "route-options",
                "tls_fragment" to true,
                "tls_fragment_fallback_delay" to
                    "${options.tlsFragmentFallbackDelayMs.coerceIn(0, 10000)}ms"
            )
        }

        // sing-box 1.12+ removed legacy geosite/geoip keys; exact extended consumes
        // binary local/remote rule sets. One tag is shared by duplicate references.
        val ruleSets = mutableListOf<Map<String, Any?>>()
        val ruleSetDir = options.geoRuleSetDir.trim()
        // A rule set the app already downloaded is loaded from disk. This is what
        // keeps startup independent of GitHub: the core resolves a `remote` rule
        // set while it starts and a failed download is fatal, so a user with geo
        // rules could not connect at all on a network that blocks raw.github.com.
        fun localRuleSetPath(tag: String): String? {
            if (ruleSetDir.isEmpty()) return null
            return java.io.File(ruleSetDir, "$tag.srs")
                .takeIf { it.isFile && it.length() > 0L }
                ?.absolutePath
        }
        fun ensureRuleSet(kind: String, code: String): String? {
            val tag = "$kind-$code"
            if (ruleSets.none { it["tag"] == tag }) {
                val localPath = localRuleSetPath(tag)
                when {
                    localPath != null -> ruleSets += mapOf(
                        "type" to "local",
                        "tag" to tag,
                        "format" to "binary",
                        "path" to localPath
                    )
                    // Nothing on disk and the caller downloads the sets itself:
                    // drop the rule instead of risking a fatal remote fetch.
                    options.requireLocalRuleSets -> return null
                    else -> ruleSets += mapOf(
                        "type" to "remote",
                        "tag" to tag,
                        "format" to "binary",
                        "url" to ruleSetUrl(kind, code, options.geoResourceSource),
                        // GitHub is often blocked on the networks these rules target.
                        "download_detour" to "proxy"
                    )
                }
            }
            return tag
        }

        fun appendUserRule(item: UserRouteRule) {
            val rule = mutableMapOf<String, Any?>()
            when (item.kind) {
                "geosite", "geoip" -> {
                    if (item.kind == "geoip" && item.value == "private") {
                        // This matcher is built into sing-box. A remote
                        // geoip-private.srs is provider-specific and can 404.
                        rule["ip_is_private"] = true
                    } else {
                        // A dropped rule set means the rule cannot be expressed;
                        // skipping it keeps the config startable.
                        val tag = ensureRuleSet(item.kind, item.value) ?: return
                        rule["rule_set"] = listOf(tag)
                        if (item.kind == "geosite" && item.action == "direct" &&
                            options.dnsGeoCheck && dnsMode != "json"
                        ) {
                            val directDnsRule =
                                mapOf("rule_set" to listOf(tag), "server" to "dns-direct-final")
                            val fakeIpIndex = dnsRules.indexOfFirst { it["server"] == "dns-fake" }
                            if (fakeIpIndex >= 0) {
                                dnsRules.add(fakeIpIndex, directDnsRule)
                            } else {
                                dnsRules.add(directDnsRule)
                            }
                        }
                    }
                }
                "ip" -> rule["ip_cidr"] = listOf(item.value)
                else -> rule[item.field] = listOf(item.value)
            }
            if (item.action == "block") rule["action"] = "reject"
            else rule["outbound"] = item.action
            routeRules += rule
        }

        orderedUserRules.filter { it.action == "block" }.forEach(::appendUserRule)
        orderedUserRules.filterNot { it.action == "block" }.forEach(::appendUserRule)

        // Explicit user rules above retain priority over the broad LAN shortcut.
        if (options.bypassLan) {
            routeRules += mapOf("ip_is_private" to true, "outbound" to "direct")
            routeRules += mapOf(
                "ip_cidr" to listOf("224.0.0.0/4", "ff00::/8"),
                "outbound" to "direct"
            )
        } else {
            routeRules += mapOf(
                "ip_cidr" to listOf("224.0.0.0/4", "ff00::/8"),
                "action" to "reject"
            )
        }
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
            val allConfiguredTypes = buildList {
                outbounds.mapNotNullTo(this) { it["type"]?.toString()?.trim()?.lowercase() }
                dependencyEndpoints.mapNotNullTo(this) {
                    it["type"]?.toString()?.trim()?.lowercase()
                }
            }
            if ("warp" in allConfiguredTypes) cacheFile["store_warp_config"] = true
            if ("masque" in allConfiguredTypes) cacheFile["store_masque_config"] = true
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
        applyBootstrapResolvers(root, pinnedServerHosts.keys)
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

    private val GEO_CODE_PATTERN = Regex("[a-z0-9][a-z0-9_.@-]{0,79}")
    private val ISO_COUNTRY_CODES =
        java.util.Locale.getISOCountries().map(String::lowercase).toSet()
    private val EXTRA_GEOIP_CODES =
        setOf("private", "cloudflare", "cloudfront", "google", "netflix", "telegram", "tor")

    /**
     * Rejects malformed/incompatible codes before a remote rule-set can turn a
     * harmless typo into a fatal core startup. Chocolate4U publishes the Iranian
     * pair only; other categories must use their canonical provider.
     */
    private fun normalizeGeoCode(kind: String, raw: String, source: String): String? {
        val code = raw.trim().lowercase()
        if (!code.matches(GEO_CODE_PATTERN)) return null
        if ("chocolate4u" in source.trim().lowercase() && code != "ir") return null
        if (kind == "geoip" && code !in ISO_COUNTRY_CODES && code !in EXTRA_GEOIP_CODES) {
            return null
        }
        return code
    }

    private fun isIpAddress(value: String): Boolean {
        val host = value.trim().removePrefix("[").removeSuffix("]")
        return host.isNotEmpty() && !host.contains('/') && isIpPrefix(host)
    }

    private fun normalizedIpPrefixOrNull(value: String): String? {
        val trimmed = value.trim()
        if (!isIpPrefix(trimmed)) return null
        if ('/' in trimmed) return trimmed
        return "$trimmed/${if (':' in trimmed) 128 else 32}"
    }

    private fun normalizeDomainPattern(value: String): String? {
        val trimmed = value.trim().trimEnd('.').removePrefix("*.")
        if (trimmed.isEmpty() || trimmed.length > 253 || trimmed.any(Char::isISOControl) ||
            trimmed.any(Char::isWhitespace)
        ) {
            return null
        }
        return runCatching {
            java.net.IDN.toASCII(trimmed, java.net.IDN.ALLOW_UNASSIGNED).lowercase()
        }.getOrNull()?.takeIf { it.isNotEmpty() && it.length <= 253 }
    }

    /**
     * Go's regexp engine is RE2. Java accepts look-behind/back-references that RE2
     * later rejects, so validate both syntax and the common unsupported constructs.
     */
    private fun isSafeDomainRegex(value: String): Boolean {
        val pattern = value.trim()
        if (pattern.isEmpty() || pattern.length > 1024 || pattern.any(Char::isISOControl)) return false
        val unsupported = listOf(
            "(?<=", "(?<!", "(?>", "(?(", "\\k<", "++", "*+", "?+"
        )
        if (unsupported.any(pattern::contains) ||
            Regex("""\\[1-9]""").containsMatchIn(pattern) ||
            Regex("""\{\d+(?:,\d*)?}\+""").containsMatchIn(pattern)
        ) {
            return false
        }
        return runCatching { java.util.regex.Pattern.compile(pattern) }.isSuccess
    }

    /**
     * sing-box 1.12+ consumes one binary rule-set per category.  These layouts are
     * the providers' published layouts (and match desktop Lumen/v2rayN), not their
     * legacy release assets:
     *
     *  - Chocolate4U: Iran-sing-box-rules/rule-set/{tag}.srs
     *  - runetfreedom: release/sing-box/rule-set-{kind}/{tag}.srs
     *  - default: runetfreedom (the Android UI's default and the only listed
     *    provider that also publishes special sets such as geoip-cloudflare)
     *  - other/custom: SagerNet's canonical country rule sets
     */
    // Public: the app downloads exactly these .srs files ahead of time so the core
    // never has to fetch a rule set while it is starting.
    fun ruleSetUrl(kind: String, code: String, source: String): String {
        val tag = "$kind-$code"
        val normalized = source.trim().lowercase()
        return when {
            kind == "geosite" && code == "category-ads-all" ->
                "https://raw.githubusercontent.com/runetfreedom/russia-v2ray-rules-dat/release/" +
                    "sing-box/rule-set-geosite/geosite-category-ads-all.srs"
            "chocolate4u" in normalized ->
                "https://raw.githubusercontent.com/Chocolate4U/Iran-sing-box-rules/rule-set/$tag.srs"
            "runetfreedom" in normalized || normalized.isEmpty() ||
                (kind == "geoip" && code in EXTRA_GEOIP_CODES) -> {
                val runetTag = if (kind == "geosite" && code == "ru") {
                    // runetfreedom publishes the Russian aggregate under this
                    // domain-list-community tag; geosite-ru.srs is a guaranteed 404.
                    "geosite-category-ru"
                } else {
                    tag
                }
                "https://raw.githubusercontent.com/runetfreedom/russia-v2ray-rules-dat/release/" +
                    "sing-box/rule-set-$kind/$runetTag.srs"
            }
            else ->
                "https://raw.githubusercontent.com/SagerNet/sing-$kind/rule-set/$tag.srs"
        }
    }

    // Dial options are meaningless on the built-in outbounds and rejected on
    // group outbounds, so they are only written to real transports.
    private val DIAL_EXCLUDED_TYPES =
        setOf("direct", "block", "selector", "urltest", "fallback", "failover", "bond")

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

    private val CUSTOM_DNS_ROOT_KEYS = setOf(
        "servers", "rules", "final", "reverse_mapping", "strategy",
        "disable_cache", "disable_expire", "independent_cache",
        "cache_capacity", "client_subnet"
    )
    private val CUSTOM_DNS_SERVER_TYPES = setOf(
        "udp", "tcp", "tls", "https", "quic", "h3", "local", "hosts",
        "fakeip", "dhcp", "tailscale", "sdns", "fallback"
    )
    /** Provider used by the encrypted bootstrap; must be a key of the DoH host table. */
    private const val SECURE_BOOTSTRAP_HOST = "cloudflare-dns.com"

    /** Static table of outbound hostnames the app resolved before the tunnel existed. */
    private const val PINNED_SERVER_DNS_TAG = "dns-pinned-servers"

    /**
     * Tags the builder always emits itself. Custom DNS JSON may neither redefine
     * them nor collide with them: dns-bootstrap is a fallback over the encrypted
     * and plaintext transports, and both of those - plus the static host table the
     * encrypted one resolves itself from - have to survive into JSON mode too.
     */
    private val RESERVED_DNS_TAGS = setOf(
        "dns-system", "dns-bootstrap", "dns-bootstrap-secure", "dns-bootstrap-plain", "dns-doh-hosts"
    )
    private val DNS_TAG_PATTERN = Regex("[A-Za-z0-9_.@-]{1,96}")

    /**
     * Builds the exact `dns` object supplied in JSON mode while retaining the two
     * bootstrap transports used by outbound/domain resolution. The core's own
     * `check` remains the final schema authority; these checks prevent known fatal
     * references and unsafe infinite-cache settings from reaching it.
     */
    private fun buildCustomDnsMap(
        rawJson: String,
        systemTransport: Map<String, Any?>,
        bootstrapTransports: List<Map<String, Any?>>,
        options: SingboxConfigOptions
    ): MutableMap<String, Any?> {
        val text = rawJson.trim()
        require(text.isNotEmpty()) { "Custom DNS JSON is empty" }
        val custom = runCatching { LinkParser.jsonToMap(JSONObject(text)) }
            .getOrElse { throw IllegalArgumentException("Invalid custom DNS JSON: ${it.message}") }
        val unknownRootKeys = custom.keys - CUSTOM_DNS_ROOT_KEYS
        require(unknownRootKeys.isEmpty()) {
            "Unsupported custom DNS field(s): ${unknownRootKeys.sorted().joinToString()}"
        }
        require(custom["disable_expire"] != true) {
            "Custom DNS disable_expire is unsafe: it keeps stale records indefinitely"
        }

        val rawServers = custom["servers"] as? List<*>
            ?: throw IllegalArgumentException("Custom DNS JSON must contain a servers array")
        require(rawServers.isNotEmpty()) { "Custom DNS servers array is empty" }

        val servers = mutableListOf<Map<String, Any?>>(systemTransport)
        servers += bootstrapTransports
        val knownTags = RESERVED_DNS_TAGS.toMutableSet()
        val typesByTag = mutableMapOf("dns-system" to "udp")
        bootstrapTransports.forEach { transport ->
            val tag = transport["tag"]?.toString()?.trim().orEmpty()
            if (tag.isNotEmpty()) typesByTag[tag] = transport["type"]?.toString().orEmpty()
        }

        rawServers.forEachIndexed { index, raw ->
            val server = mapValue(raw)?.toMutableMap()
                ?: throw IllegalArgumentException("Custom DNS server[$index] is not an object")
            val type = server["type"]?.toString()?.trim()?.lowercase().orEmpty()
            require(type in CUSTOM_DNS_SERVER_TYPES) {
                "Unsupported custom DNS server type `${type.ifEmpty { "(empty)" }}` at index $index"
            }
            val tag = server["tag"]?.toString()?.trim().orEmpty()
            require(tag.matches(DNS_TAG_PATTERN)) {
                "Custom DNS server[$index] has an invalid or missing tag"
            }
            require(tag !in knownTags) { "Duplicate or reserved custom DNS tag `$tag`" }
            server["type"] = type
            server["tag"] = tag

            if (type == "fallback") {
                val members = (server["servers"] as? List<*>)
                    ?.mapNotNull { it?.toString()?.trim()?.takeIf(String::isNotEmpty) }
                    .orEmpty()
                require(members.isNotEmpty()) { "Custom DNS fallback `$tag` has no servers" }
                val missing = members.filterNot(knownTags::contains)
                require(missing.isEmpty()) {
                    "Custom DNS fallback `$tag` references unknown/later server(s): ${missing.joinToString()}"
                }
                val strategy = server["strategy"]?.toString()?.trim()?.lowercase().orEmpty()
                require(strategy in setOf("", "sequential", "parallel")) {
                    "Custom DNS fallback `$tag` has invalid strategy `$strategy`"
                }
                if (strategy.isEmpty()) server.remove("strategy") else server["strategy"] = strategy
            } else {
                val resolver = server["domain_resolver"]?.toString()?.trim()
                if (!resolver.isNullOrEmpty()) {
                    require(resolver in knownTags) {
                        "Custom DNS server `$tag` references unknown/later domain_resolver `$resolver`"
                    }
                }
                val address = server["server"]?.toString()?.trim().orEmpty()
                if (type in setOf("udp", "tcp", "tls", "https", "quic", "h3") &&
                    address.isNotEmpty() && !isIpAddress(address) && resolver.isNullOrEmpty()
                ) {
                    server["domain_resolver"] = "dns-bootstrap"
                }
            }

            knownTags += tag
            typesByTag[tag] = type
            servers += server
        }

        val rawRules = custom["rules"] ?: emptyList<Any?>()
        val rules = rawRules as? List<*>
            ?: throw IllegalArgumentException("Custom DNS rules must be an array")
        fun validateRuleReferences(value: Any?) {
            when (value) {
                is Map<*, *> -> {
                    value["server"]?.toString()?.trim()?.takeIf(String::isNotEmpty)?.let { server ->
                        require(server in knownTags) {
                            "Custom DNS rule references unknown server `$server`"
                        }
                    }
                    value.values.forEach(::validateRuleReferences)
                }
                is List<*> -> value.forEach(::validateRuleReferences)
            }
        }
        rules.forEach(::validateRuleReferences)

        val requestedFinal = custom["final"]?.toString()?.trim().orEmpty()
        val final = if (requestedFinal.isNotEmpty()) {
            require(requestedFinal in knownTags) {
                "Custom DNS final references unknown server `$requestedFinal`"
            }
            requestedFinal
        } else {
            typesByTag.entries.firstOrNull {
                it.key !in RESERVED_DNS_TAGS && it.value !in setOf("hosts", "fakeip")
            }?.key ?: throw IllegalArgumentException("Custom DNS has no usable final server")
        }
        require(typesByTag[final] != "fakeip") {
            "Custom DNS final cannot reference a fakeip server"
        }

        val strategy = custom["strategy"]?.toString()?.trim()?.lowercase().orEmpty()
        require(strategy.isEmpty() || strategy in DOMAIN_STRATEGIES) {
            "Custom DNS has invalid strategy `$strategy`"
        }
        val capacity = when (val value = custom["cache_capacity"]) {
            null -> options.dnsCacheCapacity.takeIf { it > 0 }?.coerceIn(64, 65536) ?: 2048
            is Number -> value.toInt()
            else -> value.toString().toIntOrNull()
                ?: throw IllegalArgumentException("Custom DNS cache_capacity must be an integer")
        }
        require(capacity in 64..65536) {
            "Custom DNS cache_capacity must be between 64 and 65536"
        }

        val result = mutableMapOf<String, Any?>(
            "servers" to servers,
            "rules" to rules,
            "final" to final,
            "reverse_mapping" to (custom["reverse_mapping"] as? Boolean ?: true),
            "independent_cache" to
                (custom["independent_cache"] as? Boolean ?: options.dnsIndependentCache),
            "cache_capacity" to capacity
        )
        if (strategy.isNotEmpty()) result["strategy"] = strategy
        if ((custom["disable_cache"] as? Boolean) == true || options.dnsDisableCache) {
            result["disable_cache"] = true
        }
        custom["client_subnet"]?.toString()?.trim()?.takeIf(String::isNotEmpty)?.let { subnet ->
            result["client_subnet"] = clientSubnetOrNull(subnet)
                ?: throw IllegalArgumentException("Custom DNS client_subnet is invalid")
        }
        return result
    }

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

    private fun applyBootstrapResolvers(
        root: MutableMap<String, Any?>,
        pinnedHosts: Set<String> = emptySet()
    ) {
        fun isDomain(value: String): Boolean {
            val host = value.trim().removePrefix("[").removeSuffix("]")
            if (host.isEmpty() || host.contains(':')) return false
            return host.any(Char::isLetter)
        }

        fun withResolver(raw: Map<*, *>): Map<String, Any?> {
            @Suppress("UNCHECKED_CAST")
            val result = (raw as Map<String, Any?>).toMutableMap()
            val type = result["type"]?.toString()?.trim()?.lowercase().orEmpty()
            if (type in DIAL_EXCLUDED_TYPES) return result
            val server = result["server"]?.toString().orEmpty()
            val peerDomain = (result["peers"] as? List<*>)
                ?.asSequence()
                ?.filterIsInstance<Map<*, *>>()
                ?.mapNotNull { it["address"]?.toString() }
                ?.firstOrNull(::isDomain)
            if ((isDomain(server) || peerDomain != null) && result["domain_resolver"] == null) {
                // A hostname the app already resolved and vetted is answered from the
                // static table instead of the network, which is the only way past a
                // carrier that answers - rather than fails - with a sinkhole address.
                val host = (server.takeIf(::isDomain) ?: peerDomain.orEmpty())
                    .trim().trimEnd('.').lowercase()
                result["domain_resolver"] =
                    if (host in pinnedHosts) PINNED_SERVER_DNS_TAG else "dns-bootstrap"
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

    /**
     * Addresses that can never terminate an authenticated DNS transport: RFC1918,
     * loopback, link-local, IPv6 ULA (all covered by [isPrivateDnsAddress]) plus
     * CGNAT 100.64.0.0/10 and 0.0.0.0/8. Certificates cannot be issued for them
     * and carrier equipment listening there serves plain UDP/53 only.
     */
    private fun isUnencryptableDnsAddress(value: String): Boolean {
        if (isPrivateDnsAddress(value)) return true
        val host = value.trim().removePrefix("[").removeSuffix("]")
        val ipv4 = host.split('.').mapNotNull(String::toIntOrNull)
        if (ipv4.size != 4 || ipv4.any { it !in 0..255 }) return false
        return ipv4[0] == 0 || (ipv4[0] == 100 && ipv4[1] in 64..127)
    }

    private fun cleanSingboxOutbound(raw: Map<String, Any?>): Map<String, Any?> {
        val nonSingboxKeys = mutableSetOf(
            "config_str", "original_link", "happ_id", "happ_token", "display_protocol",
            "protocol", "singbox", "streamSettings", "settings", "v", "ps", "add", "scy",
            "net", "host", "path", "user", "pass", "clash", "_dns", "warp",
            "_singbox_dependencies",
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
            normalizeCertificatePublicKeyPins(tls)
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

    /**
     * Restores the dependency closure captured by the native sing-box JSON parser.
     * The parser emits dependency-first maps with their original tags; both managers
     * share the tag namespace, so duplicates and the app's reserved root tags are
     * skipped deterministically.
     */
    private fun collectSingboxDependencies(
        node: ParsedNode,
        outbounds: MutableList<Map<String, Any?>>,
        endpoints: MutableList<Map<String, Any?>>,
        options: SingboxConfigOptions
    ) {
        val closure = mapValue(node.outbound["_singbox_dependencies"]) ?: return
        val occupied = mutableSetOf("proxy", "direct")
        outbounds.mapNotNullTo(occupied) { it["tag"]?.toString()?.trim() }
        endpoints.mapNotNullTo(occupied) { it["tag"]?.toString()?.trim() }

        fun rawList(key: String): List<Map<String, Any?>> =
            (closure[key] as? List<*>)?.mapNotNull(::mapValue).orEmpty()

        for (raw in rawList("outbounds")) {
            val tag = raw["tag"]?.toString()?.trim().orEmpty()
            if (tag.isEmpty() || tag in occupied) continue
            val dependency = raw.toMutableMap()
            dependency["tag"] = tag
            val type = dependency["type"]?.toString()?.trim()?.lowercase().orEmpty()
            if (type.isEmpty()) continue
            occupied += tag
            dependency["type"] = type
            if (type == "masque") sanitizeMasqueOutbound(dependency)
            normalizeOpenVpnOutbound(dependency)
            normalizeShadowsocks2022(dependency)
            applyMultiplexOptions(dependency, options)
            outbounds += dependency
        }

        for (raw in rawList("endpoints")) {
            val tag = raw["tag"]?.toString()?.trim().orEmpty()
            if (tag.isEmpty() || tag in occupied) continue
            val dependency = raw.toMutableMap()
            dependency["tag"] = tag
            val type = dependency["type"]?.toString()?.trim()?.lowercase().orEmpty()
            if (type.isEmpty()) continue
            occupied += tag
            dependency["type"] = type
            endpoints += AmneziaWGNormalizer.normalizeWireGuardEndpoint(dependency)
        }
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

    /**
     * The core models certificate_public_key_sha256 as []byte and therefore expects
     * a Base64-encoded 32-byte SHA-256 digest. Accept the common `sha256/BASE64`
     * notation and 64-digit hex/colon fingerprints, then emit one canonical form.
     */
    private fun normalizeCertificatePublicKeyPins(tls: MutableMap<String, Any?>) {
        val raw = tls["certificate_public_key_sha256"] ?: return
        val values = when (raw) {
            is List<*> -> raw.mapNotNull { it?.toString() }
            else -> listOf(raw.toString())
        }.flatMap { it.split(',') }.map(String::trim).filter(String::isNotEmpty)

        val normalized = values.map { value ->
            val compact = value.removePrefix("sha256/").removePrefix("SHA256/")
            val hex = compact.replace(":", "").replace("-", "")
            val bytes = if (hex.length == 64 && hex.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' }) {
                ByteArray(32) { index -> hex.substring(index * 2, index * 2 + 2).toInt(16).toByte() }
            } else {
                runCatching { Base64.getDecoder().decode(compact) }
                    .recoverCatching { Base64.getUrlDecoder().decode(compact) }
                    .getOrElse {
                        throw IllegalArgumentException(
                            "Certificate public-key SHA-256 must be a Base64 or hex 32-byte digest"
                        )
                    }
            }
            require(bytes.size == 32) {
                "Certificate public-key SHA-256 must decode to 32 bytes"
            }
            Base64.getEncoder().encodeToString(bytes)
        }.distinct()

        if (normalized.isEmpty()) tls.remove("certificate_public_key_sha256")
        else tls["certificate_public_key_sha256"] = normalized
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
            normalizeOpenVpnOutbound(normalized)
            normalizeShadowsocks2022(normalized)
            applyMultiplexOptions(normalized, options)
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
                    val vlessStream = outbound["streamSettings"] as? Map<*, *> ?: emptyMap<String, Any?>()
                    val vlessNetwork = vlessStream["network"]?.toString()?.lowercase() ?: "tcp"
                    val vlessSecurity = vlessStream["security"]?.toString()?.lowercase() ?: ""
                    val flow = users?.get("flow")?.toString() ?: outbound["flow"]?.toString() ?: ""
                    // XTLS Vision exists only over raw TCP carrying TLS or REALITY.
                    // Copied onto ws/grpc/httpupgrade/xhttp it makes the core reject
                    // the whole outbound, so the hint is dropped instead of forwarded.
                    if (flow.isNotEmpty() &&
                        vlessNetwork in setOf("tcp", "raw") &&
                        vlessSecurity in setOf("tls", "reality")
                    ) {
                        result["flow"] = flow
                    }
                    // VLESS Encryption (mlkem768x25519plus/...). The parser keeps it on
                    // the user object; without forwarding it the client handshakes in
                    // the clear and the server drops the connection. "none" is the
                    // protocol default and stays implicit.
                    val encryption = users?.get("encryption")?.toString()
                        ?: outbound["encryption"]?.toString()
                    if (!encryption.isNullOrBlank() && encryption != "none") {
                        result["encryption"] = encryption
                    }
                    // Xray servers negotiate UDP through XUDP. The default differs
                    // between cores and forks, so it is stated rather than assumed:
                    // without it UDP dies silently while TCP keeps working.
                    result["packet_encoding"] = "xudp"
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
                    normalizeOpenVpnOutbound(nativeMap)
                    requireOpenVpnCredentials(nativeMap, node)
                    return nativeMap
                }
                result["type"] = "openvpn"
                result["system"] = false
                result["name"] = "openvpn0"
                result["servers"] = listOf(mapOf("server" to node.server, "server_port" to node.port))
                for (key in listOf(
                    "proto", "cipher", "auth", "tls", "tls_auth", "tls_crypt", "tls_crypt_v2",
                    "key_direction", "username", "password", "key_password", "reconnect_delay",
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

        applyMultiplexOptions(result, options)

        if (result["type"] == "masque") {
            sanitizeMasqueOutbound(result)
        }
        normalizeOpenVpnOutbound(result)
        normalizeShadowsocks2022(result)

        return result
    }

    // smux is only supported by these protocols in the bundled core. Keeping the
    // sweep in one helper also covers natively imported JSON and dependency nodes.
    private fun applyMultiplexOptions(
        result: MutableMap<String, Any?>,
        options: SingboxConfigOptions
    ) {
        if (!options.multiplexEnabled ||
            result["type"]?.toString()?.trim()?.lowercase() !in
            setOf("vless", "vmess", "trojan", "shadowsocks")
        ) {
            return
        }
        val multiplexMap = mutableMapOf<String, Any?>(
            "enabled" to true,
            "protocol" to (options.multiplexProtocol.trim().lowercase()
                .takeIf { it in MULTIPLEX_PROTOCOLS } ?: "smux"),
            "max_connections" to options.multiplexConcurrency.coerceIn(1, 1024),
            "min_streams" to options.multiplexMinStreams.coerceIn(0, 1024),
            "padding" to options.multiplexPadding
        )
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
        // An encrypted private key is just as fatal as a missing login: without the
        // passphrase the core cannot even parse the key, so reject the member here.
        val privateKey = ((outbound["tls"] as? Map<*, *>)?.get("key")?.toString() ?: "").uppercase()
        val encryptedKey = privateKey.contains("ENCRYPTED PRIVATE KEY") ||
            (privateKey.contains("PROC-TYPE:") && privateKey.contains("ENCRYPTED"))
        if (encryptedKey && outbound["key_password"]?.toString()?.trim().isNullOrEmpty()) {
            throw IllegalArgumentException("OpenVPN private key is encrypted and requires a private key password")
        }
        if (!LinkParser.openVpnRequiresUserAuth(node.link)) return
        val username = outbound["username"]?.toString()?.trim().orEmpty()
        val password = outbound["password"]?.toString()?.trim().orEmpty()
        if (username.isEmpty() || password.isEmpty()) {
            throw IllegalArgumentException("OpenVPN node requires a username and password")
        }
    }

    /**
     * Native JSON nodes and old database rows bypass the .ovpn parser. Apply the
     * same compatibility rules here so one stale OpenVPN field cannot make the
     * core reject the complete connection or an entire AUTO pool.
     */
    private fun normalizeOpenVpnOutbound(outbound: MutableMap<String, Any?>) {
        if (outbound["type"]?.toString()?.lowercase() != "openvpn") return

        val proto = outbound["proto"]?.toString()?.trim()?.lowercase().orEmpty()
        outbound["proto"] = when (proto) {
            // Rows saved before the transport column existed. TCP is the only transport
            // this core can finish a handshake on, so assume it rather than the OpenVPN
            // spec default of UDP.
            "" -> "tcp"
            "tcp", "tcp-client", "tcp4", "tcp4-client", "tcp6", "tcp6-client" -> "tcp"
            // The control channel pushes the whole certificate chain in a single datagram
            // that exceeds the path MTU. Desktop OpenVPN splits it with --fragment/--mssfix;
            // neither string is present in the bundled core, and its `udp_fragment` dial
            // option only stops the dialer from forcing DF - the kernel default sets it
            // anyway, so the socket still answers EMSGSIZE and the handshake dies with
            // `write: message too long`. No lever is left, so refuse loudly instead of
            // connecting to a tunnel that cannot pass traffic.
            "udp", "udp4", "udp6" -> throw IllegalArgumentException(
                "OpenVPN over UDP is not supported by this core; re-import the profile using its TCP remote"
            )
            else -> throw IllegalArgumentException("Unsupported OpenVPN transport `$proto`")
        }

        OpenVpnConfigNormalizer.normalizeDataCipher(outbound["cipher"]?.toString())
            ?.let { outbound["cipher"] = it }
            ?: outbound.remove("cipher")
        OpenVpnConfigNormalizer.normalizeAuthDigest(outbound["auth"]?.toString())
            ?.let { outbound["auth"] = it }
            ?: outbound.remove("auth")

        val tls = (outbound["tls"] as? Map<*, *>)?.entries
            ?.associate { it.key.toString() to it.value }
            ?.toMutableMap()
        if (tls != null) {
            val suites = OpenVpnConfigNormalizer.normalizeTlsCipherSuites(tls["cipher_suites"])
            if (suites.isEmpty()) tls.remove("cipher_suites") else tls["cipher_suites"] = suites
            val verifyMode = tls["verify_x509_name_mode"]?.toString()?.trim()?.lowercase()
            if (verifyMode == "subject") {
                throw IllegalArgumentException(
                    "OpenVPN verify-x509-name mode `subject` is not supported by the bundled core"
                )
            }
            if (verifyMode != null && verifyMode !in setOf("", "name", "name-prefix", "name-suffix")) {
                throw IllegalArgumentException("Invalid OpenVPN verify-x509-name mode `$verifyMode`")
            }
            outbound["tls"] = tls
        }

        if (!outbound["tls_auth"]?.toString().isNullOrBlank() && !outbound.containsKey("key_direction")) {
            outbound["key_direction"] = -1
        }
    }

    // Static Clash/usque endpoint fields are not part of exact extended 2.5.2's
    // MASQUE outbound at either level. That implementation obtains its endpoint
    // and EC key through the Cloudflare profile/cache.
    private val MASQUE_LEGACY_ENDPOINT_KEYS =
        setOf("server", "server_port", "port", "private_key", "public_key", "address", "mtu")

    // The only fields sing-box accepts inside `masque.profile`. Everything else
    // aborts config decoding with `json: unknown field "..."`.
    private val MASQUE_PROFILE_KEYS =
        setOf("detour", "id", "auth_token", "private_key", "recreate")

    private val MASQUE_OUTBOUND_KEYS = setOf(
        "type", "tag",
        "detour", "bind_interface", "inet4_bind_address", "inet6_bind_address",
        "bind_address_no_port", "protect_path", "routing_mark", "reuse_addr",
        "netns", "connect_timeout", "tcp_fast_open", "tcp_multi_path",
        "disable_tcp_keep_alive", "tcp_keep_alive", "tcp_keep_alive_interval",
        "udp_fragment", "domain_resolver", "network_strategy", "network_type",
        "fallback_network_type", "fallback_delay", "domain_strategy",
        "system", "name", "allowed_ips", "use_http2", "use_ipv6", "profile",
        "udp_timeout", "udp_keepalive_period", "udp_initial_packet_size",
        "reconnect_delay", "congestion_controller", "cwnd", "tls"
    )

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
                if (key in MASQUE_PROFILE_KEYS) profile[key] = v
            }
        }

        MASQUE_LEGACY_ENDPOINT_KEYS.forEach(result::remove)
        result.keys.retainAll(MASQUE_OUTBOUND_KEYS)
        result["system"] = false
        result.putIfAbsent("name", "masque0")
        result["profile"] = profile

        val allowed = (result["allowed_ips"] as? List<*>)
            ?.mapNotNull { normalizedIpPrefixOrNull(it?.toString().orEmpty()) }
            ?.distinct()
            .orEmpty()
        if (allowed.isEmpty()) result.remove("allowed_ips") else result["allowed_ips"] = allowed

        mapValue(result["tls"])?.let { rawTls ->
            val accepted = setOf(
                "server_name", "insecure", "cipher_suites", "curve_preferences",
                "fragment", "fragment_fallback_delay", "record_fragment", "kernel_tx", "kernel_rx"
            )
            val tls = rawTls.filterKeys(accepted::contains).toMutableMap()
            if (tls.isEmpty()) result.remove("tls") else result["tls"] = tls
        }
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

    private val XHTTP_BASE_FIELD_ALIASES = listOf(
        "host" to listOf("host"),
        "path" to listOf("path"),
        "headers" to listOf("headers"),
        "domain_strategy" to listOf("domain_strategy", "domainStrategy"),
        "x_padding_bytes" to listOf("x_padding_bytes", "xPaddingBytes"),
        "no_grpc_header" to listOf("no_grpc_header", "noGRPCHeader", "noGrpcHeader"),
        "no_sse_header" to listOf("no_sse_header", "noSSEHeader", "noSseHeader"),
        "sc_max_each_post_bytes" to listOf("sc_max_each_post_bytes", "scMaxEachPostBytes"),
        "sc_min_posts_interval_ms" to listOf("sc_min_posts_interval_ms", "scMinPostsIntervalMs"),
        "sc_max_buffered_posts" to listOf("sc_max_buffered_posts", "scMaxBufferedPosts"),
        "sc_stream_up_server_secs" to listOf("sc_stream_up_server_secs", "scStreamUpServerSecs"),
        "server_max_header_bytes" to listOf("server_max_header_bytes", "serverMaxHeaderBytes"),
        "trusted_x_forwarded_for" to listOf("trusted_x_forwarded_for", "trustedXForwardedFor"),
        "xmux" to listOf("xmux"),
        "x_padding_obfs_mode" to listOf("x_padding_obfs_mode", "xPaddingObfsMode"),
        "x_padding_key" to listOf("x_padding_key", "xPaddingKey"),
        "x_padding_header" to listOf("x_padding_header", "xPaddingHeader"),
        "x_padding_placement" to listOf("x_padding_placement", "xPaddingPlacement"),
        "x_padding_method" to listOf("x_padding_method", "xPaddingMethod"),
        "uplink_http_method" to listOf("uplink_http_method", "uplinkHTTPMethod", "uplinkHttpMethod"),
        "session_placement" to listOf("session_placement", "sessionPlacement"),
        "session_key" to listOf("session_key", "sessionKey"),
        "seq_placement" to listOf("seq_placement", "seqPlacement"),
        "seq_key" to listOf("seq_key", "seqKey"),
        "uplink_data_placement" to listOf("uplink_data_placement", "uplinkDataPlacement"),
        "uplink_data_key" to listOf("uplink_data_key", "uplinkDataKey"),
        "uplink_chunk_size" to listOf("uplink_chunk_size", "uplinkChunkSize"),
        "session_id_table" to listOf("session_id_table", "sessionIDTable", "sessionIdTable"),
        "session_id_length" to listOf("session_id_length", "sessionIDLength", "sessionIdLength"),
        "congestion_controller" to listOf("congestion_controller", "congestionController"),
        "cwnd" to listOf("cwnd")
    )

    private val XHTTP_TLS_KEYS = setOf(
        "enabled", "disable_sni", "server_name", "insecure", "alpn",
        "min_version", "max_version", "cipher_suites", "curve_preferences",
        "certificate", "certificate_path", "certificate_public_key_sha256",
        "client_certificate", "client_certificate_path", "client_key",
        "client_key_path", "fragment", "fragment_fallback_delay",
        "record_fragment", "kernel_tx", "kernel_rx", "ech", "utls", "reality"
    )

    private fun firstPresent(source: Map<String, Any?>, keys: List<String>): Any? {
        for (key in keys) {
            if (source.containsKey(key)) return source[key]
        }
        return null
    }

    private fun copyXhttpBaseOptions(
        source: Map<String, Any?>,
        target: MutableMap<String, Any?>
    ) {
        for ((targetKey, aliases) in XHTTP_BASE_FIELD_ALIASES) {
            val raw = firstPresent(source, aliases) ?: continue
            when (targetKey) {
                "headers" -> {
                    val headers = mapValue(raw)
                        ?.mapNotNull { (key, value) ->
                            value?.toString()?.let { key to it }
                        }
                        ?.toMap()
                        .orEmpty()
                    if (headers.isNotEmpty()) target[targetKey] = headers
                }
                "domain_strategy" -> raw.toString().trim().lowercase()
                    .takeIf { it in setOf("prefer_ipv4", "prefer_ipv6", "ipv4_only", "ipv6_only") }
                    ?.let { target[targetKey] = it }
                "no_grpc_header", "no_sse_header", "x_padding_obfs_mode" ->
                    target[targetKey] = toBoolValue(raw)
                "sc_max_buffered_posts" -> {
                    val value = when (raw) {
                        is Number -> raw.toLong()
                        else -> raw.toString().toLongOrNull()
                    }
                    value?.takeIf { it >= 0 }?.let { target[targetKey] = it }
                }
                "server_max_header_bytes", "cwnd" ->
                    intValue(raw)?.let { target[targetKey] = it }
                "trusted_x_forwarded_for" -> {
                    val values = when (raw) {
                        is List<*> -> raw.mapNotNull { it?.toString()?.trim()?.takeIf(String::isNotEmpty) }
                        else -> listOfNotNull(raw.toString().trim().takeIf(String::isNotEmpty))
                    }
                    if (values.isNotEmpty()) target[targetKey] = values
                }
                "xmux" -> {
                    val xmux = mapValue(raw)
                        ?.filterKeys {
                            it in setOf(
                                "max_concurrency", "max_connections", "c_max_reuse_times",
                                "h_max_request_times", "h_max_reusable_secs", "h_keep_alive_period"
                            )
                        }
                        .orEmpty()
                    if (xmux.isNotEmpty()) target[targetKey] = xmux
                }
                "host", "path", "x_padding_key", "x_padding_header",
                "x_padding_placement", "x_padding_method", "uplink_http_method",
                "session_placement", "session_key", "seq_placement", "seq_key",
                "uplink_data_placement", "uplink_data_key", "session_id_table",
                "congestion_controller" -> raw.toString().takeIf(String::isNotEmpty)
                    ?.let { target[targetKey] = it }
                else -> target[targetKey] = raw
            }
        }
    }

    /**
     * Xray stores the secondary XHTTP leg as `downloadSettings`, while the bundled
     * extended core accepts the same data only as the exact `download` object.
     * Convert both Xray's stream-shaped form and an already-native sing-box form,
     * and never leak Xray-only keys into the strict Go JSON decoder.
     */
    private fun buildXhttpDownload(raw: Any?): Map<String, Any?>? {
        val source = mapValue(raw) ?: return null
        val stream = mapValue(source["streamSettings"]) ?: source
        val xhttp = mapValue(stream["xhttpSettings"] ?: stream["xhttp_settings"])
            ?: mapValue(source["xhttpSettings"] ?: source["xhttp_settings"])
            ?: source

        val server = sequenceOf(
            firstPresent(source, listOf("server", "address")),
            firstPresent(stream, listOf("server", "address"))
        ).mapNotNull { it?.toString()?.trim()?.takeIf(String::isNotEmpty) }.firstOrNull()
            ?: throw IllegalArgumentException("XHTTP download settings are missing server")
        val port = sequenceOf(
            firstPresent(source, listOf("server_port", "port")),
            firstPresent(stream, listOf("server_port", "port"))
        ).mapNotNull(::intValue).firstOrNull()
            ?: throw IllegalArgumentException("XHTTP download settings are missing server port")

        val download = mutableMapOf<String, Any?>(
            "server" to server,
            "server_port" to port
        )
        copyXhttpBaseOptions(source, download)
        if (xhttp !== source) copyXhttpBaseOptions(xhttp, download)
        download.putIfAbsent("x_padding_bytes", "100-1000")

        (source["detour"] ?: stream["detour"])?.toString()?.trim()
            ?.takeIf(String::isNotEmpty)
            ?.let { download["detour"] = it }

        val rawTls = mapValue(stream["tls"] ?: source["tls"])
        val tlsSettings = mapValue(stream["tlsSettings"] ?: stream["tls_settings"])
        val security = stream["security"]?.toString()?.trim()?.lowercase().orEmpty()
        if (rawTls != null || tlsSettings != null || security in setOf("tls", "reality")) {
            val tls = rawTls
                ?.filterKeys(XHTTP_TLS_KEYS::contains)
                ?.toMutableMap()
                ?: mutableMapOf()
            tls.putIfAbsent("enabled", true)
            if (tlsSettings != null) {
                (tlsSettings["serverName"] ?: tlsSettings["server_name"])?.toString()
                    ?.takeIf(String::isNotEmpty)
                    ?.let { tls["server_name"] = it }
                (tlsSettings["allowInsecure"] ?: tlsSettings["insecure"])?.let {
                    tls["insecure"] = toBoolValue(it)
                }
                tlsSettings["alpn"]?.let { tls["alpn"] = it }
                (tlsSettings["certificatePublicKeySha256"]
                    ?: tlsSettings["certificate_public_key_sha256"])?.let {
                    tls["certificate_public_key_sha256"] = it
                }
                tlsSettings["fingerprint"]?.toString()?.let {
                    tls["utls"] = mapOf(
                        "enabled" to true,
                        "fingerprint" to normalizeUtlsFingerprint(it).ifEmpty { "chrome" }
                    )
                }
            }
            if (security == "reality") {
                val realitySettings =
                    mapValue(stream["realitySettings"] ?: stream["reality_settings"]).orEmpty()
                (realitySettings["serverName"] ?: realitySettings["server_name"])?.toString()
                    ?.takeIf(String::isNotEmpty)
                    ?.let { tls["server_name"] = it }
                val publicKey =
                    (realitySettings["publicKey"] ?: realitySettings["public_key"])
                        ?.toString()
                        .orEmpty()
                require(publicKey.isNotEmpty()) {
                    "XHTTP download REALITY settings are missing public key"
                }
                tls["reality"] = mapOf(
                    "enabled" to true,
                    "public_key" to publicKey,
                    "short_id" to
                        (realitySettings["shortId"] ?: realitySettings["short_id"])
                            ?.toString()
                            .orEmpty()
                )
                tls.putIfAbsent(
                    "utls",
                    mapOf(
                        "enabled" to true,
                        "fingerprint" to normalizeUtlsFingerprint(
                            (realitySettings["fingerprint"] ?: realitySettings["fp"])?.toString()
                        ).ifEmpty { "chrome" }
                    )
                )
            }
            mapValue(tls["utls"])?.let { rawUtls ->
                val utls = rawUtls.toMutableMap()
                val fingerprint = normalizeUtlsFingerprint(utls["fingerprint"]?.toString())
                if (fingerprint.isNotEmpty()) {
                    utls["enabled"] = true
                    utls["fingerprint"] = fingerprint
                } else {
                    utls.remove("fingerprint")
                }
                tls["utls"] = utls
            }
            tls.putIfAbsent("server_name", server)
            normalizeCertificatePublicKeyPins(tls)
            download["tls"] = tls
        }
        return download
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
                tlsSettings["certificatePublicKeySha256"]?.let {
                    tls["certificate_public_key_sha256"] = it
                }
                tlsSettings["fingerprint"]?.let {
                    tls["utls"] = mapOf("enabled" to true, "fingerprint" to it.toString())
                }
                val alpn = tlsSettings["alpn"]
                if (alpn is List<*>) {
                    tls["alpn"] = alpn
                }
            }
            normalizeCertificatePublicKeyPins(tls)
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
                    val xhttpSettings = mapValue(stream["xhttpSettings"]) ?: emptyMap()
                    val transport = mutableMapOf<String, Any?>("type" to "xhttp")
                    xhttpSettings["mode"]?.let { transport["mode"] = it.toString() }
                    copyXhttpBaseOptions(xhttpSettings, transport)
                    (xhttpSettings["downloadSettings"] ?: xhttpSettings["download"])?.let {
                        buildXhttpDownload(it)?.let { download -> transport["download"] = download }
                    }
                    transport.putIfAbsent("x_padding_bytes", "100-1000")
                    result["transport"] = transport
                }
                "quic" -> {
                    require(security == "tls") {
                        "QUIC transport requires TLS in sing-box-extended"
                    }
                    // The exact extended core intentionally exposes an empty QUIC
                    // transport schema. Xray's key/security/header options are not
                    // valid JSON fields here and would abort decoding.
                    result["transport"] = mapOf("type" to "quic")
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
