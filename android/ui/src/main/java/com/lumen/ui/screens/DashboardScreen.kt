package com.lumen.ui.screens

import androidx.activity.compose.BackHandler
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.TextButton
import androidx.compose.ui.draw.shadow
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.NetworkCheck
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import kotlinx.coroutines.launch
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.foundation.BorderStroke
import androidx.compose.material3.Surface
import com.lumen.ui.components.ConnectionSliderBar
import com.lumen.ui.components.ConnectionState
import com.lumen.ui.components.HeroConnectButton
import com.lumen.ui.components.CountryFlagIcon
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.draw.scale
import kotlinx.coroutines.delay

@Composable
fun DashboardScreen(
    connectionState: ConnectionState,
    nodes: List<NodeUiModel>,
    subscriptions: List<SubscriptionUiModel>,
    pingingNodeIds: Set<String> = emptySet(),
    sortByPingOverride: Boolean = false,
    dashboardStyle: DashboardStyle = DashboardStyle.DEFAULT,
    onToggleConnection: () -> Unit,
    onSelectNode: (NodeUiModel) -> Unit,
    onImportClipboard: () -> Unit,
    onImportFile: () -> Unit,
    onImportQr: () -> Unit,
    onAddManualNode: () -> Unit,
    onRefreshSubscription: (String) -> Unit = {},
    onDeleteSubscription: (String) -> Unit = {},
    onPingAll: () -> Unit = {},
    onUdpPingAll: () -> Unit = {},
    onPingGroup: (String?) -> Unit = {},
    onUdpPingGroup: (String?) -> Unit = {},
    onEditNode: (NodeUiModel) -> Unit = {},
    onPingNode: (NodeUiModel) -> Unit = {},
    onUdpPingNode: (NodeUiModel) -> Unit = {},
    onCopyNodeLink: (NodeUiModel) -> Unit = {},
    onExportQrCode: (NodeUiModel) -> Unit = {},
    onDeleteNode: (NodeUiModel) -> Unit = {},
    onPingNodes: (List<NodeUiModel>) -> Unit = {},
    onExportNodesText: (Set<String>) -> String = { "" },
    onExportSubscriptionText: (String?) -> String = { "" },
    onCopyText: (String) -> Unit = {},
    onShareText: (String) -> Unit = {},
    modifier: Modifier = Modifier
) {
    val strings = LocalStrings.current
    var showImportMenu by remember { mutableStateOf(false) }
    var showSortMenu by remember { mutableStateOf(false) }
    var showPingAllMenu by remember { mutableStateOf(false) }
    var sortByPing by remember { mutableStateOf(false) }

    // Multi-selection state
    var isSelectionMode by remember { mutableStateOf(false) }
    var selectedNodeIds by remember { mutableStateOf(setOf<String>()) }

    // Back in selection mode only clears the selection, it must not change tabs.
    BackHandler(enabled = isSelectionMode) {
        isSelectionMode = false
        selectedNodeIds = emptySet()
    }

    var showDonateDialog by remember { mutableStateOf(false) }
    var showUsdtDialog by remember { mutableStateOf(false) }
    val uriHandler = LocalUriHandler.current
    val clipboard = LocalClipboardManager.current
    val haptics = LocalHapticFeedback.current
    val hapticsEnabled = LocalHapticsEnabled.current
    // Subscription properties dialog state
    var propertiesSub by remember { mutableStateOf<SubscriptionUiModel?>(null) }

    // One grouping pass + allocation-free name comparator: ping refreshes rebuild this
    // list on every update, so it must not scan the node list once per subscription.
    // "Sort after ping" from settings overrides the local sort choice.
    val effectiveSortByPing = sortByPing || sortByPingOverride
    val groups = remember(nodes, subscriptions, strings.manual, effectiveSortByPing) {
        val byName = compareBy(String.CASE_INSENSITIVE_ORDER, NodeUiModel::name)
        val byPing = compareBy<NodeUiModel> { it.pingMs?.takeIf { p -> p >= 0 } ?: Int.MAX_VALUE }
        val comparator = if (effectiveSortByPing) byPing else byName
        val bySubscription = nodes.filterNot { it.isAutoNode }.groupBy { it.subscriptionId }
        buildList {
            bySubscription[null]?.takeIf { it.isNotEmpty() }?.let {
                add(HomeServerGroup("manual", strings.groupDefault, it.sortedWith(comparator)))
            }
            subscriptions.forEach { subscription ->
                val subscriptionNodes = bySubscription[subscription.id].orEmpty()
                if (subscriptionNodes.isNotEmpty()) {
                    add(HomeServerGroup(subscription.id, subscription.name, subscriptionNodes.sortedWith(comparator), true, subscription))
                }
            }
        }
    }
    // Key on group ids: a new group appears expanded, while ping refreshes (which
    // rebuild `groups` but keep ids) no longer reset the user's collapse state.
    val groupIds = remember(groups) { groups.map { it.id } }
    var expandedGroups by remember(groupIds) { mutableStateOf(groupIds.toSet()) }

    // Dashboard shows only the group (manual list or subscription) that owns the
    // currently selected server. Full list stays available on the Servers tab.
    // Group of the currently selected server, remembered across restarts so the
    // dashboard reopens on the same subscription/manual list even before nodes load.
    // Both tabs read the same preference, so the group chosen on the Servers tab
    // is exactly what the dashboard shows ("all", "manual" or a subscription id).
    val serversGroupPref by rememberUiPreference("servers_last_group", "all")
    val activeGroupId = remember(nodes) {
        nodes.firstOrNull { it.isSelected && !it.isAutoNode }?.let { it.subscriptionId ?: "manual" }
    }
    val visibleGroups = remember(groups, serversGroupPref, activeGroupId) {
        if (serversGroupPref.isBlank() || serversGroupPref == "all") {
            groups.filter { it.id == activeGroupId }.ifEmpty { groups }
        } else {
            groups.filter { it.id == serversGroupPref }
                .ifEmpty { groups.filter { it.id == activeGroupId } }
                .ifEmpty { groups }
        }
    }

    Column(modifier = modifier.fillMaxSize()) {
        // Selection mode top bar
        if (isSelectionMode) {
            SelectionTopBar(
                selectedCount = selectedNodeIds.size,
                onCancel = {
                    isSelectionMode = false
                    selectedNodeIds = emptySet()
                },
                onExportSelected = {
                    val text = onExportNodesText(selectedNodeIds)
                    if (text.isNotBlank()) onShareText(text)
                },
                onPingSelected = {
                    val selectedNodes = nodes.filter { it.id in selectedNodeIds }
                    if (selectedNodes.isNotEmpty()) onPingNodes(selectedNodes)
                }
            )
        } else {
            LumenScreenHeader(title = "Lumen", subtitle = "v${LumenVersion.appVersion}", actions = {
                IconButton(onClick = { uriHandler.openUri("https://github.com/krambovic/Lumen") }) {
                    Icon(Icons.Filled.Code, contentDescription = "GitHub")
                }
                Box {
                    IconButton(onClick = { showDonateDialog = true }) {
                        Icon(Icons.Filled.Favorite, contentDescription = "Donate")
                    }
                    LumenMenu(
                        expanded = showDonateDialog,
                        onDismissRequest = { showDonateDialog = false }
                    ) {
                        DropdownMenuItem(
                            text = { Text("DonationAlerts") },
                            onClick = {
                                uriHandler.openUri("https://www.donationalerts.com/r/studiobebraedition")
                                showDonateDialog = false
                            }
                        )
                        DropdownMenuItem(
                            text = { Text("USDT TRC20") },
                            onClick = {
                                showUsdtDialog = true
                                showDonateDialog = false
                            }
                        )
                    }
                }
                if (showUsdtDialog) {
                    com.lumen.ui.components.LumenDialog(
                        title = "USDT TRC20",
                        onDismissRequest = { showUsdtDialog = false },
                        confirmText = "Скопировать",
                        onConfirm = {
                            clipboard.setText(AnnotatedString("TWHsuUDru4pXBcGpfeKfzymbkfajyFnb2s"))
                            showUsdtDialog = false
                        },
                        dismissText = "Закрыть",
                        onDismiss = { showUsdtDialog = false }
                    ) {
                        Column(
                            modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
                            horizontalAlignment = Alignment.CenterHorizontally
                        ) {
                            Text(
                                text = "TWHsuUDru4pXBcGpfeKfzymbkfajyFnb2s",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurface,
                                textAlign = androidx.compose.ui.text.style.TextAlign.Center
                            )
                        }
                    }
                }
            })
        }

        val listState = rememberLazyListState()
        Box(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
        ) {
            LazyColumn(
                state = listState,
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 16.dp),
                // Extra bottom room so the last tiles never blend into the nav bar.
                contentPadding = PaddingValues(bottom = 20.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(6.dp)
            ) {
                item(key = "header_spacer") { Spacer(Modifier.height(4.dp)) }

                // Hero block: connect button, live speed and session tiles.
                if (!isSelectionMode) {
                    item(key = "hero_connect") {
                        val heroNode = nodes.firstOrNull { it.isSelected } ?: nodes.firstOrNull()
                        DashboardHero(
                            connectionState = connectionState,
                            style = dashboardStyle,
                            serverName = heroNode?.name,
                            serverCountryCode = heroNode?.countryCode,
                            serverProtocol = heroNode?.displayProtocol ?: heroNode?.protocol,
                            onToggleConnection = {
                                if (hapticsEnabled) haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                                onToggleConnection()
                            }
                        )
                    }
                }

                // Sort selector + ping-all row (above all groups)
                if (groups.isNotEmpty() && !isSelectionMode) {
                    item(key = "sort_and_ping_row") {
                        Row(
                            modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 2.dp),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(
                                    text = strings.sortBy,
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant
                                )
                                Spacer(Modifier.width(6.dp))
                                Box {
                                    Text(
                                        text = if (sortByPing) "Ping" else strings.sortByName,
                                        style = MaterialTheme.typography.bodySmall,
                                        fontWeight = FontWeight.Bold,
                                        color = MaterialTheme.colorScheme.primary,
                                        modifier = Modifier.clickable { showSortMenu = true }
                                    )
                                    LumenMenu(
                                        expanded = showSortMenu,
                                        onDismissRequest = { showSortMenu = false }
                                    ) {
                                        Column {
                                            DropdownMenuItem(
                                                text = { Text(strings.sortByName, color = MaterialTheme.colorScheme.onSurface) },
                                                trailingIcon = if (!sortByPing) {
                                                    { Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) }
                                                } else null,
                                                onClick = {
                                                    sortByPing = false
                                                    showSortMenu = false
                                                }
                                            )
                                            DropdownMenuItem(
                                                text = { Text("Ping", color = MaterialTheme.colorScheme.onSurface) },
                                                trailingIcon = if (sortByPing) {
                                                    { Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) }
                                                } else null,
                                                onClick = {
                                                    sortByPing = true
                                                    showSortMenu = false
                                                }
                                            )
                                        }
                                    }
                                }
                            }
                            // Icon-only check-all button, same as the Servers tab,
                            // with a Ping / Ping UDP choice.
                            Box {
                                IconButton(
                                    onClick = { showPingAllMenu = true },
                                    modifier = Modifier.size(32.dp)
                                ) {
                                    Icon(
                                        imageVector = Icons.Filled.NetworkCheck,
                                        contentDescription = strings.pingAll,
                                        tint = MaterialTheme.colorScheme.primary,
                                        modifier = Modifier.size(20.dp)
                                    )
                                }
                                LumenMenu(
                                    expanded = showPingAllMenu,
                                    onDismissRequest = { showPingAllMenu = false }
                                ) {
                                    DropdownMenuItem(
                                        text = { Text("Ping", color = MaterialTheme.colorScheme.onSurface) },
                                        trailingIcon = { Icon(Icons.Filled.NetworkCheck, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) },
                                        onClick = { showPingAllMenu = false; onPingAll() }
                                    )
                                    DropdownMenuItem(
                                        text = { Text("Ping UDP", color = MaterialTheme.colorScheme.onSurface) },
                                        trailingIcon = { Icon(Icons.Filled.NetworkCheck, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) },
                                        onClick = { showPingAllMenu = false; onUdpPingAll() }
                                    )
                                }
                            }
                        }
                    }
                }

                // Server / Subscription Groups (Virtualized Layout)
                if (groups.isEmpty()) {
                    item(key = "empty_servers_card") {
                        LumenCard {
                            Text(strings.noServers, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                } else {
                    visibleGroups.forEach { group ->
                        val isExpanded = group.id in expandedGroups

                        // Group Header Tile
                        item(key = "group_header_${group.id}") {
                            SubscriptionHeaderTile(
                                group = group,
                                isExpanded = isExpanded,
                                onToggleExpand = {
                                    expandedGroups = if (isExpanded) expandedGroups - group.id else expandedGroups + group.id
                                },
                                onRefreshSubscription = { onRefreshSubscription(group.id) },
                                onDeleteSubscription = { onDeleteSubscription(group.id) },
                                onPingGroup = { onPingGroup(if (group.isSubscription) group.id else null) },
                                onUdpPingGroup = { onUdpPingGroup(if (group.isSubscription) group.id else null) },
                                onShowProperties = { sub -> propertiesSub = sub },
                                onExportAll = { subId ->
                                    val text = onExportSubscriptionText(subId)
                                    if (text.isNotBlank()) onShareText(text)
                                }
                            )
                        }

                        // Traffic + premium summary belongs to the opened group:
                        // a collapsed row stays a single clean line.
                        if (group.isSubscription && group.subscription != null && isExpanded) {
                            item(key = "group_info_${group.id}") {
                                // With rows below the bar must stay square and cover the 6.dp list gap.
                                SubscriptionInfoBar(
                                    sub = group.subscription,
                                    roundedBottom = group.nodes.isEmpty(),
                                    extraBottom = if (group.nodes.isEmpty()) 0.dp else 6.dp
                                )
                            }
                        }

                        // Nodes stay virtualized: a subscription with hundreds of
                        // servers opens instantly because only visible rows compose.
                        // The rail background is painted per row, so the header,
                        // the traffic bar and the list still read as one card.
                        if (isExpanded) {
                            if (group.nodes.isEmpty()) {
                                item(key = "empty_group_${group.id}") {
                                    Box(
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .offset(y = (-6).dp)
                                            .clip(RoundedCornerShape(bottomStart = 18.dp, bottomEnd = 18.dp))
                                            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.38f))
                                            .padding(16.dp),
                                        contentAlignment = Alignment.Center
                                    ) {
                                        Text(
                                            text = strings.noServers,
                                            style = MaterialTheme.typography.bodyMedium,
                                            color = MaterialTheme.colorScheme.onSurfaceVariant
                                        )
                                    }
                                }
                            } else {
                                items(
                                    count = group.nodes.size,
                                    key = { index -> "node_${group.id}_${group.nodes[index].id}" }
                                ) { index ->
                                    val node = group.nodes[index]
                                    val isLast = index == group.nodes.lastIndex
                                    var appeared by remember(group.id) { mutableStateOf(false) }
                                    LaunchedEffect(group.id) { appeared = true }
                                    val appearAlpha by animateFloatAsState(
                                        targetValue = if (appeared) 1f else 0f,
                                        animationSpec = tween(220, easing = FastOutSlowInEasing),
                                        label = "node_appear"
                                    )
                                    Box(
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .offset(y = (-6).dp)
                                            .then(
                                                if (isLast) Modifier.clip(RoundedCornerShape(bottomStart = 18.dp, bottomEnd = 18.dp)) else Modifier
                                            )
                                            .background(
                                                MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.38f)
                                            )
                                            .padding(
                                                start = 10.dp,
                                                end = 10.dp,
                                                top = if (index == 0) 10.dp else 3.dp,
                                                bottom = if (isLast) 10.dp else 3.dp
                                            )
                                            .alpha(appearAlpha)
                                    ) {
                                        ServerTileRow(
                                            node = node,
                                            isSelectionMode = isSelectionMode,
                                            isNodeSelected = node.id in selectedNodeIds,
                                            isPinging = node.id in pingingNodeIds,
                                            modern = true,
                                            onClick = {
                                                if (isSelectionMode) {
                                                    val newSet = if (node.id in selectedNodeIds) selectedNodeIds - node.id else selectedNodeIds + node.id
                                                    selectedNodeIds = newSet
                                                    if (newSet.isEmpty()) isSelectionMode = false
                                                } else {
                                                    onSelectNode(node)
                                                }
                                            },
                                            onLongClick = {
                                                if (hapticsEnabled) haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                                                isSelectionMode = true
                                                selectedNodeIds = setOf(node.id)
                                            },
                                            onEditNode = { onEditNode(node) },
                                            onPingNode = { onPingNode(node) },
                                            onUdpPingNode = { onUdpPingNode(node) },
                                            onCopyLink = { onCopyNodeLink(node) },
                                            onExportQr = { onExportQrCode(node) },
                                            onDeleteNode = { onDeleteNode(node) }
                                        )
                                    }
                                }
                            }
                        }
                    }
                }

                // 2 Side-by-side action buttons: Equal size "+ Добавить" and separate "📋 Вставить"
                if (!isSelectionMode) {
                    item(key = "import_action_buttons") {
                        Spacer(Modifier.height(4.dp))
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(10.dp)
                        ) {
                            // Button 1: "+ Добавить"
                            Box(modifier = Modifier.weight(1f)) {
                                Surface(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .clip(RoundedCornerShape(14.dp))
                                        .border(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.5f), RoundedCornerShape(14.dp))
                                        .clickable { showImportMenu = true },
                                    color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.60f),
                                    shape = RoundedCornerShape(14.dp)
                                ) {
                                    Row(
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .padding(vertical = 10.dp, horizontal = 8.dp),
                                        horizontalArrangement = Arrangement.Center,
                                        verticalAlignment = Alignment.CenterVertically
                                    ) {
                                        Icon(
                                            imageVector = Icons.Filled.Add,
                                            contentDescription = null,
                                            tint = MaterialTheme.colorScheme.primary,
                                            modifier = Modifier.size(20.dp)
                                        )
                                        Spacer(Modifier.width(6.dp))
                                        Text(
                                            text = strings.add,
                                            style = MaterialTheme.typography.titleSmall,
                                            fontWeight = FontWeight.SemiBold,
                                            color = MaterialTheme.colorScheme.primary
                                        )
                                    }
                                }

                                LumenMenu(
                                    expanded = showImportMenu,
                                    onDismissRequest = { showImportMenu = false }
                                ) {
                                    Column {
                                        DropdownMenuItem(
                                            text = { Text(strings.importQrCode, color = MaterialTheme.colorScheme.onSurface) },
                                            trailingIcon = { Icon(Icons.Filled.Search, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp)) },
                                            onClick = {
                                                showImportMenu = false
                                                onImportQr()
                                            }
                                        )
                                        DropdownMenuItem(
                                            text = { Text(strings.importFromFile, color = MaterialTheme.colorScheme.onSurface) },
                                            trailingIcon = { Icon(Icons.Filled.Add, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp)) },
                                            onClick = {
                                                showImportMenu = false
                                                onImportFile()
                                            }
                                        )
                                        DropdownMenuItem(
                                            text = { Text(strings.importManually, color = MaterialTheme.colorScheme.onSurface) },
                                            trailingIcon = { Icon(Icons.Filled.Edit, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp)) },
                                            onClick = {
                                                showImportMenu = false
                                                onAddManualNode()
                                            }
                                        )
                                    }
                                }
                            }

                            // Button 2: "📋 Вставить"
                            Surface(
                                modifier = Modifier
                                    .weight(1f)
                                    .clip(RoundedCornerShape(14.dp))
                                    .border(1.dp, MaterialTheme.colorScheme.outline.copy(alpha = 0.35f), RoundedCornerShape(14.dp))
                                    .clickable(onClick = onImportClipboard),
                                color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.60f),
                                shape = RoundedCornerShape(14.dp)
                            ) {
                                Row(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .padding(vertical = 10.dp, horizontal = 8.dp),
                                    horizontalArrangement = Arrangement.Center,
                                    verticalAlignment = Alignment.CenterVertically
                                ) {
                                    Icon(
                                        imageVector = Icons.Filled.Edit,
                                        contentDescription = null,
                                        tint = MaterialTheme.colorScheme.onSurface,
                                        modifier = Modifier.size(18.dp)
                                    )
                                    Spacer(Modifier.width(6.dp))
                                    Text(
                                        text = strings.paste,
                                        style = MaterialTheme.typography.titleSmall,
                                        fontWeight = FontWeight.SemiBold,
                                        color = MaterialTheme.colorScheme.onSurface
                                    )
                                }
                            }
                        }
                        Spacer(Modifier.height(8.dp))
                    }
                }
            }
        }

        // Slide-to-connect moved to the Servers tab; the hero button handles it here.
    }

    // Subscription properties dialog
    propertiesSub?.let { sub ->
        SubscriptionPropertiesDialog(
            sub = sub,
            onDismiss = { propertiesSub = null },
            onCopyUrl = { onCopyText(sub.url) }
        )
    }
}

@Composable
private fun SelectionTopBar(
    selectedCount: Int,
    onCancel: () -> Unit,
    onExportSelected: () -> Unit,
    onPingSelected: () -> Unit
) {
    val strings = LocalStrings.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .statusBarsPadding()
            .padding(horizontal = 16.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = onCancel, modifier = Modifier.size(34.dp)) {
                Icon(
                    imageVector = Icons.Filled.Close,
                    contentDescription = "Cancel",
                    tint = MaterialTheme.colorScheme.onSurface
                )
            }
            Spacer(Modifier.width(8.dp))
            Text(
                text = "$selectedCount ${strings.selected}",
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.ExtraBold,
                color = MaterialTheme.colorScheme.onSurface
            )
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = onExportSelected) {
                Icon(
                    imageVector = Icons.Filled.Share,
                    contentDescription = strings.exportSelected,
                    tint = MaterialTheme.colorScheme.primary
                )
            }
            IconButton(onClick = onPingSelected) {
                Icon(
                    imageVector = Icons.Filled.PlayArrow,
                    contentDescription = strings.ping,
                    tint = MaterialTheme.colorScheme.primary
                )
            }
        }
    }
}

@Composable
fun SubscriptionPropertiesDialog(
    sub: SubscriptionUiModel,
    onDismiss: () -> Unit,
    onCopyUrl: () -> Unit
) {
    val strings = LocalStrings.current
    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = RoundedCornerShape(20.dp),
            color = MaterialTheme.colorScheme.surfaceContainerHigh,
            border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f))
        ) {
            Column(Modifier.padding(20.dp)) {
                Text(
                    strings.subscriptionProperties,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.onSurface
                )
                Spacer(Modifier.height(16.dp))

                // Name
                Text(
                    strings.sortByName,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                Spacer(Modifier.height(4.dp))
                Text(
                    sub.name,
                    style = MaterialTheme.typography.bodyLarge,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface
                )
                Spacer(Modifier.height(12.dp))

                // URL
                Text(
                    strings.url,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                Spacer(Modifier.height(4.dp))
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        sub.url,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.weight(1f),
                        maxLines = 3,
                        overflow = TextOverflow.Ellipsis
                    )
                    Spacer(Modifier.width(8.dp))
                    IconButton(onClick = onCopyUrl, modifier = Modifier.size(32.dp)) {
                        Icon(
                            imageVector = Icons.Filled.Edit,
                            contentDescription = "Copy",
                            tint = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.size(18.dp)
                        )
                    }
                }
                Spacer(Modifier.height(16.dp))
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    Surface(
                        modifier = Modifier
                            .clip(RoundedCornerShape(12.dp))
                            .clickable(onClick = onDismiss),
                        color = MaterialTheme.colorScheme.primary.copy(alpha = 0.15f),
                        shape = RoundedCornerShape(12.dp)
                    ) {
                        Text(
                            strings.done,
                            modifier = Modifier.padding(horizontal = 20.dp, vertical = 10.dp),
                            style = MaterialTheme.typography.titleSmall,
                            fontWeight = FontWeight.SemiBold,
                            color = MaterialTheme.colorScheme.primary
                        )
                    }
                }
            }
        }
    }
}

/** Shared group header: used by the dashboard and by the Servers tab. */
@Composable
fun SubscriptionHeaderTile(
    group: HomeServerGroup,
    isExpanded: Boolean,
    onToggleExpand: () -> Unit,
    onRefreshSubscription: () -> Unit,
    onDeleteSubscription: () -> Unit,
    onPingGroup: () -> Unit = {},
    onUdpPingGroup: () -> Unit = {},
    onShowProperties: (SubscriptionUiModel) -> Unit = {},
    onExportAll: (String?) -> Unit = {}
) {
    val strings = LocalStrings.current
    var showMenu by remember { mutableStateOf(false) }
    var showPingMenu by remember { mutableStateOf(false) }
    // Subscriptions always carry the traffic block underneath, so their title
    // tile keeps square bottom corners and the two read as one card.
    val tileShape = if (isExpanded || group.isSubscription) {
        RoundedCornerShape(topStart = 18.dp, topEnd = 18.dp)
    } else {
        RoundedCornerShape(18.dp)
    }
    val primaryColor = MaterialTheme.colorScheme.primary

    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .clip(tileShape)
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f), tileShape),
        color = MaterialTheme.colorScheme.surfaceVariant,
        shape = tileShape
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable(onClick = onToggleExpand)
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.weight(1f)
            ) {
                Icon(
                    imageVector = if (isExpanded) Icons.Filled.KeyboardArrowUp else Icons.Filled.KeyboardArrowDown,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.size(24.dp)
                )
                Spacer(Modifier.width(10.dp))
                Column {
                    Text(
                        text = if (group.isSubscription) "${group.title} (${group.nodes.size})" else group.title,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )
                    Text(
                        text = if (group.isSubscription) "${group.nodes.size} ${strings.serverCount} | ${strings.autoUpdateOneHour}" else "${group.nodes.size} ${strings.serverCount}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        fontSize = 11.sp
                    )
                }
            }

            // Header action buttons
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box {
                    IconButton(
                        onClick = { showPingMenu = true },
                        modifier = Modifier.size(32.dp)
                    ) {
                        Icon(
                            imageVector = Icons.Filled.NetworkCheck,
                            contentDescription = "Ping",
                            tint = primaryColor,
                            modifier = Modifier.size(20.dp)
                        )
                    }
                    LumenMenu(
                        expanded = showPingMenu,
                        onDismissRequest = { showPingMenu = false }
                    ) {
                        DropdownMenuItem(text = { Text("Ping") }, onClick = { onPingGroup(); showPingMenu = false })
                        DropdownMenuItem(text = { Text("UDP Ping") }, onClick = { onUdpPingGroup(); showPingMenu = false })
                    }
                }
                if (group.isSubscription) {
                    IconButton(
                        onClick = onRefreshSubscription,
                        modifier = Modifier.size(32.dp)
                    ) {
                        Icon(
                            imageVector = Icons.Filled.Refresh,
                            contentDescription = "Refresh",
                            tint = primaryColor,
                            modifier = Modifier.size(20.dp)
                        )
                    }
                }
                Box {
                    IconButton(
                        onClick = { showMenu = true },
                        modifier = Modifier.size(32.dp)
                    ) {
                        Icon(
                            imageVector = Icons.Filled.MoreVert,
                            contentDescription = "More",
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.size(20.dp)
                        )
                    }

                    LumenMenu(
                        expanded = showMenu,
                        onDismissRequest = { showMenu = false }
                    ) {
                        Column {
                            if (group.isSubscription) {
                                // Subscription menu: Properties, Export all, Delete
                                DropdownMenuItem(
                                    text = { Text(strings.subscriptionProperties, color = MaterialTheme.colorScheme.onSurface) },
                                    trailingIcon = { Icon(Icons.Filled.Info, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp)) },
                                    onClick = {
                                        showMenu = false
                                        group.subscription?.let { onShowProperties(it) }
                                    }
                                )
                                DropdownMenuItem(
                                    text = { Text(strings.exportAll, color = MaterialTheme.colorScheme.onSurface) },
                                    trailingIcon = { Icon(Icons.Filled.Share, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp)) },
                                    onClick = {
                                        showMenu = false
                                        onExportAll(group.id)
                                    }
                                )
                                DropdownMenuItem(
                                    text = { Text(strings.deleteSubscription, color = MaterialTheme.colorScheme.error) },
                                    trailingIcon = { Icon(Icons.Filled.Delete, contentDescription = null, tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(20.dp)) },
                                    onClick = {
                                        showMenu = false
                                        onDeleteSubscription()
                                    }
                                )
                            } else {
                                // Default group menu: Export all, Ping
                                DropdownMenuItem(
                                    text = { Text(strings.exportAll, color = MaterialTheme.colorScheme.onSurface) },
                                    trailingIcon = { Icon(Icons.Filled.Share, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp)) },
                                    onClick = {
                                        showMenu = false
                                        onExportAll(null)
                                    }
                                )
                                DropdownMenuItem(
                                    text = { Text(strings.ping, color = MaterialTheme.colorScheme.onSurface) },
                                    trailingIcon = { Icon(Icons.Filled.NetworkCheck, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp)) },
                                    onClick = {
                                        showMenu = false
                                        onPingGroup()
                                    }
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

/** Shared traffic/premium bar: used by the dashboard and by the Servers tab. */
@Composable
fun SubscriptionInfoBar(
    sub: SubscriptionUiModel,
    // Cancels the list spacing above so the bar sticks to the title tile.
    pullUp: androidx.compose.ui.unit.Dp = 6.dp,
    roundedBottom: Boolean = true,
    // Covers the list spacing below so the rows continue the same tile.
    extraBottom: androidx.compose.ui.unit.Dp = 0.dp
) {
    val strings = LocalStrings.current
    val primaryColor = MaterialTheme.colorScheme.primary
    val infoShape = if (roundedBottom) {
        RoundedCornerShape(bottomStart = 18.dp, bottomEnd = 18.dp)
    } else {
        RoundedCornerShape(0.dp)
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .offset(y = -pullUp)
            .clip(infoShape)
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.85f))
            .padding(start = 14.dp, end = 14.dp, top = 10.dp, bottom = 12.dp + extraBottom)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    imageVector = Icons.Filled.Info,
                    contentDescription = null,
                    tint = primaryColor,
                    modifier = Modifier.size(16.dp)
                )
                Spacer(Modifier.width(6.dp))
                Text(
                    text = sub.trafficSummary ?: "— / ∞",
                    style = MaterialTheme.typography.bodySmall,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.onSurface
                )
            }
            Text(
                text = sub.expiryDaysLeft?.let { "$it ${strings.daysRemaining}" } ?: strings.expiresUnlimited,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                fontSize = 11.sp
            )
        }
        Spacer(Modifier.height(6.dp))
        LinearProgressIndicator(
            // Real usage ratio from subscription-userinfo; unlimited plans stay empty.
            progress = { sub.trafficRatio ?: 0f },
            modifier = Modifier.fillMaxWidth().height(4.dp).clip(RoundedCornerShape(2.dp)),
            color = primaryColor,
            trackColor = primaryColor.copy(alpha = 0.2f)
        )
        Spacer(Modifier.height(6.dp))
        Text(
            text = buildString {
                append("${sub.nodeCount} ${strings.serverCount}")
                sub.updateIntervalHours?.let { append("  |  \u21bb ${it}h") }
            },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            fontSize = 11.sp
        )
        sub.announce?.takeIf { it.isNotBlank() }?.let { announce ->
            Spacer(Modifier.height(4.dp))
            Text(
                text = announce,
                style = MaterialTheme.typography.bodySmall,
                color = primaryColor,
                fontSize = 11.sp
            )
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun ServerTileRow(
    node: NodeUiModel,
    isSelectionMode: Boolean,
    isNodeSelected: Boolean,
    isPinging: Boolean = false,
    modern: Boolean = true,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    onEditNode: () -> Unit,
    onPingNode: () -> Unit,
    onUdpPingNode: () -> Unit = {},
    onCopyLink: () -> Unit,
    onExportQr: () -> Unit,
    onDeleteNode: () -> Unit
) {
    val strings = LocalStrings.current
    val selected = node.isSelected
    val primaryColor = MaterialTheme.colorScheme.primary
    // Tiles always carry the palette accent; AMOLED only makes the tint slightly stronger.
    val amoled = MaterialTheme.colorScheme.background.luminance() < 0.02f
    val rowBg = when {
        isSelectionMode && isNodeSelected -> primaryColor.copy(alpha = if (amoled) 0.24f else 0.18f)
        selected -> primaryColor.copy(alpha = if (amoled) 0.20f else 0.14f)
        else -> primaryColor.copy(alpha = if (amoled) 0.08f else 0.06f)
            .compositeOver(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f))
    }
    var showActionMenu by remember { mutableStateOf(false) }

    // modern = reworked look; false restores the original compact row.
    val tileShape = RoundedCornerShape(if (modern) 18.dp else 12.dp)

    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .clip(tileShape)
            .border(
                if (modern && selected) 2.dp else 1.dp,
                if (selected) primaryColor.copy(alpha = 0.7f) else MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.45f),
                tileShape
            )
            .combinedClickable(
                onClick = onClick,
                onLongClick = onLongClick
            ),
        color = rowBg,
        shape = tileShape
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = if (modern) 12.dp else 14.dp, vertical = if (modern) 13.dp else 10.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            if (modern) {
                // Accent rail marks the active server without an extra row.
                Box(
                    Modifier
                        .width(3.dp)
                        .height(32.dp)
                        .clip(RoundedCornerShape(2.dp))
                        .background(primaryColor.copy(alpha = if (selected) 1f else 0f))
                )
                Spacer(Modifier.width(10.dp))
            }
            // Checkbox in selection mode
            if (isSelectionMode) {
                Checkbox(
                    checked = isNodeSelected,
                    onCheckedChange = { onClick() },
                    colors = CheckboxDefaults.colors(
                        checkedColor = primaryColor,
                        uncheckedColor = MaterialTheme.colorScheme.onSurfaceVariant
                    ),
                    modifier = Modifier.size(24.dp)
                )
                Spacer(Modifier.width(8.dp))
            }

            // Larger flags on the dashboard; unknown codes fall back to the US placeholder.
            CountryFlagIcon(
                countryCode = node.countryCode,
                width = 34.dp,
                height = 23.dp
            )
            Spacer(Modifier.width(12.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = node.name,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium,
                    color = if (selected) primaryColor else MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
                if (modern) {
                    Spacer(Modifier.height(4.dp))
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        // Per-protocol badge colour, shared with the Servers tab.
                        val badgeColor = protocolColor(node.protocol)
                        Box(
                            Modifier
                                .clip(RoundedCornerShape(6.dp))
                                .background(badgeColor.copy(alpha = 0.14f))
                                .padding(horizontal = 6.dp, vertical = 2.dp)
                        ) {
                            Text(
                                text = node.displayProtocol.uppercase(),
                                style = MaterialTheme.typography.labelSmall,
                                color = badgeColor,
                                fontSize = 10.sp,
                                fontWeight = FontWeight.Bold
                            )
                        }
                        Spacer(Modifier.width(6.dp))
                        Text(
                            text = if (node.isAutoNode) strings.autoSelect else "${node.server}:${node.port}",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            fontSize = 11.sp,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis
                        )
                    }
                } else {
                    Text(
                        text = node.displayProtocol,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        fontSize = 11.sp
                    )
                }
            }
            if (isPinging) {
                // Spinner replaces the (already cleared) ping value while the probe runs.
                CircularProgressIndicator(
                    modifier = Modifier.size(14.dp),
                    strokeWidth = 2.dp,
                    color = primaryColor
                )
                Spacer(Modifier.width(8.dp))
            } else {
                node.pingMs?.let { ping ->
                    if (modern) {
                        Box(
                            Modifier
                                .clip(RoundedCornerShape(8.dp))
                                .background(pingColor(ping).copy(alpha = 0.15f))
                                .padding(horizontal = 8.dp, vertical = 4.dp)
                        ) {
                            Text(
                                text = if (ping >= 0) "$ping ms" else "—",
                                style = MaterialTheme.typography.bodySmall,
                                color = pingColor(ping),
                                fontSize = 12.sp,
                                fontWeight = FontWeight.Bold
                            )
                        }
                    } else {
                        Text(
                            text = if (ping >= 0) "$ping ms" else "—",
                            style = MaterialTheme.typography.bodySmall,
                            color = pingColor(ping),
                            fontSize = 12.sp,
                            fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal
                        )
                    }
                    Spacer(Modifier.width(8.dp))
                }
            }

            // Trailing overflow button: same three-dot menu as the Servers tab.
            if (!isSelectionMode) {
                Box {
                    IconButton(
                        onClick = { showActionMenu = true },
                        modifier = Modifier.size(28.dp)
                    ) {
                        Icon(
                            imageVector = Icons.Filled.MoreVert,
                            contentDescription = null,
                            tint = if (selected) primaryColor else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                            modifier = Modifier.size(20.dp)
                        )
                    }
                    LumenMenu(
                        expanded = showActionMenu,
                        onDismissRequest = { showActionMenu = false }
                    ) {
                        Column {
                            DropdownMenuItem(
                                text = { Text(strings.edit, color = MaterialTheme.colorScheme.onSurface) },
                                trailingIcon = { Icon(Icons.Filled.Edit, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) },
                                onClick = {
                                    showActionMenu = false
                                    onEditNode()
                                }
                            )
                            DropdownMenuItem(
                                text = { Text(strings.copyLink, color = MaterialTheme.colorScheme.onSurface) },
                                trailingIcon = { Icon(Icons.Filled.Share, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) },
                                onClick = {
                                    showActionMenu = false
                                    onCopyLink()
                                }
                            )
                            DropdownMenuItem(
                                text = { Text(strings.exportQrCode, color = MaterialTheme.colorScheme.onSurface) },
                                trailingIcon = { Icon(Icons.Filled.Search, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) },
                                onClick = {
                                    showActionMenu = false
                                    onExportQr()
                                }
                            )
                            DropdownMenuItem(
                                text = { Text(strings.ping, color = MaterialTheme.colorScheme.onSurface) },
                                trailingIcon = { Icon(Icons.Filled.NetworkCheck, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) },
                                onClick = {
                                    showActionMenu = false
                                    onPingNode()
                                }
                            )
                            DropdownMenuItem(
                                text = { Text("Ping UDP", color = MaterialTheme.colorScheme.onSurface) },
                                trailingIcon = { Icon(Icons.Filled.NetworkCheck, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) },
                                onClick = {
                                    showActionMenu = false
                                    onUdpPingNode()
                                }
                            )
                            DropdownMenuItem(
                                text = { Text(strings.delete, color = MaterialTheme.colorScheme.error) },
                                trailingIcon = { Icon(Icons.Filled.Delete, contentDescription = null, tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(18.dp)) },
                                onClick = {
                                    showActionMenu = false
                                    onDeleteNode()
                                }
                            )
                        }
                    }
                }
            }
        }
    }
}
/** Compact human readable speed, e.g. "637.8 KB/s". */
private fun formatHeroSpeed(bytesPerSecond: Long): String {
    val kb = bytesPerSecond / 1024.0
    return when {
        kb < 1.0 -> "0 KB/s"
        kb < 1024.0 -> String.format(java.util.Locale.US, "%.1f KB/s", kb)
        else -> String.format(java.util.Locale.US, "%.2f MB/s", kb / 1024.0)
    }
}

private fun formatHeroDuration(seconds: Long): String {
    val hours = seconds / 3600
    val minutes = (seconds % 3600) / 60
    val secs = seconds % 60
    return if (hours > 0) {
        String.format(java.util.Locale.US, "%02d:%02d:%02d", hours, minutes, secs)
    } else {
        String.format(java.util.Locale.US, "%02d:%02d", minutes, secs)
    }
}

/** Server tile caption: region only, without the transport type. */
private fun heroServerLabel(countryCode: String?, protocol: String?, fallbackName: String?): String {
    val region = countryCode
        ?.takeIf { it.length == 2 }
        ?.let { code ->
            java.util.Locale("", code.uppercase(java.util.Locale.US))
                .getDisplayCountry(java.util.Locale.getDefault())
                .takeIf { it.isNotBlank() }
        }
        ?: fallbackName?.trim()?.takeIf { it.isNotBlank() }
    return region?.takeIf { it.isNotBlank() } ?: "\u2014"
}

/**
 * Dashboard hero: round connect button, live throughput with a sparkline and
 * the server/session tiles. Replaces the old slide bar, which now lives on the
 * Servers tab.
 */
@Composable
private fun DashboardHero(
    connectionState: ConnectionState,
    style: DashboardStyle = DashboardStyle.DEFAULT,
    serverName: String?,
    serverCountryCode: String?,
    serverProtocol: String?,
    onToggleConnection: () -> Unit
) {
    val s = LocalStrings.current
    val accent = MaterialTheme.colorScheme.primary
    val connected = connectionState == ConnectionState.Connected
    val connecting = connectionState == ConnectionState.Connecting

    var downSpeed by remember { mutableStateOf(0L) }
    var upSpeed by remember { mutableStateOf(0L) }
    var history by remember { mutableStateOf(listOf<Float>()) }
    var sessionSeconds by remember { mutableStateOf(0L) }

    // The session start timestamp is persisted, so reopening the tab or even
    // the whole app keeps counting from the real connection time.
    val heroContext = androidx.compose.ui.platform.LocalContext.current
    val heroPrefs = remember {
        heroContext.getSharedPreferences("lumen_prefs", android.content.Context.MODE_PRIVATE)
    }

    // Poll the device counters once per second while the tunnel is up.
    LaunchedEffect(connected) {
        if (!connected) {
            downSpeed = 0L
            upSpeed = 0L
            sessionSeconds = 0L
            history = emptyList()
            heroPrefs.edit().remove("session_started_at").apply()
            return@LaunchedEffect
        }
        val stored = heroPrefs.getLong("session_started_at", 0L)
        val startedAt = if (stored > 0L) stored else System.currentTimeMillis().also {
            heroPrefs.edit().putLong("session_started_at", it).apply()
        }
        sessionSeconds = ((System.currentTimeMillis() - startedAt) / 1000L).coerceAtLeast(0L)
        var lastRx = android.net.TrafficStats.getTotalRxBytes()
        var lastTx = android.net.TrafficStats.getTotalTxBytes()
        while (true) {
            delay(1000)
            val rx = android.net.TrafficStats.getTotalRxBytes()
            val tx = android.net.TrafficStats.getTotalTxBytes()
            downSpeed = (rx - lastRx).coerceAtLeast(0L)
            upSpeed = (tx - lastTx).coerceAtLeast(0L)
            lastRx = rx
            lastTx = tx
            history = (history + downSpeed.toFloat()).takeLast(36)
            sessionSeconds = ((System.currentTimeMillis() - startedAt) / 1000L).coerceAtLeast(0L)
        }
    }

    val pulse by rememberInfiniteTransition(label = "hero_pulse").animateFloat(
        initialValue = 1f,
        targetValue = if (connected || connecting) 1.06f else 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(1600, easing = LinearEasing),
            repeatMode = RepeatMode.Reverse
        ),
        label = "hero_pulse_value"
    )

    // Alternative dashboard layouts reuse the same live data; only the connect
    // control and its placement differ.
    if (style != DashboardStyle.DEFAULT) {
        val statusText = when (connectionState) {
            ConnectionState.Connected -> s.protectedStatus
            ConnectionState.Connecting -> s.connectingStatus
            ConnectionState.Error -> s.connectionError
            else -> s.unprotectedStatus
        }
        Column(
            modifier = Modifier.fillMaxWidth().padding(top = 4.dp, bottom = 10.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            if (style == DashboardStyle.CENTERED) {
                HeroConnectButton(
                    state = connectionState,
                    onConnectClick = onToggleConnection,
                    buttonSize = 208.dp,
                    statusText = statusText
                )
                Spacer(Modifier.height(14.dp))
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(10.dp)
            ) {
                HeroStatTile(
                    label = s.serverLabel,
                    value = heroServerLabel(serverCountryCode, serverProtocol, serverName),
                    modifier = Modifier.weight(1f)
                )
                HeroStatTile(
                    label = s.sessionLabel,
                    value = formatHeroDuration(sessionSeconds),
                    modifier = Modifier.weight(1f)
                )
            }
            if (style == DashboardStyle.SLIDER) {
                Spacer(Modifier.height(14.dp))
                ConnectionSliderBar(
                    connectionState = connectionState,
                    onToggleConnection = onToggleConnection,
                    modifier = Modifier.fillMaxWidth()
                )
            }
            if (connectionState == ConnectionState.Error) {
                Spacer(Modifier.height(10.dp))
                Text(
                    text = s.connectionError,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.error
                )
            }
        }
        return
    }

    Column(modifier = Modifier.fillMaxWidth().padding(top = 4.dp, bottom = 10.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Box(
                modifier = Modifier.size(136.dp),
                contentAlignment = Alignment.Center
            ) {
                Box(
                    modifier = Modifier
                        .size(136.dp)
                        .scale(pulse)
                        .clip(CircleShape)
                        .background(accent.copy(alpha = if (connected) 0.16f else 0.08f))
                )

                // Premium start-up animation: two counter-rotating arcs while
                // the tunnel is coming up.
                if (connecting) {
                    val spinner = rememberInfiniteTransition(label = "hero_spinner")
                    val angleFast by spinner.animateFloat(
                        initialValue = 0f,
                        targetValue = 360f,
                        animationSpec = infiniteRepeatable(
                            animation = tween(1100, easing = LinearEasing)
                        ),
                        label = "hero_spinner_fast"
                    )
                    val angleSlow by spinner.animateFloat(
                        initialValue = 360f,
                        targetValue = 0f,
                        animationSpec = infiniteRepeatable(
                            animation = tween(2200, easing = LinearEasing)
                        ),
                        label = "hero_spinner_slow"
                    )
                    Canvas(modifier = Modifier.size(128.dp)) {
                        drawArc(
                            color = accent,
                            startAngle = angleFast,
                            sweepAngle = 96f,
                            useCenter = false,
                            style = Stroke(width = 7f, cap = StrokeCap.Round)
                        )
                        drawArc(
                            color = accent.copy(alpha = 0.4f),
                            startAngle = angleSlow,
                            sweepAngle = 42f,
                            useCenter = false,
                            style = Stroke(width = 4f, cap = StrokeCap.Round)
                        )
                    }
                }
                // Empty ring while idle; while connecting the accent "water"
                // pours in from the top-left and levels out when connected.
                val fillTarget = when {
                    connected -> 1f
                    connecting -> 0.62f
                    else -> 0f
                }
                val fillProgress by animateFloatAsState(
                    targetValue = fillTarget,
                    animationSpec = tween(
                        durationMillis = if (connected) 900 else 1500,
                        easing = FastOutSlowInEasing
                    ),
                    label = "hero_fill"
                )
                val waveShift by rememberInfiniteTransition(label = "hero_wave").animateFloat(
                    initialValue = 0f,
                    targetValue = 1f,
                    animationSpec = infiniteRepeatable(
                        animation = tween(2600, easing = LinearEasing)
                    ),
                    label = "hero_wave_shift"
                )
                Box(
                    modifier = Modifier
                        .size(112.dp)
                        .clip(CircleShape)
                        .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.55f))
                        .border(
                            2.dp,
                            accent.copy(alpha = if (connected) 0f else 0.55f),
                            CircleShape
                        )
                        .clickable(onClick = onToggleConnection),
                    contentAlignment = Alignment.Center
                ) {
                    if (fillProgress > 0.001f) {
                        Canvas(modifier = Modifier.fillMaxSize()) {
                            val w = size.width
                            val h = size.height
                            val level = h * (1f - fillProgress)
                            // Slanted surface: the left edge fills first.
                            val tilt = h * 0.16f * (1f - fillProgress)
                            val amp = h * 0.035f * (1f - fillProgress).coerceAtLeast(0.12f)
                            val twoPi = 6.2831855f
                            val phase = waveShift * twoPi
                            val path = Path()
                            path.moveTo(0f, level - tilt)
                            var x = 0f
                            while (x <= w) {
                                val t = x / w
                                val y = level - tilt * (1f - 2f * t) +
                                    amp * kotlin.math.sin(phase + t * twoPi * 1.4f)
                                path.lineTo(x, y)
                                x += 3f
                            }
                            path.lineTo(w, h)
                            path.lineTo(0f, h)
                            path.close()
                            drawPath(
                                path = path,
                                color = accent.copy(alpha = if (connected) 1f else 0.85f)
                            )
                        }
                    }
                    Column {
                        AnimatedVisibility(
                            visible = connected,
                            enter = fadeIn(animationSpec = tween(520, easing = FastOutSlowInEasing)) +
                                scaleIn(
                                    animationSpec = tween(520, easing = FastOutSlowInEasing),
                                    initialScale = 0.7f
                                ),
                            exit = fadeOut(animationSpec = tween(180))
                        ) {
                            Icon(
                                imageVector = Icons.Filled.Check,
                                contentDescription = null,
                                tint = MaterialTheme.colorScheme.onPrimary,
                                modifier = Modifier.size(48.dp)
                            )
                        }
                    }
                }
            }

            Spacer(Modifier.width(14.dp))

            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = s.speedLabel.uppercase(java.util.Locale.getDefault()),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    fontWeight = FontWeight.SemiBold
                )
                Spacer(Modifier.height(2.dp))
                Text(
                    text = formatHeroSpeed(downSpeed),
                    style = MaterialTheme.typography.headlineSmall,
                    fontWeight = FontWeight.ExtraBold,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
                Spacer(Modifier.height(6.dp))
                Canvas(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(34.dp)
                ) {
                    val points = history
                    if (points.size >= 2) {
                        val maxValue = (points.maxOrNull() ?: 1f).coerceAtLeast(1f)
                        val stepX = size.width / (points.size - 1)
                        val path = Path()
                        points.forEachIndexed { index, value ->
                            val x = index * stepX
                            val y = size.height - (value / maxValue) * size.height * 0.9f
                            if (index == 0) path.moveTo(x, y) else path.lineTo(x, y)
                        }
                        drawPath(
                            path = path,
                            color = accent,
                            style = Stroke(width = 3f, cap = StrokeCap.Round)
                        )
                    } else {
                        drawLine(
                            color = accent.copy(alpha = 0.25f),
                            start = Offset(0f, size.height - 2f),
                            end = Offset(size.width, size.height - 2f),
                            strokeWidth = 3f,
                            cap = StrokeCap.Round
                        )
                    }
                }
                Spacer(Modifier.height(4.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        text = "\u2193 " + formatHeroSpeed(downSpeed),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    Spacer(Modifier.width(10.dp))
                    Text(
                        text = "\u2191 " + formatHeroSpeed(upSpeed),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }
        }

        // Only a hard error is worth a caption; "Connecting" and the
        // "protecting your data" line are noise, the button already says it.
        if (connectionState == ConnectionState.Error) {
            Spacer(Modifier.height(10.dp))
            Text(
                text = s.connectionError,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.fillMaxWidth(),
                textAlign = androidx.compose.ui.text.style.TextAlign.Center
            )
        }

        Spacer(Modifier.height(10.dp))

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(10.dp)
        ) {
            HeroStatTile(
                label = s.serverLabel,
                value = heroServerLabel(serverCountryCode, serverProtocol, serverName),
                modifier = Modifier.weight(1f)
            )
            HeroStatTile(
                label = s.sessionLabel,
                value = formatHeroDuration(sessionSeconds),
                modifier = Modifier.weight(1f)
            )
        }
    }
}

@Composable
private fun HeroStatTile(
    label: String,
    value: String,
    modifier: Modifier = Modifier
) {
    val shape = RoundedCornerShape(18.dp)
    Column(
        modifier = modifier
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.7f))
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f), shape)
            .padding(horizontal = 14.dp, vertical = 12.dp)
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
        Spacer(Modifier.height(3.dp))
        Text(
            text = value,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
    }
}
