package com.lumen.ui.screens

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.CloudDownload
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.Palette
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Speed
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp

// Shared slow-out curve for settings transitions and card entrance.
private val PremiumEasing = CubicBezierEasing(0.2f, 0f, 0f, 1f)

private val LANGUAGES = listOf("en", "ru", "fa", "zh")
private enum class SettingsPage { HUB, SUBSCRIPTIONS, TRAFFIC, DNS, PING, APP, THEME }

private fun languageLabel(code: String): String = when (code) {
    "ru" -> "Русский"
    "fa" -> "فارسی"
    "zh" -> "中文"
    else -> "English"
}

@Composable
fun SettingsScreen(
    state: SettingsUiState,
    onUpdate: (SettingsUiState) -> Unit,
    onLanguageChange: (String) -> Unit,
    onOpenRouting: () -> Unit,
    onOpenLogs: () -> Unit,
    onOpenCommunity: () -> Unit,
    resetToHubSignal: Int = 0,
    modifier: Modifier = Modifier
) {
    val s = LocalStrings.current
    var page by rememberSaveable { mutableStateOf(SettingsPage.HUB) }

    BackHandler(enabled = page != SettingsPage.HUB) {
        page = SettingsPage.HUB
    }

    androidx.compose.runtime.LaunchedEffect(resetToHubSignal) {
        if (resetToHubSignal > 0) {
            page = SettingsPage.HUB
        }
    }

    AnimatedContent(
        targetState = page,
        transitionSpec = {
            val dir = if (targetState != SettingsPage.HUB) 1 else -1
            (slideInHorizontally(tween(320, easing = PremiumEasing)) { dir * it / 6 } +
                fadeIn(tween(260, easing = PremiumEasing)) +
                scaleIn(tween(320, easing = PremiumEasing), initialScale = 0.98f))
                .togetherWith(
                    slideOutHorizontally(tween(320, easing = PremiumEasing)) { -dir * it / 6 } +
                        fadeOut(tween(180)) +
                        scaleOut(tween(320, easing = PremiumEasing), targetScale = 0.98f)
                )
        },
        label = "settings_page_transition"
    ) { currentPage ->
        // The customization page brings its own scroll container, so it renders
        // outside the shared Column but still inside the page animation.
        if (currentPage == SettingsPage.THEME) {
            ThemeSettingsScreen(
                state = state,
                onUpdate = onUpdate,
                onBack = { page = SettingsPage.HUB },
                modifier = modifier
            )
            return@AnimatedContent
        }
        Column(
            modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp)
        ) {
            when (currentPage) {
                SettingsPage.HUB -> SettingsHub(
                    onTheme = { page = SettingsPage.THEME },
                    onSubscriptions = { page = SettingsPage.SUBSCRIPTIONS },
                    onTraffic = { page = SettingsPage.TRAFFIC },
                    onDns = { page = SettingsPage.DNS },
                    onPing = { page = SettingsPage.PING },
                    onApp = { page = SettingsPage.APP },
                    onRouting = onOpenRouting,
                    onLogs = onOpenLogs,
                    onCommunity = onOpenCommunity
                )
                SettingsPage.SUBSCRIPTIONS -> {
                    LumenScreenHeader(title = s.subscriptionSettings, onBack = { page = SettingsPage.HUB })
                    SubscriptionSettings(state, onUpdate)
                }
                SettingsPage.TRAFFIC -> {
                    LumenScreenHeader(title = s.trafficSettings, onBack = { page = SettingsPage.HUB })
                    TrafficSettings(state, onUpdate)
                }
                SettingsPage.DNS -> {
                    LumenScreenHeader(title = s.dnsSettings, onBack = { page = SettingsPage.HUB })
                    DnsSettings(state, onUpdate)
                }
                SettingsPage.PING -> {
                    LumenScreenHeader(title = s.pingSettings, onBack = { page = SettingsPage.HUB })
                    PingSettings(state, onUpdate)
                }
                SettingsPage.APP -> {
                    LumenScreenHeader(title = s.appSettings, onBack = { page = SettingsPage.HUB })
                    AppSettings(state, onUpdate, onLanguageChange)
                }
                SettingsPage.THEME -> Unit
            }
            // The scaffold already reserves the nav pill height, so only a small
            // breathing gap is needed; 140dp let the page scroll into emptiness.
            Spacer(Modifier.height(16.dp))
        }
    }
}

@Composable
private fun SettingsHub(
    onTheme: () -> Unit,
    onSubscriptions: () -> Unit,
    onTraffic: () -> Unit,
    onDns: () -> Unit,
    onPing: () -> Unit,
    onApp: () -> Unit,
    onRouting: () -> Unit,
    onLogs: () -> Unit,
    onCommunity: () -> Unit
) {
    val s = LocalStrings.current
    LumenScreenHeader(title = s.settings)
    Spacer(Modifier.height(8.dp))
    SectionHeader(s.categoryAppearance)
    SettingsCard {
        SettingsMenuRow(Icons.Filled.Palette, s.themeSettings, onTheme)
        SettingsDivider()
        SettingsMenuRow(Icons.Filled.Settings, s.appSettings, onApp)
    }
    SectionHeader(s.categoryConnection)
    SettingsCard {
        SettingsMenuRow(Icons.AutoMirrored.Filled.Send, s.trafficSettings, onTraffic)
        SettingsDivider()
        SettingsMenuRow(Icons.Filled.Dns, s.dnsSettings, onDns)
        SettingsDivider()
        SettingsMenuRow(Icons.Filled.Speed, s.pingSettings, onPing)
    }
    SectionHeader(s.categoryTunnel)
    SettingsCard {
        SettingsMenuRow(Icons.AutoMirrored.Filled.List, s.routing, onRouting)
    }
    SectionHeader(s.categoryProviders)
    SettingsCard {
        SettingsMenuRow(Icons.Filled.CloudDownload, s.subscriptionSettings, onSubscriptions)
    }
    SectionHeader(s.categoryOther)
    SettingsCard {
        SettingsMenuRow(Icons.Filled.Menu, s.logs, onLogs)
    }
    SectionHeader(s.infoSection)
    SettingsCard {
        InfoRow(s.version, LumenVersion.appVersion)
        SettingsDivider()
        InfoRow("sing-box extended", LumenVersion.ENGINE)
    }
    Spacer(Modifier.height(18.dp))
    SettingsCard {
        SettingsMenuRow(Icons.Filled.Person, s.community, onCommunity)
    }
}

@Composable
private fun SubscriptionSettings(state: SettingsUiState, onUpdate: (SettingsUiState) -> Unit) {
    val s = LocalStrings.current
    SectionHeader(s.requestParameters)
    SettingsCard {
    Spacer(Modifier.height(10.dp))
    LumenDropdown(
        label = "User-Agent",
        options = listOf(
            "Happ/2.18.3/Windows/2606241603601",
            "Lumen-Subscription/Android-${LumenVersion.appVersion}",
            "SFA/1.11.0",
            "clash.meta",
            "v2rayNG/1.10.31"
        ),
        selected = state.subscriptionUserAgent,
        onSelected = { onUpdate(state.copy(subscriptionUserAgent = it)) }
    )
    ToggleRow(
        s.sendHwid,
        s.sendHwidDesc,
        state.subscriptionSendHwid
    ) { onUpdate(state.copy(subscriptionSendHwid = it)) }
    if (state.subscriptionSendHwid) {
        TextSettingField("HWID", state.subscriptionHwid) {
            onUpdate(state.copy(subscriptionHwid = it.take(256).replace("\r", "").replace("\n", "")))
        }
    }
    ToggleRow(
        s.subscriptionUseProxyTun,
        s.subscriptionUseProxyTunDesc,
        state.subscriptionUseProxyTun
    ) { onUpdate(state.copy(subscriptionUseProxyTun = it)) }
    Spacer(Modifier.height(6.dp))
    }
    SectionHeader(s.subscriptionAutoUpdate)
    SettingsCard {
    Spacer(Modifier.height(4.dp))
    ToggleRow(
        s.subscriptionAutoUpdate,
        s.subscriptionAutoUpdateDesc,
        state.subscriptionAutoUpdateMinutes > 0
    ) { onUpdate(state.copy(subscriptionAutoUpdateMinutes = if (it) 240 else 0)) }
    if (state.subscriptionAutoUpdateMinutes > 0) {
        NumberField(s.subscriptionAutoUpdateInterval, state.subscriptionAutoUpdateMinutes) {
            onUpdate(state.copy(subscriptionAutoUpdateMinutes = it.coerceIn(15, 1440)))
        }
    }
    TextSettingField(s.subscriptionIncludeRegexLabel, state.subscriptionIncludeRegex) {
        onUpdate(state.copy(subscriptionIncludeRegex = it.take(512)))
    }
    TextSettingField(s.subscriptionExcludeRegexLabel, state.subscriptionExcludeRegex) {
        onUpdate(state.copy(subscriptionExcludeRegex = it.take(512)))
    }
    Spacer(Modifier.height(4.dp))
    }
    SectionHeader(s.subscriptionConverter)
    SettingsCard {
    Spacer(Modifier.height(4.dp))
    ToggleRow(
        s.subscriptionConverter,
        s.subscriptionConverterDesc,
        state.subscriptionConverterEnabled
    ) { onUpdate(state.copy(subscriptionConverterEnabled = it)) }
    if (state.subscriptionConverterEnabled) {
        TextSettingField(s.subscriptionConverterUrlLabel, state.subscriptionConverterUrl) {
            onUpdate(state.copy(subscriptionConverterUrl = it.take(512)))
        }
    }
    Spacer(Modifier.height(4.dp))
    }
    SectionHeader(s.subscriptionProfile)
    SettingsCard {
    Spacer(Modifier.height(4.dp))
    ToggleRow(
        s.allowOverrides,
        s.allowOverridesDesc,
        state.allowSubscriptionOverrides
    ) { onUpdate(state.copy(allowSubscriptionOverrides = it)) }
    Spacer(Modifier.height(4.dp))
    }
}

private val DNS_MODES = listOf("automatic", "android", "secure", "json")

private fun dnsModeLabel(mode: String, s: LumenStrings): String = when (mode) {
    "android" -> s.dnsModeAndroid
    "secure" -> s.dnsModeSecure
    "json" -> s.dnsModeJson
    else -> s.dnsModeAuto
}

private fun dnsModeHint(mode: String, s: LumenStrings): String = when (mode) {
    "android" -> s.dnsModeAndroidHint
    "secure" -> s.dnsModeSecureHint
    "json" -> s.dnsModeJsonHint
    else -> s.dnsModeAutoHint
}

@Composable
private fun DnsSettings(
    state: SettingsUiState,
    onUpdate: (SettingsUiState) -> Unit
) {
    val s = LocalStrings.current
    SectionHeader(s.dnsModeSection)
    SettingsCard {
        Spacer(Modifier.height(12.dp))
        // Two rows of chips instead of a dropdown: mode is the most-used switch here.
        DNS_MODES.chunked(2).forEach { row ->
            Row(
                Modifier.fillMaxWidth().padding(bottom = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                row.forEach { mode ->
                    DnsModeChip(
                        label = dnsModeLabel(mode, s),
                        selected = state.dnsMode == mode,
                        modifier = Modifier.weight(1f)
                    ) { onUpdate(state.copy(dnsMode = mode)) }
                }
            }
        }
        Text(
            dnsModeHint(state.dnsMode, s),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(bottom = 14.dp)
        )
    }

    SectionHeader(s.dnsDirectSection)
    SettingsCard {
        Spacer(Modifier.height(8.dp))
        TextAreaSettingField(s.dnsServersLabel, state.dnsDirectServers) {
            onUpdate(state.copy(dnsDirectServers = it.take(2048)))
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            LumenDropdown(
                label = s.dnsTypeLabel,
                options = listOf("udp", "tcp", "tls", "https"),
                selected = state.dnsDirectType,
                onSelected = { onUpdate(state.copy(dnsDirectType = it)) },
                modifier = Modifier.weight(1f)
            )
            LumenDropdown(
                label = s.dnsStrategyLabel,
                options = listOf("prefer_ipv4", "prefer_ipv6", "ipv4_only", "ipv6_only"),
                selected = state.dnsDirectStrategy,
                onSelected = { onUpdate(state.copy(dnsDirectStrategy = it)) },
                modifier = Modifier.weight(1f)
            )
        }
        Spacer(Modifier.height(8.dp))
    }

    SectionHeader(s.dnsProxySection)
    SettingsCard {
        Spacer(Modifier.height(8.dp))
        TextAreaSettingField(s.dnsServersLabel, state.dnsProxyServers) {
            onUpdate(state.copy(dnsProxyServers = it.take(2048)))
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            LumenDropdown(
                label = s.dnsTypeLabel,
                options = listOf("udp", "tcp", "tls", "https"),
                selected = state.dnsProxyType,
                onSelected = { onUpdate(state.copy(dnsProxyType = it)) },
                modifier = Modifier.weight(1f)
            )
            LumenDropdown(
                label = s.dnsStrategyLabel,
                options = listOf("prefer_ipv4", "prefer_ipv6", "ipv4_only", "ipv6_only"),
                selected = state.dnsProxyStrategy,
                onSelected = { onUpdate(state.copy(dnsProxyStrategy = it)) },
                modifier = Modifier.weight(1f)
            )
        }
        Spacer(Modifier.height(4.dp))
        SettingsDivider()
        ToggleRow(s.dnsIpv4Only, s.dnsIpv4OnlyDesc, state.dnsProxyIpv4Only) {
            onUpdate(state.copy(dnsProxyIpv4Only = it))
        }
        Spacer(Modifier.height(4.dp))
    }

    SectionHeader(s.dnsBehaviorSection)
    SettingsCard {
        Spacer(Modifier.height(4.dp))
        ToggleRow(s.dnsHijack, s.dnsHijackDesc, state.dnsHijackEnabled) {
            onUpdate(state.copy(dnsHijackEnabled = it))
        }
        SettingsDivider()
        ToggleRow(s.dnsFakeIp, s.dnsFakeIpDesc, state.dnsFakeIpEnabled) {
            onUpdate(state.copy(dnsFakeIpEnabled = it))
        }
        SettingsDivider()
        ToggleRow(s.dnsParallel, s.dnsParallelDesc, state.dnsParallelQuery) {
            onUpdate(state.copy(dnsParallelQuery = it))
        }
        SettingsDivider()
        ToggleRow(s.dnsOptimistic, s.dnsOptimisticDesc, state.dnsOptimisticCache) {
            onUpdate(state.copy(dnsOptimisticCache = it))
        }
        SettingsDivider()
        ToggleRow(s.dnsGeoCheck, s.dnsGeoCheckDesc, state.dnsGeoCheck) {
            onUpdate(state.copy(dnsGeoCheck = it))
        }
        Spacer(Modifier.height(4.dp))
    }

    SectionHeader(s.dnsHostsSection)
    SettingsCard {
        Spacer(Modifier.height(8.dp))
        TextAreaSettingField(s.dnsHostsLabel, state.dnsHosts) { onUpdate(state.copy(dnsHosts = it.take(4096))) }
        SettingsDivider()
        ToggleRow(s.dnsOverride, s.dnsOverrideDesc, state.dnsOverrideEnabled) {
            onUpdate(state.copy(dnsOverrideEnabled = it))
        }
        if (state.dnsOverrideEnabled) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Box(Modifier.weight(1.4f)) {
                    TextSettingField(s.dnsHostname, state.dnsOverrideHostname) {
                        onUpdate(state.copy(dnsOverrideHostname = it.take(253)))
                    }
                }
                Box(Modifier.weight(1f)) {
                    TextSettingField(s.dnsIpv4, state.dnsOverrideIpv4) {
                        onUpdate(state.copy(dnsOverrideIpv4 = it.take(15)))
                    }
                }
            }
        }
        Spacer(Modifier.height(8.dp))
    }
}

@Composable
private fun DnsModeChip(
    label: String,
    selected: Boolean,
    modifier: Modifier = Modifier,
    onClick: () -> Unit
) {
    val shape = RoundedCornerShape(12.dp)
    val accent = MaterialTheme.colorScheme.primary
    Box(
        modifier
            .clip(shape)
            .background(if (selected) accent.copy(alpha = 0.18f) else MaterialTheme.colorScheme.surface)
            .border(1.dp, if (selected) accent else MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f), shape)
            .clickable(onClick = onClick)
            .padding(vertical = 10.dp),
        contentAlignment = Alignment.Center
    ) {
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
            color = if (selected) accent else MaterialTheme.colorScheme.onSurface
        )
    }
}

@Composable
private fun TrafficSettings(
    state: SettingsUiState,
    onUpdate: (SettingsUiState) -> Unit
) {
    val s = LocalStrings.current
    SectionHeader(s.connection)
    SettingsCard {
    Spacer(Modifier.height(4.dp))
    ToggleRow("Multiplex (MUX)", s.muxDescription, state.muxEnabled) {
        onUpdate(state.copy(muxEnabled = it))
    }
    if (state.muxEnabled) NumberField(s.muxConcurrency, state.muxConcurrency) {
        onUpdate(state.copy(muxConcurrency = it.coerceIn(1, 1024)))
    }
    ToggleRow(s.tlsFragmentation, s.tlsDescription, state.fragmentEnabled) {
        onUpdate(state.copy(fragmentEnabled = it))
    }
    if (state.fragmentEnabled) {
        TextSettingField(s.fragmentPackets, state.fragmentPackets) { onUpdate(state.copy(fragmentPackets = it)) }
        TextSettingField(s.fragmentLength, state.fragmentLength) { onUpdate(state.copy(fragmentLength = it)) }
        TextSettingField(s.fragmentDelay, state.fragmentDelay) { onUpdate(state.copy(fragmentDelay = it)) }
    }
    NumberField(s.tunnelMtu, state.mtu) { onUpdate(state.copy(mtu = it.coerceIn(1280, 9000))) }
    ToggleRow(s.preferIpv6, s.preferIpv6Description, state.preferIpv6) {
        onUpdate(state.copy(preferIpv6 = it))
    }
    ToggleRow(s.blockQuic, s.blockQuicDescription, state.blockQuic) {
        onUpdate(state.copy(blockQuic = it))
    }
    ToggleRow(s.sniffRouteOnly, s.sniffRouteOnlyDescription, state.sniffRouteOnly) {
        onUpdate(state.copy(sniffRouteOnly = it))
    }
    Spacer(Modifier.height(4.dp))
    }
}

@Composable
private fun PingSettings(
    state: SettingsUiState,
    onUpdate: (SettingsUiState) -> Unit
) {
    val s = LocalStrings.current
    SectionHeader(s.ping)
    SettingsCard {
        Spacer(Modifier.height(10.dp))
        LumenDropdown(
            label = s.pingTypeLabel,
            options = PING_TYPES,
            selected = state.pingType,
            onSelected = { onUpdate(state.copy(pingType = it)) },
            optionLabel = { it.uppercase() }
        )
        if (state.pingType == "url") {
            TextSettingField(s.pingUrlLabel, state.pingUrl) { onUpdate(state.copy(pingUrl = it.take(256))) }
        }
        NumberField(s.pingTimeoutLabel, state.pingTimeoutMs) {
            onUpdate(state.copy(pingTimeoutMs = it.coerceIn(500, 20000)))
        }
        NumberField(s.pingConcurrencyLabel, state.pingConcurrency) {
            onUpdate(state.copy(pingConcurrency = it.coerceIn(1, 32)))
        }
        Spacer(Modifier.height(6.dp))
    }
    SectionHeader(s.behavior)
    SettingsCard {
        Spacer(Modifier.height(4.dp))
        ToggleRow(s.pingSortAfter, s.pingSortAfterDesc, state.pingSortAfter) {
            onUpdate(state.copy(pingSortAfter = it))
        }
        Spacer(Modifier.height(4.dp))
    }
    // url-test settings live next to ping: both measure server latency.
    SectionHeader(s.autoSelect)
    SettingsCard {
        Spacer(Modifier.height(10.dp))
        LumenDropdown(
            label = s.autoSelectUrl,
            options = listOf(
                "https://www.gstatic.com/generate_204",
                "https://cp.cloudflare.com/generate_204",
                "https://www.google.com/generate_204",
                "https://connectivitycheck.platform.hicloud.com/generate_204"
            ),
            selected = state.urlTestUrl,
            onSelected = { onUpdate(state.copy(urlTestUrl = it)) }
        )
        NumberField(s.checkInterval, state.urlTestIntervalMinutes) {
            onUpdate(state.copy(urlTestIntervalMinutes = it.coerceIn(1, 1440)))
        }
        NumberField(s.toleranceMs, state.urlTestToleranceMs) {
            onUpdate(state.copy(urlTestToleranceMs = it.coerceIn(0, 5000)))
        }
        Spacer(Modifier.height(6.dp))
    }
}

@Composable
private fun AppSettings(
    state: SettingsUiState,
    onUpdate: (SettingsUiState) -> Unit,
    onLanguageChange: (String) -> Unit
) {
    val s = LocalStrings.current
    SectionHeader(s.behavior)
    SettingsCard {
        Spacer(Modifier.height(4.dp))
        ToggleRow(s.vibration, s.vibrationDesc, state.hapticsEnabled) {
            onUpdate(state.copy(hapticsEnabled = it))
        }
        SettingsDivider()
        ToggleRow(s.autoConnect, s.autoConnectDescription, state.autoConnectOnBoot) {
            onUpdate(state.copy(autoConnectOnBoot = it))
        }
        SettingsDivider()
        ToggleRow(s.speedStats, s.speedStatsDesc, state.enableSpeedStats) {
            onUpdate(state.copy(enableSpeedStats = it))
        }
        SettingsDivider()
        ToggleRow(s.showNotification, s.showNotificationDesc, state.showNotification) {
            onUpdate(state.copy(showNotification = it))
        }
        if (state.showNotification) {
            SettingsDivider()
            ToggleRow(s.notificationSpeed, s.notificationSpeedDesc, state.showNotificationSpeed) {
                onUpdate(state.copy(showNotificationSpeed = it))
            }
        }
        Spacer(Modifier.height(4.dp))
    }
    SectionHeader(s.localProxy)
    SettingsCard {
        Spacer(Modifier.height(4.dp))
        ToggleRow(s.localInbound, s.localInboundDescription, state.localInboundEnabled) {
            onUpdate(state.copy(localInboundEnabled = it))
        }
        if (state.localInboundEnabled) {
            SettingsDivider()
            Spacer(Modifier.height(4.dp))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Box(Modifier.weight(1f)) {
                    NumberField(s.socksPort, state.localSocksPort) {
                        onUpdate(state.copy(localSocksPort = it.coerceIn(1024, 65535)))
                    }
                }
                Box(Modifier.weight(1f)) {
                    NumberField(s.httpPort, state.localHttpPort) {
                        onUpdate(state.copy(localHttpPort = it.coerceIn(1024, 65535)))
                    }
                }
            }
            SettingsDivider()
            ToggleRow(s.allowLan, s.allowLanDescription, state.lanSharingEnabled) {
                onUpdate(state.copy(lanSharingEnabled = it))
            }
        }
        Spacer(Modifier.height(4.dp))
    }
    SectionHeader(s.language)
    SettingsCard {
        Spacer(Modifier.height(10.dp))
        LumenDropdown(
            label = "",
            options = LANGUAGES,
            selected = state.language.ifBlank { "en" },
            onSelected = {
                onUpdate(state.copy(language = it))
                onLanguageChange(it)
            },
            optionLabel = { languageLabel(it) }
        )
        Spacer(Modifier.height(10.dp))
    }
}
@Composable
internal fun SettingsCard(content: @Composable () -> Unit) {
    val shape = RoundedCornerShape(20.dp)
    // One-shot fade + lift so cards settle in instead of popping.
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { shown = true }
    val appear by animateFloatAsState(
        targetValue = if (shown) 1f else 0f,
        animationSpec = tween(340, easing = PremiumEasing),
        label = "settings_card_appear"
    )
    Column(
        Modifier
            .fillMaxWidth()
            .graphicsLayer {
                alpha = appear
                translationY = (1f - appear) * 24f
                scaleX = 0.98f + 0.02f * appear
                scaleY = scaleX
            }
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f), shape)
            .padding(horizontal = 14.dp)
    ) { content() }
}

@Composable
private fun SettingsMenuRow(icon: ImageVector, title: String, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(vertical = 15.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(
            icon,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.primary,
            modifier = Modifier.size(26.dp)
        )
        Spacer(Modifier.width(14.dp))
        Text(title, style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
        Text("›", style = MaterialTheme.typography.headlineMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun InfoRow(title: String, value: String) {
    Row(
        Modifier.fillMaxWidth().padding(vertical = 15.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(title, style = MaterialTheme.typography.titleMedium)
        Text(
            value,
            style = MaterialTheme.typography.titleMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    }
}

@Composable
internal fun SettingsDivider() {
    Spacer(
        Modifier.fillMaxWidth().height(1.dp)
            .background(MaterialTheme.colorScheme.outline.copy(alpha = 0.3f))
    )
}

@Composable
private fun ToggleRow(title: String, description: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(vertical = 6.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(Modifier.weight(1f)) {
            Text(title)
            Text(description, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        LumenSwitch(checked = checked, onCheckedChange = onChange)
    }
}

@Composable
private fun NumberField(label: String, value: Int, onChange: (Int) -> Unit) {
    var text by remember(value) { mutableStateOf(value.toString()) }
    OutlinedTextField(
        value = text,
        onValueChange = { input -> text = input; input.toIntOrNull()?.let(onChange) },
        label = { Text(label) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)
    )
}

@Composable
private fun TextSettingField(label: String, value: String, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        singleLine = true,
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)
    )
}

@Composable
private fun TextAreaSettingField(label: String, value: String, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        minLines = 3,
        maxLines = 6,
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)
    )
}
