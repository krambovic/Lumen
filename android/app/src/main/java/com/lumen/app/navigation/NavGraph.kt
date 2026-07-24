package com.lumen.app.navigation

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.core.tween
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.AlertDialog
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import androidx.compose.foundation.Image
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.window.Dialog
import com.google.zxing.BarcodeFormat
import com.journeyapps.barcodescanner.BarcodeEncoder
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import com.lumen.app.PortraitCaptureActivity
import com.lumen.core.vpn.LumenVpnService
import com.lumen.app.vm.MainViewModel
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
import com.lumen.ui.screens.SettingsScreen
import com.lumen.ui.screens.SplitModeUi
import com.lumen.ui.screens.stringsForLanguage

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.layout.height

private data class LumenDest(val route: String, val icon: ImageVector)
private val DESTINATIONS = listOf(
    LumenDest("dashboard", Icons.Filled.Home),
    LumenDest("settings", Icons.Filled.Settings)
)

@Composable
fun LumenApp(
    viewModel: MainViewModel,
    onToggleConnection: () -> Unit,
    onRestartConnection: () -> Unit = {},
    onLanguageChange: (String) -> Unit
) {
    val settings by viewModel.settings.collectAsStateWithLifecycle()
    val importState by viewModel.importState.collectAsStateWithLifecycle()
    val systemLanguage = LocalConfiguration.current.locales[0].language
    val strings = stringsForLanguage(settings.language.ifBlank { systemLanguage })
    val navController = rememberNavController()
    var editorDraft by remember { mutableStateOf<NodeDraft?>(null) }
    var qrExportLink by remember { mutableStateOf<String?>(null) }
    val clipboard = LocalClipboardManager.current
    val context = LocalContext.current

    val filePicker = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.GetContent()
    ) { uri ->
        uri?.let {
            val content = context.contentResolver.openInputStream(it)?.use { stream ->
                stream.bufferedReader().readText()
            }
            if (!content.isNullOrBlank()) {
                viewModel.prepareImportText(content)
            }
        }
    }

    val qrScanner = rememberLauncherForActivityResult(ScanContract()) { result ->
        result.contents?.takeIf { it.isNotBlank() }?.let { viewModel.prepareImportText(it) }
    }

    var settingsResetSignal by remember { mutableIntStateOf(0) }

    CompositionLocalProvider(LocalStrings provides strings) {
        Scaffold(
            containerColor = MaterialTheme.colorScheme.background,
            contentWindowInsets = WindowInsets(0, 0, 0, 0),
            bottomBar = {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .navigationBarsPadding()
                        .padding(bottom = 12.dp),
                    contentAlignment = Alignment.Center
                ) {
                    val pillShape = RoundedCornerShape(24.dp)
                    val primaryPaletteColor = MaterialTheme.colorScheme.primary
                    val pillBgColor = MaterialTheme.colorScheme.surfaceVariant
                    val pillBorderColor = primaryPaletteColor.copy(alpha = 0.35f)

                    Box(
                        modifier = Modifier
                            .width(170.dp)
                            .height(48.dp)
                            .clip(pillShape)
                            .background(pillBgColor)
                            .border(1.dp, pillBorderColor, pillShape)
                    ) {
                        Row(
                            modifier = Modifier.fillMaxSize(),
                            horizontalArrangement = Arrangement.SpaceEvenly,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            val backStack by navController.currentBackStackEntryAsState()
                            val currentRoute = backStack?.destination?.route
                            DESTINATIONS.forEach { dest ->
                                val selected = currentRoute == dest.route
                                val label = when (dest.route) {
                                    "dashboard" -> strings.home
                                    "settings" -> strings.settings
                                    else -> strings.home
                                }
                                val iconColor by animateColorAsState(
                                    targetValue = if (selected) primaryPaletteColor else MaterialTheme.colorScheme.onSurfaceVariant,
                                    animationSpec = tween(190),
                                    label = "nav_icon_color"
                                )
                                val bgColor by animateColorAsState(
                                    targetValue = if (selected) primaryPaletteColor.copy(alpha = 0.22f) else Color.Transparent,
                                    animationSpec = tween(190),
                                    label = "nav_bg_color"
                                )

                                Box(
                                    modifier = Modifier
                                        .size(width = 60.dp, height = 34.dp)
                                        .clip(RoundedCornerShape(17.dp))
                                        .background(bgColor)
                                         .clickable {
                                            if (dest.route == "settings") {
                                                settingsResetSignal++
                                                if (currentRoute != "settings") {
                                                    navController.navigate("settings") {
                                                        launchSingleTop = true
                                                    }
                                                }
                                            } else if (!selected) {
                                                navController.navigate(dest.route) {
                                                    launchSingleTop = true
                                                }
                                            }
                                        },
                                    contentAlignment = Alignment.Center
                                ) {
                                    Icon(
                                        imageVector = dest.icon,
                                        contentDescription = label,
                                        tint = iconColor,
                                        modifier = Modifier.size(20.dp)
                                    )
                                }
                            }
                        }
                    }
                }
            }
            val routeOrder = remember { listOf("dashboard", "servers", "routing", "settings") }
            fun getRouteIndex(route: String?): Int {
                val idx = routeOrder.indexOf(route)
                return if (idx >= 0) idx else 99
            }

            NavHost(
                navController = navController,
                startDestination = "dashboard",
                modifier = Modifier.padding(padding),
                enterTransition = {
                    val initialIdx = getRouteIndex(initialState.destination.route)
                    val targetIdx = getRouteIndex(targetState.destination.route)
                    if (targetIdx >= initialIdx) {
                        slideInHorizontally(tween(220, easing = FastOutSlowInEasing)) { it / 3 } + fadeIn(tween(220))
                    } else {
                        slideInHorizontally(tween(220, easing = FastOutSlowInEasing)) { -it / 3 } + fadeIn(tween(220))
                    }
                },
                exitTransition = {
                    val initialIdx = getRouteIndex(initialState.destination.route)
                    val targetIdx = getRouteIndex(targetState.destination.route)
                    if (targetIdx >= initialIdx) {
                        slideOutHorizontally(tween(220, easing = FastOutSlowInEasing)) { -it / 3 } + fadeOut(tween(220))
                    } else {
                        slideOutHorizontally(tween(220, easing = FastOutSlowInEasing)) { it / 3 } + fadeOut(tween(220))
                    }
                },
                popEnterTransition = {
                    slideInHorizontally(tween(220, easing = FastOutSlowInEasing)) { -it / 3 } + fadeIn(tween(220))
                },
                popExitTransition = {
                    slideOutHorizontally(tween(220, easing = FastOutSlowInEasing)) { it / 3 } + fadeOut(tween(220))
                }
            ) {
                composable("dashboard") {
                    val connectionState by viewModel.connectionState.collectAsStateWithLifecycle()
                    val nodes by viewModel.nodes.collectAsStateWithLifecycle()
                    val subscriptions by viewModel.subscriptions.collectAsStateWithLifecycle()
                    DashboardScreen(
                        connectionState = connectionState,
                        nodes = nodes,
                        subscriptions = subscriptions,
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
                        onImportQr = {
                            qrScanner.launch(
                                ScanOptions()
                                    .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                                    .setBeepEnabled(false)
                                    .setOrientationLocked(true)
                                    .setPrompt("")
                            )
                        },
                        onAddManualNode = { editorDraft = NodeDraft() },
                        onRefreshSubscription = viewModel::refreshSubscription,
                        onDeleteSubscription = viewModel::deleteSubscription,
                        onPingAll = viewModel::pingAll,
                        onPingGroup = viewModel::pingGroup,
                        onUdpPingGroup = viewModel::pingGroupUdp,
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
                        }
                    )
                }
                composable("routing") {
                    RoutingHubScreen(
                        onOpenDomainIp = { navController.navigate("routing/domain") },
                        onOpenApps = { navController.navigate("routing/apps") },
                        onOpenGeoResources = { navController.navigate("routing/geo") },
                        onBack = { navController.popBackStack() }
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
                        onSourceChange = { viewModel.updateSettings(settings.copy(geoResourceSource = it)) },
                        onDownload = viewModel::downloadGeoResources,
                        onBack = { navController.popBackStack() }
                    )
                }
                composable("settings") {
                    SettingsScreen(
                        state = settings,
                        onUpdate = viewModel::updateSettings,
                        onLanguageChange = onLanguageChange,
                        onOpenRouting = { navController.navigate("routing") { launchSingleTop = true } },
                        onOpenLogs = { navController.navigate("logs") { launchSingleTop = true } },
                        onOpenCommunity = {
                            context.startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse("https://t.me/lumenkvn")))
                        },
                        resetToHubSignal = settingsResetSignal
                    )
                }
                composable("logs") {
                    val logs by viewModel.logs.collectAsStateWithLifecycle()
                    LogsScreen(
                        logs = logs,
                        onClear = viewModel::clearLogs,
                        onExport = { viewModel.exportLogs(context) },
                        onBack = { navController.popBackStack() }
                    )
                }
            }
        }

        if (importState.phase != ImportPhaseUi.HIDDEN) {
            val busy = importState.phase == ImportPhaseUi.IMPORTING
            AlertDialog(
                onDismissRequest = { if (!busy) viewModel.dismissImport() },
                title = { Text(importState.title) },
                text = {
                    if (busy) CircularProgressIndicator()
                    else Text(importState.message)
                },
                confirmButton = {
                    if (importState.phase == ImportPhaseUi.AWAITING) {
                        Button(onClick = viewModel::confirmImport) { Text("Import") }
                    } else if (!busy) {
                        Button(onClick = viewModel::dismissImport) { Text("OK") }
                    }
                },
                dismissButton = {
                    if (importState.phase == ImportPhaseUi.AWAITING) {
                        TextButton(onClick = viewModel::dismissImport) { Text(strings.cancel) }
                    }
                }
            )
        }

        qrExportLink?.let { link ->
            Dialog(onDismissRequest = { qrExportLink = null }) {
                Surface(shape = RoundedCornerShape(24.dp), color = Color.White) {
                    val qrBitmap = remember(link) {
                        runCatching {
                            BarcodeEncoder().encodeBitmap(link, BarcodeFormat.QR_CODE, 720, 720)
                        }.getOrNull()
                    }
                    if (qrBitmap != null) {
                        Image(
                            bitmap = qrBitmap.asImageBitmap(),
                            contentDescription = "QR",
                            modifier = Modifier.padding(20.dp).size(280.dp)
                        )
                    } else {
                        Text(
                            text = "QR error",
                            color = Color.Black,
                            modifier = Modifier.padding(24.dp)
                        )
                    }
                }
            }
        }

        editorDraft?.let { draft ->
            NodeEditorModal(draft, { editorDraft = null }) {
                viewModel.saveDraft(it)
                editorDraft = null
            }
        }
    }
}
