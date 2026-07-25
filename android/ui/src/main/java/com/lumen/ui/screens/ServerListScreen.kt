package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.NetworkCheck
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.ContentPaste
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.DeleteSweep
import com.lumen.ui.components.LumenDialog
import androidx.compose.material.icons.filled.QrCode2
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.NetworkPing
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.UploadFile
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.lumen.ui.components.CountryFlagIcon
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

import com.lumen.ui.components.ConnectionSliderBar
import com.lumen.ui.components.ConnectionState

private enum class ServerSort { DEFAULT, NAME, PING }

private const val GROUP_ALL = "all"
private const val GROUP_MANUAL = "manual"
private const val PROTOCOL_ALL = "__all__"

/** AWG nodes are stored as "wireguard", so facets must use the displayed protocol. */
private fun protocolKey(node: NodeUiModel): String =
    node.displayProtocol.substringBefore('/').trim().lowercase()
        .ifEmpty { node.protocol.trim().lowercase() }

@Composable
fun ServerListScreen(
    nodes: List<NodeUiModel>,
    subscriptions: List<SubscriptionUiModel>,
    refreshingIds: Set<String>,
    isPinging: Boolean,
    pingingNodeIds: Set<String> = emptySet(),
    pingSortActive: Boolean = false,
    testingNodeId: String?,
    serverTestResults: Map<String, String>,
    connectionState: ConnectionState,
    onToggleConnection: () -> Unit,
    onSelectNode: (NodeUiModel) -> Unit,
    onEditNode: (NodeUiModel) -> Unit,
    onDeleteNode: (NodeUiModel) -> Unit,
    onDeleteAllNodes: () -> Unit = {},
    onAddNode: () -> Unit,
    onPingAll: () -> Unit,
    onUdpPingAll: () -> Unit = {},
    onPingNode: (NodeUiModel) -> Unit,
    onUdpPingNode: (NodeUiModel) -> Unit = {},
    onCopyNodeLink: (NodeUiModel) -> Unit = {},
    onExportQrCode: (NodeUiModel) -> Unit = {},
    onImportClipboard: () -> Unit,
    onAddSubscription: (String, String) -> Unit,
    onRefreshSubscription: (SubscriptionUiModel) -> Unit,
    onDeleteSubscription: (SubscriptionUiModel) -> Unit,
    onImportFile: () -> Unit = {},
    onImportQr: () -> Unit = {},
    onPingGroup: (String?) -> Unit = {},
    onUdpPingGroup: (String?) -> Unit = {},
    onExportSubscriptionText: (String?) -> String = { "" },
    onShareText: (String) -> Unit = {},
    onCopyText: (String) -> Unit = {},
    onOpen: () -> Unit = {},
    modifier: Modifier = Modifier
) {
    LaunchedEffect(Unit) { onOpen() }
    val s = LocalStrings.current
    val tick = rememberHapticTick()

    var query by remember { mutableStateOf("") }
    // Group / protocol / sort choices survive app restarts so the tab reopens as it was left.
    var group by rememberUiPreference("servers_last_group", GROUP_ALL)
    var protocol by rememberUiPreference("servers_last_protocol", PROTOCOL_ALL)
    var sortKey by rememberUiPreference("servers_last_sort", ServerSort.DEFAULT.name)
    val sort = remember(sortKey) {
        runCatching { ServerSort.valueOf(sortKey) }.getOrDefault(ServerSort.DEFAULT)
    }
    var showAddSubscription by remember { mutableStateOf(false) }
    var showSortMenu by remember { mutableStateOf(false) }
    var showPingAllMenu by remember { mutableStateOf(false) }
    var showDeleteAllConfirm by remember { mutableStateOf(false) }
    var propertiesSub by remember { mutableStateOf<SubscriptionUiModel?>(null) }
    // Groups collapse here just like on the dashboard.
    var groupExpanded by remember(group) { mutableStateOf(true) }

    val hasManual = remember(nodes) { nodes.any { it.subscriptionId == null } }
    val groups = remember(subscriptions, hasManual) {
        buildList {
            add(GROUP_ALL)
            if (hasManual) add(GROUP_MANUAL)
            subscriptions.forEach { add(it.id) }
        }
    }
    // Only drop a remembered group once real data is loaded, otherwise the first
    // (still empty) composition would wipe the persisted choice.
    if (groups.size > 1 && group !in groups) group = GROUP_ALL
    val selectedSubscription = subscriptions.firstOrNull { it.id == group }

    // Nodes of the active group, before protocol/search narrowing.
    val groupNodes = remember(nodes, group) {
        nodes.filter { node ->
            when (group) {
                GROUP_ALL -> true
                GROUP_MANUAL -> node.subscriptionId == null
                else -> node.subscriptionId == group
            }
        }
    }

    // Protocol facets are derived from the active group so counts always match.
    val protocolCounts = remember(groupNodes) {
        groupNodes.filterNot { it.isAutoNode }
            .groupingBy { protocolKey(it) }
            .eachCount()
            .toList()
            .sortedWith(compareByDescending<Pair<String, Int>> { it.second }.thenBy { it.first })
    }
    if (protocol != PROTOCOL_ALL && protocolCounts.none { it.first == protocol }) protocol = PROTOCOL_ALL

    val filtered = remember(groupNodes, query, protocol, sort, pingSortActive) {
        val matched = groupNodes.filter { node ->
            val protocolOk = protocol == PROTOCOL_ALL || node.isAutoNode ||
                protocolKey(node) == protocol
            val queryOk = query.isBlank() || node.name.contains(query, true) ||
                node.server.contains(query, true) || node.protocol.contains(query, true)
            protocolOk && queryOk
        }
        // "Sort after ping" overrides the manual sort until the list changes again.
        val comparator = if (pingSortActive) {
            compareBy<NodeUiModel> { it.pingMs?.takeIf { ping -> ping >= 0 } ?: Int.MAX_VALUE }
        } else when (sort) {
            ServerSort.NAME -> compareBy<NodeUiModel> { it.name.lowercase() }
            ServerSort.PING -> compareBy { it.pingMs?.takeIf { ping -> ping >= 0 } ?: Int.MAX_VALUE }
            ServerSort.DEFAULT -> compareBy { it.name.lowercase() }
        }
        // Auto nodes always stay pinned to the top of the list.
        matched.sortedWith(compareByDescending<NodeUiModel> { it.isAutoNode }.then(comparator))
    }

    val groupCount = subscriptions.size + if (hasManual) 1 else 0
    val countsSubtitle = "$groupCount ${s.groupsWord} \u2022 ${nodes.size} ${s.serversWord}"

    Column(modifier = modifier.fillMaxSize()) {
        LumenScreenHeader(
            title = s.servers,
            subtitle = countsSubtitle,
            actions = {
                AddEntryButton(
                    onImportClipboard = onImportClipboard,
                    onImportQr = onImportQr,
                    onImportFile = onImportFile,
                    onAddNode = onAddNode,
                    onAddSubscription = { showAddSubscription = true }
                )
            }
        )

        // Group facets: All / Manual / one chip per subscription, each with its node count.
        LazyRow(
            modifier = Modifier.fillMaxWidth(),
            contentPadding = PaddingValues(horizontal = 16.dp, vertical = 6.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            items(groups, key = { it }) { id ->
                val label = when (id) {
                    GROUP_ALL -> s.allGroups
                    GROUP_MANUAL -> s.groupDefault.ifBlank { s.manual }
                    else -> subscriptions.firstOrNull { it.id == id }?.name ?: s.subscriptions
                }
                val count = when (id) {
                    GROUP_ALL -> nodes.size
                    GROUP_MANUAL -> nodes.count { it.subscriptionId == null }
                    else -> nodes.count { it.subscriptionId == id }
                }
                LumenFilterChip(
                    label = label,
                    selected = group == id,
                    badge = count.toString(),
                    onClick = {
                        group = id
                        protocol = PROTOCOL_ALL
                    }
                )
            }
        }

        // Protocol facets, derived from the nodes of the active group.
        if (protocolCounts.size > 1) {
            LazyRow(
                modifier = Modifier.fillMaxWidth(),
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 2.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                item(key = "proto_all") {
                    LumenFilterChip(
                        label = s.allProtocols,
                        selected = protocol == PROTOCOL_ALL,
                        badge = groupNodes.size.toString(),
                        onClick = { protocol = PROTOCOL_ALL }
                    )
                }
                items(protocolCounts, key = { it.first }) { entry ->
                    LumenFilterChip(
                        label = entry.first.uppercase(),
                        selected = protocol == entry.first,
                        badge = entry.second.toString(),
                        onClick = { protocol = entry.first }
                    )
                }
            }
        }

        Spacer(Modifier.height(6.dp))
        OutlinedTextField(
            value = query,
            onValueChange = { query = it },
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp),
            placeholder = { Text(s.searchServers, style = MaterialTheme.typography.bodySmall) },
            leadingIcon = {
                Icon(
                    imageVector = Icons.Filled.Search,
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant
                )
            },
            singleLine = true,
            shape = RoundedCornerShape(14.dp)
        )

        Spacer(Modifier.height(8.dp))
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            // Same "Sort by <value>" control as the dashboard.
            Text(
                text = s.sortBy,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            Box {
                val sortLabel = when (sort) {
                    ServerSort.NAME -> s.sortByName
                    ServerSort.PING -> s.sortByPing
                    ServerSort.DEFAULT -> s.sortDefaultLabel
                }
                Text(
                    text = sortLabel,
                    style = MaterialTheme.typography.bodySmall,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.clickable { showSortMenu = true }
                )
                LumenMenu(
                    expanded = showSortMenu,
                    onDismissRequest = { showSortMenu = false }
                ) {
                    listOf(
                        ServerSort.DEFAULT to s.sortDefaultLabel,
                        ServerSort.NAME to s.sortByName,
                        ServerSort.PING to s.sortByPing
                    ).forEach { (value, label) ->
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
            Spacer(Modifier.weight(1f))
            // Wipe-all sitting left of ping button appears only for the default/manual group
            if (group == GROUP_MANUAL) {
                val hasManualNodes = nodes.any { it.subscriptionId == null }
                IconButton(
                    onClick = {
                        tick(HapticFeedbackType.LongPress)
                        showDeleteAllConfirm = true
                    },
                    enabled = hasManualNodes,
                    modifier = Modifier.size(38.dp)
                ) {
                    Icon(
                        imageVector = Icons.Filled.DeleteSweep,
                        contentDescription = s.deleteAllServers,
                        tint = if (!hasManualNodes) MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.4f)
                        else MaterialTheme.colorScheme.error
                    )
                }
            }
            if (showDeleteAllConfirm) {
                LumenDialog(
                    title = s.deleteAllServers,
                    message = s.deleteAllServersConfirm,
                    onDismissRequest = { showDeleteAllConfirm = false },
                    confirmText = s.delete,
                    destructive = true,
                    onConfirm = {
                        showDeleteAllConfirm = false
                        onDeleteAllNodes()
                    },
                    dismissText = s.cancel,
                    onDismiss = { showDeleteAllConfirm = false }
                )
            }
            Box {
                IconButton(
                    onClick = {
                        tick(HapticFeedbackType.LongPress)
                        showPingAllMenu = true
                    },
                    enabled = !isPinging,
                    modifier = Modifier.size(38.dp)
                ) {
                    if (isPinging) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(16.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.primary
                        )
                    } else {
                        Icon(
                            imageVector = Icons.Filled.NetworkCheck,
                            contentDescription = s.pingAll,
                            tint = MaterialTheme.colorScheme.primary
                        )
                    }
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

        LazyColumn(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 10.dp, bottom = 24.dp),
            // Zero spacing: every item brings its own padding, so the subscription
            // header, traffic bar and rows render as one uninterrupted tile.
            verticalArrangement = Arrangement.spacedBy(0.dp)
        ) {
            if (selectedSubscription != null) {
                // Exactly the dashboard group header: all buttons, counts, traffic.
                item(key = "sub_head_" + selectedSubscription.id) {
                    SubscriptionHeaderTile(
                        group = HomeServerGroup(
                            id = selectedSubscription.id,
                            title = selectedSubscription.name,
                            nodes = groupNodes,
                            isSubscription = true,
                            subscription = selectedSubscription
                        ),
                        isExpanded = groupExpanded,
                        onToggleExpand = { groupExpanded = !groupExpanded },
                        onRefreshSubscription = { onRefreshSubscription(selectedSubscription) },
                        onDeleteSubscription = {
                            onDeleteSubscription(selectedSubscription)
                            group = GROUP_ALL
                        },
                        onPingGroup = { onPingGroup(selectedSubscription.id) },
                        onUdpPingGroup = { onUdpPingGroup(selectedSubscription.id) },
                        onShowProperties = { sub -> propertiesSub = sub },
                        onExportAll = { subId ->
                            val text = onExportSubscriptionText(subId)
                            if (text.isNotBlank()) onShareText(text)
                        }
                    )
                }
                if (groupExpanded) {
                    item(key = "sub_info_" + selectedSubscription.id) {
                        // No rounding and no gap: the rows below continue the same tile.
                        SubscriptionInfoBar(
                            sub = selectedSubscription,
                            pullUp = 0.dp,
                            roundedBottom = false
                        )
                    }
                }
            }
            val showRows = selectedSubscription == null || groupExpanded
            if (filtered.isEmpty() && showRows) {
                item(key = "empty") {
                    EmptyState(text = if (groupNodes.isEmpty()) s.noServers else s.nothingFound)
                }
            } else if (showRows) {
                itemsIndexed(filtered, key = { _, node -> node.id }) { index, node ->
                    // Under a subscription the rows share the header rail, so the
                    // whole block reads as a single tile with no gaps.
                    val railed = selectedSubscription != null
                    val isLast = index == filtered.lastIndex
                    Box(
                        modifier = if (railed) {
                            Modifier
                                .fillMaxWidth()
                                .then(
                                    if (isLast) Modifier.clip(RoundedCornerShape(bottomStart = 18.dp, bottomEnd = 18.dp)) else Modifier
                                )
                                .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.38f))
                                .padding(start = 8.dp, end = 8.dp, top = 4.dp, bottom = if (isLast) 10.dp else 4.dp)
                        } else {
                            Modifier
                                .fillMaxWidth()
                                .padding(vertical = 4.dp)
                        }
                    ) {
                        NodeRow(
                            node = node,
                            // Group pings mark ids here; the dashboard shows the same spinner.
                            testing = testingNodeId == node.id || node.id in pingingNodeIds,
                            testResult = serverTestResults[node.id],
                            onSelect = {
                                tick(HapticFeedbackType.LongPress)
                                onSelectNode(node)
                            },
                            onEdit = { onEditNode(node) },
                            onDelete = { onDeleteNode(node) },
                            onPing = { onPingNode(node) },
                            onUdpPing = { onUdpPingNode(node) },
                            onCopyLink = { onCopyNodeLink(node) },
                            onExportQr = { onExportQrCode(node) }
                        )
                    }
                }
            }
        }

    }

    if (showAddSubscription) {
        AddSubscriptionDialog(
            onDismiss = { showAddSubscription = false },
            onConfirm = { name, url ->
                onAddSubscription(name, url)
                showAddSubscription = false
            }
        )
    }
}

/**
 * Single entry point for everything that adds servers: clipboard, QR, file,
 * manual editor and subscription links. Replaces the old separate buttons.
 */
@Composable
private fun AddEntryButton(
    onImportClipboard: () -> Unit,
    onImportQr: () -> Unit,
    onImportFile: () -> Unit,
    onAddNode: () -> Unit,
    onAddSubscription: () -> Unit
) {
    val s = LocalStrings.current
    val tick = rememberHapticTick()
    var expanded by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(14.dp)
    Box {
        Box(
            modifier = Modifier
                .size(38.dp)
                .clip(shape)
                .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.16f))
                .border(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.45f), shape)
                .clickable {
                    tick(HapticFeedbackType.LongPress)
                    expanded = true
                },
            contentAlignment = Alignment.Center
        ) {
            Icon(
                imageVector = Icons.Filled.Add,
                contentDescription = s.addServerOrSubscription,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(20.dp)
            )
        }
        LumenMenu(
            expanded = expanded,
            onDismissRequest = { expanded = false }
        ) {
            DropdownMenuItem(
                text = { Text(s.subscriptionLink) },
                leadingIcon = { Icon(Icons.Filled.Link, contentDescription = null) },
                onClick = { expanded = false; onAddSubscription() }
            )
            DropdownMenuItem(
                text = { Text(s.importFromClipboard) },
                leadingIcon = { Icon(Icons.Filled.ContentPaste, contentDescription = null) },
                onClick = { expanded = false; onImportClipboard() }
            )
            DropdownMenuItem(
                text = { Text(s.importQrCode) },
                leadingIcon = { Icon(Icons.Filled.QrCodeScanner, contentDescription = null) },
                onClick = { expanded = false; onImportQr() }
            )
            DropdownMenuItem(
                text = { Text(s.importFromFile) },
                leadingIcon = { Icon(Icons.Filled.UploadFile, contentDescription = null) },
                onClick = { expanded = false; onImportFile() }
            )
            DropdownMenuItem(
                text = { Text(s.importManually) },
                leadingIcon = { Icon(Icons.Filled.Edit, contentDescription = null) },
                onClick = { expanded = false; onAddNode() }
            )
        }
    }
}

@Composable
private fun EmptyState(text: String) {
    LumenCard {
        Text(
            text = text,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    }
}

@Composable
private fun SubscriptionCard(
    sub: SubscriptionUiModel,
    refreshing: Boolean,
    onRefresh: () -> Unit,
    onDelete: () -> Unit
) {
    val s = LocalStrings.current
    val updatedText = remember(sub.lastUpdated) {
        if (sub.lastUpdated <= 0L) s.never
        else SimpleDateFormat("dd.MM HH:mm", Locale.getDefault()).format(Date(sub.lastUpdated))
    }
    LumenCard {
        Column(modifier = Modifier.fillMaxWidth()) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = sub.name,
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.Bold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                    Spacer(Modifier.height(2.dp))
                    Text(
                        text = sub.nodeCount.toString() + " " + s.nodes + " \u2022 " + s.updated + " " + updatedText,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
                IconButton(onClick = onRefresh, enabled = !refreshing, modifier = Modifier.size(34.dp)) {
                    if (refreshing) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(15.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.primary
                        )
                    } else {
                        Icon(
                            imageVector = Icons.Filled.Refresh,
                            contentDescription = s.refreshSubscription,
                            tint = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.size(18.dp)
                        )
                    }
                }
                IconButton(onClick = onDelete, modifier = Modifier.size(34.dp)) {
                    Icon(
                        imageVector = Icons.Filled.Delete,
                        contentDescription = s.deleteSubscription,
                        tint = MaterialTheme.colorScheme.error,
                        modifier = Modifier.size(18.dp)
                    )
                }
            }
            if (!sub.trafficSummary.isNullOrBlank() || sub.expiryDaysLeft != null) {
                Spacer(Modifier.height(6.dp))
                Text(
                    text = listOfNotNull(
                        sub.trafficSummary?.takeIf { it.isNotBlank() },
                        sub.expiryDaysLeft?.let { it.toString() + "d" }
                    ).joinToString(" \u2022 "),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.primary
                )
            }
            Spacer(Modifier.height(6.dp))
            // Auto-update is always on now; the card only reports the effective cadence,
            // which comes from the provider when it sends one, otherwise from settings.
            Text(
                text = s.subscriptionAutoUpdate + (sub.updateIntervalHours
                    ?.let { " \u2022 " + it + "h" } ?: ""),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
    }
}

@Composable
private fun NodeRow(
    node: NodeUiModel,
    testing: Boolean,
    testResult: String?,
    onSelect: () -> Unit,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
    onPing: () -> Unit,
    onUdpPing: () -> Unit = {},
    onCopyLink: () -> Unit = {},
    onExportQr: () -> Unit = {}
) {
    val s = LocalStrings.current
    var menu by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(18.dp)
    val selected = node.isSelected
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(
                if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.12f)
                else MaterialTheme.colorScheme.surfaceVariant
            )
            .border(
                1.dp,
                if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.6f)
                else MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f),
                shape
            )
            .clickable { onSelect() }
            .padding(start = 14.dp, end = 6.dp, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        CountryFlagIcon(
            countryCode = node.countryCode,
            width = 26.dp,
            height = 18.dp,
            fallbackText = node.displayProtocol.take(2)
        )
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = node.name,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Spacer(Modifier.height(3.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                // Per-protocol badge colour, shared with the dashboard.
                val badgeColor = protocolColor(node.protocol)
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(6.dp))
                        .background(badgeColor.copy(alpha = 0.14f))
                        .padding(horizontal = 6.dp, vertical = 2.dp)
                ) {
                    Text(
                        text = node.displayProtocol,
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.SemiBold,
                        color = badgeColor
                    )
                }
                Spacer(Modifier.width(8.dp))
                Text(
                    text = node.server + ":" + node.port,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
            }
            if (!testResult.isNullOrBlank()) {
                Spacer(Modifier.height(3.dp))
                Text(
                    text = testResult,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.primary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
            }
        }
        Spacer(Modifier.width(8.dp))
        if (testing) {
            CircularProgressIndicator(
                modifier = Modifier.size(16.dp),
                strokeWidth = 2.dp,
                color = MaterialTheme.colorScheme.primary
            )
        } else {
            val ping = node.pingMs
            if (ping != null) {
                Text(
                    text = if (ping < 0) "\u2014" else ping.toString() + " ms",
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Bold,
                    color = pingColor(ping)
                )
            }
        }
        Box {
            IconButton(onClick = { menu = true }, modifier = Modifier.size(34.dp)) {
                Icon(
                    imageVector = Icons.Filled.MoreVert,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.size(18.dp)
                )
            }
            // Same actions and order as the dashboard server menu.
            LumenMenu(
                expanded = menu,
                onDismissRequest = { menu = false }
            ) {
                DropdownMenuItem(
                    text = { Text(s.edit) },
                    leadingIcon = { Icon(Icons.Filled.Edit, contentDescription = null) },
                    onClick = { menu = false; onEdit() }
                )
                DropdownMenuItem(
                    text = { Text(s.copyLink) },
                    leadingIcon = { Icon(Icons.Filled.ContentCopy, contentDescription = null) },
                    onClick = { menu = false; onCopyLink() }
                )
                DropdownMenuItem(
                    text = { Text(s.exportQrCode) },
                    leadingIcon = { Icon(Icons.Filled.QrCode2, contentDescription = null) },
                    onClick = { menu = false; onExportQr() }
                )
                DropdownMenuItem(
                    text = { Text(s.ping) },
                    leadingIcon = { Icon(Icons.Filled.NetworkPing, contentDescription = null) },
                    onClick = { menu = false; onPing() }
                )
                DropdownMenuItem(
                    text = { Text("Ping UDP") },
                    leadingIcon = { Icon(Icons.Filled.NetworkPing, contentDescription = null) },
                    onClick = { menu = false; onUdpPing() }
                )
                DropdownMenuItem(
                    text = { Text(s.delete) },
                    leadingIcon = { Icon(Icons.Filled.Delete, contentDescription = null) },
                    onClick = { menu = false; onDelete() }
                )
            }
        }
    }
}

@Composable
private fun AddSubscriptionDialog(
    onDismiss: () -> Unit,
    onConfirm: (String, String) -> Unit
) {
    val s = LocalStrings.current
    var name by remember { mutableStateOf("") }
    var url by remember { mutableStateOf("") }
    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = RoundedCornerShape(22.dp),
            color = MaterialTheme.colorScheme.surface
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(20.dp)
            ) {
                Text(
                    text = s.addSubscription,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.onSurface
                )
                Spacer(Modifier.height(14.dp))
                OutlinedTextField(
                    value = url,
                    onValueChange = { url = it },
                    label = { Text(s.url) },
                    singleLine = true,
                    shape = RoundedCornerShape(14.dp),
                    modifier = Modifier.fillMaxWidth()
                )
                Spacer(Modifier.height(10.dp))
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text(s.nameOptional) },
                    singleLine = true,
                    shape = RoundedCornerShape(14.dp),
                    modifier = Modifier.fillMaxWidth()
                )
                Spacer(Modifier.height(16.dp))
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    TextButton(onClick = onDismiss) { Text(s.cancel) }
                    Spacer(Modifier.width(8.dp))
                    OutlinedButton(
                        onClick = { onConfirm(name.trim(), url.trim()) },
                        enabled = url.isNotBlank()
                    ) { Text(s.add) }
                }
            }
        }
    }
}
