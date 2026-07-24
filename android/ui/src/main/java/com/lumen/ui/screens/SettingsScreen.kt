package com.lumen.ui.screens

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Settings
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
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp

private val LANGUAGES = listOf("en", "ru", "fa", "zh")
private enum class SettingsPage { HUB, SUBSCRIPTIONS, TRAFFIC, APP, THEME }

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

    if (page == SettingsPage.THEME) {
        ThemeSettingsScreen(
            state = state,
            onUpdate = onUpdate,
            onBack = { page = SettingsPage.HUB },
            modifier = modifier
        )
        return
    }

    AnimatedContent(
        targetState = page,
        transitionSpec = {
            if (targetState != SettingsPage.HUB) {
                (slideInHorizontally(tween(190, easing = FastOutSlowInEasing)) { it / 4 } + fadeIn(tween(190)))
                    .togetherWith(slideOutHorizontally(tween(190, easing = FastOutSlowInEasing)) { -it / 4 } + fadeOut(tween(190)))
            } else {
                (slideInHorizontally(tween(190, easing = FastOutSlowInEasing)) { -it / 4 } + fadeIn(tween(190)))
                    .togetherWith(slideOutHorizontally(tween(190, easing = FastOutSlowInEasing)) { it / 4 } + fadeOut(tween(190)))
            }
        },
        label = "settings_page_transition"
    ) { currentPage ->
        Column(
            modifier.fillMaxSize().padding(horizontal = 16.dp)
        ) {
            when (currentPage) {
                SettingsPage.HUB -> SettingsHub(
                    onTheme = { page = SettingsPage.THEME },
                    onSubscriptions = { page = SettingsPage.SUBSCRIPTIONS },
                    onTraffic = { page = SettingsPage.TRAFFIC },
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
                SettingsPage.APP -> {
                    LumenScreenHeader(title = s.appSettings, onBack = { page = SettingsPage.HUB })
                    AppSettings(state, onUpdate, onLanguageChange)
                }
                SettingsPage.THEME -> Unit
            }
            Spacer(Modifier.height(84.dp))
        }
    }
}

@Composable
private fun SettingsHub(
    onTheme: () -> Unit,
    onSubscriptions: () -> Unit,
    onTraffic: () -> Unit,
    onApp: () -> Unit,
    onRouting: () -> Unit,
    onLogs: () -> Unit,
    onCommunity: () -> Unit
) {
    val s = LocalStrings.current
    LumenScreenHeader(title = s.settings)
    Spacer(Modifier.height(8.dp))
    SettingsCard {
        SettingsMenuRow(Icons.Filled.Settings, s.themeSettings, onTheme)
        SettingsDivider()
        SettingsMenuRow(Icons.AutoMirrored.Filled.List, s.subscriptionSettings, onSubscriptions)
        SettingsDivider()
        SettingsMenuRow(Icons.AutoMirrored.Filled.Send, s.trafficSettings, onTraffic)
        SettingsDivider()
        SettingsMenuRow(Icons.AutoMirrored.Filled.List, s.routing, onRouting)
        SettingsDivider()
        SettingsMenuRow(Icons.Filled.Settings, s.appSettings, onApp)
        SettingsDivider()
        SettingsMenuRow(Icons.Filled.Menu, s.logs, onLogs)
    }
    SectionHeader(s.infoSection)
    SettingsCard {
        InfoRow(s.version, "0.2.0")
        SettingsDivider()
        InfoRow("sing-box extended", "1.13.14-extended-2.5.2")
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
    LumenDropdown(
        label = "User-Agent",
        options = listOf(
            "Happ/2.18.3/Windows/2606241603601",
            "Lumen-Subscription/Android-0.2.0",
            "SFA/1.11.0",
            "clash.meta",
            "v2rayNG/1.10.31"
        ),
        selected = state.subscriptionUserAgent,
        onSelected = { onUpdate(state.copy(subscriptionUserAgent = it)) }
    )
    ToggleRow(
        s.directLoad,
        s.directLoadDesc,
        state.subscriptionDirect
    ) { onUpdate(state.copy(subscriptionDirect = it)) }
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
    SectionHeader(s.subscriptionProfile)
    ToggleRow(
        s.allowOverrides,
        s.allowOverridesDesc,
        state.allowSubscriptionOverrides
    ) { onUpdate(state.copy(allowSubscriptionOverrides = it)) }
}

@Composable
private fun TrafficSettings(
    state: SettingsUiState,
    onUpdate: (SettingsUiState) -> Unit
) {
    val s = LocalStrings.current
    SectionHeader("DNS")
    TextSettingField(s.proxyDns, state.proxyDnsServer) {
        onUpdate(state.copy(proxyDnsServer = it.take(253)))
    }
    TextSettingField(s.directDns, state.directDnsServer) {
        onUpdate(state.copy(directDnsServer = it.take(253)))
    }
    SectionHeader(s.autoSelect)
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
    SectionHeader(s.connection)
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
}

@Composable
private fun AppSettings(
    state: SettingsUiState,
    onUpdate: (SettingsUiState) -> Unit,
    onLanguageChange: (String) -> Unit
) {
    val s = LocalStrings.current
    SectionHeader(s.localProxy)
    ToggleRow(s.localInbound, s.localInboundDescription, state.localInboundEnabled) {
        onUpdate(state.copy(localInboundEnabled = it))
    }
    if (state.localInboundEnabled) {
        NumberField(s.socksPort, state.localSocksPort) {
            onUpdate(state.copy(localSocksPort = it.coerceIn(1024, 65535)))
        }
        NumberField(s.httpPort, state.localHttpPort) {
            onUpdate(state.copy(localHttpPort = it.coerceIn(1024, 65535)))
        }
        ToggleRow(s.allowLan, s.allowLanDescription, state.lanSharingEnabled) {
            onUpdate(state.copy(lanSharingEnabled = it))
        }
    }
    SectionHeader(s.language)
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
}
@Composable
internal fun SettingsCard(content: @Composable () -> Unit) {
    val shape = RoundedCornerShape(20.dp)
    Column(
        Modifier
            .fillMaxWidth()
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
            .background(MaterialTheme.colorScheme.outline.copy(alpha = 0.18f))
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
        Switch(checked = checked, onCheckedChange = onChange)
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
