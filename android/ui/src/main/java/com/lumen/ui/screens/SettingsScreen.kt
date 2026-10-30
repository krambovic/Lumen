package com.lumen.ui.screens







import androidx.compose.foundation.interaction.collectIsPressedAsState

import androidx.compose.ui.draw.drawBehind



import androidx.compose.material.icons.filled.Code



import androidx.compose.material.icons.filled.Favorite



import androidx.compose.material.icons.filled.ContentCopy



import androidx.compose.material.icons.filled.PhoneAndroid



import androidx.compose.material.icons.filled.AltRoute



import androidx.compose.material.icons.filled.Description



import androidx.compose.material.icons.filled.SystemUpdate



import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight



import androidx.compose.foundation.layout.IntrinsicSize



import androidx.compose.foundation.layout.fillMaxHeight



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



import androidx.compose.foundation.text.KeyboardActions



import androidx.compose.foundation.text.KeyboardOptions



import androidx.compose.foundation.layout.width



import androidx.compose.foundation.verticalScroll



import com.lumen.ui.components.expressiveControlLabel
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.Icons



import androidx.compose.material.icons.automirrored.filled.List



import androidx.compose.material.icons.automirrored.filled.Send



import androidx.compose.material.icons.filled.CloudDownload



import androidx.compose.material.icons.filled.Dns



import androidx.compose.material.icons.filled.Menu



import androidx.compose.material.icons.filled.Palette



import androidx.compose.material.icons.filled.Person



import androidx.compose.material.icons.filled.Vibration

import androidx.compose.material.icons.filled.Notifications

import androidx.compose.material.icons.filled.PowerSettingsNew

import androidx.compose.material.icons.filled.Shield

import androidx.compose.material.icons.filled.Sync

import androidx.compose.foundation.selection.toggleable

import androidx.compose.ui.semantics.Role

import com.lumen.ui.components.BrandTopBar

import androidx.compose.material.icons.filled.Settings



import androidx.compose.material.icons.filled.Speed



import androidx.compose.material3.Button



import androidx.compose.material3.Icon



import androidx.compose.material3.LinearProgressIndicator



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
import androidx.compose.runtime.saveable.rememberSaveableStateHolder



import androidx.compose.runtime.setValue



import androidx.compose.ui.Alignment



import androidx.compose.ui.Modifier



import androidx.compose.ui.draw.clip



import androidx.compose.ui.focus.onFocusChanged



import androidx.compose.ui.graphics.Brush



import androidx.compose.ui.graphics.Color



import androidx.compose.ui.graphics.luminance



import androidx.compose.ui.graphics.graphicsLayer



import androidx.compose.runtime.LaunchedEffect



import androidx.compose.ui.graphics.vector.ImageVector



import androidx.compose.ui.text.font.FontWeight



import androidx.compose.ui.text.input.ImeAction



import androidx.compose.ui.text.input.KeyboardType



import androidx.compose.ui.unit.dp

import androidx.compose.ui.unit.sp

import androidx.compose.foundation.layout.offset







// Shared slow-out curve for settings transitions and card entrance.



private val PremiumEasing = CubicBezierEasing(0.2f, 0f, 0f, 1f)







private val LANGUAGES = listOf("en", "ru", "fa", "zh")



private enum class SettingsPage { HUB, SUBSCRIPTIONS, TRAFFIC, DNS, PING, APP, THEME, UPDATES }







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



    updateChecked: Boolean = false,



    updateIsChecking: Boolean = false,



    updateLatestVersion: String? = null,



    updateReleaseTag: String? = null,



    updateAvailable: Boolean = false,



    updateError: String? = null,



    updateIsDownloading: Boolean = false,



    updateDownloadProgress: Int? = null,



    onCheckUpdates: (Boolean) -> Unit = {},



    onInstallUpdate: () -> Unit = {},



    resetToHubSignal: Int = 0,
    onReset: (LumenResetTarget) -> Unit = {},
    onOpenWelcome: (() -> Unit)? = null,
    onPageOpened: () -> Unit = {},



    modifier: Modifier = Modifier



) {



    val s = LocalStrings.current



    var page by rememberSaveable { mutableStateOf(SettingsPage.HUB) }
    androidx.compose.runtime.LaunchedEffect(page) { onPageOpened() }







    BackHandler(enabled = page != SettingsPage.HUB) {



        page = SettingsPage.HUB



    }







    androidx.compose.runtime.LaunchedEffect(resetToHubSignal) {



        if (resetToHubSignal > 0) {



            page = SettingsPage.HUB



        }



    }







    val pageStates = rememberSaveableStateHolder()
    AnimatedContent(



        targetState = page,



        transitionSpec = {
            val forward = targetState != SettingsPage.HUB
            com.lumen.ui.components.expressivePageEnter(forward)
                .togetherWith(com.lumen.ui.components.expressivePageExit(forward))
        },



        label = "settings_page_transition"



    ) { currentPage ->
        pageStates.SaveableStateProvider(currentPage.name) {



        // The customization page brings its own scroll container, so it renders



        // outside the shared Column but still inside the page animation.



        if (currentPage == SettingsPage.HUB) {
            SettingsHub(
                state = state, onUpdate = onUpdate,
                onTheme = { page = SettingsPage.THEME },
                onSubscriptions = { page = SettingsPage.SUBSCRIPTIONS },
                onTraffic = { page = SettingsPage.TRAFFIC },
                onDns = { page = SettingsPage.DNS },
                onPing = { page = SettingsPage.PING },
                onApp = { page = SettingsPage.APP },
                onUpdates = { page = SettingsPage.UPDATES },
                onRouting = onOpenRouting, onLogs = onOpenLogs,
                modifier = modifier, resetSignal = resetToHubSignal
            )
            return@SaveableStateProvider
        }

        if (currentPage == SettingsPage.THEME) {



            ThemeSettingsScreen(



                state = state,



                onUpdate = onUpdate,



                onBack = { page = SettingsPage.HUB },



                modifier = modifier



            )



            return@SaveableStateProvider



        }



        Column(



            // The whole settings page owns the status bar inset: without it the header and



            // the gear icon slide under the system clock while the page is scrolled.



            modifier.fillMaxSize()



                .statusBarsPadding()



                .verticalScroll(rememberScrollState())



                .padding(horizontal = 16.dp)



        ) {



            when (currentPage) {



                SettingsPage.HUB -> Unit



                SettingsPage.SUBSCRIPTIONS -> {



                    SettingsDetailHeader(title = s.subscriptionSettings, description = s.settingsSubscriptionsSummary, onBack = { page = SettingsPage.HUB })



                    SubscriptionSettings(state, onUpdate)



                }



                SettingsPage.TRAFFIC -> {



                    SettingsDetailHeader(title = s.trafficSettings, description = s.settingsTrafficSummary, onBack = { page = SettingsPage.HUB })



                    TrafficSettings(state, onUpdate)



                }



                SettingsPage.DNS -> {



                    SettingsDetailHeader(title = s.dnsSettings, description = s.settingsDnsSummary, onBack = { page = SettingsPage.HUB })



                    DnsSettings(state, onUpdate)



                }



                SettingsPage.PING -> {



                    SettingsDetailHeader(title = s.pingSettings, description = s.settingsPingSummary, onBack = { page = SettingsPage.HUB })



                    PingSettings(state, onUpdate)



                }



                SettingsPage.APP -> {



                    SettingsDetailHeader(title = s.appSettings, description = s.settingsAppSummary, onBack = { page = SettingsPage.HUB })



                    AppSettings(state, onUpdate, onLanguageChange, onReset, onOpenWelcome)



                }



                SettingsPage.UPDATES -> {



                    LaunchedEffect(updateChecked) {



                        if (!updateChecked) onCheckUpdates(false)



                    }



                    SettingsDetailHeader(title = s.updates, description = s.settingsUpdatesSummary, onBack = { page = SettingsPage.HUB })



                    UpdateSettings(



                        state = state,



                        onUpdate = onUpdate,



                        checked = updateChecked,



                        isChecking = updateIsChecking,



                        latestVersion = updateLatestVersion,



                        releaseTag = updateReleaseTag,



                        updateAvailable = updateAvailable,



                        error = updateError,



                        isDownloading = updateIsDownloading,



                        downloadProgress = updateDownloadProgress,



                        onCheck = { onCheckUpdates(true) },



                        onInstall = onInstallUpdate



                    )



                }



                SettingsPage.THEME -> Unit



            }



            // The scaffold already reserves the nav pill height, so only a small



            // breathing gap is needed; 140dp let the page scroll into emptiness.



            Spacer(Modifier.height(if (currentPage == SettingsPage.HUB) 68.dp else 100.dp))



        }



    }
        } // Keep each removed settings page's scroll and saveable state.



}







@Composable
private fun SettingsHub(
    state: SettingsUiState, onUpdate: (SettingsUiState) -> Unit,
    onTheme: () -> Unit, onSubscriptions: () -> Unit,
    onTraffic: () -> Unit, onDns: () -> Unit, onPing: () -> Unit, onApp: () -> Unit,
    onUpdates: () -> Unit, onRouting: () -> Unit, onLogs: () -> Unit,
    modifier: Modifier = Modifier, resetSignal: Int = 0
) {
    var communityOpen by remember { mutableStateOf(false) }
    val uri = androidx.compose.ui.platform.LocalUriHandler.current
    SettingsTilesHub(state, onUpdate, onOpen = { id ->
        when (id) {
            SettingsTileId.APPEARANCE -> onTheme()
            SettingsTileId.ROUTING -> onRouting()
            SettingsTileId.DNS -> onDns()
            SettingsTileId.PING -> onPing()
            SettingsTileId.SUBSCRIPTIONS -> onSubscriptions()
            SettingsTileId.APP -> onApp()
            SettingsTileId.TRAFFIC -> onTraffic()
            SettingsTileId.LOGS -> onLogs()
            SettingsTileId.UPDATES -> onUpdates()
            SettingsTileId.COMMUNITY -> communityOpen = true
        }
    }, modifier = modifier, resetSignal = resetSignal)
    if (communityOpen) CommunitySheet({ communityOpen = false }, uri::openUri)
}








private fun themePresetLabel(preset: ThemePreset, s: LumenStrings): String = when (preset) {



    ThemePreset.LIGHT -> s.lightTheme



    ThemePreset.DARK -> s.darkTheme



    ThemePreset.DRACULA -> "Dracula"



    ThemePreset.CATPPUCCIN -> "Catppuccin"



    ThemePreset.NORD -> "Nord"



    ThemePreset.GITHUB -> "GitHub Dark"



    ThemePreset.GRUVBOX -> "Gruvbox"



    ThemePreset.TOKYO_NIGHT -> "Tokyo Night"



    ThemePreset.MONOKAI -> "Monokai"



    ThemePreset.MATERIAL -> "Material"



    ThemePreset.SOLARIZED -> "Solarized"



    ThemePreset.ROSE_PINE -> s.rosePineTheme



}







/** Accent icon on a soft accent square, shared by menu rows and tiles. */



@Composable



internal fun SettingsIconTile(icon: ImageVector) {
    Box(Modifier.size(40.dp).clip(com.lumen.ui.components.expressiveShape(com.lumen.ui.components.Motif.Pebble))
        .background(MaterialTheme.colorScheme.primaryContainer), contentAlignment = Alignment.Center) {
        Icon(icon, null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(21.dp))
    }
}







@Composable



private fun UpdateSettings(



    state: SettingsUiState,



    onUpdate: (SettingsUiState) -> Unit,



    checked: Boolean,



    isChecking: Boolean,



    latestVersion: String?,



    releaseTag: String?,



    updateAvailable: Boolean,



    error: String?,



    isDownloading: Boolean,



    downloadProgress: Int?,



    onCheck: () -> Unit,



    onInstall: () -> Unit



) {



    val s = LocalStrings.current



    val art = com.lumen.ui.theme.expressivePalette()
    Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(32.dp)).background(art.stage)) {
        com.lumen.ui.components.ExpressiveMotif(com.lumen.ui.components.Motif.Sun, art.onStage.copy(alpha = .1f),
            Modifier.align(Alignment.CenterEnd).offset(x = 28.dp).size(160.dp))
        Column(Modifier.padding(22.dp)) {
            Text(s.currentVersion.uppercase(), style = MaterialTheme.typography.labelSmall, color = art.onStage.copy(alpha = .7f))
            Spacer(Modifier.height(16.dp))
            Text(LumenVersion.appVersion, style = MaterialTheme.typography.displayLarge, color = art.onStage)
            Text("Lumen / Android", style = MaterialTheme.typography.labelLarge, color = art.onStage.copy(alpha = .7f))
        }
    }
    if (!latestVersion.isNullOrBlank() || !releaseTag.isNullOrBlank()) {
        SectionHeader(s.infoSection)
        Row(Modifier.fillMaxWidth().height(androidx.compose.foundation.layout.IntrinsicSize.Min), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (!latestVersion.isNullOrBlank()) Column(Modifier.weight(1f).fillMaxHeight().clip(RoundedCornerShape(24.dp, 10.dp, 24.dp, 10.dp))
                .background(art.lime).padding(16.dp)) {
                Text(s.latestVersion, style = MaterialTheme.typography.labelMedium, color = art.onLime)
                Spacer(Modifier.height(18.dp))
                Text(latestVersion, style = MaterialTheme.typography.headlineMedium, color = art.onLime)
            }
            if (!releaseTag.isNullOrBlank()) Column(Modifier.weight(1f).fillMaxHeight().clip(RoundedCornerShape(10.dp, 24.dp, 10.dp, 24.dp))
                .background(art.blue).padding(16.dp)) {
                Text(s.androidReleaseTag, style = MaterialTheme.typography.labelMedium, color = art.onBlue)
                Spacer(Modifier.height(18.dp))
                Text(releaseTag, style = MaterialTheme.typography.titleLarge, color = art.onBlue)
            }
        }
    }
    Spacer(Modifier.height(12.dp))



    SettingsCard {



        Spacer(Modifier.height(4.dp))



        ToggleRow(



            s.autoCheckUpdates,



            s.autoCheckUpdatesDesc,



            state.autoCheckUpdates



        ) { onUpdate(state.copy(autoCheckUpdates = it)) }



        Spacer(Modifier.height(4.dp))



    }



    Spacer(Modifier.height(12.dp))



    val status = when {



        isChecking -> s.checkingUpdates



        isDownloading -> buildString {



            append(s.downloadingUpdate)



            downloadProgress?.let { append(" $it%") }



        }



        !error.isNullOrBlank() -> "${s.updateCheckFailed}: $error"



        checked && updateAvailable -> s.updateAvailable



        checked -> s.upToDate



        else -> s.updatesDesc



    }



    Text(



        text = status,



        style = MaterialTheme.typography.bodyMedium,



        color = if (error.isNullOrBlank()) {



            MaterialTheme.colorScheme.onSurfaceVariant



        } else {



            MaterialTheme.colorScheme.error



        },



        modifier = Modifier.padding(horizontal = 4.dp, vertical = 4.dp)



    )



    Spacer(Modifier.height(8.dp))



    com.lumen.ui.components.ExpressiveAction(
        label = if (isChecking) s.checkingUpdates else s.checkUpdates,
        onClick = onCheck, enabled = !isChecking && !isDownloading,
        modifier = Modifier.fillMaxWidth(), container = art.lime, content = art.onLime)
    if (updateAvailable) {



        Spacer(Modifier.height(8.dp))



        if (isDownloading) {



            if (downloadProgress != null) {



                LinearProgressIndicator(



                    progress = { downloadProgress.coerceIn(0, 100) / 100f },



                    modifier = Modifier.fillMaxWidth()



                )



            } else {



                LinearProgressIndicator(modifier = Modifier.fillMaxWidth())



            }



            Spacer(Modifier.height(8.dp))



        }



        Button(



            onClick = onInstall,



            enabled = !isChecking && !isDownloading,



            modifier = Modifier.fillMaxWidth()



        ) {



            Text(



                if (isDownloading) {



                    buildString {



                        append(s.downloadingUpdate)



                        downloadProgress?.let { append(" $it%") }



                    }



                } else {



                    s.downloadUpdate



                }



            )



        }



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



        SettingsDivider()

ToggleRow(



        s.subscriptionUseProxyTun,



        s.subscriptionUseProxyTunDesc,



        state.subscriptionUseProxyTun



    ) { onUpdate(state.copy(subscriptionUseProxyTun = it)) }



        SettingsDivider()

ToggleRow(



        s.subscriptionAllowHttp,



        s.subscriptionAllowHttpDesc,



        state.subscriptionAllowHttp



    ) { onUpdate(state.copy(subscriptionAllowHttp = it)) }



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



private fun DnsSettings(state: SettingsUiState, onUpdate: (SettingsUiState) -> Unit) {
    val s = LocalStrings.current
    val art = com.lumen.ui.theme.expressivePalette()
    SectionHeader(s.dnsModeSection)
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        DNS_MODES.chunked(2).forEach { row ->
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                row.forEach { mode ->
                    DnsModeChip(dnsModeLabel(mode, s), state.dnsMode == mode, Modifier.weight(1f)) {
                        onUpdate(state.copy(dnsMode = mode))
                    }
                }
            }
        }
    }
    Text(dnsModeHint(state.dnsMode, s), style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(vertical = 12.dp))
    if (state.dnsMode == "json") {
        SettingsPanel(s.dnsModeJson, "{ }", art.lavender, art.onLavender) {
            TextAreaSettingField(s.dnsCustomJsonLabel, state.dnsCustomJson) {
                onUpdate(state.copy(dnsCustomJson = it.take(65_536)))
            }
        }
        Spacer(Modifier.height(10.dp))
    }
    SettingsPanel(s.dnsDirectSection, "01", MaterialTheme.colorScheme.surfaceContainerLow, MaterialTheme.colorScheme.onSurface) {
        TextAreaSettingField(s.dnsServersLabel, state.dnsDirectServers) {
            onUpdate(state.copy(dnsDirectServers = it.take(2048)))
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            LumenDropdown(s.dnsTypeLabel, listOf("udp", "tcp", "tls", "https"), state.dnsDirectType,
                { onUpdate(state.copy(dnsDirectType = it)) }, Modifier.weight(1f), optionLabel = { it.uppercase() })
            LumenDropdown(s.dnsStrategyLabel, listOf("prefer_ipv4", "prefer_ipv6", "ipv4_only", "ipv6_only"),
                state.dnsDirectStrategy, { onUpdate(state.copy(dnsDirectStrategy = it)) }, Modifier.weight(1f), optionLabel = ::dnsStrategyDisplay)
        }
    }
    Spacer(Modifier.height(10.dp))
    SettingsPanel(s.dnsProxySection, "02", MaterialTheme.colorScheme.surfaceContainerLow, MaterialTheme.colorScheme.onSurface) {
        TextAreaSettingField(s.dnsServersLabel, state.dnsProxyServers) {
            onUpdate(state.copy(dnsProxyServers = it.take(2048)))
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            LumenDropdown(s.dnsTypeLabel, listOf("udp", "tcp", "tls", "https"), state.dnsProxyType,
                { onUpdate(state.copy(dnsProxyType = it)) }, Modifier.weight(1f), optionLabel = { it.uppercase() })
            LumenDropdown(s.dnsStrategyLabel, listOf("prefer_ipv4", "prefer_ipv6", "ipv4_only", "ipv6_only"),
                state.dnsProxyStrategy, { onUpdate(state.copy(dnsProxyStrategy = it)) }, Modifier.weight(1f), optionLabel = ::dnsStrategyDisplay)
        }
        ToggleRow(s.dnsIpv4Only, s.dnsIpv4OnlyDesc, state.dnsProxyIpv4Only) {
            onUpdate(state.copy(dnsProxyIpv4Only = it))
        }
    }
    SectionHeader(s.dnsBehaviorSection)
    SettingsCard {
        ToggleRow(s.dnsHijack, s.dnsHijackDesc, state.dnsHijackEnabled) { onUpdate(state.copy(dnsHijackEnabled = it)) }
        ToggleRow(s.dnsFakeIp, s.dnsFakeIpDesc, state.dnsFakeIpEnabled) { onUpdate(state.copy(dnsFakeIpEnabled = it)) }
        ToggleRow(s.dnsParallel, s.dnsParallelDesc, state.dnsParallelQuery) { onUpdate(state.copy(dnsParallelQuery = it)) }
        ToggleRow(s.dnsOptimistic, s.dnsOptimisticDesc, state.dnsOptimisticCache) { onUpdate(state.copy(dnsOptimisticCache = it)) }
        ToggleRow(s.dnsGeoCheck, s.dnsGeoCheckDesc, state.dnsGeoCheck) { onUpdate(state.copy(dnsGeoCheck = it)) }
    }
    SectionHeader(s.dnsHostsSection)
    SettingsPanel(s.dnsHostsSection, "03", MaterialTheme.colorScheme.surfaceContainerLow, MaterialTheme.colorScheme.onSurface) {
        TextAreaSettingField(s.dnsHostsLabel, state.dnsHosts) { onUpdate(state.copy(dnsHosts = it.take(4096))) }
        ToggleRow(s.dnsOverride, s.dnsOverrideDesc, state.dnsOverrideEnabled) { onUpdate(state.copy(dnsOverrideEnabled = it)) }
        if (state.dnsOverrideEnabled) {
            TextSettingField(s.dnsHostname, state.dnsOverrideHostname) { onUpdate(state.copy(dnsOverrideHostname = it.take(253))) }
            TextSettingField(s.dnsIpv4, state.dnsOverrideIpv4) { onUpdate(state.copy(dnsOverrideIpv4 = it.take(15))) }
        }
    }
}


@Composable



private fun DnsModeChip(label: String, selected: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val art = com.lumen.ui.theme.expressivePalette()
    Row(modifier.expressiveControlLabel(label).height(64.dp).clip(com.lumen.ui.components.selectionShape(selected, 12.dp, 26.dp))
        .background(if (selected) art.stage else MaterialTheme.colorScheme.surfaceContainerLow)
        .clickable(onClick = onClick).padding(horizontal = 14.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, modifier = Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.SemiBold, maxLines = 2,
            color = if (selected) art.onStage else MaterialTheme.colorScheme.onSurface)
        if (selected) Icon(Icons.Default.Check, null, Modifier.size(18.dp), tint = art.onStage)
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



    if (state.muxEnabled) {



        NumberField(s.muxConcurrency, state.muxConcurrency) {



            onUpdate(state.copy(muxConcurrency = it.coerceIn(1, 1024)))



        }



        NumberField(s.muxMinStreamsLabel, state.multiplexMinStreams) {



            onUpdate(state.copy(multiplexMinStreams = it.coerceIn(0, 1024)))



        }



        LumenDropdown(



            label = s.muxProtocolLabel,



            options = MULTIPLEX_PROTOCOLS,



            selected = state.multiplexProtocol,



            onSelected = { onUpdate(state.copy(multiplexProtocol = it)) }



        )



        ToggleRow(s.muxPadding, s.muxPaddingDesc, state.multiplexPadding) {



            onUpdate(state.copy(multiplexPadding = it))



        }



            SettingsDivider()

ToggleRow(s.muxBrutal, s.muxBrutalDesc, state.multiplexBrutalEnabled) {



            onUpdate(state.copy(multiplexBrutalEnabled = it))



        }



        // Both directions are mandatory: the core aborts the config with



        // "brutal: invalid download speed" when only one rate is set.



        if (state.multiplexBrutalEnabled) {



            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {



                Box(Modifier.weight(1f)) {



                    NumberField(s.muxBrutalUpLabel, state.multiplexBrutalUpMbps) {



                        onUpdate(state.copy(multiplexBrutalUpMbps = it.coerceIn(0, 10000)))



                    }



                }



                Box(Modifier.weight(1f)) {



                    NumberField(s.muxBrutalDownLabel, state.multiplexBrutalDownMbps) {



                        onUpdate(state.copy(multiplexBrutalDownMbps = it.coerceIn(0, 10000)))



                    }



                }



            }



        }



    }



        SettingsDivider()

ToggleRow(s.tlsFragmentation, s.tlsDescription, state.fragmentEnabled) {



        onUpdate(state.copy(fragmentEnabled = it))



    }



    // The packets/length/delay sub-fields are Xray-only: sing-box-extended exposes



    // just tls_fragment and tls_fragment_fallback_delay, and route-options silently



    // ignores anything else, so showing them would promise an effect that never happens.



    NumberField(s.tunnelMtu, state.mtu) { onUpdate(state.copy(mtu = it.coerceIn(1280, 9000))) }



        SettingsDivider()

ToggleRow(s.preferIpv6, s.preferIpv6Description, state.preferIpv6) {



        onUpdate(state.copy(preferIpv6 = it))



    }



        SettingsDivider()

ToggleRow(s.blockQuic, s.blockQuicDescription, state.blockQuic) {



        onUpdate(state.copy(blockQuic = it))



    }



    // No "sniff route only" row: this core rejects route_only/sniff_override_destination



    // on a sniff rule, and a bare {"action":"sniff"} already routes without overriding



    // the destination, so the toggle could only ever have been a no-op.



    Spacer(Modifier.height(4.dp))



    }



    // Dial options: the builder stamps these on every outbound and endpoint it emits.



    SectionHeader(s.outboundSection)



    SettingsCard {



        Spacer(Modifier.height(4.dp))



        ToggleRow(s.tcpFastOpen, s.tcpFastOpenDesc, state.outboundTcpFastOpen) {



            onUpdate(state.copy(outboundTcpFastOpen = it))



        }



        SettingsDivider()



        ToggleRow(s.tcpMultiPath, s.tcpMultiPathDesc, state.outboundTcpMultiPath) {



            onUpdate(state.copy(outboundTcpMultiPath = it))



        }



        SettingsDivider()



        ToggleRow(s.udpFragmentLabel, s.udpFragmentDesc, state.outboundUdpFragment) {



            onUpdate(state.copy(outboundUdpFragment = it))



        }



        SettingsDivider()



        // shadowsocks is the only outbound in this core that carries udp_over_tcp.



        ToggleRow(s.udpOverTcpLabel, s.udpOverTcpDesc, state.udpOverTcp) {



            onUpdate(state.copy(udpOverTcp = it))



        }



        NumberField(s.connectTimeoutLabel, state.outboundConnectTimeoutSeconds) {



            onUpdate(state.copy(outboundConnectTimeoutSeconds = it.coerceIn(0, 600)))



        }



        Spacer(Modifier.height(4.dp))



    }



}







/** Read-only credential row: the value can only be copied, never edited. */



@Composable



private fun Socks5CredentialRow(label: String, value: String) {



    val s = LocalStrings.current



    val clipboard = androidx.compose.ui.platform.LocalClipboardManager.current



    val context = androidx.compose.ui.platform.LocalContext.current



    Row(



        Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, top = 10.dp, bottom = 10.dp),



        verticalAlignment = Alignment.CenterVertically



    ) {



        Column(Modifier.weight(1f)) {



            Text(label, style = MaterialTheme.typography.bodyLarge)



            Text(



                value.ifBlank { "\u2014" },



                style = MaterialTheme.typography.bodyMedium,



                color = MaterialTheme.colorScheme.onSurfaceVariant



            )



        }



        androidx.compose.material3.TextButton(



            onClick = {



                clipboard.setText(androidx.compose.ui.text.AnnotatedString(value))



                showLumenNotice(s.copied)



            },



            enabled = value.isNotBlank()



        ) {



            Text(s.copyAction)



        }



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



            optionLabel = {



                when (it) {



                    "tcping" -> "TCPing"



                    "icmp" -> "ICMP"



                    "http" -> "HTTP GET"



                    else -> it



                }



            }



        )



        // HTTP GET measures against the configured endpoint.



        if (state.pingType == "http") {



            TextSettingField(s.pingUrlLabel, state.pingUrl) { onUpdate(state.copy(pingUrl = it.take(256))) }



            // Presets save typing for the endpoints people actually use.



            LumenDropdown(



                label = s.pingUrlPresets,



                options = PING_URL_PRESETS,



                selected = state.pingUrl.takeIf { it in PING_URL_PRESETS } ?: PING_URL_PRESETS.first(),



                onSelected = { onUpdate(state.copy(pingUrl = it)) }



            )



        }



        NumberField(s.pingTimeoutLabel, state.pingTimeoutMs) {



            onUpdate(state.copy(pingTimeoutMs = it.coerceIn(500, 20000)))



        }



        NumberField(s.pingConcurrencyLabel, state.pingConcurrency) {



            onUpdate(state.copy(pingConcurrency = it.coerceIn(1, 32)))



        }



        // Several probes smooth out one-off spikes on mobile networks.



        NumberField(s.pingAttemptsLabel, state.pingAttempts) {



            onUpdate(state.copy(pingAttempts = it.coerceIn(1, 10)))



        }



        if (state.pingAttempts > 1) {



            LumenDropdown(



                label = s.pingAggregateLabel,



                options = PING_AGGREGATES,



                selected = state.pingAggregate,



                onSelected = { onUpdate(state.copy(pingAggregate = it)) },



                optionLabel = {



                    when (it) {



                        "avg" -> s.aggregateAvg



                        "median" -> s.aggregateMedian



                        else -> s.aggregateMin



                    }



                }



            )



            NumberField(s.pingRetryDelayLabel, state.pingRetryDelayMs) {



                onUpdate(state.copy(pingRetryDelayMs = it.coerceIn(0, 5000)))



            }



        }



        Spacer(Modifier.height(6.dp))



    }



    SectionHeader(s.pingThresholds)



    SettingsCard {



        Spacer(Modifier.height(10.dp))



        NumberField(s.pingGoodLabel, state.pingGoodMs) {



            onUpdate(state.copy(pingGoodMs = it.coerceIn(10, 2000)))



        }



        NumberField(s.pingFairLabel, state.pingFairMs) {



            // The "average" threshold must stay above the "good" one.



            onUpdate(state.copy(pingFairMs = it.coerceIn(state.pingGoodMs + 10, 5000)))



        }



        Spacer(Modifier.height(6.dp))



    }



    SectionHeader(s.behavior)



    SettingsCard {



        Spacer(Modifier.height(4.dp))



        ToggleRow(s.pingAutoOnOpen, s.pingAutoOnOpenDesc, state.pingAutoOnOpen) {



            onUpdate(state.copy(pingAutoOnOpen = it))



        }



        SettingsDivider()



        ToggleRow(



            s.pingAutoDeleteUnreachable,



            s.pingAutoDeleteUnreachableDesc,



            state.pingAutoDeleteUnreachable



        ) { onUpdate(state.copy(pingAutoDeleteUnreachable = it)) }



        if (state.pingAutoDeleteUnreachable) {



            NumberField(



                s.pingAutoDeleteThresholdLabel,



                state.pingAutoDeleteThresholdMs



            ) {



                onUpdate(state.copy(pingAutoDeleteThresholdMs = it.coerceIn(0, 100)))



            }



        }



        Spacer(Modifier.height(4.dp))



        OutlinedButton(



            onClick = {



                onUpdate(



                    state.copy(



                        pingType = "http",



                        pingTimeoutMs = 2000,



                        pingConcurrency = 16,



                        pingUrl = PING_URL_PRESETS.first(),



                        pingAttempts = 1,



                        pingAggregate = "min",



                        pingRetryDelayMs = 200,



                        pingGoodMs = 150,



                        pingFairMs = 300,



                        pingAutoOnOpen = false,



                        pingAutoDeleteUnreachable = false,



                        pingAutoDeleteThresholdMs = 1



                    )



                )



            },



            modifier = Modifier.fillMaxWidth()



        ) {



            Text(s.resetToDefaults)



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



        NumberField(s.urlTestIdleTimeoutLabel, state.urlTestIdleTimeoutMinutes) {



            onUpdate(state.copy(urlTestIdleTimeoutMinutes = it.coerceIn(0, 1440)))



        }



        SettingsDivider()



        ToggleRow(s.urlTestInterrupt, s.urlTestInterruptDesc, state.urlTestInterruptExistConnections) {



            onUpdate(state.copy(urlTestInterruptExistConnections = it))



        }



        Spacer(Modifier.height(6.dp))



    }



}







@Composable



private fun AppSettings(



    state: SettingsUiState,



    onUpdate: (SettingsUiState) -> Unit,



    onLanguageChange: (String) -> Unit,
    onReset: (LumenResetTarget) -> Unit,
    onOpenWelcome: (() -> Unit)?
) {



    val s = LocalStrings.current



    if (onOpenWelcome != null) {
        TextButton(onClick = onOpenWelcome, modifier = Modifier.fillMaxWidth()) {
            Icon(Icons.Default.Settings, null, Modifier.size(20.dp))
            Spacer(Modifier.width(8.dp))
            Text(WelcomeText.REOPEN.inLanguage(state.language))
        }
    }
    SectionHeader(s.behavior)



    SettingsCard {



        Spacer(Modifier.height(4.dp))



        ToggleRow(s.vibration, s.vibrationDesc, state.hapticsEnabled) {



            onUpdate(state.copy(hapticsEnabled = it))



        }



        SettingsDivider()



        ToggleRow(s.telemetryEnabled, s.telemetryEnabledDesc, state.telemetryEnabled) {



            onUpdate(state.copy(telemetryEnabled = it))



        }



        SettingsDivider()



        // Master switch: off silences the core, the log bus and the persisted store.



        ToggleRow(s.loggingEnabled, s.loggingEnabledDesc, state.loggingEnabled) {



            onUpdate(state.copy(loggingEnabled = it))



        }



        SettingsDivider()



        ToggleRow(s.autoReconnectNetwork, s.autoReconnectNetworkDesc, state.reconnectOnNetworkChange) {



            onUpdate(state.copy(reconnectOnNetworkChange = it))



        }



        SettingsDivider()



        ToggleRow(s.validateProxyDataPath, s.validateProxyDataPathDesc, state.validateProxyDataPath) {



            onUpdate(state.copy(validateProxyDataPath = it))



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



        ToggleRow(s.proxyOnly, s.proxyOnlyDesc, state.proxyOnly) {



            onUpdate(



                state.copy(



                    proxyOnly = it,



                    localInboundEnabled = if (it) true else state.localInboundEnabled



                )



            )



        }



        SettingsDivider()



        ToggleRow(



            s.localInbound,



            s.localInboundDescription,



            state.localInboundEnabled,



            enabled = !state.proxyOnly



        ) {



            onUpdate(state.copy(localInboundEnabled = it))



        }



        if (state.localInboundEnabled) {



            SettingsDivider()



            Spacer(Modifier.height(4.dp))



            NumberField(s.socksPort, state.localSocksPort) {
                onUpdate(state.copy(localSocksPort = it.coerceIn(1024, 65535)))
            }
            SettingsDivider()
            NumberField(s.httpPort, state.localHttpPort) {
                onUpdate(state.copy(localHttpPort = it.coerceIn(1024, 65535)))
            }
            SettingsDivider()



            ToggleRow(s.allowLan, s.allowLanDescription, state.lanSharingEnabled) {



                onUpdate(state.copy(lanSharingEnabled = it))



            }



        }



        Spacer(Modifier.height(4.dp))



    }



    SectionHeader(s.socks5Auth)



    SettingsCard {



        Spacer(Modifier.height(4.dp))



        ToggleRow(s.socks5Auth, s.socks5AuthDesc, state.socks5AuthEnabled) { enabled ->



            onUpdate(



                state.copy(



                    socks5AuthEnabled = enabled,



                    socks5Username = state.socks5Username.ifEmpty { generateSocks5Username() },



                    socks5Password = state.socks5Password.ifEmpty { generateSocks5Password() }



                )



            )



        }



        if (state.socks5AuthEnabled) {



            SettingsDivider()



            Socks5CredentialsEditor(



                username = state.socks5Username,



                password = state.socks5Password,



                onSave = { username, password ->



                    onUpdate(state.copy(socks5Username = username, socks5Password = password))



                }



            )



        }



        Spacer(Modifier.height(4.dp))



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



        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),



        optionLabel = { languageLabel(it) }



    )
    ResetLumenControl(onReset)
}



@Composable



internal fun SettingsCard(content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(28.dp))
        .background(MaterialTheme.colorScheme.background).padding(0.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp)) { content() }
}



@Composable



private fun SettingsMenuRow(icon: ImageVector, title: String, onClick: () -> Unit, value: String? = null) {
    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp))
        .background(MaterialTheme.colorScheme.surfaceContainerLow)
        .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(18.dp)).clickable(onClick = onClick)
        .padding(horizontal = 12.dp, vertical = 14.dp), verticalAlignment = Alignment.CenterVertically) {
        SettingsIconTile(icon)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.SemiBold)
            if (!value.isNullOrBlank()) Text(value, style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null,
            tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(22.dp))
    }
}







@Composable



private fun InfoRow(title: String, value: String) {



    Row(



        Modifier.fillMaxWidth().padding(vertical = 14.dp),



        horizontalArrangement = Arrangement.SpaceBetween,



        verticalAlignment = Alignment.CenterVertically



    ) {



        Text(title, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)



        Text(



            value,



            style = MaterialTheme.typography.bodySmall,



            color = MaterialTheme.colorScheme.onSurfaceVariant



        )



    }



}







/** Kept for call sites; SettingsCard draws the hairlines between rows itself. */



@Composable



internal fun SettingsDivider() { Spacer(Modifier.height(3.dp)) }







@Composable



private fun ToggleRow(



    title: String,



    description: String,



    checked: Boolean,



    enabled: Boolean = true,



    onChange: (Boolean) -> Unit



) {

    val s = LocalStrings.current

    val icon = when (title) {

        s.vibration -> Icons.Filled.Vibration

        s.telemetryEnabled -> Icons.Filled.Shield

        s.loggingEnabled -> Icons.Filled.Description

        s.autoReconnectNetwork -> Icons.Filled.Sync

        s.validateProxyDataPath -> Icons.Filled.Shield

        s.autoConnect -> Icons.Filled.PowerSettingsNew

        s.speedStats, s.notificationSpeed -> Icons.Filled.Speed

        s.showNotification -> Icons.Filled.Notifications

        s.subscriptionAutoUpdate, s.autoCheckUpdates -> Icons.Filled.CloudDownload

        s.subscriptionUseProxyTun, s.proxyOnly, s.localInbound, s.allowLan -> Icons.Filled.AltRoute

        s.subscriptionAllowHttp, s.sendHwid, s.socks5Auth -> Icons.Filled.Shield

        s.subscriptionConverter, s.allowOverrides -> Icons.Filled.Code

        s.dnsFakeIp, s.dnsGeoCheck, s.dnsHijack, s.dnsIpv4Only, s.dnsOptimistic,

        s.dnsOverride, s.dnsParallel, s.preferIpv6 -> Icons.Filled.Dns

        s.pingAutoOnOpen, s.pingAutoDeleteUnreachable, s.urlTestInterrupt -> Icons.Filled.Speed

        s.muxBrutal, s.muxPadding, s.blockQuic, s.tlsFragmentation, s.tcpFastOpen,

        s.tcpMultiPath, s.udpFragmentLabel, s.udpOverTcpLabel -> Icons.Filled.AltRoute

        else -> Icons.Filled.Settings

    }

    PreferenceSwitchRow(title, description, icon, checked, enabled, onChange)
}







@Composable



private fun NumberField(label: String, value: Int, onChange: (Int) -> Unit) {
    var text by remember { mutableStateOf(value.toString()) }
    var focused by remember { mutableStateOf(false) }
    LaunchedEffect(value, focused) { if (!focused) text = value.toString() }
    Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically) {
        Text(label, style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.weight(1f).padding(end = 12.dp))
        ExpressiveTextField(value = text, onValueChange = { text = it }, singleLine = true,
            textStyle = MaterialTheme.typography.titleLarge.copy(fontFamily = com.lumen.ui.theme.LumenMono,
                textAlign = androidx.compose.ui.text.style.TextAlign.Center),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { text.toIntOrNull()?.let(onChange) }),
            modifier = Modifier.width(104.dp).onFocusChanged { focusState ->
                if (focused && !focusState.isFocused) text.toIntOrNull()?.let(onChange)
                focused = focusState.isFocused
            })
    }
}







@Composable



private fun TextSettingField(label: String, value: String, onChange: (String) -> Unit) {



    SettingsTextBox(label = label, value = value, onChange = onChange, singleLine = true)



}







@Composable



private fun TextAreaSettingField(label: String, value: String, onChange: (String) -> Unit) {



    SettingsTextBox(label = label, value = value, onChange = onChange, singleLine = false)



}







/** Caption above a quiet card field; multi-line boxes use the mono face for configs. */



@Composable



private fun SettingsTextBox(label: String, value: String, onChange: (String) -> Unit, singleLine: Boolean) {
    ExpressiveTextField(value = value, onValueChange = onChange, label = { Text(label) },
        singleLine = singleLine, minLines = if (singleLine) 1 else 2, maxLines = if (singleLine) 1 else 6,
        textStyle = MaterialTheme.typography.bodyMedium.copy(
            fontFamily = if (singleLine) MaterialTheme.typography.bodyMedium.fontFamily else com.lumen.ui.theme.LumenMono),
        modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp))
}
