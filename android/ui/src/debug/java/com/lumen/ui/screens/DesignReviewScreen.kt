package com.lumen.ui.screens

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.lumen.ui.components.ConnectionState
import com.lumen.ui.theme.LumenTheme
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** Debug-only visual harness. It never reads preferences, databases, engines or a ViewModel. */
@Composable
fun DesignReviewScreen(
    initialPage: String, dark: Boolean, style: DashboardStyle,
    initialConnection: ConnectionState, language: String
) {
    var page by remember { mutableStateOf(initialPage) }
    var settings by remember { mutableStateOf(SettingsUiState(
        themePreset = if (dark) ThemePreset.DARK else ThemePreset.LIGHT,
        themeMode = if (dark) ThemeMode.DARK else ThemeMode.LIGHT,
        language = language, dashboardStyle = style
    )) }
    // Sample-only onboarding: these callbacks do not register WARP or import real data.
    var welcome by remember { mutableStateOf(OnboardingUiState(
        visible = true, setupRequired = initialPage != "welcome-update",
        previousVersion = if (initialPage == "welcome-update") "1.4.0" else ""
    )) }
    var previewServerCount by remember { mutableIntStateOf(0) }
    var selectedGroup by remember { mutableStateOf("subscription:sample") }
    var connection by remember { mutableStateOf(initialConnection) }
    var username by remember { mutableStateOf("lumen") }
    var password by remember { mutableStateOf("sample-password") }
    val scope = rememberCoroutineScope()
    var sampleNodes by remember {
        mutableStateOf(listOf(
            NodeUiModel("sample-nl", "Amsterdam / 01", "vless", "example.invalid", 443,
                countryCode = "NL", pingMs = 38, isSelected = true, subscriptionId = "sample", displayProtocol = "VLESS / REALITY"),
            NodeUiModel("sample-de", "Frankfurt / 02", "trojan", "example.invalid", 443,
                countryCode = "DE", pingMs = 54, subscriptionId = "sample"),
            NodeUiModel("sample-jp", "Tokyo / 03", "hysteria2", "example.invalid", 443,
                countryCode = "JP", pingMs = 126, subscriptionId = "sample", groupId = "sample-group")
        ))
    }
    val sampleSubscriptions = remember { listOf(SubscriptionUiModel("sample", "Lumen network", "https://example.invalid",
        nodeCount = 3, trafficSummary = "12.4 / 100 GB", expiryDaysLeft = 24, trafficRatio = .124f,
        announce = "Network maintenance: new routes are available. Refresh your subscription to receive them.",
        announceUrl = "https://example.invalid/news", description = "Private network · Europe and Asia",
        supportUrl = "https://example.invalid/support", supportEmail = "support@example.invalid",
        telegramUrl = "https://example.invalid/channel", websiteUrl = "https://example.invalid",
        premiumUrl = "https://example.invalid/plans", bannerText = "Provider message",
        bannerButtonText = "Details", bannerButtonUrl = "https://example.invalid/details",
        updateIntervalHours = 12)) }
    val sampleGroups = remember { listOf(ServerGroupUiModel("sample-group", "Work", 1)) }
    var sampleApps by remember { mutableStateOf(listOf(
        AppEntryUiModel("com.example.browser", "Browser", isSelected = true),
        AppEntryUiModel("com.example.chat", "Chat", isSelected = true),
        AppEntryUiModel("com.example.music", "Music"),
        AppEntryUiModel("com.example.maps", "Maps")
    )) }
    var splitMode by remember { mutableStateOf(SplitModeUi.ALLOW_LIST) }
    var domainRules by remember { mutableStateOf("10.0.0.0/8\n172.16.0.0/12\n192.168.0.0/16") }
    val toggle: () -> Unit = {
        if (connection == ConnectionState.Connected || connection == ConnectionState.Connecting) {
            connection = ConnectionState.Disconnected
        } else scope.launch {
            connection = ConnectionState.Connecting
            delay(1400)
            if (connection == ConnectionState.Connecting) connection = ConnectionState.Connected
        }
    }
    val select: (NodeUiModel) -> Unit = { chosen -> sampleNodes = sampleNodes.map { it.copy(isSelected = it.id == chosen.id) } }
    androidx.activity.compose.BackHandler(enabled = page == "servers") { page = "home" }
    LumenTheme(themePreset = settings.themePreset, themeMode = settings.themeMode, accentColor = settings.accentColor) {
        CompositionLocalProvider(LocalStrings provides stringsForLanguage(settings.language)) {
            val s = LocalStrings.current
            Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
                when (page) {
                    "welcome", "welcome-update", "welcome-look", "welcome-routes", "welcome-configs", "welcome-system", "welcome-done" ->
                        OnboardingScreen(welcome, settings, previewServerCount, false,
                            onUpdate = { settings = it }, onLanguage = { settings = settings.copy(language = it) },
                            onImportText = { previewServerCount++ }, onImportClipboard = { previewServerCount++ },
                            onImportFile = { previewServerCount++ }, onImportQr = { previewServerCount++ },
                            onGenerateWarp = { welcome = welcome.copy(warpCount = 5); previewServerCount += 5 },
                            onCancelWarp = { welcome = welcome.copy(warpBusy = false) },
                            onCancelPreparation = { welcome = welcome.copy(preparing = false) },
                            onFinish = { page = "home" }, onOpenLink = {}, onMinimize = { page = "home" },
                            initialStep = when (initialPage) {
                                "welcome-look" -> WelcomeStep.LOOK; "welcome-routes" -> WelcomeStep.ROUTES
                                "welcome-configs" -> WelcomeStep.CONFIGS; "welcome-system" -> WelcomeStep.SYSTEM
                                "welcome-done" -> WelcomeStep.DONE; else -> null
                            })

                    "credentials" -> Column(Modifier.fillMaxSize().statusBarsPadding()
                        .verticalScroll(rememberScrollState()).imePadding().padding(16.dp)) {
                        SettingsDetailHeader(s.socks5Auth, s.socks5AuthDesc, null)
                        SettingsCard { Socks5CredentialsEditor(username, password) { user, secret ->
                            username = user; password = secret
                        } }
                    }
                    "settings" -> SettingsScreen(settings, { settings = it },
                        onLanguageChange = { settings = settings.copy(language = it) },
                        onOpenRouting = { page = "routing" }, onOpenLogs = { page = "logs" }, onOpenCommunity = {},
                        updateChecked = true, updateLatestVersion = "1.2.1", updateReleaseTag = "v1.2.1-android")
                    "appearance" -> ThemeSettingsScreen(settings, { settings = it }, { page = "settings" })
                    "routing" -> RoutingHubScreen({ page = "domain" }, { page = "apps" }, {}, onBack = { page = "settings" })
                    "domain" -> DomainRoutingScreen(domainRules, "", { rules, _ -> domainRules = rules }, { page = "routing" })
                    "apps" -> RoutingScreen(splitMode, sampleApps, false, { splitMode = it },
                        { app -> sampleApps = sampleApps.map { if (it.packageName == app.packageName) it.copy(isSelected = !it.isSelected) else it } },
                        { sampleApps = sampleApps.map { it.copy(isSelected = true) } },
                        { sampleApps = sampleApps.map { it.copy(isSelected = false) } }, { page = "routing" })
                    "logs" -> LogsScreen(emptyList(), {}, {}, { page = "settings" }, entries = (0 until 450).map { index ->
                        val level = listOf("info", "debug", "warning", "error")[index % 4]
                        LogEntryUi(index.toLong(), "18:${40 + index / 60}:${(index % 60).toString().padStart(2, '0')}", level,
                            if (index % 2 == 0) "VPN" else "DNS", "Sample event $index: " + when (level) {
                                "error" -> "upstream connection timed out; retry scheduled"
                                "warning" -> "network changed; selecting another route"
                                "debug" -> "resolved example.invalid using secure resolver"
                                else -> "tunnel route ready"
                            })
                    }, persistPreferences = false)
                    "servers" -> ServerListScreen(sampleNodes, sampleSubscriptions, serverGroups = sampleGroups,
                        refreshingIds = emptySet(), isPinging = false, testingNodeId = null, serverTestResults = emptyMap(),
                        connectionState = connection, onToggleConnection = toggle, onSelectNode = select,
                        onEditNode = {}, onDeleteNode = {}, onAddNode = {}, onPingAll = {}, onPingNode = {},
                        onImportClipboard = {}, onRefreshSubscription = {}, onDeleteSubscription = {}, persistPreferences = false,
                        selectedGroupFilter = selectedGroup, onSelectedGroupFilter = { selectedGroup = it })
                    else -> Box(Modifier.fillMaxSize()) {
                        DashboardScreen(connection, sampleNodes, sampleSubscriptions, serverGroups = sampleGroups,
                            selectedGroupFilter = selectedGroup,
                            dashboardStyle = settings.dashboardStyle,
                            downloadSpeed = 12_345_678L, uploadSpeed = 1_234_567L,
                            onToggleConnection = toggle, onSelectNode = select,
                            onImportClipboard = {}, onImportFile = {}, onImportQr = {}, onAddManualNode = {},
                            onOpenServers = { page = "servers" })
                        if (settings.dashboardStyle == DashboardStyle.SLIDER) com.lumen.ui.components.ConnectionSliderBar(
                            connection, toggle, Modifier.align(Alignment.BottomCenter).navigationBarsPadding()
                                .padding(start = 16.dp, end = 16.dp, bottom = 80.dp))
                    }
                }
            }
        }
    }
}
