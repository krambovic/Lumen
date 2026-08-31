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
import androidx.compose.material.icons.filled.Folder
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
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import kotlinx.coroutines.launch
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.material3.Surface
import com.lumen.ui.components.ConnectionSliderBar
import com.lumen.ui.components.ConnectionState
import com.lumen.ui.components.HeroConnectButton
import com.lumen.ui.components.CountryFlagIcon
import com.lumen.ui.components.SubscriptionDetailsDialog
import com.lumen.ui.components.SubscriptionProviderCard
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
    serverGroups: List<ServerGroupUiModel> = emptyList(),
    speedStatsEnabled: Boolean = true,
    downloadSpeed: Long = 0L,
    uploadSpeed: Long = 0L,
    pingingNodeIds: Set<String> = emptySet(),
    dashboardStyle: DashboardStyle = DashboardStyle.DEFAULT,
    // "Check" result for the server that is connected right now, plus its busy flag.
    connectedPing: String? = null,
    isCheckingPing: Boolean = false,
    onCheckPing: () -> Unit = {},
    onToggleConnection: () -> Unit,
    onSelectNode: (NodeUiModel) -> Unit,
    onImportClipboard: () -> Unit,
    onImportFile: () -> Unit,
    onImportQr: () -> Unit,
    onAddManualNode: () -> Unit,
    onRefreshSubscription: (String) -> Unit = {},
    onDeleteSubscription: (String) -> Unit = {},
    onPingGroup: (String?) -> Unit = {},
    onEditNode: (NodeUiModel) -> Unit = {},
    onPingNode: (NodeUiModel) -> Unit = {},
    onCopyNodeLink: (NodeUiModel) -> Unit = {},
    onExportQrCode: (NodeUiModel) -> Unit = {},
    onDeleteNode: (NodeUiModel) -> Unit = {},
    onPingNodes: (List<NodeUiModel>) -> Unit = {},
    onExportNodesText: (Set<String>) -> String = { "" },
    onExportSubscriptionText: (String?) -> String = { "" },
    onCopyText: (String) -> Unit = {},
    onShareText: (String) -> Unit = {},
    // Provider links from the subscription card; no-op on hosts that cannot open one.
    onOpenUrl: (String) -> Unit = {},
    modifier: Modifier = Modifier
) {
    val strings = LocalStrings.current
    var showImportMenu by remember { mutableStateOf(false) }
    var showSortMenu by remember { mutableStateOf(false) }
    // Same persisted choice as the Servers tab: picking a sort here moves that tab too.
    var sortKey by rememberUiPreference(SERVERS_SORT_PREF, ServerSort.DEFAULT.name)
    val sort = remember(sortKey) { serverSortOf(sortKey) }

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
    // Shared tick so long presses vibrate through the same path as the rest of the app.
    val tick = rememberHapticTick()
    // Ask for the camera at the point of use so the scanner never falls back to the
    // library's own "the camera encountered a problem" dialog.
    val scanQr = rememberQrScanRequest(onImportQr)
    // Subscription properties dialog state
    var propertiesSub by remember { mutableStateOf<SubscriptionUiModel?>(null) }

    // One grouping pass: ping refreshes rebuild this list on every update, so it must
    // not scan the node list once per subscription. The comparator is the shared one,
    // applied inside each group so every sort option works on the grouped layout.
    val groups = remember(nodes, subscriptions, serverGroups, strings.manual, sort) {
        val comparator = serverSortComparator(sort)
        // A custom group takes a server out of its subscription bucket, so bucket on the
        // custom group first and fall back to the subscription. Same rule as [nodeInGroup].
        val byBucket = nodes.groupBy { it.groupId ?: it.subscriptionId }
        buildList {
            // Keep Default visible even while it is empty so the user can select it and
            // immediately add/paste the first manual node.
            add(HomeServerGroup(GROUP_MANUAL, strings.groupDefault, byBucket[null].orEmpty().sortedWith(comparator)))
            serverGroups.forEach { custom ->
                add(
                    HomeServerGroup(
                        id = custom.id,
                        title = custom.name,
                        nodes = byBucket[custom.id].orEmpty().sortedWith(comparator),
                        isCustom = true
                    )
                )
            }
            subscriptions.forEach { subscription ->
                val subscriptionNodes = byBucket[subscription.id].orEmpty()
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
    val serversGroupPref by rememberUiPreference("servers_last_group", GROUP_ALL)
    val activeGroupId = remember(nodes) {
        nodes.firstOrNull { it.isSelected }?.let { it.groupId ?: it.subscriptionId ?: GROUP_MANUAL }
    }
    val visibleGroups = remember(groups, serversGroupPref, activeGroupId) {
        if (serversGroupPref.isBlank() || serversGroupPref == GROUP_ALL) {
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
            LumenScreenHeader(
                title = "Lumen",
                subtitle = "v${LumenVersion.appVersion}",
                stackSubtitleOnCompact = false,
                modifier = Modifier.padding(horizontal = 16.dp),
                actions = {
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
                // The SLIDER layout owns a fixed bottom control; reserve its full
                // height so the last server never scrolls underneath it.
                contentPadding = PaddingValues(
                    bottom = if (dashboardStyle == DashboardStyle.SLIDER && !isSelectionMode) {
                        142.dp
                    } else {
                        20.dp
                    }
                ),
                horizontalAlignment = Alignment.CenterHorizontally,
                // Group headers, subscription information and server rows own their
                // spacing so each group can render as one uninterrupted panel.
                verticalArrangement = Arrangement.spacedBy(0.dp)
            ) {
                item(key = "header_spacer") { Spacer(Modifier.height(4.dp)) }

                // DEFAULT and CENTERED keep their hero in the scrollable header.
                // SLIDER is fixed above the bottom navigation instead.
                if (!isSelectionMode && dashboardStyle != DashboardStyle.SLIDER) {
                    item(key = "hero_connect") {
                        Column {
                            val heroNode = nodes.firstOrNull { it.isSelected } ?: nodes.firstOrNull()
                            DashboardHero(
                                connectionState = connectionState,
                                style = dashboardStyle,
                                speedStatsEnabled = speedStatsEnabled,
                                downSpeed = downloadSpeed,
                                upSpeed = uploadSpeed,
                                serverName = heroNode?.name,
                                serverCountryCode = heroNode?.countryCode,
                                serverProtocol = heroNode?.displayProtocol ?: heroNode?.protocol,
                                connectedPing = connectedPing,
                                isCheckingPing = isCheckingPing,
                                onCheckPing = onCheckPing,
                                onToggleConnection = {
                                    tick(HapticFeedbackType.LongPress)
                                    onToggleConnection()
                                }
                            )
                            Spacer(Modifier.height(6.dp))
                        }
                    }
                }

                // Sorting stays above the groups; ping is available only in
                // each group header so the action is not duplicated.
                if (groups.isNotEmpty() && !isSelectionMode) {
                    item(key = "sort_and_ping_row") {
                        Row(
                            modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 6.dp),
                            horizontalArrangement = Arrangement.Start,
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
                                        text = serverSortLabel(sort, strings),
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
                                            serverSortOptions(strings).forEach { (value, label) ->
                                                DropdownMenuItem(
                                                    text = { Text(label, color = MaterialTheme.colorScheme.onSurface) },
                                                    trailingIcon = if (sort == value) {
                                                        { Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) }
                                                    } else null,
                                                    onClick = {
                                                        sortKey = value.name
                                                        showSortMenu = false
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

                // Server / Subscription Groups (Virtualized Layout)
                if (groups.isEmpty()) {
                    item(key = "empty_servers_card") {
                        LumenCard {
                            Text(strings.noServers, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                } else {
                    visibleGroups.forEachIndexed { groupIndex, group ->
                        val isExpanded = group.id in expandedGroups

                        if (groupIndex > 0) {
                            item(key = "group_gap_${group.id}") { Spacer(Modifier.height(8.dp)) }
                        }

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
                                // A custom group is not a subscription bucket, so it pings and
                                // exports by the nodes it actually holds.
                                onPingGroup = {
                                    if (group.isCustom) {
                                        onPingNodes(group.nodes)
                                    } else {
                                        onPingGroup(if (group.isSubscription) group.id else null)
                                    }
                                },
                                onShowProperties = { sub -> propertiesSub = sub },
                                onExportAll = { subId ->
                                    val text = if (group.isCustom) {
                                        onExportNodesText(group.nodes.mapTo(mutableSetOf()) { it.id })
                                    } else {
                                        onExportSubscriptionText(subId)
                                    }
                                    if (text.isNotBlank()) onShareText(text)
                                }
                            )
                        }

                        // Traffic + premium summary belongs to the opened group:
                        // a collapsed row stays a single clean line.
                        if (group.isSubscription && group.subscription != null && isExpanded) {
                            val subscription = group.subscription
                            item(key = "group_info_${group.id}") {
                                // Same shape and zero pull-up as the Servers tab.
                                SubscriptionInfoBar(
                                    sub = subscription,
                                    pullUp = 0.dp,
                                    roundedBottom = false
                                )
                            }
                            item(key = "group_provider_${group.id}") {
                                // Renders nothing when the provider did not send metadata.
                                SubscriptionProviderCard(
                                    sub = subscription,
                                    onOpenUrl = onOpenUrl
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
                                    val railShape = if (isLast) {
                                        RoundedCornerShape(bottomStart = 18.dp, bottomEnd = 18.dp)
                                    } else {
                                        RoundedCornerShape(0.dp)
                                    }
                                    Box(
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .clip(railShape)
                                            .background(
                                                MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.38f)
                                            )
                                            .padding(
                                                start = 8.dp,
                                                end = 8.dp,
                                                top = 4.dp,
                                                bottom = if (isLast) 10.dp else 4.dp
                                            )
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
                                                tick(HapticFeedbackType.LongPress)
                                                isSelectionMode = true
                                                selectedNodeIds = setOf(node.id)
                                            },
                                            onEditNode = { onEditNode(node) },
                                            onPingNode = { onPingNode(node) },
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
                // Add / paste fill the manual bucket, so they only make sense for
                // "All servers" and the Default group; a subscription is filled by
                // its link. The last clause covers a stale group id: when the pref
                // points at a deleted subscription the dashboard falls back to the
                // Default group and the buttons must come back with it. The group
                // itself is always reselectable from the Servers tab.
                val showImportActions = serversGroupPref.isBlank() ||
                    serversGroupPref == GROUP_ALL ||
                    serversGroupPref == GROUP_MANUAL ||
                    visibleGroups.any { !it.isSubscription && !it.isCustom }
                if (!isSelectionMode && showImportActions) {
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
                                                tick(HapticFeedbackType.LongPress)
                                                scanQr()
                                            }
                                        )
                                        DropdownMenuItem(
                                            text = { Text(strings.importFromFile, color = MaterialTheme.colorScheme.onSurface) },
                                            trailingIcon = { Icon(Icons.Filled.Add, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp)) },
                                            onClick = {
                                                showImportMenu = false
                                                tick(HapticFeedbackType.LongPress)
                                                onImportFile()
                                            }
                                        )
                                        DropdownMenuItem(
                                            text = { Text(strings.importManually, color = MaterialTheme.colorScheme.onSurface) },
                                            trailingIcon = { Icon(Icons.Filled.Edit, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp)) },
                                            onClick = {
                                                showImportMenu = false
                                                tick(HapticFeedbackType.LongPress)
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
                                    .clickable {
                                        tick(HapticFeedbackType.LongPress)
                                        onImportClipboard()
                                    },
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

            if (!isSelectionMode && dashboardStyle == DashboardStyle.SLIDER) {
                val heroNode = nodes.firstOrNull { it.isSelected } ?: nodes.firstOrNull()
                Box(
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .fillMaxWidth()
                        .background(MaterialTheme.colorScheme.background)
                        .padding(start = 16.dp, end = 16.dp, top = 3.dp, bottom = 2.dp)
                ) {
                    DashboardHero(
                        connectionState = connectionState,
                        style = DashboardStyle.SLIDER,
                        speedStatsEnabled = speedStatsEnabled,
                        downSpeed = downloadSpeed,
                        upSpeed = uploadSpeed,
                        serverName = heroNode?.name,
                        serverCountryCode = heroNode?.countryCode,
                        serverProtocol = heroNode?.displayProtocol ?: heroNode?.protocol,
                        connectedPing = connectedPing,
                        isCheckingPing = isCheckingPing,
                        onCheckPing = onCheckPing,
                        onToggleConnection = {
                            tick(HapticFeedbackType.LongPress)
                            onToggleConnection()
                        }
                    )
                }
            }
        }

    }

    // "Subscription properties" from the header menu. Same dialog as the Servers tab, so
    // the URL row is left out when the provider asked for it with hide-url.
    propertiesSub?.let { sub ->
        SubscriptionDetailsDialog(
            sub = sub,
            onDismiss = { propertiesSub = null },
            onCopyUrl = onCopyText,
            onOpenUrl = onOpenUrl
        )
    }
}

@Composable
internal fun SelectionTopBar(
    selectedCount: Int,
    onCancel: () -> Unit,
    onExportSelected: () -> Unit,
    onPingSelected: () -> Unit,
    // Null on screens that do not offer custom groups; the action is then hidden.
    onMoveSelected: (() -> Unit)? = null
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
            if (onMoveSelected != null) {
                IconButton(onClick = onMoveSelected) {
                    Icon(
                        imageVector = Icons.Filled.Folder,
                        contentDescription = strings.moveToGroup,
                        tint = MaterialTheme.colorScheme.primary
                    )
                }
            }
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

/** Shared group header: used by the dashboard and by the Servers tab. */
@Composable
fun SubscriptionHeaderTile(
    group: HomeServerGroup,
    isExpanded: Boolean,
    onToggleExpand: () -> Unit,
    onRefreshSubscription: () -> Unit,
    onDeleteSubscription: () -> Unit,
    onPingGroup: () -> Unit = {},
    onShowProperties: (SubscriptionUiModel) -> Unit = {},
    onEditSubscription: ((SubscriptionUiModel) -> Unit)? = null,
    onExportAll: (String?) -> Unit = {},
    // Custom groups only, and only where the screen can host the dialogs. Null hides
    // the entry, which is what the dashboard wants: groups are managed on Servers.
    onRenameGroup: (() -> Unit)? = null,
    onDeleteGroup: (() -> Unit)? = null
) {
    val strings = LocalStrings.current
    var showMenu by remember { mutableStateOf(false) }
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
                        text = if (group.isSubscription) "${strings.serverCountLabel(group.nodes.size)} | ${strings.autoUpdateOneHour}" else strings.serverCountLabel(group.nodes.size),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        fontSize = 11.sp
                    )
                }
            }

            // Header action buttons
            Row(verticalAlignment = Alignment.CenterVertically) {
                IconButton(
                    onClick = onPingGroup,
                    modifier = Modifier.size(32.dp)
                ) {
                    Icon(
                        imageVector = Icons.Filled.NetworkCheck,
                        contentDescription = "Ping",
                        tint = primaryColor,
                        modifier = Modifier.size(20.dp)
                    )
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
                                // Subscription menu: Edit, Properties, Export all, Delete.
                                if (onEditSubscription != null) {
                                    DropdownMenuItem(
                                        text = { Text(strings.edit, color = MaterialTheme.colorScheme.onSurface) },
                                        trailingIcon = { Icon(Icons.Filled.Edit, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp)) },
                                        onClick = {
                                            showMenu = false
                                            group.subscription?.let(onEditSubscription)
                                        }
                                    )
                                }
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
                                // Default / custom group menu: Export all, Ping, and for a
                                // custom group the rename and delete actions on top.
                                DropdownMenuItem(
                                    text = { Text(strings.exportAll, color = MaterialTheme.colorScheme.onSurface) },
                                    trailingIcon = { Icon(Icons.Filled.Share, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp)) },
                                    onClick = {
                                        showMenu = false
                                        onExportAll(if (group.isCustom) group.id else null)
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
                                if (group.isCustom && onRenameGroup != null) {
                                    DropdownMenuItem(
                                        text = { Text(strings.renameGroup, color = MaterialTheme.colorScheme.onSurface) },
                                        trailingIcon = { Icon(Icons.Filled.Edit, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp)) },
                                        onClick = {
                                            showMenu = false
                                            onRenameGroup()
                                        }
                                    )
                                }
                                if (group.isCustom && onDeleteGroup != null) {
                                    DropdownMenuItem(
                                        text = { Text(strings.deleteGroup, color = MaterialTheme.colorScheme.error) },
                                        trailingIcon = { Icon(Icons.Filled.Delete, contentDescription = null, tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(20.dp)) },
                                        onClick = {
                                            showMenu = false
                                            onDeleteGroup()
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
                append(strings.serverCountLabel(sub.nodeCount))
                sub.updateIntervalHours?.let { append("  |  \u21bb ${it}h") }
            },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            fontSize = 11.sp
        )
        // The announcement is not drawn here: SubscriptionProviderCard lays it out
        // below the bar, where it can wrap over several lines next to the provider
        // buttons instead of being squeezed into one 11sp line.
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun ServerTileRow(
    node: NodeUiModel,
    isSelectionMode: Boolean,
    isNodeSelected: Boolean,
    isPinging: Boolean = false,
    supportingText: String? = null,
    modern: Boolean = true,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    onEditNode: () -> Unit,
    onPingNode: () -> Unit,
    onCopyLink: () -> Unit,
    onExportQr: () -> Unit,
    onDeleteNode: () -> Unit,
    // Null on screens that do not offer custom groups; the entry is then hidden.
    onMoveToGroup: (() -> Unit)? = null
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

            // Larger flags on the dashboard; unresolved countries use a neutral tile.
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
                            text = if (node.isAutoNode && node.displayProtocol.equals("AUTO", true)) {
                                strings.autoNodeDescriptionLabel
                            } else if (node.isAutoNode && node.displayProtocol.endsWith("/WARP", true)) {
                                "WARP"
                            } else {
                                "${node.server}:${node.port}"
                            },
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
                supportingText?.takeIf { it.isNotBlank() }?.let { detail ->
                    Spacer(Modifier.height(3.dp))
                    Text(
                        text = detail,
                        style = MaterialTheme.typography.labelSmall,
                        color = primaryColor,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
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
                            if (onMoveToGroup != null) {
                                DropdownMenuItem(
                                    text = { Text(strings.moveToGroup, color = MaterialTheme.colorScheme.onSurface) },
                                    trailingIcon = { Icon(Icons.Filled.Folder, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) },
                                    onClick = {
                                        showActionMenu = false
                                        onMoveToGroup()
                                    }
                                )
                            }
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
 * Dashboard hero: round/centered controls or a bottom slider, live throughput
 * and the shared server/session tiles.
 */
/**
 * "Check" action shared by every dashboard style. Instead of a loose button under
 * the hero it is a third stat tile next to "Server" and "Session": tapping it
 * measures the currently connected server with the ping method chosen in settings
 * and replaces its own value with the result (also delivered as a toast).
 */
@Composable
private fun HeroPingTile(
    connectedPing: String?,
    isChecking: Boolean,
    onCheckPing: () -> Unit,
    modifier: Modifier = Modifier,
    compact: Boolean = false
) {
    val s = LocalStrings.current
    val shape = RoundedCornerShape(if (compact) 13.dp else 18.dp)
    Column(
        modifier = modifier
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f), shape)
            .clickable(enabled = !isChecking, onClick = onCheckPing)
            .padding(
                horizontal = if (compact) 10.dp else 14.dp,
                vertical = if (compact) 5.dp else 12.dp
            )
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = s.checkPing,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                fontSize = if (compact) 9.sp else 11.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Spacer(Modifier.width(4.dp))
            Icon(
                imageVector = Icons.Filled.NetworkCheck,
                contentDescription = s.checkPing,
                tint = MaterialTheme.colorScheme.primary.copy(alpha = 0.75f),
                modifier = Modifier.size(if (compact) 10.dp else 12.dp)
            )
        }
        Spacer(Modifier.height(if (compact) 1.dp else 3.dp))
        if (isChecking) {
            CircularProgressIndicator(
                modifier = Modifier.size(if (compact) 12.dp else 16.dp),
                strokeWidth = 2.dp,
                color = MaterialTheme.colorScheme.primary
            )
        } else {
            Text(
                text = if (connectedPing.isNullOrBlank()) "—" else connectedPing,
                style = if (compact) MaterialTheme.typography.bodySmall else MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
                color = if (connectedPing.isNullOrBlank()) {
                    MaterialTheme.colorScheme.onSurfaceVariant
                } else {
                    MaterialTheme.colorScheme.primary
                },
                fontSize = if (compact) 12.sp else 16.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        }
    }
}

@Composable
private fun DashboardHero(
    connectionState: ConnectionState,
    style: DashboardStyle = DashboardStyle.DEFAULT,
    speedStatsEnabled: Boolean,
    downSpeed: Long,
    upSpeed: Long,
    serverName: String?,
    serverCountryCode: String?,
    serverProtocol: String?,
    connectedPing: String? = null,
    isCheckingPing: Boolean = false,
    onCheckPing: () -> Unit = {},
    onToggleConnection: () -> Unit
) {
    val s = LocalStrings.current
    val accent = MaterialTheme.colorScheme.primary
    val connected = connectionState == ConnectionState.Connected
    val connecting = connectionState == ConnectionState.Connecting

    var history by remember { mutableStateOf(listOf<Float>()) }
    var sessionSeconds by remember { mutableStateOf(0L) }

    // The session start timestamp is persisted, so reopening the tab or even
    // the whole app keeps counting from the real connection time.
    val heroContext = androidx.compose.ui.platform.LocalContext.current
    val heroPrefs = remember {
        heroContext.getSharedPreferences("lumen_prefs", android.content.Context.MODE_PRIVATE)
    }

    // Session time is connection state, not speed telemetry. Keep counting even
    // when the user turns traffic statistics off.
    LaunchedEffect(connected) {
        if (!connected) {
            sessionSeconds = 0L
            return@LaunchedEffect
        }
        while (true) {
            val startedAt = heroPrefs.getLong("session_started_at", 0L)
            sessionSeconds = if (startedAt > 0L) {
                ((System.currentTimeMillis() - startedAt) / 1000L).coerceAtLeast(0L)
            } else {
                0L
            }
            delay(1_000)
        }
    }

    // Sampled on a timer like the session counter: keying on the speed value would
    // only record transitions, so an idle or perfectly steady tunnel froze the curve.
    val currentDownSpeed by rememberUpdatedState(downSpeed)
    LaunchedEffect(connected, speedStatsEnabled) {
        if (!connected || !speedStatsEnabled) {
            history = emptyList()
            return@LaunchedEffect
        }
        while (true) {
            history = (history + currentDownSpeed.toFloat()).takeLast(36)
            delay(1_000)
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
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(
                    top = if (style == DashboardStyle.SLIDER) 2.dp else 6.dp,
                    bottom = if (style == DashboardStyle.SLIDER) 2.dp else 10.dp
                ),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            if (style == DashboardStyle.CENTERED) {
                // The button draws a 208.dp circle inside a 1.3x glow box, so it
                // already carries 31.dp of transparent halo on every side. With the
                // status caption gone, 6.dp above and below leaves the circle
                // optically centred between the header and the stat tiles.
                HeroConnectButton(
                    state = connectionState,
                    onConnectClick = onToggleConnection,
                    buttonSize = 208.dp
                )
                Spacer(Modifier.height(6.dp))
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(if (style == DashboardStyle.SLIDER) 6.dp else 10.dp)
            ) {
                HeroStatTile(
                    label = s.serverLabel,
                    value = heroServerLabel(serverCountryCode, serverProtocol, serverName),
                    modifier = Modifier.weight(1f),
                    compact = style == DashboardStyle.SLIDER
                )
                HeroStatTile(
                    label = s.sessionLabel,
                    value = formatHeroDuration(sessionSeconds),
                    modifier = Modifier.weight(1f),
                    compact = style == DashboardStyle.SLIDER
                )
                // Ping sits in the same row as the other stats, so the hero keeps a
                // single tidy strip instead of a stray button underneath it.
                HeroPingTile(
                    connectedPing = connectedPing,
                    isChecking = isCheckingPing,
                    onCheckPing = onCheckPing,
                    modifier = Modifier.weight(0.72f),
                    compact = style == DashboardStyle.SLIDER
                )
            }
            // Slide-to-connect stays at the bottom of the SLIDER dashboard
            if (style == DashboardStyle.SLIDER) {
                Spacer(Modifier.height(6.dp))
                ConnectionSliderBar(
                    connectionState = connectionState,
                    onToggleConnection = onToggleConnection,
                    modifier = Modifier.fillMaxWidth()
                )
            }
        }
        return
    }

    Column(modifier = Modifier.fillMaxWidth().padding(top = 4.dp, bottom = 10.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = if (speedStatsEnabled) {
                Arrangement.Start
            } else {
                Arrangement.Center
            }
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
                        AnimatedVisibility(
                            visible = connectionState == ConnectionState.Error,
                            enter = fadeIn(animationSpec = tween(240)) +
                                scaleIn(animationSpec = tween(240), initialScale = 0.72f),
                            exit = fadeOut(animationSpec = tween(160))
                        ) {
                            Icon(
                                imageVector = Icons.Filled.Close,
                                contentDescription = s.connectionError,
                                tint = MaterialTheme.colorScheme.error,
                                modifier = Modifier.size(48.dp)
                            )
                        }
                    }
                }
            }

            if (speedStatsEnabled) {
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
            // Ping joins the same strip as the other stats instead of hanging
            // underneath the hero as a loose button.
            HeroPingTile(
                connectedPing = connectedPing,
                isChecking = isCheckingPing,
                onCheckPing = onCheckPing,
                modifier = Modifier.weight(0.72f)
            )
        }
    }
}

@Composable
private fun HeroStatTile(
    label: String,
    value: String,
    modifier: Modifier = Modifier,
    compact: Boolean = false
) {
    val shape = RoundedCornerShape(if (compact) 13.dp else 18.dp)
    Column(
        modifier = modifier
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f), shape)
            .padding(
                horizontal = if (compact) 10.dp else 14.dp,
                vertical = if (compact) 5.dp else 12.dp
            )
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            fontSize = if (compact) 9.sp else 11.sp
        )
        Spacer(Modifier.height(if (compact) 1.dp else 3.dp))
        Text(
            text = value,
            style = if (compact) MaterialTheme.typography.bodySmall else MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
            color = MaterialTheme.colorScheme.onSurface,
            fontSize = if (compact) 12.sp else 16.sp,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
    }
}
