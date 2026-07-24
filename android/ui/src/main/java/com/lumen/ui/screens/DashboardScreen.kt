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
import com.lumen.ui.components.CountryFlagIcon

@Composable
fun DashboardScreen(
    connectionState: ConnectionState,
    nodes: List<NodeUiModel>,
    subscriptions: List<SubscriptionUiModel>,
    pingingNodeIds: Set<String> = emptySet(),
    onToggleConnection: () -> Unit,
    onSelectNode: (NodeUiModel) -> Unit,
    onImportClipboard: () -> Unit,
    onImportFile: () -> Unit,
    onImportQr: () -> Unit,
    onAddManualNode: () -> Unit,
    onRefreshSubscription: (String) -> Unit = {},
    onDeleteSubscription: (String) -> Unit = {},
    onPingAll: () -> Unit = {},
    onPingGroup: (String?) -> Unit = {},
    onUdpPingGroup: (String?) -> Unit = {},
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
    modifier: Modifier = Modifier
) {
    val strings = LocalStrings.current
    var showImportMenu by remember { mutableStateOf(false) }
    var showSortMenu by remember { mutableStateOf(false) }
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
    val groups = remember(nodes, subscriptions, strings.manual, sortByPing) {
        val byName = compareBy(String.CASE_INSENSITIVE_ORDER, NodeUiModel::name)
        val byPing = compareBy<NodeUiModel> { it.pingMs?.takeIf { p -> p >= 0 } ?: Int.MAX_VALUE }
        val comparator = if (sortByPing) byPing else byName
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
                IconButton(onClick = { showDonateDialog = true }) {
                    Icon(Icons.Filled.Favorite, contentDescription = "Donate")
                }
                if (showDonateDialog) {
                    AlertDialog(onDismissRequest = { showDonateDialog = false },
                        title = { Text("Поддержать") },
                        text = { Text("Выберите способ поддержки") },
                        confirmButton = {
                            TextButton(onClick = { uriHandler.openUri("https://www.donationalerts.com/r/studiobebraedition"); showDonateDialog = false }) { Text("DonationAlerts") }
                        },
                        dismissButton = {
                            TextButton(onClick = { showUsdtDialog = true; showDonateDialog = false }) { Text("USDT TRC20") }
                        })
                }
                if (showUsdtDialog) {
                    AlertDialog(onDismissRequest = { showUsdtDialog = false },
                        title = { Text("USDT TRC20") },
                        text = {
                            Column {
                                Text("TWHsuUDru4pXBcGpfeKfzymbkfajyFnb2s")
                                Spacer(Modifier.height(8.dp))
                                TextButton(onClick = { clipboard.setText(AnnotatedString("TWHsuUDru4pXBcGpfeKfzymbkfajyFnb2s")); showUsdtDialog = false }) { Text("Скопировать") }
                            }
                        },
                        confirmButton = { TextButton(onClick = { showUsdtDialog = false }) { Text("Закрыть") } })
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
                // Extra bottom room so the last tiles can be scrolled clear of the bars.
                contentPadding = PaddingValues(bottom = 96.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(6.dp)
            ) {
                item(key = "header_spacer") { Spacer(Modifier.height(4.dp)) }

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
                                    val menuShape = RoundedCornerShape(4.dp)
                                    DropdownMenu(
                                        expanded = showSortMenu,
                                        onDismissRequest = { showSortMenu = false },
                                        modifier = Modifier
                                            .widthIn(min = 220.dp)
                                            .clip(menuShape)
                                            .background(MaterialTheme.colorScheme.surface)
                                            .border(BorderStroke(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.35f)), menuShape)
                                    ) {
                                        Column(Modifier.padding(vertical = 6.dp)) {
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
                            Text(
                                text = strings.pingAll,
                                style = MaterialTheme.typography.bodySmall,
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.primary,
                                modifier = Modifier.clickable(onClick = onPingAll)
                            )
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
                    groups.forEach { group ->
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

                        // Expanded Info Bar & Node Items (Fully Virtualized in LazyColumn)
                        if (isExpanded) {
                            if (group.isSubscription && group.subscription != null) {
                                item(key = "group_info_${group.id}") {
                                    SubscriptionInfoBar(sub = group.subscription)
                                }
                            }

                            items(
                                items = group.nodes,
                                key = { node -> "node_${node.id}" }
                            ) { node ->
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
                                    onCopyLink = { onCopyNodeLink(node) },
                                    onExportQr = { onExportQrCode(node) },
                                    onDeleteNode = { onDeleteNode(node) }
                                )
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

                                val menuShape = RoundedCornerShape(4.dp)
                                DropdownMenu(
                                    expanded = showImportMenu,
                                    onDismissRequest = { showImportMenu = false },
                                    modifier = Modifier
                                        .widthIn(min = 220.dp)
                                        .clip(menuShape)
                                        .background(MaterialTheme.colorScheme.surface)
                                        .border(BorderStroke(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.35f)), menuShape)
                                ) {
                                    Column(Modifier.padding(vertical = 4.dp)) {
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

        // Bottom connection bar (Шкала подключения)
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 12.dp)
        ) {
            ConnectionSliderBar(
                connectionState = connectionState,
                onToggleConnection = {
                    if (hapticsEnabled) haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                    onToggleConnection()
                }
            )
        }
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
private fun SubscriptionPropertiesDialog(
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

@Composable
private fun SubscriptionHeaderTile(
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
    val tileShape = if (isExpanded) RoundedCornerShape(topStart = 18.dp, topEnd = 18.dp) else RoundedCornerShape(18.dp)
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
                    val pingMenuShape = RoundedCornerShape(4.dp)
                    DropdownMenu(
                        expanded = showPingMenu,
                        onDismissRequest = { showPingMenu = false },
                        modifier = Modifier
                            .clip(pingMenuShape)
                            .background(MaterialTheme.colorScheme.surface)
                            .border(BorderStroke(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.35f)), pingMenuShape)
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

                    val menuShape = RoundedCornerShape(4.dp)
                    DropdownMenu(
                        expanded = showMenu,
                        onDismissRequest = { showMenu = false },
                        modifier = Modifier
                            .widthIn(min = 220.dp)
                            .clip(menuShape)
                            .background(MaterialTheme.colorScheme.surface)
                            .border(BorderStroke(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.35f)), menuShape)
                    ) {
                        Column(Modifier.padding(vertical = 6.dp)) {
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

@Composable
private fun SubscriptionInfoBar(sub: SubscriptionUiModel) {
    val strings = LocalStrings.current
    val primaryColor = MaterialTheme.colorScheme.primary
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.6f))
            .padding(12.dp)
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
                        Box(
                            Modifier
                                .clip(RoundedCornerShape(6.dp))
                                .background(primaryColor.copy(alpha = 0.14f))
                                .padding(horizontal = 6.dp, vertical = 2.dp)
                        ) {
                            Text(
                                text = node.displayProtocol.uppercase(),
                                style = MaterialTheme.typography.labelSmall,
                                color = primaryColor,
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

            // Trailing ">" action button with dropdown menu
            if (!isSelectionMode) {
                Box {
                    IconButton(
                        onClick = { showActionMenu = true },
                        modifier = Modifier.size(28.dp)
                    ) {
                        Icon(
                            imageVector = Icons.AutoMirrored.Filled.KeyboardArrowRight,
                            contentDescription = null,
                            tint = if (selected) primaryColor else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
                            modifier = Modifier.size(20.dp)
                        )
                    }
                    val menuShape = RoundedCornerShape(4.dp)
                    DropdownMenu(
                        expanded = showActionMenu,
                        onDismissRequest = { showActionMenu = false },
                        modifier = Modifier
                            .widthIn(min = 200.dp)
                            .clip(menuShape)
                            .background(MaterialTheme.colorScheme.surface)
                            .border(BorderStroke(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.35f)), menuShape)
                    ) {
                        Column(Modifier.padding(vertical = 4.dp)) {
                            DropdownMenuItem(
                                text = { Text(strings.copyLink, color = MaterialTheme.colorScheme.onSurface) },
                                trailingIcon = { Icon(Icons.Filled.Edit, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) },
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
                                text = { Text(strings.edit, color = MaterialTheme.colorScheme.onSurface) },
                                trailingIcon = { Icon(Icons.Filled.Edit, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) },
                                onClick = {
                                    showActionMenu = false
                                    onEditNode()
                                }
                            )
                            DropdownMenuItem(
                                text = { Text(strings.ping, color = MaterialTheme.colorScheme.onSurface) },
                                trailingIcon = { Icon(Icons.Filled.PlayArrow, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) },
                                onClick = {
                                    showActionMenu = false
                                    onPingNode()
                                }
                            )
                        }
                    }
                }
            }
        }
    }
}