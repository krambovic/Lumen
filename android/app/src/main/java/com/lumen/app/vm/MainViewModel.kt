package com.lumen.app.vm

import android.app.Application
import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import androidx.compose.ui.graphics.asImageBitmap
import androidx.core.graphics.drawable.toBitmap
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import androidx.room.Room
import androidx.room.withTransaction
import com.lumen.app.subscription.ImportClassification
import com.lumen.app.subscription.ImportClassifier
import com.lumen.app.subscription.ImportKind
import com.lumen.app.subscription.SubscriptionClient
import com.lumen.app.util.NodeDraftMapper
import com.lumen.core.config.builder.SingboxConfigBuilder
import com.lumen.core.config.builder.SingboxConfigOptions
import com.lumen.core.config.parser.LinkParser
import com.lumen.core.config.parser.ParsedNode
import org.json.JSONObject
import com.lumen.core.database.AppDatabase
import com.lumen.core.database.model.NodeEntity
import com.lumen.core.database.model.SubscriptionEntity
import com.lumen.core.vpn.LumenVpnService
import com.lumen.core.vpn.VpnLogBus
import com.lumen.ui.components.ConnectionState
import com.lumen.ui.components.CountryFlagHelper
import com.lumen.ui.screens.AppEntryUiModel
import com.lumen.ui.screens.DashboardStyle
import com.lumen.ui.screens.GeoResourceUiModel
import com.lumen.ui.screens.ImportKindUi
import com.lumen.ui.screens.ImportPhaseUi
import com.lumen.ui.screens.ImportUiState
import com.lumen.ui.screens.NodeDraft
import com.lumen.ui.screens.NodeUiModel
import com.lumen.ui.screens.SettingsUiState
import com.lumen.ui.screens.SplitModeUi
import com.lumen.ui.screens.SubscriptionUiModel
import com.lumen.ui.screens.ThemeMode
import com.lumen.ui.screens.ThemePreset
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withPermit
import java.io.File
import java.net.HttpURLConnection
import java.net.InetSocketAddress
import java.net.Socket
import java.net.URL
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.UUID

class MainViewModel(app: Application) : AndroidViewModel(app) {

    private val db = Room.databaseBuilder(app, AppDatabase::class.java, "lumen.db")
        .addMigrations(AppDatabase.MIGRATION_1_2, AppDatabase.MIGRATION_2_3)
        .fallbackToDestructiveMigration()
        .build()
    private val nodeDao = db.nodeDao()
    private val subscriptionDao = db.subscriptionDao()
    private val prefs = app.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    private val subscriptionHwid: String by lazy {
        prefs.getString("subscription_hwid", null)?.takeIf { it.isNotBlank() } ?: UUID.randomUUID().toString().also {
            prefs.edit().putString("subscription_hwid", it).apply()
        }
    }

    init {
        // Single source of truth for the version shown in the UI and sent in headers.
        com.lumen.ui.screens.LumenVersion.appVersion = net.kramb.lumen.BuildConfig.VERSION_NAME
        com.lumen.core.vpn.TelemetryManager.appVersion = net.kramb.lumen.BuildConfig.VERSION_NAME
        // Heartbeat on every launch: relying on the VPN service alone missed most users.
        com.lumen.core.vpn.TelemetryManager.sendStartupHeartbeat(app, viewModelScope)
    }

    // ---------- Logs ----------
    // Log text is only formatted while the logs tab is open; elsewhere the flow stays empty.
    private val _logsVisible = MutableStateFlow(false)

    fun setLogsVisible(visible: Boolean) { _logsVisible.value = visible }

    val logs: StateFlow<List<String>> = combine(VpnLogBus.entries, _logsVisible) { entries, visible ->
        if (!visible) emptyList() else entries.map { entry ->
            "[${entry.formattedTime}] [${entry.level.name}] [${entry.component}] ${entry.message}"
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), emptyList())

    fun log(message: String) = VpnLogBus.info("APP", message)

    fun clearLogs() = VpnLogBus.clear()

    fun exportLogs(context: Context) {
        val text = logs.value.joinToString("\n")
        if (text.isBlank()) return
        val intent = Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_SUBJECT, "Lumen logs")
            putExtra(Intent.EXTRA_TEXT, text)
        }
        context.startActivity(
            Intent.createChooser(intent, "Export logs").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        )
    }

    // ---------- Settings ----------
    private val _settings = MutableStateFlow(loadSettings())
    val settings: StateFlow<SettingsUiState> = _settings

    private fun loadSettings() = SettingsUiState(
        engine = "SINGBOX",
        muxEnabled = prefs.getBoolean("mux_enabled", false),
        muxConcurrency = prefs.getInt("mux_concurrency", 8),
        fragmentEnabled = prefs.getBoolean("fragment_enabled", false),
        fragmentPackets = prefs.getString("fragment_packets", "tlshello") ?: "tlshello",
        fragmentLength = prefs.getString("fragment_length", "50-100") ?: "50-100",
        fragmentDelay = prefs.getString("fragment_delay", "10-20") ?: "10-20",
        localInboundEnabled = prefs.getBoolean("local_inbound", true),
        localSocksPort = prefs.getInt("local_socks_port", 10808),
        localHttpPort = prefs.getInt("local_http_port", 10809),
        lanSharingEnabled = prefs.getBoolean("lan_sharing", false),
        autoConnectOnBoot = prefs.getBoolean("boot_auto_connect", false),
        showNotification = prefs.getBoolean("show_notification", true),
        showNotificationSpeed = prefs.getBoolean("show_notification_speed", true),
        preferIpv6 = prefs.getBoolean("prefer_ipv6", false),
        blockQuic = prefs.getBoolean("block_quic", false),
        sniffRouteOnly = prefs.getBoolean("sniff_route_only", false),
        mtu = prefs.getInt("mtu", 1500),
        directDomains = prefs.getString("routing_direct_domains", "") ?: "",
        directIpCidrs = prefs.getString("routing_direct_ip_cidrs", "") ?: "",
        geoResourceSource = prefs.getString(
            "geo_resource_source",
            "https://github.com/runetfreedom/russia-v2ray-rules-dat/"
        ) ?: "https://github.com/runetfreedom/russia-v2ray-rules-dat/",
        proxyDnsServer = prefs.getString("proxy_dns", "cloudflare-dns.com") ?: "cloudflare-dns.com",
        directDnsServer = prefs.getString("direct_dns", "1.1.1.1") ?: "1.1.1.1",
        dnsMode = prefs.getString("dns_mode", "automatic") ?: "automatic",
        dnsDirectServers = prefs.getString("dns_direct_servers", null)
            ?: (prefs.getString("direct_dns", "1.1.1.1") + "\n8.8.8.8"),
        dnsProxyServers = prefs.getString("dns_proxy_servers", null)
            ?: (prefs.getString("proxy_dns", "cloudflare-dns.com") + "\ndns.google"),
        dnsDirectType = prefs.getString("dns_direct_type", "udp") ?: "udp",
        dnsProxyType = prefs.getString("dns_proxy_type", "https") ?: "https",
        dnsDirectStrategy = prefs.getString("dns_direct_strategy", "ipv4_only") ?: "ipv4_only",
        dnsProxyStrategy = prefs.getString("dns_proxy_strategy", "ipv4_only") ?: "ipv4_only",
        dnsHijackEnabled = prefs.getBoolean("dns_hijack_enabled", true),
        dnsFakeIpEnabled = prefs.getBoolean("dns_fake_ip_enabled", false),
        dnsParallelQuery = prefs.getBoolean("dns_parallel_query", false),
        dnsOptimisticCache = prefs.getBoolean("dns_optimistic_cache", false),
        dnsGeoCheck = prefs.getBoolean("dns_geo_check", true),
        dnsProxyIpv4Only = prefs.getBoolean("dns_proxy_ipv4_only", true),
        dnsHosts = prefs.getString("dns_hosts", "") ?: "",
        dnsOverrideEnabled = prefs.getBoolean("dns_override_enabled", true),
        dnsOverrideHostname = prefs.getString("dns_override_hostname", "ntc.party") ?: "ntc.party",
        dnsOverrideIpv4 = prefs.getString("dns_override_ipv4", "130.255.77.28") ?: "130.255.77.28",
        urlTestUrl = prefs.getString("url_test_url", "https://www.gstatic.com/generate_204")
            ?: "https://www.gstatic.com/generate_204",
        urlTestIntervalMinutes = prefs.getInt("url_test_interval_minutes", 3),
        urlTestToleranceMs = prefs.getInt("url_test_tolerance_ms", 50),
        subscriptionUserAgent = prefs.getString("subscription_user_agent", "Happ/2.18.3/Windows/2606241603601")
            ?: "Happ/2.18.3/Windows/2606241603601",
        subscriptionHwid = prefs.getString("subscription_hwid", subscriptionHwid) ?: subscriptionHwid,
        subscriptionSendHwid = prefs.getBoolean("subscription_send_hwid", true),
        subscriptionDirect = prefs.getBoolean("subscription_direct", true),
        allowSubscriptionOverrides = prefs.getBoolean("allow_subscription_overrides", true),
        subscriptionAutoUpdateMinutes = prefs.getInt("subscription_auto_update_minutes", 240),
        subscriptionIncludeRegex = prefs.getString("subscription_include_regex", "") ?: "",
        subscriptionExcludeRegex = prefs.getString("subscription_exclude_regex", "") ?: "",
        subscriptionUseProxyTun = prefs.getBoolean("subscription_use_proxy_tun", false),
        subscriptionConverterEnabled = prefs.getBoolean("subscription_converter_enabled", false),
        subscriptionConverterUrl = prefs.getString("subscription_converter_url", "") ?: "",
        // Core debug logging is the single biggest CPU/battery drain: every line
        // crosses the log bus and recomposes the UI. Keep it opt-in.
        logLevel = prefs.getString("log_level", "warn") ?: "warn",
        language = prefs.getString("language", "en")?.takeIf { it in setOf("en", "ru", "fa", "zh") } ?: "en",
        themeMode = runCatching {
            ThemeMode.valueOf(prefs.getString("theme_mode", ThemeMode.DARK.name) ?: ThemeMode.DARK.name)
        }.getOrDefault(ThemeMode.DARK),
        themePreset = runCatching {
            ThemePreset.valueOf(prefs.getString("theme_preset", ThemePreset.DARK.name) ?: ThemePreset.DARK.name)
        }.getOrDefault(ThemePreset.DARK),
        useMaterialYou = prefs.getBoolean("use_material_you", false),
        useAmoledBlack = prefs.getBoolean("use_amoled_black", false),
        hapticsEnabled = prefs.getBoolean("haptics_enabled", true),
        pingType = prefs.getString("server_speed_test_type", "tcp")?.lowercase()
            ?.takeIf { it in setOf("tcp", "udp", "url") } ?: "tcp",
        pingTimeoutMs = prefs.getInt("ping_timeout_ms", 3000),
        pingConcurrency = prefs.getInt("ping_concurrency", 15),
        pingUrl = prefs.getString("ping_url", "https://www.gstatic.com/generate_204")
            ?: "https://www.gstatic.com/generate_204",
        pingSortAfter = prefs.getBoolean("ping_sort_after", false),
        dashboardStyle = runCatching {
            DashboardStyle.valueOf(prefs.getString("dashboard_style", DashboardStyle.DEFAULT.name) ?: DashboardStyle.DEFAULT.name)
        }.getOrDefault(DashboardStyle.DEFAULT)
    )

    fun updateSettings(s: SettingsUiState) {
        _settings.value = s
        prefs.edit()
            .putString("engine_type", s.engine)
            .putString("log_level", s.logLevel)
            .putBoolean("mux_enabled", s.muxEnabled)
            .putInt("mux_concurrency", s.muxConcurrency)
            .putBoolean("fragment_enabled", s.fragmentEnabled)
            .putString("fragment_packets", s.fragmentPackets)
            .putString("fragment_length", s.fragmentLength)
            .putString("fragment_delay", s.fragmentDelay)
            .putBoolean("local_inbound", s.localInboundEnabled)
            .putInt("local_socks_port", s.localSocksPort.coerceIn(1024, 65535))
            .putInt("local_http_port", s.localHttpPort.coerceIn(1024, 65535))
            .putBoolean("lan_sharing", s.lanSharingEnabled)
            .putBoolean("boot_auto_connect", s.autoConnectOnBoot)
            .putBoolean("show_notification", s.showNotification)
            .putBoolean("show_notification_speed", s.showNotificationSpeed)
            .putBoolean("prefer_ipv6", s.preferIpv6)
            .putBoolean("block_quic", s.blockQuic)
            .putBoolean("sniff_route_only", s.sniffRouteOnly)
            .putInt("mtu", s.mtu.coerceIn(1280, 9000))
            .putString("routing_direct_domains", s.directDomains)
            .putString("routing_direct_ip_cidrs", s.directIpCidrs)
            .putString("geo_resource_source", s.geoResourceSource)
            .putString("proxy_dns", s.dnsProxyServers.lineSequence().firstOrNull()?.trim().orEmpty().take(253))
            .putString("direct_dns", s.dnsDirectServers.lineSequence().firstOrNull()?.trim().orEmpty().take(253))
            .putString("dns_mode", s.dnsMode)
            .putString("dns_direct_servers", s.dnsDirectServers.take(2048))
            .putString("dns_proxy_servers", s.dnsProxyServers.take(2048))
            .putString("dns_direct_type", s.dnsDirectType)
            .putString("dns_proxy_type", s.dnsProxyType)
            .putString("dns_direct_strategy", s.dnsDirectStrategy)
            .putString("dns_proxy_strategy", s.dnsProxyStrategy)
            .putBoolean("dns_hijack_enabled", s.dnsHijackEnabled)
            .putBoolean("dns_fake_ip_enabled", s.dnsFakeIpEnabled)
            .putBoolean("dns_parallel_query", s.dnsParallelQuery)
            .putBoolean("dns_optimistic_cache", s.dnsOptimisticCache)
            .putBoolean("dns_geo_check", s.dnsGeoCheck)
            .putBoolean("dns_proxy_ipv4_only", s.dnsProxyIpv4Only)
            .putString("dns_hosts", s.dnsHosts.take(4096))
            .putBoolean("dns_override_enabled", s.dnsOverrideEnabled)
            .putString("dns_override_hostname", s.dnsOverrideHostname.trim().take(253))
            .putString("dns_override_ipv4", s.dnsOverrideIpv4.trim().take(15))
            .putString("url_test_url", s.urlTestUrl.trim().take(512))
            .putInt("url_test_interval_minutes", s.urlTestIntervalMinutes.coerceIn(1, 1440))
            .putInt("url_test_tolerance_ms", s.urlTestToleranceMs.coerceIn(0, 5000))
            .putString("subscription_user_agent", s.subscriptionUserAgent.trim().take(256))
            .putString("subscription_hwid", s.subscriptionHwid.trim().take(256))
            .putBoolean("subscription_send_hwid", s.subscriptionSendHwid)
            .putBoolean("subscription_direct", s.subscriptionDirect)
            .putBoolean("allow_subscription_overrides", s.allowSubscriptionOverrides)
            .putInt("subscription_auto_update_minutes", s.subscriptionAutoUpdateMinutes.coerceIn(15, 1440))
            .putString("subscription_include_regex", s.subscriptionIncludeRegex.trim().take(512))
            .putString("subscription_exclude_regex", s.subscriptionExcludeRegex.trim().take(512))
            .putBoolean("subscription_use_proxy_tun", s.subscriptionUseProxyTun)
            .putBoolean("subscription_converter_enabled", s.subscriptionConverterEnabled)
            .putString("subscription_converter_url", s.subscriptionConverterUrl.trim().take(512))
            .putString("engine_log_level", "debug")
            .putString("language", s.language)
            .putString("theme_mode", s.themeMode.name)
            .putString("theme_preset", s.themePreset.name)
            .putBoolean("use_material_you", s.useMaterialYou)
            .putBoolean("use_amoled_black", s.useAmoledBlack)
            .putBoolean("haptics_enabled", s.hapticsEnabled)
            .putString("server_speed_test_type", s.pingType)
            .putInt("ping_timeout_ms", s.pingTimeoutMs.coerceIn(500, 20000))
            .putInt("ping_concurrency", s.pingConcurrency.coerceIn(1, 32))
            .putString("ping_url", s.pingUrl.trim().take(512))
            .putBoolean("ping_sort_after", s.pingSortAfter)
            .putString("dashboard_style", s.dashboardStyle.name)
            .apply()
    }

    // ---------- Geo resources ----------
    private val geoResourcesDir = File(app.filesDir, "georesources").apply { mkdirs() }
    private val _geoResources = MutableStateFlow(scanGeoResources())
    val geoResources: StateFlow<List<GeoResourceUiModel>> = _geoResources
    private val _isUpdatingGeoResources = MutableStateFlow(false)
    val isUpdatingGeoResources: StateFlow<Boolean> = _isUpdatingGeoResources

    private fun scanGeoResources(): List<GeoResourceUiModel> =
        listOf("geosite.dat", "geoip.dat").mapNotNull { name ->
            File(geoResourcesDir, name).takeIf { it.isFile }?.let {
                GeoResourceUiModel(name, it.length(), it.lastModified())
            }
        }

    fun refreshGeoResources() {
        _geoResources.value = scanGeoResources()
    }

    fun downloadGeoResources() {
        if (_isUpdatingGeoResources.value) return
        viewModelScope.launch(Dispatchers.IO) {
            _isUpdatingGeoResources.value = true
            try {
                val source = _settings.value.geoResourceSource.lowercase(Locale.US)
                val repository = when {
                    "loyalsoldier" in source -> "Loyalsoldier/v2ray-rules-dat"
                    "chocolate4u" in source -> "Chocolate4U/Iran-v2ray-rules"
                    else -> "runetfreedom/russia-v2ray-rules-dat"
                }
                listOf("geosite.dat", "geoip.dat").forEach { name ->
                    val target = File(geoResourcesDir, name)
                    val temporary = File(geoResourcesDir, "$name.download")
                    val url = URL("https://github.com/$repository/releases/latest/download/$name")
                    val connection = url.openConnection() as HttpURLConnection
                    connection.connectTimeout = 20_000
                    connection.readTimeout = 120_000
                    connection.instanceFollowRedirects = true
                    connection.setRequestProperty("User-Agent", "Lumen/${net.kramb.lumen.BuildConfig.VERSION_NAME}")
                    try {
                        if (connection.responseCode !in 200..299) {
                            error("$name: HTTP ${connection.responseCode}")
                        }
                        connection.inputStream.use { input ->
                            temporary.outputStream().use { output ->
                                val copied = input.copyTo(output)
                                if (copied < 1024L) error("$name: файл слишком мал")
                                if (copied > 256L * 1024 * 1024) error("$name: превышен лимит 256 МБ")
                            }
                        }
                        if (target.exists()) target.delete()
                        check(temporary.renameTo(target)) { "$name: не удалось сохранить файл" }
                        log("Geo resource updated: $name (${target.length()} bytes)")
                    } finally {
                        connection.disconnect()
                        if (temporary.exists()) temporary.delete()
                    }
                }
                refreshGeoResources()
                autoImportGeoRules(repository)
            } catch (e: Exception) {
                log("Geo resources update failed: ${e.message}")
            } finally {
                _isUpdatingGeoResources.value = false
            }
        }
    }

    // Region bypass rules follow the downloaded geo database, so they always match it.
    private fun autoImportGeoRules(repository: String) {
        val code = when {
            repository.startsWith("Loyalsoldier", true) -> "cn"
            repository.startsWith("Chocolate4U", true) -> "ir"
            else -> "ru"
        }
        val siteTag = if (code == "ru") "geosite:category-ru" else "geosite:$code"
        val wanted = listOf(siteTag, "geoip:$code")
        val current = _settings.value.directDomains
        val existing = current.split(Regex("[\\n,;]+"))
            .map { it.trim() }
            .filter { it.isNotEmpty() }
        val stripPrefix = { value: String ->
            val prefix = listOf("proxy:", "block:", "reject:", "direct:")
                .firstOrNull { value.startsWith(it, true) }
            if (prefix != null) value.substring(prefix.length).trim() else value
        }
        val known = existing.map { stripPrefix(it).lowercase(Locale.US) }.toSet()
        val additions = wanted.filter { it.lowercase(Locale.US) !in known }.map { "direct:$it" }
        if (additions.isEmpty()) return
        val merged = (existing + additions).joinToString("\n")
        viewModelScope.launch(Dispatchers.Main) {
            updateSettings(_settings.value.copy(directDomains = merged))
            log("Auto-imported geo bypass rules for $code")
        }
    }

    // ---------- Split tunneling ----------
    private val _splitMode = MutableStateFlow(
        runCatching { SplitModeUi.valueOf(prefs.getString("split_mode", "DISABLED") ?: "DISABLED") }
            .getOrDefault(SplitModeUi.DISABLED)
    )
    val splitMode: StateFlow<SplitModeUi> = _splitMode

    private val _splitPackages = MutableStateFlow(
        prefs.getStringSet("split_packages", emptySet())?.toSet() ?: emptySet()
    )

    private val _installedApps = MutableStateFlow<List<AppEntryUiModel>>(emptyList())
    private val _isLoadingApps = MutableStateFlow(false)
    val isLoadingApps: StateFlow<Boolean> = _isLoadingApps

    val apps: StateFlow<List<AppEntryUiModel>> =
        combine(_installedApps, _splitPackages) { list, selected ->
            list.map { it.copy(isSelected = it.packageName in selected) }
        }.stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    fun setSplitMode(mode: SplitModeUi) {
        _splitMode.value = mode
        prefs.edit().putString("split_mode", mode.name).apply()
        if (mode != SplitModeUi.DISABLED) loadInstalledApps()
    }

    fun toggleApp(app: AppEntryUiModel) {
        val next = _splitPackages.value.toMutableSet()
        if (!next.add(app.packageName)) next.remove(app.packageName)
        _splitPackages.value = next
        prefs.edit().putStringSet("split_packages", next).apply()
    }

    fun autoSelectApps() {
        val selected = _installedApps.value
            .asSequence()
            .filterNot { it.isSystem }
            .map { it.packageName }
            .toSet()
        _splitPackages.value = selected
        prefs.edit().putStringSet("split_packages", selected).apply()
    }

    fun clearAppSelection() {
        _splitPackages.value = emptySet()
        prefs.edit().putStringSet("split_packages", emptySet()).apply()
    }

    fun loadInstalledApps() {
        if (_isLoadingApps.value) return
        _isLoadingApps.value = true
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val application = getApplication<Application>()
                val pm = application.packageManager
                val list = pm.getInstalledApplications(PackageManager.GET_META_DATA)
                    .asSequence()
                    .filter { it.packageName != application.packageName && it.enabled }
                    .filter {
                        pm.checkPermission(
                            android.Manifest.permission.INTERNET,
                            it.packageName
                        ) == PackageManager.PERMISSION_GRANTED
                    }
                    .map {
                        AppEntryUiModel(
                            packageName = it.packageName,
                            label = pm.getApplicationLabel(it).toString(),
                            icon = runCatching {
                                pm.getApplicationIcon(it).toBitmap(width = 48, height = 48).asImageBitmap()
                            }.getOrNull(),
                            isSystem = it.flags and ApplicationInfo.FLAG_SYSTEM != 0
                        )
                    }
                    .sortedBy { it.label.lowercase(Locale.getDefault()) }
                    .toList()
                _installedApps.value = list
                log("Found ${list.size} network-capable apps")
            } catch (e: Exception) {
                log("Failed to list apps: ${e.message}")
            } finally {
                _isLoadingApps.value = false
            }
        }
    }

    // ---------- Nodes ----------
    private val _selectedNodeId = MutableStateFlow(prefs.getString("selected_node_id", null))

    private val nodeEntities: StateFlow<List<NodeEntity>> = nodeDao.getNodes()
        .stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    // Cache of expensive per-node computations (flag stripping, country detection,
    // display-protocol extraction). Keyed by node id + content fingerprint so ping
    // updates or selection changes don't recompute regex/uppercase work per node.
    private data class NodeUiCacheEntry(val fingerprint: Int, val base: NodeUiModel)
    private val nodeUiCache = java.util.concurrent.ConcurrentHashMap<String, NodeUiCacheEntry>()

    // Set after a ping run when "sort after ping" is enabled; reset on any list change source.
    private val _sortByPing = MutableStateFlow(false)

    // Screens read this to override their own sort order once a ping run finished.
    val sortByPing: StateFlow<Boolean> = _sortByPing

    val nodes: StateFlow<List<NodeUiModel>> =
        combine(nodeEntities, _selectedNodeId, _sortByPing) { list, selectedId, sortByPing ->
            val liveIds = HashSet<String>(list.size * 2)
            val mapped = list.mapNotNull { e ->
                runCatching {
                    liveIds.add(e.id)
                    val fingerprint = 31 * (31 * (31 * e.name.hashCode() + e.server.hashCode()) +
                        e.protocol.hashCode()) + (e.outboundJson.hashCode() xor e.link.hashCode())
                    val cached = nodeUiCache[e.id]
                    val base = if (cached != null && cached.fingerprint == fingerprint) {
                        cached.base
                    } else {
                        val sourceName = e.name.ifBlank { e.server.ifBlank { "Server" } }
                        val safeName = stripFlagEmoji(sourceName).ifBlank { e.server.ifBlank { "Server" } }
                        val safeServer = e.server
                        val safeProtocol = e.protocol.ifBlank { "vless" }
                        // Auto pool nodes are always labelled AUTO, whatever protocol got stored.
                        val autoFlag = e.isAutoNode || safeProtocol.equals("auto", true)
                        val displayProto = if (autoFlag) "AUTO"
                            else extractDisplayProtocol(safeProtocol, e.outboundJson, e.link)
                        NodeUiModel(
                            id = e.id,
                            name = safeName,
                            protocol = safeProtocol,
                            server = safeServer,
                            port = e.port,
                            pingMs = null,
                            countryCode = runCatching { CountryFlagHelper.detectCountry(sourceName, safeServer) }.getOrDefault(""),
                            isAutoNode = autoFlag,
                            isSelected = false,
                            subscriptionId = e.subscriptionId,
                            displayProtocol = displayProto
                        ).also { nodeUiCache[e.id] = NodeUiCacheEntry(fingerprint, it) }
                    }
                    base.copy(
                        port = e.port,
                        pingMs = e.pingMs,
                        isAutoNode = e.isAutoNode || e.protocol.equals("auto", true),
                        subscriptionId = e.subscriptionId,
                        isSelected = e.id == selectedId
                    )
                }.getOrNull()
            }
            nodeUiCache.keys.retainAll(liveIds)
            // Auto nodes stay pinned on top; unreachable ones sink to the bottom.
            if (sortByPing) mapped.sortedWith(
                compareByDescending<NodeUiModel> { it.isAutoNode }
                    .thenBy { it.pingMs ?: Int.MAX_VALUE }
            ) else mapped
        }.flowOn(Dispatchers.Default)
            .stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    val activeNode: StateFlow<NodeUiModel?> = nodes
        .map { list -> list.firstOrNull { it.isSelected } ?: list.firstOrNull() }
        .stateIn(viewModelScope, SharingStarted.Eagerly, null)

    fun selectNode(node: NodeUiModel) {
        _selectedNodeId.value = node.id
        // The name is mirrored into prefs so the home screen widget can label
        // itself without opening the database.
        // Base64 of the UTF-8 bytes avoids any charset mangling between the app
        // process and the widget process.
        val nameB64 = android.util.Base64.encodeToString(
            node.name.toByteArray(Charsets.UTF_8),
            android.util.Base64.NO_WRAP
        )
        prefs.edit()
            .putString("selected_node_id", node.id)
            .putString("selected_node_name", node.name)
            .putString("selected_node_name_b64", nameB64)
            .apply()
        com.lumen.app.widget.LumenWidgetProvider.sendUpdateBroadcast(getApplication())
        log("Selected ${node.name}")
    }

    fun deleteNode(node: NodeUiModel) {
        viewModelScope.launch(Dispatchers.IO) {
            nodeDao.deleteNodeById(node.id)
            log("Deleted node ${node.name}")
        }
    }

    // Deletes servers belonging to the default / manual group only.
    fun deleteAllNodes() {
        viewModelScope.launch(Dispatchers.IO) {
            nodeDao.deleteManualNodes()
            log("Deleted all manual nodes")
        }
    }

    fun draftForNode(node: NodeUiModel): NodeDraft =
        NodeDraftMapper.draftFromEntity(nodeEntities.value.firstOrNull { it.id == node.id })
            ?: NodeDraft()

    fun saveDraft(draft: NodeDraft) {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val entity = NodeDraftMapper.entityFromDraft(draft)
                if (draft.id != null) nodeDao.updateNode(entity) else nodeDao.insertNode(entity)
                log("Saved node ${entity.name}")
            } catch (e: Exception) {
                log("Failed to save node: ${e.message}")
            }
        }
    }

    private val _importState = MutableStateFlow(ImportUiState())
    val importState: StateFlow<ImportUiState> = _importState

    fun prepareImportText(text: String?) {
        _importState.value = when (val classification = ImportClassifier.classify(text)) {
            is ImportClassification.Rejected -> ImportUiState(
                phase = ImportPhaseUi.ERROR,
                title = "Nothing to import",
                message = classification.message
            )
            is ImportClassification.Ready -> ImportUiState(
                phase = ImportPhaseUi.AWAITING,
                kind = if (classification.kind == ImportKind.SUBSCRIPTION) {
                    ImportKindUi.SUBSCRIPTION
                } else ImportKindUi.CONFIG,
                raw = classification.normalized,
                title = if (classification.kind == ImportKind.SUBSCRIPTION) {
                    "Import subscription?"
                } else "Import server config?",
                message = if (classification.kind == ImportKind.SUBSCRIPTION) {
                    "The link will be downloaded and added as a subscription."
                } else "Supported servers will be added to Default."
            )
        }
    }

    fun dismissImport() {
        if (_importState.value.phase != ImportPhaseUi.IMPORTING) {
            _importState.value = ImportUiState()
        }
    }

    fun confirmImport() {
        val pending = _importState.value
        if (pending.phase != ImportPhaseUi.AWAITING || pending.kind == null) return
        _importState.value = pending.copy(phase = ImportPhaseUi.IMPORTING, message = "Importing…")
        viewModelScope.launch(Dispatchers.IO) {
            try {
                when (pending.kind) {
                    ImportKindUi.SUBSCRIPTION -> {
                        val url = (ImportClassifier.classify(pending.raw) as? ImportClassification.Ready)
                            ?.takeIf { it.kind == ImportKind.SUBSCRIPTION }?.normalized
                            ?: error("Invalid subscription URL")
                        val sub = SubscriptionEntity(
                            name = url.substringAfter("://").substringBefore('/').take(80),
                            url = url
                        )
                        subscriptionDao.insertSubscription(sub)
                        val imported = try {
                            refreshSubscriptionInternal(sub, rethrow = true)
                        } catch (error: Exception) {
                            nodeDao.deleteNodesBySubscription(sub.id)
                            subscriptionDao.deleteSubscriptionById(sub.id)
                            throw error
                        }
                        if (imported <= 0) {
                            nodeDao.deleteNodesBySubscription(sub.id)
                            subscriptionDao.deleteSubscriptionById(sub.id)
                            error("Subscription contains no supported servers")
                        }
                        _importState.value = ImportUiState(
                            phase = ImportPhaseUi.SUCCESS,
                            title = "Subscription imported",
                            message = "$imported server(s) added"
                        )
                    }
                    ImportKindUi.CONFIG -> {
                        val (parsed, errors) = LinkParser.parseLinksText(pending.raw)
                        errors.take(3).forEach { log("Import warning: $it") }
                        val valid = parsed.filter {
                            it.name.length <= 512 && it.server.length <= 512 &&
                                (it.scheme == "auto" || it.server.isNotBlank()) &&
                                (it.scheme == "auto" || it.port in 1..65535) && it.link.length <= 65_536
                        }.take(LinkParser.MAX_IMPORT_NODES).let { collapseAutoNodes(it) }
                        if (valid.isEmpty()) error("No supported server configs found")
                        db.withTransaction { nodeDao.insertNodes(valid.map { it.toEntity(null) }) }
                        _importState.value = ImportUiState(
                            phase = ImportPhaseUi.SUCCESS,
                            title = "Import complete",
                            message = "${valid.size} server(s) added to Default"
                        )
                        log("Imported ${valid.size} node(s)")
                    }
                    null -> error("Import request expired")
                }
            } catch (e: Exception) {
                val reason = e.message?.take(240) ?: "Unknown import error"
                _importState.value = ImportUiState(
                    phase = ImportPhaseUi.ERROR,
                    title = "Import failed",
                    message = reason
                )
                log("Import failed: $reason")
            }
        }
    }

    @Deprecated("Use prepareImportText so untrusted input requires confirmation")
    fun importText(text: String) = prepareImportText(text)

    /**
     * Like [prepareImportText] but for files: bypasses the clipboard-size limit,
     * reads charset-safe bytes, and skips the 1 MiB ImportClassifier gate.
     * For files we trust the user selected them intentionally, so we go straight
     * to the parser and show a confirmation dialog with the result count.
     */
    fun prepareImportFileContent(content: String?) {
        if (content.isNullOrBlank()) {
            _importState.value = ImportUiState(
                phase = ImportPhaseUi.ERROR,
                title = "Nothing to import",
                message = "The file is empty"
            )
            return
        }
        val bytes = content.toByteArray(Charsets.UTF_8)
        if (bytes.size > LinkParser.MAX_IMPORT_BYTES) {
            _importState.value = ImportUiState(
                phase = ImportPhaseUi.ERROR,
                title = "File too large",
                message = "File exceeds the ${LinkParser.MAX_IMPORT_BYTES / 1024 / 1024} MiB import limit"
            )
            return
        }
        // Quick-check: if it's a bare http/https URL treat it as a subscription URL.
        val trimmed = content.trim()
        if (!trimmed.contains('\n') && (trimmed.startsWith("http://") || trimmed.startsWith("https://"))) {
            prepareImportText(trimmed)
            return
        }
        // Pre-parse so we can show a meaningful summary before the user confirms.
        val (parsed, parseErrors) = try {
            LinkParser.parseLinksText(content)
        } catch (e: Exception) {
            _importState.value = ImportUiState(
                phase = ImportPhaseUi.ERROR,
                title = "File parse error",
                message = e.message?.take(240) ?: "Unknown error"
            )
            return
        }
        val valid = parsed.filter {
            it.name.length <= 512 && it.server.length <= 512 &&
                (it.scheme == "auto" || it.server.isNotBlank()) &&
                (it.scheme == "auto" || it.port in 1..65535) && it.link.length <= 65_536
        }.take(LinkParser.MAX_IMPORT_NODES).let { collapseAutoNodes(it) }
        if (valid.isEmpty()) {
            _importState.value = ImportUiState(
                phase = ImportPhaseUi.ERROR,
                title = "No supported servers",
                message = if (parseErrors.isNotEmpty()) parseErrors.take(3).joinToString("\n") else "No recognised server configs found in file"
            )
            return
        }
        _importState.value = ImportUiState(
            phase = ImportPhaseUi.AWAITING,
            kind = ImportKindUi.CONFIG,
            raw = content,
            title = "Import from file?",
            message = "${valid.size} server(s) found. They will be added to Default."
        )
    }

    /**
     * Providers often ship a whole pool of "auto"/URLTest entries. Desktop Lumen
     * keeps a single AUTO server, so the mobile import does the same: only the
     * first auto entry survives and the rest are dropped.
     */
    private fun collapseAutoNodes(list: List<ParsedNode>): List<ParsedNode> {
        var autoSeen = false
        return list.filter { node ->
            val isAuto = node.scheme.equals("auto", ignoreCase = true)
            if (!isAuto) true else if (autoSeen) false else { autoSeen = true; true }
        }
    }

    private fun ParsedNode.toEntity(subscriptionId: String?) = NodeEntity(
        name = name.ifBlank { server },
        protocol = scheme,
        server = server,
        port = port,
        link = link,
        outboundJson = if (outbound.isNotEmpty()) LinkParser.toJsonString(outbound) else "",
        subscriptionId = subscriptionId,
        isAutoNode = scheme == "auto"
    )

    // ---------- Subscriptions ----------
    private val subEntities: StateFlow<List<SubscriptionEntity>> = subscriptionDao.getSubscriptions()
        .stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())
    private val _subscriptionPremium = MutableStateFlow<Map<String, Map<String, String>>>(emptyMap())
    // Premium API extras kept in one map so the subscriptions combine stays 5-arity.
    private data class SubMeta(
        val summary: String,
        val ratio: Float?,
        val intervalHours: Int?,
        val announce: String?
    )
    private val _subscriptionTraffic = MutableStateFlow<Map<String, SubMeta>>(emptyMap())
    private val _subscriptionExpiry = MutableStateFlow<Map<String, Long>>(emptyMap())

    val subscriptions: StateFlow<List<SubscriptionUiModel>> =
        combine(subEntities, nodeEntities, _subscriptionPremium, _subscriptionTraffic, _subscriptionExpiry) { subs, allNodes, premium, traffic, expiry ->
            subs.map { s ->
                SubscriptionUiModel(
                    id = s.id,
                    name = s.name,
                    url = s.url,
                    lastUpdated = s.lastUpdated,
                    nodeCount = allNodes.count { it.subscriptionId == s.id },
                    autoUpdateEnabled = s.autoUpdateEnabled,
                    premiumFeatureCount = premium[s.id]?.size ?: 0,
                    trafficSummary = traffic[s.id]?.summary,
                    expiryDaysLeft = expiry[s.id]?.let { expireEpochSec ->
                        val daysLeft = ((expireEpochSec * 1000L - System.currentTimeMillis()) / 86_400_000L).toInt()
                        daysLeft.coerceAtLeast(0)
                    },
                    trafficRatio = traffic[s.id]?.ratio,
                    updateIntervalHours = traffic[s.id]?.intervalHours,
                    announce = traffic[s.id]?.announce
                )
            }
        }.stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    private val _refreshingIds = MutableStateFlow<Set<String>>(emptySet())
    val refreshingIds: StateFlow<Set<String>> = _refreshingIds

    // Declared here (not in the top init block) so every field the loop touches exists.
    init { startSubscriptionAutoUpdate() }

    fun addSubscription(name: String, url: String) {
        viewModelScope.launch(Dispatchers.IO) {
            val sub = SubscriptionEntity(
                name = name.ifBlank { url.substringAfter("://").take(40) },
                url = url
            )
            subscriptionDao.insertSubscription(sub)
            log("Added subscription ${sub.name}")
            refreshSubscriptionInternal(sub)
        }
    }

    fun refreshSubscription(model: SubscriptionUiModel) {
        viewModelScope.launch(Dispatchers.IO) {
            subEntities.value.firstOrNull { it.id == model.id }?.let {
                refreshSubscriptionInternal(it)
            }
        }
    }

    fun refreshSubscription(subscriptionId: String) {
        val model = subscriptions.value.firstOrNull { it.id == subscriptionId } ?: return
        refreshSubscription(model)
    }

    private suspend fun refreshSubscriptionInternal(
        sub: SubscriptionEntity,
        rethrow: Boolean = false
    ): Int {
        _refreshingIds.value = _refreshingIds.value + sub.id
        var importedCount = 0
        try {
            val subscriptionSettings = _settings.value
            val payload = SubscriptionClient.fetch(
                rawUrl = sub.url,
                hwid = subscriptionSettings.subscriptionHwid.trim()
                    .takeIf { subscriptionSettings.subscriptionSendHwid && it.isNotBlank() },
                customUserAgent = subscriptionSettings.subscriptionUserAgent.trim().ifBlank { null },
                direct = subscriptionSettings.subscriptionDirect
            )
            val (parsed, errors) = LinkParser.parseLinksText(payload.body)
            errors.take(3).forEach { log("Subscription warning: $it") }
            val valid = parsed.filter {
                it.name.length <= 512 && it.server.length <= 512 &&
                    (it.scheme == "auto" || it.server.isNotBlank()) &&
                    (it.scheme == "auto" || it.port in 1..65535) && it.link.length <= 65_536
            }.take(LinkParser.MAX_IMPORT_NODES).let { collapseAutoNodes(it) }
            if (valid.isNotEmpty()) {
                db.withTransaction {
                    nodeDao.deleteNodesBySubscription(sub.id)
                    nodeDao.insertNodes(valid.map { it.toEntity(sub.id) })
                }
                importedCount = valid.size
                _subscriptionPremium.value = _subscriptionPremium.value + (sub.id to payload.premiumFeatures)
                payload.userInfo.takeIf { it.isNotEmpty() }?.let { info ->
                    val used = (info["upload"] ?: 0L) + (info["download"] ?: 0L)
                    val total = info["total"] ?: 0L
                    val meta = SubMeta(
                        summary = formatSubscriptionTraffic(info),
                        ratio = if (total > 0L) (used.toDouble() / total).coerceIn(0.0, 1.0).toFloat() else null,
                        intervalHours = payload.updateIntervalHours?.takeIf { it in 1..8760 },
                        announce = payload.premiumFeatures["announce"]?.take(240)?.ifBlank { null }
                    )
                    _subscriptionTraffic.value = _subscriptionTraffic.value + (sub.id to meta)
                    val expireEpochSec = info["expire"] ?: 0L
                    _subscriptionExpiry.value = if (expireEpochSec > 0L) {
                        _subscriptionExpiry.value + (sub.id to expireEpochSec)
                    } else {
                        _subscriptionExpiry.value - sub.id
                    }
                }
                val premiumApplied = applyCompatiblePremiumFeatures(payload.premiumFeatures)
                val replacementUrl = payload.effectiveUrl
                    ?: SubscriptionClient.replaceDomain(sub.url, payload.premiumFeatures["new-domain"])
                val autoUpdate = payload.premiumFeatures["subscription-auto-update-enable"]
                    ?.let(::premiumEnabled) ?: sub.autoUpdateEnabled
                subscriptionDao.updateSubscription(
                    sub.copy(
                        name = payload.profileTitle?.take(160)?.ifBlank { sub.name } ?: sub.name,
                        url = replacementUrl ?: sub.url,
                        lastUpdated = System.currentTimeMillis(),
                        autoUpdateEnabled = autoUpdate
                    )
                )
                log("Subscription ${sub.name}: ${parsed.size} node(s), profile ${payload.clientProfile}")
                if (premiumApplied.isNotEmpty()) log("Applied premium settings: ${premiumApplied.joinToString()}")
            } else {
                log("Subscription ${sub.name}: no nodes found")
            }
        } catch (e: Exception) {
            log("Subscription refresh failed: ${e.message}")
            if (rethrow) throw e
        } finally {
            _refreshingIds.value = _refreshingIds.value - sub.id
        }
        return importedCount
    }

    fun deleteSubscription(model: SubscriptionUiModel) {
        viewModelScope.launch(Dispatchers.IO) {
            nodeDao.deleteNodesBySubscription(model.id)
            subscriptionDao.deleteSubscriptionById(model.id)
            log("Deleted subscription ${model.name}")
        }
    }

    fun deleteSubscription(subscriptionId: String) {
        val model = subscriptions.value.firstOrNull { it.id == subscriptionId } ?: return
        deleteSubscription(model)
    }

    // Subscriptions always auto-update; there is no per-subscription switch anymore.
    // Cadence priority: interval requested by the provider (profile-update-interval),
    // otherwise the interval configured in app settings.
    private fun startSubscriptionAutoUpdate() {
        viewModelScope.launch(Dispatchers.IO) {
            while (true) {
                kotlinx.coroutines.delay(60_000L)
                val configuredMinutes = _settings.value.subscriptionAutoUpdateMinutes
                    .takeIf { it > 0 } ?: 240
                val now = System.currentTimeMillis()
                subEntities.value.forEach { sub ->
                    val providerMinutes = _subscriptionTraffic.value[sub.id]?.intervalHours?.times(60)
                    val intervalMinutes = providerMinutes ?: configuredMinutes
                    val due = now - sub.lastUpdated >= intervalMinutes * 60_000L
                    if (due && sub.id !in _refreshingIds.value) {
                        runCatching { refreshSubscriptionInternal(sub) }
                    }
                }
            }
        }
    }

    private fun premiumEnabled(value: String): Boolean =
        value.trim().lowercase(Locale.US) in setOf("1", "true", "yes", "on", "enabled")

    private fun applyCompatiblePremiumFeatures(premium: Map<String, String>): List<String> {
        if (premium.isEmpty() || !_settings.value.allowSubscriptionOverrides) return emptyList()
        val applied = mutableListOf<String>()
        var next = _settings.value

        premium["subscription-autoconnect"]?.let {
            next = next.copy(autoConnectOnBoot = premiumEnabled(it))
            applied += "subscription-autoconnect"
        }
        premium["fragmentation-enable"]?.let {
            next = next.copy(fragmentEnabled = premiumEnabled(it))
            applied += "fragmentation-enable"
        }
        premium["fragmentation-packets"]?.takeIf { it.isNotBlank() }?.let {
            next = next.copy(fragmentPackets = it.take(64)); applied += "fragmentation-packets"
        }
        premium["fragmentation-length"]?.takeIf { it.isNotBlank() }?.let {
            next = next.copy(fragmentLength = it.take(32)); applied += "fragmentation-length"
        }
        premium["fragmentation-interval"]?.takeIf { it.isNotBlank() }?.let {
            next = next.copy(fragmentDelay = it.take(32)); applied += "fragmentation-interval"
        }
        premium["mux-enable"]?.let {
            next = next.copy(muxEnabled = premiumEnabled(it)); applied += "mux-enable"
        }
        premium["mux-tcp-connections"]?.toIntOrNull()?.coerceIn(-1, 1024)?.let {
            next = next.copy(muxConcurrency = it); applied += "mux-tcp-connections"
        }
        if (next != _settings.value) updateSettings(next)

        premium["change-user-agent"]?.trim()?.takeIf {
            it.isNotBlank() && it.length <= 256 && '\r' !in it && '\n' !in it
        }?.let {
            prefs.edit().putString("subscription_user_agent", it).apply()
            applied += "change-user-agent"
        }
        premium["ping-type"]?.trim()?.takeIf { it.isNotBlank() }?.let {
            prefs.edit().putString("server_speed_test_type", it.take(32)).apply()
            applied += "ping-type"
        }
        premium["per-app-proxy-mode"]?.trim()?.lowercase(Locale.US)?.let { mode ->
            val mapped = when (mode) {
                "allow", "allow-list", "include", "whitelist", "1" -> SplitModeUi.ALLOW_LIST
                "disallow", "disallow-list", "exclude", "blacklist", "2" -> SplitModeUi.DISALLOW_LIST
                "off", "disabled", "0" -> SplitModeUi.DISABLED
                else -> null
            }
            if (mapped != null) {
                setSplitMode(mapped)
                applied += "per-app-proxy-mode"
            }
        }
        premium["per-app-proxy-list"]?.let { raw ->
            val packages = raw.split(Regex("[\\s,;]+"))
                .map { it.trim() }
                .filter { it.matches(Regex("[A-Za-z0-9_.]+")) }
                .take(500)
                .toSet()
            if (packages.isNotEmpty()) {
                _splitPackages.value = packages
                prefs.edit().putStringSet("split_packages", packages).apply()
                applied += "per-app-proxy-list"
            }
        }
        return applied
    }

    private fun formatSubscriptionTraffic(info: Map<String, Long>): String {
        val used = (info["upload"] ?: 0L) + (info["download"] ?: 0L)
        val total = info["total"] ?: 0L
        return if (total > 0L) "${formatBytes(used)} / ${formatBytes(total)}" else formatBytes(used)
    }

    private fun formatBytes(bytes: Long): String {
        if (bytes < 1024) return "$bytes B"
        val units = arrayOf("KB", "MB", "GB", "TB")
        var value = bytes.toDouble()
        var index = -1
        while (value >= 1024 && index < units.lastIndex) {
            value /= 1024.0
            index++
        }
        return String.format(Locale.US, "%.1f %s", value, units[index.coerceAtLeast(0)])
    }

    // ---------- Ping ----------
    private val _isPinging = MutableStateFlow(false)
    val isPinging: StateFlow<Boolean> = _isPinging
    private val _testingNodeId = MutableStateFlow<String?>(null)
    val testingNodeId: StateFlow<String?> = _testingNodeId
    private val _serverTestResults = MutableStateFlow<Map<String, String>>(emptyMap())
    val serverTestResults: StateFlow<Map<String, String>> = _serverTestResults
    private val _pingingNodeIds = MutableStateFlow<Set<String>>(emptySet())
    val pingingNodeIds: StateFlow<Set<String>> = _pingingNodeIds

    // ICMP fallback wakes the radio; keep it far below the ping concurrency.
    private val icmpSemaphore = kotlinx.coroutines.sync.Semaphore(4)

    fun pingAll() {
        val targets = nodeEntities.value.filter { !it.isAutoNode }
        pingNodeListInternal(targets)
    }

    /** "Check all" with the UDP probe instead of TCP. */
    fun pingAllUdp() {
        val targets = nodeEntities.value.filter { !it.isAutoNode }
        pingNodeListInternal(targets, useUdp = true)
    }

    /** Single-node UDP probe, used by the server row menu. */
    fun pingNodeUdp(node: NodeUiModel) {
        val target = nodeEntities.value.filter { it.id == node.id }
        pingNodeListInternal(target, useUdp = true)
    }

    fun pingGroup(subscriptionId: String?) {
        val targets = nodeEntities.value.filter { !it.isAutoNode && it.subscriptionId == subscriptionId }
        pingNodeListInternal(targets)
    }

    fun pingGroupUdp(subscriptionId: String?) {
        val targets = nodeEntities.value.filter { !it.isAutoNode && it.subscriptionId == subscriptionId }
        pingNodeListInternal(targets, useUdp = true)
    }

    fun pingNodes(nodes: List<NodeUiModel>) {
        val nodeIds = nodes.map { it.id }.toSet()
        val targets = nodeEntities.value.filter { it.id in nodeIds }
        pingNodeListInternal(targets)
    }

    private fun pingNodeListInternal(targets: List<NodeEntity>, useUdp: Boolean = false) {
        if (_isPinging.value || targets.isEmpty()) return
        val cfg = _settings.value
        val udp = useUdp || cfg.pingType == "udp"
        val limit = cfg.pingConcurrency.coerceIn(1, 32)
        viewModelScope.launch(Dispatchers.IO) {
            _isPinging.value = true
            log("Pinging ${targets.size} node(s), $limit at a time${if (udp) " (UDP)" else ""}…")
            // Drop stale values first: the row must show "measuring", not the previous ping.
            val targetIds = targets.map { it.id }
            _serverTestResults.value = _serverTestResults.value - targetIds.toSet()
            _pingingNodeIds.value = targetIds.toSet()
            nodeDao.updatePingsBatch(targetIds.map { Pair(it, null as Int?) })
            val pending = java.util.Collections.synchronizedSet(targetIds.toMutableSet())
            val semaphore = kotlinx.coroutines.sync.Semaphore(limit)
            val jobs = targets.map { node ->
                async {
                    semaphore.withPermit {
                        val ping = when {
                            udp -> udpPing(node.server, node.port)
                            cfg.pingType == "url" -> urlPing()
                            else -> tcpPing(node.server, node.port)
                        }
                        nodeDao.updatePing(node.id, ping.takeIf { it >= 0 })
                        synchronized(pending) {
                            pending.remove(node.id)
                            _pingingNodeIds.value = pending.toSet()
                        }
                    }
                }
            }
            jobs.awaitAll()
            _pingingNodeIds.value = emptySet()
            _isPinging.value = false
            if (cfg.pingSortAfter) _sortByPing.value = true
            log("Ping finished for ${targets.size} node(s)")
        }
    }

    fun pingNode(node: NodeUiModel) {
        if (node.id in _pingingNodeIds.value) return
        viewModelScope.launch(Dispatchers.IO) {
            _testingNodeId.value = node.id
            _pingingNodeIds.value = _pingingNodeIds.value + node.id
            // Old value is dropped before measuring so the row never shows a stale ping.
            _serverTestResults.value = _serverTestResults.value - node.id
            nodeDao.updatePing(node.id, null)
            val ping = when (_settings.value.pingType) {
                "udp" -> udpPing(node.server, node.port)
                "url" -> urlPing()
                else -> tcpPing(node.server, node.port)
            }
            nodeDao.updatePing(node.id, ping.takeIf { it >= 0 })
            val result = if (ping >= 0) "$ping ms" else "Timeout"
            _serverTestResults.value = _serverTestResults.value + (node.id to result)
            log("Ping ${node.name}: $result")
            _pingingNodeIds.value = _pingingNodeIds.value - node.id
            _testingNodeId.value = null
        }
    }

    fun exportNodesText(nodeIds: Set<String>): String {
        val allEntities = nodeEntities.value.associateBy { it.id }
        return nodeIds.mapNotNull { id -> allEntities[id]?.link?.takeIf { it.isNotBlank() } }
            .joinToString("\n")
    }

    fun exportSubscriptionText(subscriptionId: String?): String {
        val targets = nodeEntities.value.filter { it.subscriptionId == subscriptionId }
        return targets.mapNotNull { it.link.takeIf { l -> l.isNotBlank() } }.joinToString("\n")
    }

    private fun pingTimeout(): Int = _settings.value.pingTimeoutMs.coerceIn(500, 20_000)

    private fun tcpPing(host: String, port: Int, timeoutMs: Int = pingTimeout()): Int = try {
        val start = System.nanoTime()
        Socket().use { it.connect(InetSocketAddress(host, port), timeoutMs) }
        ((System.nanoTime() - start) / 1_000_000).toInt()
    } catch (e: Exception) {
        -1
    }

    /** URL delay test: measures time to first response byte, like the url-test outbound. */
    private fun urlPing(timeoutMs: Int = pingTimeout()): Int = try {
        val url = URL(_settings.value.pingUrl.trim().ifBlank { "https://www.gstatic.com/generate_204" })
        val conn = url.openConnection() as HttpURLConnection
        conn.connectTimeout = timeoutMs
        conn.readTimeout = timeoutMs
        conn.requestMethod = "HEAD"
        val start = System.nanoTime()
        val code = conn.responseCode
        conn.disconnect()
        if (code in 200..399) ((System.nanoTime() - start) / 1_000_000).toInt() else -1
    } catch (e: Exception) {
        -1
    }

    /**
     * UDP "ping": sends a small probe to the server port and waits for any reply.
     * Many VPN servers don't answer garbage datagrams, so on timeout we fall back
     * to an ICMP echo to the host to still get a latency estimate.
     */
    private suspend fun udpPing(host: String, port: Int, timeoutMs: Int = pingTimeout()): Int = try {
        val address = java.net.InetAddress.getByName(host)
        var latency = -1
        java.net.DatagramSocket().use { socket ->
            socket.soTimeout = timeoutMs
            socket.connect(address, port)
            val payload = ByteArray(8)
            val buffer = ByteArray(128)
            val start = System.nanoTime()
            latency = try {
                socket.send(java.net.DatagramPacket(payload, payload.size))
                socket.receive(java.net.DatagramPacket(buffer, buffer.size))
                ((System.nanoTime() - start) / 1_000_000).toInt()
            } catch (e: java.net.PortUnreachableException) {
                -1
            } catch (e: java.net.SocketTimeoutException) {
                // Throttled separately: mass pings would otherwise fire dozens of ICMP probes at once.
                icmpSemaphore.withPermit {
                    val icmpStart = System.nanoTime()
                    if (address.isReachable(1500)) {
                        ((System.nanoTime() - icmpStart) / 1_000_000).toInt()
                    } else {
                        -1
                    }
                }
            }
        }
        latency
    } catch (e: Exception) {
        -1
    }

    // ---------- Connection ----------
    private val _connectionState = MutableStateFlow(ConnectionState.Disconnected)
    val connectionState: StateFlow<ConnectionState> = _connectionState

    private val _uploadHistory = MutableStateFlow<List<Float>>(emptyList())
    val uploadHistory: StateFlow<List<Float>> = _uploadHistory
    private val _downloadHistory = MutableStateFlow<List<Float>>(emptyList())
    val downloadHistory: StateFlow<List<Float>> = _downloadHistory

    init {
        viewModelScope.launch {
            LumenVpnService.isRunning.collect { running ->
                _connectionState.value = when {
                    running -> ConnectionState.Connected
                    VpnLogBus.lastError.value != null -> ConnectionState.Error
                    else -> ConnectionState.Disconnected
                }
            }
        }
        viewModelScope.launch {
            VpnLogBus.lastError.collect { error ->
                if (error != null) _connectionState.value = ConnectionState.Error
            }
        }
    }

    private var connectTimeoutJob: kotlinx.coroutines.Job? = null

    fun markConnecting() {
        VpnLogBus.clearLastError()
        _connectionState.value = ConnectionState.Connecting
        // Repeated taps must not stack watchdogs racing to flip the state to Error.
        connectTimeoutJob?.cancel()
        connectTimeoutJob = viewModelScope.launch {
            delay(20_000)
            if (!LumenVpnService.isRunning.value &&
                _connectionState.value == ConnectionState.Connecting
            ) {
                _connectionState.value = ConnectionState.Error
                log("Connection attempt timed out")
            }
        }
    }

    fun buildStartIntent(context: Context): Intent? {
        if (nodes.value.isEmpty() || activeNode.value == null) {
            log("No active servers available to connect")
            return null
        }
        val entities = nodeEntities.value
        if (entities.isEmpty()) {
            log("No servers configured in database")
            return null
        }
        val selected = entities.firstOrNull { it.id == _selectedNodeId.value } ?: entities.first()
        val parsedSelected = parseEntity(selected)
        val s = _settings.value.copy(engine = "SINGBOX")
        val configJson = try {
            run {
                val pool = entities.map { parseEntity(it) }
                SingboxConfigBuilder.buildConfig(
                    pool,
                    parsedSelected,
                    SingboxConfigOptions(
                        tunMode = false,
                        tunMtu = s.mtu.coerceIn(1280, 9000),
                        localSocksPort = s.localSocksPort.coerceIn(1024, 65535),
                        localHttpPort = if (s.localInboundEnabled) s.localHttpPort.coerceIn(1024, 65535) else 0,
                        multiplexEnabled = s.muxEnabled,
                        multiplexConcurrency = s.muxConcurrency,
                        enableFinalFragment = s.fragmentEnabled,
                        fragmentPackets = s.fragmentPackets,
                        fragmentLength = s.fragmentLength,
                        fragmentDelay = s.fragmentDelay,
                        preferIpv6 = s.preferIpv6,
                        blockQuic = s.blockQuic,
                        sniffRouteOnly = s.sniffRouteOnly,
                        proxyDnsServer = s.dnsProxyServers.lineSequence().firstOrNull()?.trim().orEmpty().ifBlank { "cloudflare-dns.com" },
                        directDnsServer = s.dnsDirectServers.lineSequence().firstOrNull()?.trim().orEmpty().ifBlank { "1.1.1.1" },
                        dnsMode = s.dnsMode,
                        dnsDirectServers = s.dnsDirectServers.split(Regex("[\\n,;]+")).map(String::trim).filter(String::isNotEmpty),
                        dnsProxyServers = s.dnsProxyServers.split(Regex("[\\n,;]+")).map(String::trim).filter(String::isNotEmpty),
                        dnsDirectType = s.dnsDirectType,
                        dnsProxyType = s.dnsProxyType,
                        dnsDirectStrategy = s.dnsDirectStrategy,
                        dnsProxyStrategy = s.dnsProxyStrategy,
                        dnsHijackEnabled = s.dnsHijackEnabled,
                        dnsFakeIpEnabled = s.dnsFakeIpEnabled,
                        dnsParallelQuery = s.dnsParallelQuery,
                        dnsOptimisticCache = s.dnsOptimisticCache,
                        dnsGeoCheck = s.dnsGeoCheck,
                        dnsProxyIpv4Only = s.dnsProxyIpv4Only,
                        dnsHosts = s.dnsHosts.lineSequence().mapNotNull { line ->
                            val host = line.substringBefore('=').trim().trimEnd('.').lowercase()
                            val addresses = line.substringAfter('=', "").split(',').map(String::trim).filter(String::isNotEmpty)
                            if (host.isNotBlank() && addresses.isNotEmpty()) host to addresses else null
                        }.toMap(),
                        dnsOverrideEnabled = s.dnsOverrideEnabled,
                        dnsOverrideHostname = s.dnsOverrideHostname,
                        dnsOverrideIpv4 = s.dnsOverrideIpv4,
                        logLevel = s.logLevel,
                        urlTestUrl = s.urlTestUrl.ifBlank { "https://www.gstatic.com/generate_204" },
                        urlTestIntervalMinutes = s.urlTestIntervalMinutes.coerceIn(1, 1440),
                        urlTestToleranceMs = s.urlTestToleranceMs.coerceIn(0, 5000),
                        directDomains = s.directDomains.split(Regex("[\\n,;]+")).map { it.trim() }.filter { it.isNotEmpty() },
                        directIpCidrs = s.directIpCidrs.split(Regex("[\\n,;]+")).map { it.trim() }.filter { it.isNotEmpty() }
                    )
                )
            }
        } catch (e: Exception) {
            log("Config build failed: ${e.message}")
            return null
        }
        prefs.edit()
            .putString("active_config_json", configJson)
            .putString("engine_type", s.engine)
            .putString("split_mode", _splitMode.value.name)
            .putStringSet("split_packages", _splitPackages.value)
            .putInt("mtu", s.mtu)
            .apply()
        VpnLogBus.clearLastError()
        log("Starting sing-box extended \u2192 ${selected.name}")
        return Intent(context, LumenVpnService::class.java).apply {
            action = LumenVpnService.ACTION_START_VPN
            putExtra(LumenVpnService.EXTRA_ENGINE_TYPE, s.engine)
            putExtra(LumenVpnService.EXTRA_CONFIG_JSON, configJson)
            putExtra(LumenVpnService.EXTRA_SPLIT_MODE, _splitMode.value.name)
            putStringArrayListExtra(
                LumenVpnService.EXTRA_SPLIT_PACKAGES,
                ArrayList(_splitPackages.value)
            )
            putExtra(LumenVpnService.EXTRA_MTU, s.mtu.coerceIn(1280, 9000))
            putExtra(LumenVpnService.EXTRA_LOCAL_SOCKS_PORT, s.localSocksPort.coerceIn(1024, 65535))
            putExtra(LumenVpnService.EXTRA_DNS_MODE, s.dnsMode)
        }
    }

    fun buildStopIntent(context: Context): Intent =
        Intent(context, LumenVpnService::class.java).apply {
            action = LumenVpnService.ACTION_STOP_VPN
        }

    private fun parseEntity(entity: NodeEntity): ParsedNode {
        try {
            if (entity.isAutoNode || entity.protocol.equals("auto", true) || entity.link.trim().equals("auto", true)) {
                // An imported AUTO group stores its server pool in outboundJson.
                val storedAuto = entity.outboundJson.trim()
                val autoOutbound = if (storedAuto.startsWith("{")) {
                    runCatching { LinkParser.jsonToMap(JSONObject(storedAuto)) }.getOrDefault(emptyMap())
                } else emptyMap()
                return ParsedNode(
                    name = entity.name,
                    scheme = "auto",
                    server = "",
                    port = 0,
                    link = entity.link,
                    outbound = autoOutbound
                )
            }

            var parsed: ParsedNode? = null

            val linkText = entity.link.trim()

            // AWG/WireGuard: the stored outbound JSON (with the singbox endpoint map)
            // is authoritative — re-parsing the link can lose the private key when it
            // contains base64 characters. Fixes "missing private key" on connect.
            if (entity.protocol.lowercase(Locale.US) in setOf("awg", "wireguard", "wg")) {
                val storedJson = entity.outboundJson.trim()
                if (storedJson.startsWith("{") && storedJson.contains("singbox")) {
                    try {
                        val outboundMap = LinkParser.jsonToMap(JSONObject(storedJson))
                        if (outboundMap.containsKey("singbox")) {
                            parsed = ParsedNode(
                                name = entity.name.ifBlank { entity.server },
                                scheme = entity.protocol,
                                server = entity.server,
                                port = entity.port,
                                link = entity.link,
                                outbound = outboundMap
                            )
                        }
                    } catch (_: Exception) {}
                }
            }

            if (parsed == null && linkText.isNotBlank() && !linkText.startsWith("{") && !linkText.startsWith("[")) {
                try {
                    val (nodes, _) = LinkParser.parseLinksText(linkText)
                    parsed = nodes.firstOrNull()
                } catch (_: Exception) {}
            }

            if (parsed == null) {
                val jsonCandidate = when {
                    linkText.startsWith("{") -> linkText
                    entity.outboundJson.trim().startsWith("{") -> entity.outboundJson.trim()
                    else -> ""
                }
                if (jsonCandidate.isNotEmpty()) {
                    try {
                        val json = JSONObject(jsonCandidate)
                        parsed = LinkParser.parseJsonObjectOutbound(json)
                    } catch (_: Exception) {}
                }
            }

            if (parsed == null) {
                var outboundMap: Map<String, Any?> = emptyMap()
                if (entity.outboundJson.trim().startsWith("{")) {
                    try {
                        outboundMap = LinkParser.jsonToMap(JSONObject(entity.outboundJson.trim()))
                    } catch (_: Exception) {}
                }
                parsed = ParsedNode(
                    name = entity.name.ifBlank { entity.server },
                    scheme = entity.protocol.ifBlank { "unknown" },
                    server = entity.server,
                    port = entity.port,
                    link = entity.link,
                    outbound = outboundMap
                )
            }

            if (parsed.name.isBlank()) parsed.name = entity.name
            if (parsed.server.isBlank()) parsed.server = entity.server
            if (parsed.port <= 0) parsed.port = entity.port

            return parsed
        } catch (e: Exception) {
            log("Parse error for ${entity.name}: ${e.message}")
            return ParsedNode(
                name = entity.name.ifBlank { "Node" },
                scheme = entity.protocol.ifBlank { "unknown" },
                server = entity.server,
                port = entity.port,
                link = entity.link,
                outbound = emptyMap()
            )
        }
    }

    private fun extractDisplayProtocol(rawProtocol: String, outboundJson: String?, link: String?): String {
        val jsonUpper = outboundJson?.uppercase() ?: ""
        val linkUpper = link?.uppercase() ?: ""

        val isAwg = rawProtocol.equals("awg", ignoreCase = true) ||
                rawProtocol.equals("amneziawg", ignoreCase = true) ||
                jsonUpper.contains("\"AMNEZIA\"") ||
                jsonUpper.contains("\"JC\"") ||
                jsonUpper.contains("\"JMIN\"") ||
                jsonUpper.contains("\"H1\"") ||
                jsonUpper.contains("\"S1\"") ||
                linkUpper.contains("AWG://") ||
                linkUpper.contains("AMNEZIA-WG") ||
                linkUpper.contains("JC=") ||
                linkUpper.contains("H1=") ||
                linkUpper.contains("S1=")

        val proto = when {
            isAwg -> "AWG"
            rawProtocol.equals("wireguard", ignoreCase = true) || rawProtocol.equals("wg", ignoreCase = true) -> "WireGuard"
            rawProtocol.equals("openvpn", ignoreCase = true) -> "OpenVPN"
            rawProtocol.equals("hysteria2", ignoreCase = true) || rawProtocol.equals("hy2", ignoreCase = true) -> "Hysteria2"
            rawProtocol.equals("tuic", ignoreCase = true) -> "TUIC"
            rawProtocol.equals("shadowsocks", ignoreCase = true) || rawProtocol.equals("ss", ignoreCase = true) -> "Shadowsocks"
            else -> rawProtocol.trim().uppercase()
        }

        if (proto == "AWG" || proto == "WireGuard" || proto == "OpenVPN" || proto == "Hysteria2" || proto == "TUIC" || proto == "Shadowsocks") {
            return proto
        }

        val security = when {
            jsonUpper.contains("\"REALITY\"") || linkUpper.contains("SECURITY=REALITY") || linkUpper.contains("PBK=") -> "REALITY"
            jsonUpper.contains("\"TLS\"") || linkUpper.contains("SECURITY=TLS") -> "TLS"
            else -> ""
        }

        val network = when {
            jsonUpper.contains("\"XHTTP\"") || linkUpper.contains("TYPE=XHTTP") || linkUpper.contains("HEADER=XHTTP") -> "XHTTP"
            jsonUpper.contains("\"HTTPUPGRADE\"") || linkUpper.contains("TYPE=HTTPUPGRADE") -> "HTTPUpgrade"
            jsonUpper.contains("\"GRPC\"") || linkUpper.contains("TYPE=GRPC") -> "gRPC"
            jsonUpper.contains("\"WS\"") || linkUpper.contains("TYPE=WS") -> "WS"
            jsonUpper.contains("\"H2\"") || linkUpper.contains("TYPE=H2") -> "H2"
            else -> ""
        }

        val subType = when {
            security == "REALITY" -> "REALITY"
            network.isNotEmpty() && security.isNotEmpty() -> "$security/$network"
            network.isNotEmpty() -> network
            security.isNotEmpty() -> security
            else -> ""
        }

        return if (subType.isNotEmpty()) "$proto/$subType" else proto
    }

    private fun stripFlagEmoji(value: String): String {
        val result = StringBuilder()
        value.codePoints().forEach { codePoint ->
            if (codePoint !in 0x1F1E6..0x1F1FF && codePoint != 0xFE0F) {
                result.appendCodePoint(codePoint)
            }
        }
        return result.toString()
            .replace(Regex("^[\\s|•·:—–-]+|[\\s|•·:—–-]+$"), "")
            .replace(Regex("\\s{2,}"), " ")
            .trim()
    }

    companion object {
        const val PREFS_NAME = "lumen_prefs"
    }
}
