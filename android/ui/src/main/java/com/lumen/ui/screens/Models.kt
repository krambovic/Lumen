package com.lumen.ui.screens

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.ImageBitmap

/**
 * UI-facing models shared by all screens. The :ui module is intentionally
 * decoupled from :core:database — the app layer maps entities to these models.
 */

@Immutable
data class NodeUiModel(
    val id: String,
    val name: String,
    val protocol: String,
    val server: String,
    val port: Int,
    val pingMs: Int? = null,
    val countryCode: String = "",
    val isAutoNode: Boolean = false,
    val isSelected: Boolean = false,
    val subscriptionId: String? = null,
    val displayProtocol: String = protocol.uppercase()
)

@Immutable
data class SubscriptionUiModel(
    val id: String,
    val name: String,
    val url: String,
    val lastUpdated: Long = 0L,
    val nodeCount: Int = 0,
    val autoUpdateEnabled: Boolean = true,
    val premiumFeatureCount: Int = 0,
    val trafficSummary: String? = null,
    val expiryDaysLeft: Int? = null,
    // Premium API extras shown on the dashboard card.
    val trafficRatio: Float? = null,
    val updateIntervalHours: Int? = null,
    val announce: String? = null
)

@Immutable
data class HomeServerGroup(
    val id: String,
    val title: String,
    val nodes: List<NodeUiModel>,
    val isSubscription: Boolean = false,
    val subscription: SubscriptionUiModel? = null
)

data class AppEntryUiModel(
    val packageName: String,
    val label: String,
    val icon: ImageBitmap? = null,
    val isSelected: Boolean = false,
    val isSystem: Boolean = false
)

data class GeoResourceUiModel(
    val name: String,
    val sizeBytes: Long = 0L,
    val modifiedAt: Long = 0L
)

enum class SplitModeUi { DISABLED, ALLOW_LIST, DISALLOW_LIST }

enum class ThemeMode { SYSTEM, LIGHT, DARK }

enum class ThemePreset {
    LIGHT,
    DARK,
    DRACULA,
    CATPPUCCIN,
    NORD,
    GITHUB,
    GRUVBOX,
    TOKYO_NIGHT,
    MONOKAI,
    MATERIAL,
    SOLARIZED
}

enum class ImportKindUi { SUBSCRIPTION, CONFIG }

enum class ImportPhaseUi { HIDDEN, AWAITING, IMPORTING, SUCCESS, ERROR }

data class ImportUiState(
    val phase: ImportPhaseUi = ImportPhaseUi.HIDDEN,
    val kind: ImportKindUi? = null,
    val raw: String = "",
    val title: String = "",
    val message: String = ""
)

data class SettingsUiState(
    val engine: String = "SINGBOX",
    val muxEnabled: Boolean = false,
    val muxConcurrency: Int = 8,
    val fragmentEnabled: Boolean = false,
    val fragmentPackets: String = "tlshello",
    val fragmentLength: String = "50-100",
    val fragmentDelay: String = "10-20",
    val localInboundEnabled: Boolean = true,
    val localSocksPort: Int = 10808,
    val localHttpPort: Int = 10809,
    val lanSharingEnabled: Boolean = false,
    val autoConnectOnBoot: Boolean = false,
    val preferIpv6: Boolean = false,
    val blockQuic: Boolean = false,
    val sniffRouteOnly: Boolean = false,
    val mtu: Int = 1500,
    val directDomains: String = "",
    val directIpCidrs: String = "",
    val geoResourceSource: String = "https://github.com/runetfreedom/russia-v2ray-rules-dat/",
    // Legacy single-server fields are kept for preference migration.
    val proxyDnsServer: String = "cloudflare-dns.com",
    val directDnsServer: String = "1.1.1.1",
    val dnsMode: String = "automatic",
    val dnsDirectServers: String = "1.1.1.1\n8.8.8.8",
    val dnsProxyServers: String = "cloudflare-dns.com\ndns.google",
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
    val dnsHosts: String = "",
    val dnsOverrideEnabled: Boolean = true,
    val dnsOverrideHostname: String = "ntc.party",
    val dnsOverrideIpv4: String = "130.255.77.28",
    val urlTestUrl: String = "https://www.gstatic.com/generate_204",
    val urlTestIntervalMinutes: Int = 3,
    val urlTestToleranceMs: Int = 50,
    val subscriptionUserAgent: String = "Happ/2.18.3/Windows/2606241603601",
    val subscriptionHwid: String = "",
    val subscriptionSendHwid: Boolean = true,
    val subscriptionDirect: Boolean = true,
    val allowSubscriptionOverrides: Boolean = true,
    val subscriptionAutoUpdateMinutes: Int = 240,
    val subscriptionIncludeRegex: String = "",
    val subscriptionExcludeRegex: String = "",
    val subscriptionUseProxyTun: Boolean = false,
    val subscriptionConverterEnabled: Boolean = false,
    val subscriptionConverterUrl: String = "",
    val logLevel: String = "debug",
    val language: String = "en",
    val themeMode: ThemeMode = ThemeMode.DARK,
    val themePreset: ThemePreset = ThemePreset.DARK,
    val useMaterialYou: Boolean = false,
    val useAmoledBlack: Boolean = false,
    val hapticsEnabled: Boolean = true,
    val pingType: String = "tcp",
    val pingTimeoutMs: Int = 3000,
    val pingConcurrency: Int = 15,
    val pingUrl: String = "https://www.gstatic.com/generate_204",
    val pingOnOpen: Boolean = false,
    val pingSortAfter: Boolean = false
)

/**
 * Editable node draft used by [NodeEditorModal]. [secret] holds the
 * UUID / password / WireGuard private key depending on the protocol.
 */
data class NodeDraft(
    val id: String? = null,
    val name: String = "",
    val protocol: String = "vless",
    val server: String = "",
    val port: String = "443",
    val secret: String = "",
    val flow: String = "",
    val network: String = "tcp",
    val security: String = "none",
    val path: String = "",
    val host: String = "",
    val serviceName: String = "",
    val sni: String = "",
    val alpn: String = "",
    val fingerprint: String = "",
    val publicKey: String = "",
    val shortId: String = "",
    val method: String = "aes-256-gcm",
    val address: String = "",
    val presharedKey: String = "",
    val allowedIps: String = "0.0.0.0/0, ::/0",
    val reserved: String = "",
    val jc: String = "",
    val jmin: String = "",
    val jmax: String = "",
    val s1: String = "",
    val s2: String = "",
    val s3: String = "",
    val s4: String = "",
    val obfs: String = "",
    val obfsPassword: String = "",
    val congestionControl: String = "bbr",
    val insecure: Boolean = false,
    val rawConfig: String = ""
)

val SUPPORTED_PROTOCOLS: List<String> = listOf(
    "vless", "vmess", "trojan", "ss", "hysteria2", "tuic",
    "wireguard", "awg", "openvpn", "socks", "http", "auto"
)

val NETWORK_TRANSPORTS: List<String> = listOf("tcp", "ws", "grpc", "xhttp", "http")

val SECURITY_OPTIONS: List<String> = listOf("none", "tls", "reality")

val SS_METHODS: List<String> = listOf(
    "aes-256-gcm", "aes-128-gcm", "chacha20-ietf-poly1305",
    "2022-blake3-aes-256-gcm", "2022-blake3-aes-128-gcm", "2022-blake3-chacha20-poly1305"
)

val CONGESTION_OPTIONS: List<String> = listOf("bbr", "cubic", "new_reno")

val PING_TYPES: List<String> = listOf("tcp", "udp", "url")

/** Global haptics switch so any screen can respect the vibration setting. */
val LocalHapticsEnabled = androidx.compose.runtime.staticCompositionLocalOf { true }

/** Single source of version strings; the app layer fills [appVersion] from BuildConfig. */
object LumenVersion {
    var appVersion: String = "0.7.0"
    const val ENGINE: String = "1.13.14-extended-2.5.2"
}
