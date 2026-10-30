package com.lumen.app.navigation

import androidx.compose.animation.animateContentSize

import androidx.compose.ui.draw.scale
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import com.lumen.ui.screens.DashboardStyle








import androidx.activity.compose.BackHandler



import androidx.activity.compose.rememberLauncherForActivityResult



import androidx.compose.runtime.mutableStateListOf



import androidx.activity.result.contract.ActivityResultContracts



import androidx.compose.animation.fadeIn



import androidx.compose.animation.fadeOut



import androidx.compose.animation.core.tween



import androidx.compose.foundation.border
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.foundation.layout.wrapContentHeight



import androidx.compose.foundation.layout.WindowInsets



import androidx.compose.foundation.layout.padding



import androidx.compose.foundation.layout.navigationBarsPadding



import androidx.compose.foundation.layout.statusBars



import androidx.compose.foundation.shape.RoundedCornerShape



import androidx.compose.material.icons.Icons



import androidx.compose.material.icons.automirrored.filled.List



import androidx.compose.material.icons.filled.Home



import androidx.compose.material.icons.outlined.Dns



import androidx.compose.material.icons.outlined.Home



import androidx.compose.material.icons.outlined.Tune
import androidx.compose.material.icons.outlined.Public



import androidx.compose.material.icons.filled.Settings



import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Share



import androidx.compose.material3.AlertDialog
import androidx.compose.material3.IconButton



import androidx.compose.material3.Button



import androidx.compose.material3.CircularProgressIndicator



import androidx.compose.material3.Icon



import androidx.compose.material3.MaterialTheme



import androidx.compose.material3.NavigationBar



import androidx.compose.material3.NavigationBarItem



import androidx.compose.material3.NavigationBarItemDefaults



import androidx.compose.material3.Scaffold



import androidx.compose.material3.Text



import androidx.compose.material3.TextButton



import androidx.compose.runtime.Composable



import androidx.compose.runtime.CompositionLocalProvider



import androidx.compose.runtime.DisposableEffect



import androidx.compose.runtime.LaunchedEffect



import androidx.compose.runtime.getValue



import androidx.lifecycle.compose.collectAsStateWithLifecycle



import androidx.compose.runtime.mutableIntStateOf



import androidx.compose.runtime.mutableStateOf



import androidx.compose.runtime.remember



import androidx.compose.runtime.saveable.rememberSaveable



import androidx.compose.runtime.setValue



import androidx.compose.animation.animateColorAsState



import androidx.compose.animation.core.animateDpAsState



import androidx.compose.animation.core.animateFloatAsState



import androidx.compose.animation.core.spring



import androidx.compose.foundation.layout.fillMaxHeight



import androidx.compose.foundation.layout.offset



import androidx.compose.ui.draw.scale



import androidx.compose.foundation.background



import androidx.compose.foundation.selection.selectable
import com.lumen.ui.components.expressiveControlLabel
import androidx.compose.foundation.clickable



import androidx.compose.foundation.interaction.MutableInteractionSource



import androidx.compose.animation.core.CubicBezierEasing



import androidx.compose.foundation.layout.Arrangement



import androidx.compose.foundation.layout.Box



import androidx.compose.foundation.layout.Row



import androidx.compose.foundation.layout.fillMaxSize



import androidx.compose.foundation.layout.fillMaxWidth



import androidx.compose.foundation.layout.height



import androidx.compose.foundation.layout.size



import androidx.compose.foundation.layout.width



import androidx.compose.material3.Surface



import androidx.compose.ui.Alignment



import androidx.compose.ui.Modifier



import androidx.compose.ui.draw.clip



import androidx.compose.ui.graphics.Brush



import androidx.compose.ui.graphics.Color



import androidx.compose.ui.graphics.vector.ImageVector



import androidx.compose.ui.platform.LocalClipboardManager



import androidx.compose.ui.platform.LocalConfiguration



import androidx.compose.ui.platform.LocalContext



import androidx.compose.ui.unit.dp

import androidx.compose.ui.unit.sp

import androidx.compose.ui.text.font.FontWeight



import androidx.navigation.NavGraph.Companion.findStartDestination



import androidx.navigation.compose.NavHost



import androidx.navigation.compose.composable



import androidx.navigation.compose.currentBackStackEntryAsState



import androidx.navigation.compose.rememberNavController



import androidx.compose.foundation.Image



import androidx.compose.ui.graphics.asImageBitmap



import androidx.compose.ui.window.Dialog



import androidx.compose.ui.window.DialogProperties



import androidx.compose.ui.text.style.TextAlign



import androidx.compose.foundation.layout.Column



import androidx.compose.foundation.layout.Spacer



import androidx.compose.foundation.layout.aspectRatio



import com.google.zxing.BarcodeFormat















import com.lumen.app.PortraitCaptureActivity



import com.lumen.app.update.AndroidUpdateInstaller



import com.lumen.core.vpn.LumenVpnService



import com.lumen.app.vm.MainViewModel



import com.lumen.ui.components.LumenDialog



import com.lumen.ui.screens.DashboardScreen



import com.lumen.ui.screens.DomainRoutingScreen



import com.lumen.ui.screens.GeoResourcesScreen



import com.lumen.ui.screens.ImportPhaseUi



import com.lumen.ui.screens.LocalStrings



import com.lumen.ui.screens.LogsScreen



import com.lumen.ui.screens.NodeDraft



import com.lumen.ui.screens.NodeEditorModal



import com.lumen.ui.screens.RoutingHubScreen



import com.lumen.ui.screens.RoutingScreen



import com.lumen.ui.screens.ServerListScreen



import com.lumen.ui.screens.SettingsScreen



import com.lumen.ui.screens.SplitModeUi



import com.lumen.ui.screens.stringsForLanguage



import com.lumen.ui.components.ConnectionSliderBar
import androidx.compose.animation.expandHorizontally
import androidx.compose.animation.shrinkHorizontally
import androidx.compose.animation.core.Spring
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.graphicsLayer




import com.lumen.ui.theme.LocalLumenColors



import androidx.compose.animation.AnimatedVisibility



import androidx.compose.animation.expandVertically



import androidx.compose.animation.shrinkVertically







import androidx.compose.animation.core.FastOutSlowInEasing



import androidx.compose.animation.slideInHorizontally



import androidx.compose.animation.slideOutHorizontally



import androidx.compose.foundation.layout.height



import kotlinx.coroutines.Dispatchers



import kotlinx.coroutines.withContext



import java.io.File







// Shared slow-out easing for the bottom bar indicator.



private val NavPremiumEasing = CubicBezierEasing(0.2f, 0f, 0f, 1f)







/**



 * Provider links go to the platform browser / mail client. A device with no handler for



 * the scheme must not take the screen down, so the failure is swallowed.



 */



private fun openExternalUrl(context: android.content.Context, url: String) {



    runCatching {



        context.startActivity(



            android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse(url))



        )



    }



}







private data class LumenDest(val route: String, val icon: ImageVector)



// Outline icons, as in the redesign mockups.



private val DESTINATIONS = listOf(



    LumenDest("dashboard", Icons.Outlined.Home),



    LumenDest("servers", Icons.Outlined.Dns),

    LumenDest("network", Icons.Outlined.Public),



    LumenDest("settings", Icons.Outlined.Tune)



)







@Composable



fun LumenApp(



    viewModel: MainViewModel,



    onToggleConnection: () -> Unit,



    onRestartConnection: () -> Unit = {},



    onLanguageChange: (String) -> Unit



) {



    val settings by viewModel.settings.collectAsStateWithLifecycle()
    val selectedServerFilter by viewModel.selectedServerFilter.collectAsStateWithLifecycle()



    val importState by viewModel.importState.collectAsStateWithLifecycle()



    val systemLanguage = LocalConfiguration.current.locales[0].language



    val language = settings.language.ifBlank { systemLanguage }
    val strings = remember(language) { stringsForLanguage(language) }



    val navController = rememberNavController()



    var editorDraft by remember { mutableStateOf<NodeDraft?>(null) }



    var editorError by remember { mutableStateOf<String?>(null) }



    var qrExportLink by remember { mutableStateOf<String?>(null) }



    val clipboard = LocalClipboardManager.current



    val context = LocalContext.current



    val updateState by viewModel.androidUpdateState.collectAsStateWithLifecycle()







    // Android 8+ grants "install unknown apps" per source. Returning from that



    // settings page is the continuation of the same user-initiated update action.



    val unknownSourcesLauncher = rememberLauncherForActivityResult(



        contract = ActivityResultContracts.StartActivityForResult()



    ) {



        if (AndroidUpdateInstaller.canRequestPackageInstalls(context)) {



            viewModel.prepareAndroidUpdate()



        } else {



            viewModel.reportAndroidUpdateError(strings.updateInstallPermissionRequired)



        }



    }







    // A successful download increments the request id. Keeping the effect at the



    // app root means navigation or recomposition cannot lose the installer launch.



    LaunchedEffect(updateState.installRequestId) {



        val requestId = updateState.installRequestId



        val path = updateState.downloadedApkPath



        if (requestId <= 0L || path.isNullOrBlank()) return@LaunchedEffect



        viewModel.consumeAndroidUpdateInstallRequest(requestId)



        runCatching {



            withContext(Dispatchers.IO) {



                AndroidUpdateInstaller.commitInstall(context.applicationContext, File(path))



            }



        }.onFailure { error ->



            viewModel.reportAndroidUpdateError(



                error.message ?: strings.updateInstallerUnavailable



            )



        }



    }







    fun requestAndroidUpdate() {



        if (AndroidUpdateInstaller.canRequestPackageInstalls(context)) {



            viewModel.prepareAndroidUpdate()



            return



        }



        runCatching {



            unknownSourcesLauncher.launch(



                AndroidUpdateInstaller.unknownSourcesIntent(context)



            )



        }.recoverCatching {



            unknownSourcesLauncher.launch(



                AndroidUpdateInstaller.fallbackSecuritySettingsIntent()



            )



        }.onFailure {



            viewModel.reportAndroidUpdateError(strings.updateInstallerUnavailable)



        }



    }







    val filePicker = rememberLauncherForActivityResult(



        contract = ActivityResultContracts.GetContent()



    ) { uri ->



        // The view model reads the stream on IO with a byte bound: a multi-MiB pick



        // used to be materialised and parsed on the main thread.



        uri?.let { viewModel.prepareImportFromUri(context, it, asFile = true) }



    }







    val qrScanner = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        if (result.resultCode == android.app.Activity.RESULT_OK) {
            result.data?.getStringExtra(PortraitCaptureActivity.RESULT_TEXT)?.takeIf { it.isNotBlank() }
                ?.let { viewModel.prepareImportText(it) }
        }
    }

    var settingsResetSignal by remember { mutableIntStateOf(0) }



    var notifiedUpdateTag by rememberSaveable { mutableStateOf<String?>(null) }







    val mainTabRoutes = remember { MAIN_TAB_ROUTES }



    val activity = context as? android.app.Activity







    // The CAMERA permission is asked for at the point of use, inside :ui



    // (rememberQrScanRequest), which also owns the localized rationale dialog. A second



    // check here would only produce a second prompt, so this stays a plain launch.



    fun startQrScanner() {
        qrScanner.launch(android.content.Intent(context, PortraitCaptureActivity::class.java))
    }

    fun openTab(route: String) { navController.openMainTab(route) }
    BackHandler {
        navController.backOrMinimize { activity?.finish() }
    }

    // A finished import already pointed KEY_SERVERS_LAST_GROUP at the group it landed



    // in; show the user that group instead of leaving them on the dashboard.



    val serverGroupFocus by viewModel.serverGroupFocus.collectAsStateWithLifecycle()



    LaunchedEffect(serverGroupFocus) {



        if (viewModel.consumeServerGroupFocus()) openTab("servers")



    }







    CompositionLocalProvider(



        LocalStrings provides strings,



        com.lumen.ui.screens.LocalHapticsEnabled provides settings.hapticsEnabled



    ) {



        // Connection and import events the user should feel rather than read. Inside the



        // provider on purpose: rememberHapticTick reads LocalHapticsEnabled, which is



        // what keeps the in-app vibration switch in charge of these ticks too.



        val hapticTick = com.lumen.ui.screens.rememberHapticTick()



        LaunchedEffect(hapticTick) {



            viewModel.events.collect { event ->



                hapticTick(



                    if (event == com.lumen.app.vm.LumenEvent.Connected) {



                        androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress



                    } else {



                        androidx.compose.ui.hapticfeedback.HapticFeedbackType.TextHandleMove



                    }



                )



            }



        }







        // Results the user must actually notice: the dashboard "Check" ping and the



        // outcome of a subscription refresh. Drawn by Lumen itself (LumenToastHost)



        // instead of android.widget.Toast so they follow the app theme and always sit



        // right above the navigation pill.



        val toastState = com.lumen.ui.screens.rememberLumenToastState()



        LaunchedEffect(toastState) {



            viewModel.toasts.collect { message -> toastState.show(message) }



        }



        LaunchedEffect(strings, toastState) {



            viewModel.subscriptionSummaries.collect { summary ->



                val text = if (summary.unchanged) {



                    "${summary.subscriptionName}: ${strings.subscriptionNoChanges}"



                } else {



                    val parts = buildList {



                        if (summary.added > 0) add("${strings.subscriptionAddedCount} ${summary.added}")



                        if (summary.updated > 0) add("${strings.subscriptionUpdatedCount} ${summary.updated}")



                        if (summary.removed > 0) add("${strings.subscriptionRemovedCount} ${summary.removed}")



                    }



                    "${strings.subscriptionUpdated} — ${summary.subscriptionName}: " + parts.joinToString(", ")



                }



                toastState.show(text)



            }



        }



        val availableUpdateTag = updateState.latest?.tag



        val availableUpdateVersion = updateState.latest?.version



        LaunchedEffect(



            settings.autoCheckUpdates,



            updateState.updateAvailable,



            availableUpdateTag,



            availableUpdateVersion,



            strings,



            toastState



        ) {



            if (



                settings.autoCheckUpdates &&



                updateState.updateAvailable &&



                !availableUpdateTag.isNullOrBlank() &&



                availableUpdateTag != notifiedUpdateTag



            ) {



                val version = availableUpdateVersion



                    ?.takeIf { it.isNotBlank() }



                    ?.let { "v$it" }



                    ?: availableUpdateTag



                toastState.show(



                    strings.updateAvailableVersion.replace("{version}", version)



                )



                notifiedUpdateTag = availableUpdateTag



            }



        }







        val overlayEntry by navController.currentBackStackEntryAsState()
        val sliderShowing = overlayEntry?.destination?.route == "dashboard" && settings.dashboardStyle == DashboardStyle.SLIDER
        val dockConnection = settings.dashboardStyle == DashboardStyle.DOCK
        var dockVisible by remember { mutableStateOf(true) }
        val dockThresholdPx = with(androidx.compose.ui.platform.LocalDensity.current) { 32.dp.toPx() }
        val dockVisibility = remember(dockThresholdPx, sliderShowing) { com.lumen.ui.components.DockScrollVisibility(dockThresholdPx, keepHiddenAtBottom = sliderShowing) }
        val dockScrollConnection = remember(dockVisibility) {
            object : androidx.compose.ui.input.nestedscroll.NestedScrollConnection {
                override fun onPostScroll(
                    consumed: androidx.compose.ui.geometry.Offset,
                    available: androidx.compose.ui.geometry.Offset,
                    source: androidx.compose.ui.input.nestedscroll.NestedScrollSource
                ): androidx.compose.ui.geometry.Offset {
                    dockVisible = dockVisibility.onScroll(consumed.y, available.y, source == androidx.compose.ui.input.nestedscroll.NestedScrollSource.UserInput)
                    return androidx.compose.ui.geometry.Offset.Zero
                }
                override suspend fun onPreFling(available: androidx.compose.ui.unit.Velocity): androidx.compose.ui.unit.Velocity {
                    dockVisible = dockVisibility.onFling(available.y)
                    return androidx.compose.ui.unit.Velocity.Zero
                }
                override suspend fun onPostFling(consumed: androidx.compose.ui.unit.Velocity, available: androidx.compose.ui.unit.Velocity): androidx.compose.ui.unit.Velocity {
                    if (kotlin.math.abs(available.y) > .5f) dockVisible = dockVisibility.onScroll(0f, available.y)
                    return androidx.compose.ui.unit.Velocity.Zero
                }
            }
        }

        Scaffold(



            containerColor = MaterialTheme.colorScheme.background,



            contentWindowInsets = WindowInsets(0, 0, 0, 0),



            // The snackbar slot is an overlay layer: Scaffold draws it on top of the



            // content, just above the bottom bar, without reserving any layout space.



            // That is exactly what a floating toast island needs - the page below does



            // not move when a notice appears or disappears.



            snackbarHost = {



                com.lumen.ui.screens.LumenToastHost(



                    state = toastState,



                    modifier = Modifier



                        .fillMaxWidth()



                        .navigationBarsPadding().padding(horizontal = 16.dp)
                        .padding(bottom = if (sliderShowing) 12.dp else if (dockVisible) 78.dp else 16.dp)



                )



            },



            bottomBar = {
                val backStack by navController.currentBackStackEntryAsState()
                val currentRoute = backStack?.destination?.route
                val effectiveRoute = when {
                    currentRoute == null -> "dashboard"
                    currentRoute.startsWith("routing") || currentRoute.startsWith("logs") -> "settings"
                    else -> currentRoute
                }
                val selectedIndex = DESTINATIONS.indexOfFirst { it.route == effectiveRoute }
                val lastTabIndex = remember { mutableIntStateOf(0) }
                LaunchedEffect(selectedIndex) {
                    if (selectedIndex >= 0) lastTabIndex.intValue = selectedIndex
                }
                val activeIndex = if (selectedIndex >= 0) selectedIndex else lastTabIndex.intValue
                val navHaptics = androidx.compose.ui.platform.LocalHapticFeedback.current
                val lumenColors = LocalLumenColors.current
                val connectionState by viewModel.connectionState.collectAsStateWithLifecycle()

                com.lumen.ui.components.BottomConnectionControls(
                        showSlider = effectiveRoute == "dashboard" && settings.dashboardStyle == DashboardStyle.SLIDER,
                        dockVisible = sliderShowing || dockConnection || dockVisible, connectionState = connectionState,
                        dockConnection = dockConnection,
                        onToggleConnection = onToggleConnection,
                        items = remember(strings) {
                            DESTINATIONS.map { dest ->
                                com.lumen.ui.components.DockItem(dest.route, when (dest.route) {
                                    "dashboard" -> strings.home
                                    "servers" -> strings.servers
                                    "network" -> com.lumen.ui.screens.NetworkInfoText.TITLE.inLanguage(strings.localeCode)
                                    else -> strings.settings
                                }, dest.icon)
                            }
                        },
                        activeId = DESTINATIONS.getOrNull(activeIndex)?.route ?: "dashboard",
                        onSelect = { route ->
                            if (settings.hapticsEnabled) navHaptics.performHapticFeedback(
                                androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
                            if (route == "settings") {
                                settingsResetSignal++
                                openTab("settings")
                            } else if (route != effectiveRoute) openTab(route)
                        }
                    )
            }
        ) { padding ->



            val mainTabs = remember { MAIN_TAB_ROUTES }



            // Slow-out easing shared by all screen transitions.



            val PremiumEasing = androidx.compose.animation.core.CubicBezierEasing(0.2f, 0f, 0f, 1f)



            fun isTabForward(from: String?, to: String?): Boolean {



                if (from in mainTabs && to in mainTabs) {



                    return mainTabs.indexOf(to) >= mainTabs.indexOf(from)



                }



                return true



            }







            val navBackStackEntry by navController.currentBackStackEntryAsState()



            val currentNavRoute = navBackStackEntry?.destination?.route

            LaunchedEffect(currentNavRoute) { dockVisibility.reset(); dockVisible = true }



            Column(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {



                NavHost(



                    navController = navController,



                    startDestination = "dashboard",



                    // Every tab reserves room for the bar so nothing scrolls underneath it.



                    modifier = Modifier.weight(1f)
                        .then(if (sliderShowing || dockConnection) Modifier else Modifier.nestedScroll(dockScrollConnection)).padding(



                    top = padding.calculateTopPadding(),



                    bottom = padding.calculateBottomPadding()



                    ),



                enterTransition = { com.lumen.ui.components.expressivePageEnter(isTabForward(initialState.destination.route, targetState.destination.route)) },
                exitTransition = { com.lumen.ui.components.expressivePageExit(isTabForward(initialState.destination.route, targetState.destination.route)) },
                popEnterTransition = { com.lumen.ui.components.expressivePageEnter(false) },
                popExitTransition = { com.lumen.ui.components.expressivePageExit(false) }
            ) {
                composable("dashboard") {



                    val connectionState by viewModel.connectionState.collectAsStateWithLifecycle()



                    val nodes by viewModel.nodes.collectAsStateWithLifecycle()



                    val subscriptions by viewModel.subscriptions.collectAsStateWithLifecycle()



                    val serverGroups by viewModel.serverGroups.collectAsStateWithLifecycle()



                    val pingingNodeIds by viewModel.pingingNodeIds.collectAsStateWithLifecycle()



                    val trafficStats = LumenVpnService.trafficStats.collectAsStateWithLifecycle()



                    val connectedPing by viewModel.connectedPing.collectAsStateWithLifecycle()



                    val checkingPing by viewModel.checkingConnectedPing.collectAsStateWithLifecycle()



                    DashboardScreen(
                        selectedGroupFilter = selectedServerFilter,



                        onOpenServers = { openTab("servers") },



                        connectionState = connectionState,



                        nodes = nodes,



                        subscriptions = subscriptions,



                        serverGroups = serverGroups,



                        speedStatsEnabled = settings.enableSpeedStats,



                        speedValues = {
                            val value = trafficStats.value
                            com.lumen.ui.screens.DashboardSpeeds(value.downloadSpeed, value.uploadSpeed)
                        },







                        pingingNodeIds = pingingNodeIds,



                        dashboardStyle = settings.dashboardStyle,



                        connectedPing = connectedPing,



                        isCheckingPing = checkingPing,



                        onCheckPing = {



                            // A failed measurement reads as "0 ms" instead of a localized



                            // "unreachable", matching how the server rows report it.



                            viewModel.checkConnectedPing("0 ms", strings.noServerSelected)



                        },



                        onToggleConnection = onToggleConnection,



                        onSelectNode = { node ->



                            val wasSelected = node.isSelected



                            viewModel.selectNode(node)



                            if (!wasSelected && LumenVpnService.isRunning.value) onRestartConnection()



                        },



                        onImportClipboard = {



                            viewModel.prepareImportText(clipboard.getText()?.text)



                        },



                        onImportFile = { filePicker.launch("*/*") },



                        onImportQr = { startQrScanner() },



                        onAddManualNode = { editorDraft = NodeDraft() },



                        onRefreshSubscription = viewModel::refreshSubscription,



                        onDeleteSubscription = viewModel::deleteSubscription,



                        onPingGroup = viewModel::pingGroup,
                        onStopPing = viewModel::stopPing,

                        onEditNode = { node -> editorDraft = viewModel.draftForNode(node) },



                        onPingNode = viewModel::pingNode,



                        onCopyNodeLink = { node ->



                            val link = viewModel.exportNodesText(setOf(node.id))



                            if (link.isNotBlank()) {



                                clipboard.setText(androidx.compose.ui.text.AnnotatedString(link))



                            }



                        },



                        onExportQrCode = { node ->



                            val link = viewModel.exportNodesText(setOf(node.id))



                            if (link.isNotBlank()) {



                                qrExportLink = link



                            }



                        },



                        onDeleteNode = viewModel::deleteNode,



                        onPingNodes = viewModel::pingNodes,



                        onExportNodesText = viewModel::exportNodesText,



                        onExportSubscriptionText = viewModel::exportSubscriptionText,



                        onCopyText = { text ->



                            clipboard.setText(androidx.compose.ui.text.AnnotatedString(text))



                        },



                        onShareText = { text ->



                            val shareIntent = android.content.Intent(android.content.Intent.ACTION_SEND).apply {



                                type = "text/plain"



                                putExtra(android.content.Intent.EXTRA_TEXT, text)



                            }



                            context.startActivity(android.content.Intent.createChooser(shareIntent, "Export"))



                        },



                        onOpenUrl = { url -> openExternalUrl(context, url) }



                    )



                }



                composable("servers") {



                    val nodes by viewModel.nodes.collectAsStateWithLifecycle()



                    val subscriptions by viewModel.subscriptions.collectAsStateWithLifecycle()



                    val serverGroups by viewModel.serverGroups.collectAsStateWithLifecycle()



                    val refreshingIds by viewModel.refreshingIds.collectAsStateWithLifecycle()



                    val isPinging by viewModel.isPinging.collectAsStateWithLifecycle()



                    val testingNodeId by viewModel.testingNodeId.collectAsStateWithLifecycle()



                    val serverTestResults by viewModel.serverTestResults.collectAsStateWithLifecycle()



                    val serversConnectionState by viewModel.connectionState.collectAsStateWithLifecycle()



                    val serversPingingIds by viewModel.pingingNodeIds.collectAsStateWithLifecycle()



                    // Latency colours follow the thresholds from the ping settings.



                    LaunchedEffect(settings.pingGoodMs, settings.pingFairMs) {



                        com.lumen.ui.screens.PingThresholds.goodMs = settings.pingGoodMs



                        com.lumen.ui.screens.PingThresholds.fairMs = settings.pingFairMs



                    }



                    ServerListScreen(
                        compactGrid = settings.compactServerGrid,
                        selectedGroupFilter = selectedServerFilter,
                        onSelectedGroupFilter = viewModel::selectServerFilter,



                        nodes = nodes,



                        subscriptions = subscriptions,



                        serverGroups = serverGroups,



                        refreshingIds = refreshingIds,



                        isPinging = isPinging,



                        pingingNodeIds = serversPingingIds,



                        autoPingOnOpen = settings.pingAutoOnOpen,



                        testingNodeId = testingNodeId,



                        serverTestResults = serverTestResults,



                        connectionState = serversConnectionState,



                        onToggleConnection = onToggleConnection,



                        onSelectNode = { node ->



                            val wasSelected = node.isSelected



                            viewModel.selectNode(node)



                            if (!wasSelected && LumenVpnService.isRunning.value) onRestartConnection()



                        },



                        onEditNode = { node -> editorDraft = viewModel.draftForNode(node) },



                        onDeleteNode = viewModel::deleteNode,



                        onDeleteAllNodes = viewModel::deleteAllNodes,



                        onAddNode = { editorDraft = NodeDraft() },



                        onPingAll = viewModel::pingAll,



                        onStopPing = viewModel::stopPing,



                        onPingNodes = viewModel::pingNodes,



                        onPingNode = viewModel::pingNode,



                        onCopyNodeLink = { node ->



                            val link = viewModel.exportNodesText(setOf(node.id))



                            if (link.isNotBlank()) {



                                clipboard.setText(androidx.compose.ui.text.AnnotatedString(link))



                            }



                        },



                        onExportQrCode = { node ->



                            val link = viewModel.exportNodesText(setOf(node.id))



                            if (link.isNotBlank()) {



                                qrExportLink = link



                            }



                        },



                        onImportClipboard = {



                            viewModel.prepareImportText(clipboard.getText()?.text)



                        },



                        onImportFile = { filePicker.launch("*/*") },



                        onImportQr = { startQrScanner() },






                        onUpdateSubscription = viewModel::updateSubscription,



                        onRefreshSubscription = viewModel::refreshSubscription,



                        onDeleteSubscription = viewModel::deleteSubscription,



                        onPingGroup = viewModel::pingGroup,



                        onCreateGroup = viewModel::createServerGroup,



                        onRenameGroup = viewModel::renameServerGroup,



                        onDeleteGroup = viewModel::deleteServerGroup,



                        onAssignNodesToGroup = viewModel::assignNodesToGroup,



                        onExportNodesText = viewModel::exportNodesText,



                        onExportSubscriptionText = viewModel::exportSubscriptionText,



                        onCopyText = { text ->



                            clipboard.setText(androidx.compose.ui.text.AnnotatedString(text))



                        },



                        onShareText = { text ->



                            val shareIntent = android.content.Intent(android.content.Intent.ACTION_SEND).apply {



                                type = "text/plain"



                                putExtra(android.content.Intent.EXTRA_TEXT, text)



                            }



                            context.startActivity(android.content.Intent.createChooser(shareIntent, "Export"))



                        },



                        onOpenUrl = { url -> openExternalUrl(context, url) }



                    )



                }



                composable("routing") {



                    val splitMode by viewModel.splitMode.collectAsStateWithLifecycle()



                    RoutingHubScreen(



                        onOpenDomainIp = { navController.navigate("routing/domain") },



                        onOpenApps = { navController.navigate("routing/apps") },



                        onOpenGeoResources = { navController.navigate("routing/geo") },



                        onBack = { navController.popBackStack() },



                        appsSummary = when (splitMode) {



                            SplitModeUi.DISABLED -> strings.off



                            SplitModeUi.ALLOW_LIST -> strings.vpnOnly



                            SplitModeUi.DISALLOW_LIST -> strings.exclude



                        }



                    )



                }



                composable("routing/domain") {



                    DomainRoutingScreen(



                        directDomains = settings.directDomains,



                        directIpCidrs = settings.directIpCidrs,



                        onDirectRulesChange = { domains, ipCidrs ->



                            viewModel.updateSettings(settings.copy(directDomains = domains, directIpCidrs = ipCidrs))



                        },



                        onBack = { navController.popBackStack() }



                    )



                }



                composable("routing/apps") {



                    val mode by viewModel.splitMode.collectAsStateWithLifecycle()



                    val apps by viewModel.apps.collectAsStateWithLifecycle()



                    val loading by viewModel.isLoadingApps.collectAsStateWithLifecycle()



                    LaunchedEffect(mode) { if (mode != SplitModeUi.DISABLED) viewModel.loadInstalledApps() }



                    RoutingScreen(



                        mode, apps, loading, viewModel::setSplitMode, viewModel::toggleApp,



                        viewModel::autoSelectApps, viewModel::clearAppSelection,



                        onBack = { navController.popBackStack() }



                    )



                }



                composable("routing/geo") {



                    val resources by viewModel.geoResources.collectAsStateWithLifecycle()



                    val updating by viewModel.isUpdatingGeoResources.collectAsStateWithLifecycle()



                    LaunchedEffect(Unit) { viewModel.refreshGeoResources() }



                    GeoResourcesScreen(



                        resources = resources,



                        source = settings.geoResourceSource,



                        isUpdating = updating,



                        onDownload = viewModel::downloadGeoResources,



                        onBack = { navController.popBackStack() }



                    )



                }



                composable("network") {
                    com.lumen.app.network.NetworkInfoRoute(viewModel)
                }

                composable("settings") {



                    SettingsScreen(



                        state = settings,



                        onUpdate = viewModel::updateSettings,
                        onReset = viewModel::resetLumen,
                        onOpenWelcome = viewModel::reopenOnboarding,
                        onPageOpened = { dockVisibility.reset(); dockVisible = true },



                        onLanguageChange = onLanguageChange,



                        onOpenRouting = { navController.navigate("routing") { launchSingleTop = true } },



                        onOpenLogs = { navController.navigate("logs") { launchSingleTop = true } },



                        onOpenCommunity = {



                            context.startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse("https://t.me/lumenkvn")))



                        },



                        updateChecked = updateState.checked,



                        updateIsChecking = updateState.isChecking,



                        updateLatestVersion = updateState.latest?.version,



                        updateReleaseTag = updateState.latest?.tag,



                        updateAvailable = updateState.updateAvailable,



                        updateError = updateState.error,



                        updateIsDownloading = updateState.isDownloading,



                        updateDownloadProgress = updateState.downloadProgress,



                        onCheckUpdates = viewModel::checkForAndroidUpdate,



                        onInstallUpdate = ::requestAndroidUpdate,



                        resetToHubSignal = settingsResetSignal



                    )



                }



                composable("logs") {



                    // Logs are only formatted while this tab is visible; leaving it clears the flow.



                    DisposableEffect(Unit) {



                        viewModel.setLogsVisible(true)



                        onDispose { viewModel.setLogsVisible(false) }



                    }



                    val logEntries by viewModel.logEntries.collectAsStateWithLifecycle()



                    val moreLogHistory by viewModel.moreLogHistory.collectAsStateWithLifecycle()
                    val loadingLogHistory by viewModel.loadingLogHistory.collectAsStateWithLifecycle()



                    LogsScreen(



                        logs = emptyList(),



                        onClear = viewModel::clearLogs,



                        onExport = { viewModel.exportLogs(context) },



                        onBack = { navController.popBackStack() },



                        entries = logEntries,



                        // Dropped once the store has handed over everything it kept.



                        onLoadMore = if (moreLogHistory) viewModel::loadOlderLogs else null,
                        isLoadingHistory = loadingLogHistory,



                        onExportText = { viewModel.exportLogText(context, it) }



                    )



                }



            }



            }



        }







        com.lumen.app.onboarding.OnboardingRoute(
            viewModel = viewModel, onLanguage = onLanguageChange,
            onImportClipboard = { viewModel.prepareImportText(clipboard.getText()?.text) },
            onImportFile = { filePicker.launch("*/*") }, onImportQr = ::startQrScanner,
            onRevealHome = {
                navController.navigate("dashboard") {
                    popUpTo(navController.graph.id) { inclusive = false }
                    launchSingleTop = true
                    restoreState = false
                }
            }
        )

        if (importState.phase != ImportPhaseUi.HIDDEN) {



            val busy = importState.phase == ImportPhaseUi.IMPORTING



            val awaiting = importState.phase == ImportPhaseUi.AWAITING



            LumenDialog(



                title = importState.title,



                message = importState.message,



                busy = busy,



                onDismissRequest = { if (!busy) viewModel.dismissImport() },



                confirmText = if (awaiting) strings.importAction else "OK",



                onConfirm = { if (awaiting) viewModel.confirmImport() else viewModel.dismissImport() },



                dismissText = if (awaiting) strings.cancel else null,



                onDismiss = { viewModel.dismissImport() }



            )



        }







        qrExportLink?.let { link ->



            // Plain portrait card: no elevation, glow or scaling effects around the code.



            Dialog(



                onDismissRequest = { qrExportLink = null },



                properties = DialogProperties(usePlatformDefaultWidth = false)



            ) {



                Surface(



                    shape = RoundedCornerShape(24.dp),



                    color = MaterialTheme.colorScheme.surface,



                    tonalElevation = 0.dp,



                    shadowElevation = 0.dp,



                    modifier = Modifier.fillMaxWidth(0.88f)



                ) {



                    val qrImage by androidx.compose.runtime.produceState<Pair<String, android.graphics.Bitmap?>?>(
                        initialValue = null, key1 = link
                    ) {
                        value = withContext(Dispatchers.Default) {
                            link to runCatching {
                                com.lumen.app.encodeQrBitmap(link)
                            }.getOrNull()
                        }
                    }
                    val qrBitmap = qrImage?.takeIf { it.first == link }?.second
                    Column(



                        modifier = Modifier.fillMaxWidth().padding(20.dp),



                        horizontalAlignment = Alignment.CenterHorizontally



                    ) {



                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                            Text(strings.exportQrCode, style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
                            IconButton(onClick = { qrExportLink = null }, modifier = Modifier.size(48.dp)) {
                                Icon(Icons.Default.Close, contentDescription = strings.close)
                            }
                        }
                        Spacer(Modifier.height(12.dp))
                        if (qrBitmap != null) {



                            Surface(



                                shape = RoundedCornerShape(16.dp),



                                color = Color.White,



                                tonalElevation = 0.dp,



                                shadowElevation = 0.dp



                            ) {



                                Image(



                                    bitmap = qrBitmap.asImageBitmap(),



                                    contentDescription = "QR",



                                    modifier = Modifier.padding(14.dp).fillMaxWidth().aspectRatio(1f)



                                )



                            }



                            Spacer(Modifier.height(16.dp))



                            Text(



                                text = link,



                                color = MaterialTheme.colorScheme.onSurfaceVariant,



                                style = MaterialTheme.typography.bodySmall,



                                maxLines = 3,



                                textAlign = TextAlign.Center



                            )



                         } else if (qrImage?.first != link) {
                            androidx.compose.material3.CircularProgressIndicator(Modifier.padding(24.dp))
                        } else {
                            Text(



                                text = strings.centerError,



                                color = MaterialTheme.colorScheme.onSurface,



                                modifier = Modifier.padding(24.dp)



                            )



                        }



                    }



                }



            }



        }







        editorDraft?.let { draft ->



            NodeEditorModal(draft, { editorDraft = null; editorError = null }) { edited ->



                // The editor stays open when the draft cannot be turned into a node,



                // instead of closing as if the save had succeeded.



                viewModel.saveDraft(edited) { error ->



                    if (error == null) editorDraft = null else editorError = error



                }



            }



        }







        // Why the last start produced no tunnel, in the core's own words. Without



        // this the button just flips back and the reason only exists in the logs.



        val connectError by viewModel.connectError.collectAsStateWithLifecycle()



        connectError?.let { message ->



            LumenDialog(



                title = strings.connectionError,



                message = message,



                onDismissRequest = viewModel::dismissConnectError,



                confirmText = "OK",



                onConfirm = viewModel::dismissConnectError



            )



        }







        editorError?.let { message ->



            LumenDialog(



                title = strings.centerError,



                message = message,



                onDismissRequest = { editorError = null },



                confirmText = "OK",



                onConfirm = { editorError = null }



            )



        }



    }



}
