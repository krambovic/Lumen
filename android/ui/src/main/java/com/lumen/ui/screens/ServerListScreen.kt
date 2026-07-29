package com.lumen.ui.screens

import androidx.activity.compose.BackHandler
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
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Sort
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.CreateNewFolder
import androidx.compose.material.icons.filled.NetworkCheck
import androidx.compose.material.icons.filled.ContentPaste
import androidx.compose.material.icons.filled.DeleteSweep
import com.lumen.ui.components.LumenDialog
import com.lumen.ui.components.SubscriptionDetailsDialog
import com.lumen.ui.components.SubscriptionProviderCard
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.FilterList
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material.icons.filled.UploadFile
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
import com.lumen.ui.components.CountryFlagHelper

import com.lumen.ui.components.ConnectionState

enum class ServerSort { DEFAULT, NAME, NAME_DESC, PING, COUNTRY, PROTOCOL }

/**
 * Sort order is a single user choice shared by the dashboard and the Servers tab:
 * both read and write this preference, so changing it in one place moves the other.
 */
const val SERVERS_SORT_PREF = "servers_last_sort"

/** Menu entries, in display order. Kept here so the two screens cannot drift apart. */
fun serverSortOptions(s: LumenStrings): List<Pair<ServerSort, String>> = listOf(
    ServerSort.DEFAULT to s.sortDefaultLabel,
    ServerSort.NAME to s.sortByName,
    ServerSort.NAME_DESC to s.sortByNameDesc,
    ServerSort.PING to s.sortByPing,
    ServerSort.COUNTRY to s.sortByCountry,
    ServerSort.PROTOCOL to s.sortByProtocol
)

fun serverSortLabel(sort: ServerSort, s: LumenStrings): String =
    serverSortOptions(s).firstOrNull { it.first == sort }?.second ?: s.sortDefaultLabel

fun serverSortOf(key: String): ServerSort =
    runCatching { ServerSort.valueOf(key) }.getOrDefault(ServerSort.DEFAULT)

/** The one comparator both screens sort with; the dashboard applies it per group. */
fun serverSortComparator(sort: ServerSort): Comparator<NodeUiModel> = when (sort) {
    ServerSort.NAME -> compareBy<NodeUiModel> { it.name.lowercase() }
    ServerSort.NAME_DESC -> compareByDescending<NodeUiModel> { it.name.lowercase() }
    ServerSort.PING -> compareBy<NodeUiModel> { it.pingMs?.takeIf { ping -> ping > 0 } ?: Int.MAX_VALUE }
    // Unknown countries sink to the bottom instead of leading the list.
    ServerSort.COUNTRY -> compareBy<NodeUiModel> { countryCodeOf(it).ifEmpty { "zzz" } }
        .thenBy { it.name.lowercase() }
    ServerSort.PROTOCOL -> compareBy<NodeUiModel> { protocolKey(it) }
        .thenBy { it.name.lowercase() }
    ServerSort.DEFAULT -> compareBy<NodeUiModel> { it.name.lowercase() }
}

private const val PROTOCOL_ALL = "__all__"

/** Latency ceilings offered by the filter menu; 0 disables the filter. */
private val MAX_PING_CHOICES = listOf(0, 100, 200, 500)

/** AWG nodes are stored as "wireguard", so facets must use the displayed protocol. */
internal fun protocolKey(node: NodeUiModel): String =
    node.displayProtocol.substringBefore('/').trim().lowercase()
        .ifEmpty { node.protocol.trim().lowercase() }

/** Country of a node, falling back to name/host detection when the code is empty. */
internal fun countryCodeOf(node: NodeUiModel): String =
    node.countryCode.trim().ifEmpty { CountryFlagHelper.detectCountryStrict(node.name, node.server) }
        .uppercase()

/** A header of the "group by country" layout. */
private data class CountrySection(
    val code: String,
    val count: Int,
    val bestPing: Int?
)

@Composable
fun ServerListScreen(
    nodes: List<NodeUiModel>,
    subscriptions: List<SubscriptionUiModel>,
    serverGroups: List<ServerGroupUiModel> = emptyList(),
    refreshingIds: Set<String>,
    isPinging: Boolean,
    pingingNodeIds: Set<String> = emptySet(),
    // Starts a check automatically the first time the list is shown.
    autoPingOnOpen: Boolean = false,
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
    onStopPing: () -> Unit = {},
    onPingNodes: (List<NodeUiModel>) -> Unit = {},
    onPingNode: (NodeUiModel) -> Unit,
    onCopyNodeLink: (NodeUiModel) -> Unit = {},
    onExportQrCode: (NodeUiModel) -> Unit = {},
    onImportClipboard: () -> Unit,
    onAddSubscription: (String, String) -> Unit,
    onUpdateSubscription: (SubscriptionUiModel, String, String) -> Boolean = { _, _, _ -> false },
    onRefreshSubscription: (SubscriptionUiModel) -> Unit,
    onDeleteSubscription: (SubscriptionUiModel) -> Unit,
    onImportFile: () -> Unit = {},
    onImportQr: () -> Unit = {},
    onPingGroup: (String?) -> Unit = {},
    onCreateGroup: (String) -> Unit = {},
    onRenameGroup: (String, String) -> Unit = { _, _ -> },
    onDeleteGroup: (String) -> Unit = {},
    // Null group id clears the assignment, sending the servers back to their default bucket.
    onAssignNodesToGroup: (List<NodeUiModel>, String?) -> Unit = { _, _ -> },
    onExportNodesText: (Set<String>) -> String = { "" },
    onExportSubscriptionText: (String?) -> String = { "" },
    onShareText: (String) -> Unit = {},
    onCopyText: (String) -> Unit = {},
    // Provider links (support, website, banner…) leave the app through the platform browser.
    onOpenUrl: (String) -> Unit = {},
    onOpen: () -> Unit = {},
    modifier: Modifier = Modifier
) {
    LaunchedEffect(Unit) { onOpen() }
    val s = LocalStrings.current
    val tick = rememberHapticTick()
    // Ask for the camera at the point of use so the scanner never falls back to the
    // library's own "the camera encountered a problem" dialog.
    val scanQr = rememberQrScanRequest(onImportQr)

    var query by remember { mutableStateOf("") }
    // Group / protocol / sort choices survive app restarts so the tab reopens as it was left.
    var group by rememberUiPreference("servers_last_group", GROUP_ALL)
    var protocol by rememberUiPreference("servers_last_protocol", PROTOCOL_ALL)
    var sortKey by rememberUiPreference(SERVERS_SORT_PREF, ServerSort.DEFAULT.name)
    val sort = remember(sortKey) { serverSortOf(sortKey) }
    // Optional automatic check when the tab opens; fires once per screen entry.
    var autoPingStarted by remember { mutableStateOf(false) }
    LaunchedEffect(autoPingOnOpen, nodes.isEmpty()) {
        if (autoPingOnOpen && !autoPingStarted && !isPinging && nodes.isNotEmpty()) {
            autoPingStarted = true
            onPingAll()
        }
    }
    // Country grouping and the reachability/latency filters also survive restarts.
    var groupByCountryPref by rememberUiPreference("servers_group_by_country", "0")
    val groupByCountry = groupByCountryPref == "1"
    var onlyReachablePref by rememberUiPreference("servers_filter_reachable", "0")
    val onlyReachable = onlyReachablePref == "1"
    var maxPingPref by rememberUiPreference("servers_filter_max_ping", "0")
    val maxPing = remember(maxPingPref) { maxPingPref.toIntOrNull()?.coerceAtLeast(0) ?: 0 }
    var showAddSubscription by remember { mutableStateOf(false) }
    var showSortMenu by remember { mutableStateOf(false) }
    var showGroupMenu by remember { mutableStateOf(false) }
    var showProtocolMenu by remember { mutableStateOf(false) }
    var showFiltersMenu by remember { mutableStateOf(false) }
    var showDeleteAllConfirm by remember { mutableStateOf(false) }
    var propertiesSub by remember { mutableStateOf<SubscriptionUiModel?>(null) }
    var editingSubscription by remember { mutableStateOf<SubscriptionUiModel?>(null) }
    // Group management: create/rename share one name dialog, delete asks first, and the
    // picker serves both the row menu and the multi-select bar.
    var showCreateGroup by remember { mutableStateOf(false) }
    var renamingGroup by remember { mutableStateOf<ServerGroupUiModel?>(null) }
    var deletingGroup by remember { mutableStateOf<ServerGroupUiModel?>(null) }
    var assigningNodes by remember { mutableStateOf<List<NodeUiModel>>(emptyList()) }
    // A new group is written through the view model, so it only reaches this screen on
    // the next state update. Remember what the creation was for and finish there.
    var pendingGroupName by remember { mutableStateOf<String?>(null) }
    var pendingAssignment by remember { mutableStateOf<List<NodeUiModel>>(emptyList()) }
    var isSelectionMode by remember { mutableStateOf(false) }
    var selectedNodeIds by remember { mutableStateOf(setOf<String>()) }
    // Groups collapse here just like on the dashboard.
    var groupExpanded by remember(group) { mutableStateOf(true) }

    BackHandler(enabled = isSelectionMode) {
        isSelectionMode = false
        selectedNodeIds = emptySet()
    }
    LaunchedEffect(nodes.map { it.id }) {
        selectedNodeIds = selectedNodeIds.intersect(nodes.mapTo(mutableSetOf()) { it.id })
        if (selectedNodeIds.isEmpty()) isSelectionMode = false
    }

    val groups = remember(subscriptions, serverGroups) {
        serverGroupIds(serverGroups, subscriptions)
    }
    // Only drop a remembered group once real data is loaded, otherwise the first
    // (still empty) composition would wipe the persisted choice. "all" and "manual"
    // are always present, so the list size says nothing about the subscriptions.
    LaunchedEffect(groups) {
        if ((subscriptions.isNotEmpty() || serverGroups.isNotEmpty()) && group !in groups) {
            group = GROUP_ALL
        }
    }
    val selectedSubscription = subscriptions.firstOrNull { it.id == group }
    val selectedCustomGroup = serverGroups.firstOrNull { it.id == group }
    // Once the group exists: move whatever the user was assigning into it and open it.
    LaunchedEffect(serverGroups, pendingGroupName) {
        val wanted = pendingGroupName ?: return@LaunchedEffect
        val created = serverGroups.lastOrNull { it.name == wanted } ?: return@LaunchedEffect
        val targets = pendingAssignment
        if (targets.isNotEmpty()) onAssignNodesToGroup(targets, created.id)
        group = created.id
        protocol = PROTOCOL_ALL
        // Cleared last: these keys restart this effect.
        pendingAssignment = emptyList()
        pendingGroupName = null
    }

    // Nodes of the active group, before protocol/search narrowing.
    val groupNodes = remember(nodes, group) { nodesInGroup(nodes, group) }
    val groupLabelOf: (String) -> String = { id ->
        when (id) {
            GROUP_ALL -> s.allGroups
            GROUP_MANUAL -> s.groupDefault.ifBlank { s.manual }
            else -> serverGroups.firstOrNull { it.id == id }?.name
                ?: subscriptions.firstOrNull { it.id == id }?.name
                ?: s.subscriptions
        }
    }

    // Protocol facets are derived from the active group so counts always match.
    val protocolCounts = remember(groupNodes) {
        groupNodes.groupingBy { protocolKey(it) }
            .eachCount()
            .toList()
            .sortedWith(compareByDescending<Pair<String, Int>> { it.second }.thenBy { it.first })
    }
    LaunchedEffect(protocolCounts) {
        if (protocol != PROTOCOL_ALL && groupNodes.isNotEmpty() &&
            protocolCounts.none { it.first == protocol }
        ) {
            protocol = PROTOCOL_ALL
        }
    }

    val filtered = remember(groupNodes, query, protocol, sort, onlyReachable, maxPing) {
        val matched = groupNodes.filter { node ->
            val protocolOk = protocol == PROTOCOL_ALL || protocolKey(node) == protocol
            val queryOk = query.isBlank() || node.name.contains(query, true) ||
                node.server.contains(query, true) || node.protocol.contains(query, true)
            val ping = node.pingMs?.takeIf { value -> value > 0 }
            val reachableOk = !onlyReachable || ping != null
            val latencyOk = maxPing <= 0 || (ping != null && ping <= maxPing)
            protocolOk && queryOk && reachableOk && latencyOk
        }
        matched.sortedWith(serverSortComparator(sort))
    }

    // With country grouping on, the list is a flat sequence of headers and rows so
    // the existing LazyColumn keying and rail logic keep working unchanged.
    val rowItems: List<Any> = remember(filtered, groupByCountry) {
        if (!groupByCountry) {
            filtered
        } else {
            buildList {
                filtered.groupBy { countryCodeOf(it) }
                    .toList()
                    .sortedBy { it.first.ifEmpty { "zzz" } }
                    .forEach { (code, sectionNodes) ->
                        add(
                            CountrySection(
                                code = code,
                                count = sectionNodes.size,
                                bestPing = sectionNodes.mapNotNull { node ->
                                    node.pingMs?.takeIf { value -> value > 0 }
                                }.minOrNull()
                            )
                        )
                        addAll(sectionNodes)
                    }
            }
        }
    }
    val firstVisibleNodeId = remember(rowItems) {
        rowItems.firstNotNullOfOrNull { (it as? NodeUiModel)?.id }
    }
    val lastVisibleNodeId = remember(rowItems) {
        rowItems.asReversed().firstNotNullOfOrNull { (it as? NodeUiModel)?.id }
    }

    val activeFilterCount = (if (protocol != PROTOCOL_ALL) 1 else 0) +
        (if (onlyReachable) 1 else 0) + (if (maxPing > 0) 1 else 0)

    val groupCount = subscriptions.size + serverGroups.size + 1
    val countsSubtitle = "$groupCount ${s.groupsWord} \u2022 ${nodes.size} ${s.serversWord}"

    Column(modifier = modifier.fillMaxSize()) {
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
                },
                onMoveSelected = {
                    val selectedNodes = nodes.filter { it.id in selectedNodeIds }
                    if (selectedNodes.isNotEmpty()) assigningNodes = selectedNodes
                }
            )
        } else {
            LumenScreenHeader(
                title = s.servers,
                subtitle = countsSubtitle,
                actions = {
                Box {
                    val shape = RoundedCornerShape(14.dp)
                    Box(
                        modifier = Modifier
                            .size(38.dp)
                            .clip(shape)
                            .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.16f))
                            .border(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.45f), shape)
                            .clickable {
                                tick(HapticFeedbackType.LongPress)
                                showSortMenu = true
                            },
                        contentAlignment = Alignment.Center
                    ) {
                        Icon(
                            imageVector = Icons.AutoMirrored.Filled.Sort,
                            contentDescription = s.sortBy,
                            tint = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.size(21.dp)
                        )
                    }
                    LumenMenu(
                        expanded = showSortMenu,
                        onDismissRequest = { showSortMenu = false }
                    ) {
                        serverSortOptions(s).forEach { (value, label) ->
                            val checked = sort == value
                            DropdownMenuItem(
                                text = { Text(label, color = MaterialTheme.colorScheme.onSurface) },
                                trailingIcon = if (checked) {
                                    {
                                        Icon(
                                            Icons.Filled.Check,
                                            contentDescription = null,
                                            tint = MaterialTheme.colorScheme.primary,
                                            modifier = Modifier.size(18.dp)
                                        )
                                    }
                                } else null,
                                onClick = {
                                    sortKey = value.name
                                    showSortMenu = false
                                }
                            )
                        }
                    }
                }
                Spacer(Modifier.width(6.dp))
                AddEntryButton(
                    onImportClipboard = onImportClipboard,
                    onImportQr = scanQr,
                    onImportFile = onImportFile,
                    onAddNode = onAddNode,
                    onAddSubscription = { showAddSubscription = true }
                )
                }
            )
        }

        // Group / protocol chip rails were replaced by the compact control row
        // below the search field, so the screen starts with the search box.
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
            // Subscription/server group comes first, protocol filter second. Both
            // controls always display their current selection, including "All".
            Box(modifier = Modifier.weight(1.15f)) {
                LumenFilterChip(
                    label = groupLabelOf(group),
                    selected = true,
                    badge = groupNodes.size.toString(),
                    modifier = Modifier.fillMaxWidth(),
                    onClick = { showGroupMenu = true }
                )
                LumenMenu(
                    expanded = showGroupMenu,
                    onDismissRequest = { showGroupMenu = false }
                ) {
                    groups.forEach { id ->
                        val label = groupLabelOf(id)
                        val count = nodes.count { nodeInGroup(it, id) }
                        DropdownMenuItem(
                            text = { Text("$label  $count", color = MaterialTheme.colorScheme.onSurface) },
                            trailingIcon = if (group == id) {
                                { Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) }
                            } else null,
                            onClick = {
                                group = id
                                protocol = PROTOCOL_ALL
                                showGroupMenu = false
                            }
                        )
                    }
                    DropdownMenuItem(
                        text = { Text(s.newGroup, color = MaterialTheme.colorScheme.primary) },
                        trailingIcon = {
                            Icon(
                                Icons.Filled.CreateNewFolder,
                                contentDescription = null,
                                tint = MaterialTheme.colorScheme.primary,
                                modifier = Modifier.size(18.dp)
                            )
                        },
                        onClick = {
                            showGroupMenu = false
                            showCreateGroup = true
                        }
                    )
                }
            }
            // Protocol filter stays on the right.
            // display their current selection, including the two "All" choices.
            Box(modifier = Modifier.weight(0.85f)) {
                val protocolLabel = if (protocol == PROTOCOL_ALL) {
                    s.allProtocols
                } else {
                    protocol.uppercase()
                }
                LumenFilterChip(
                    label = protocolLabel,
                    selected = protocol != PROTOCOL_ALL,
                    modifier = Modifier.fillMaxWidth(),
                    onClick = { showProtocolMenu = true }
                )
                LumenMenu(
                    expanded = showProtocolMenu,
                    onDismissRequest = { showProtocolMenu = false }
                ) {
                    DropdownMenuItem(
                        text = { Text(s.allProtocols, color = MaterialTheme.colorScheme.onSurface) },
                        trailingIcon = if (protocol == PROTOCOL_ALL) {
                            {
                                Icon(
                                    Icons.Filled.Check,
                                    contentDescription = null,
                                    tint = MaterialTheme.colorScheme.primary,
                                    modifier = Modifier.size(18.dp)
                                )
                            }
                        } else null,
                        onClick = {
                            protocol = PROTOCOL_ALL
                            showProtocolMenu = false
                        }
                    )
                    protocolCounts.forEach { (proto, count) ->
                        DropdownMenuItem(
                            text = {
                                Text(
                                    "${proto.uppercase()}  $count",
                                    color = MaterialTheme.colorScheme.onSurface
                                )
                            },
                            trailingIcon = if (protocol == proto) {
                                {
                                    Icon(
                                        Icons.Filled.Check,
                                        contentDescription = null,
                                        tint = MaterialTheme.colorScheme.primary,
                                        modifier = Modifier.size(18.dp)
                                    )
                                }
                            } else null,
                            onClick = {
                                protocol = proto
                                showProtocolMenu = false
                            }
                        )
                    }
                }
            }
            // Country grouping and the reachability/latency filters live behind this menu.
            Box {
                IconButton(
                    onClick = {
                        tick(HapticFeedbackType.LongPress)
                        showFiltersMenu = true
                    },
                    modifier = Modifier.size(38.dp)
                ) {
                    Icon(
                        imageVector = Icons.Filled.FilterList,
                        contentDescription = s.filtersLabel,
                        tint = if (activeFilterCount > 0) {
                            MaterialTheme.colorScheme.primary
                        } else {
                            MaterialTheme.colorScheme.onSurfaceVariant
                        }
                    )
                }
                LumenMenu(
                    expanded = showFiltersMenu,
                    onDismissRequest = { showFiltersMenu = false }
                ) {
                    DropdownMenuItem(
                        text = { Text(s.groupByCountry, color = MaterialTheme.colorScheme.onSurface) },
                        trailingIcon = if (groupByCountry) {
                            { Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) }
                        } else null,
                        onClick = { groupByCountryPref = if (groupByCountry) "0" else "1" }
                    )
                    DropdownMenuItem(
                        text = { Text(s.onlyReachable, color = MaterialTheme.colorScheme.onSurface) },
                        trailingIcon = if (onlyReachable) {
                            { Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) }
                        } else null,
                        onClick = { onlyReachablePref = if (onlyReachable) "0" else "1" }
                    )
                    MAX_PING_CHOICES.forEach { choice ->
                        val label = if (choice == 0) {
                            "${s.maxPingLabel}: ${s.offLabel}"
                        } else {
                            "${s.maxPingLabel}: $choice ms"
                        }
                        DropdownMenuItem(
                            text = { Text(label, color = MaterialTheme.colorScheme.onSurface) },
                            trailingIcon = if (maxPing == choice) {
                                { Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) }
                            } else null,
                            onClick = { maxPingPref = choice.toString() }
                        )
                    }
                    DropdownMenuItem(
                        text = { Text(s.resetFilters, color = MaterialTheme.colorScheme.primary) },
                        onClick = {
                            groupByCountryPref = "0"
                            onlyReachablePref = "0"
                            maxPingPref = "0"
                            protocol = PROTOCOL_ALL
                            showFiltersMenu = false
                        }
                    )
                }
            }
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
            IconButton(
                onClick = {
                    tick(HapticFeedbackType.LongPress)
                    if (isPinging) onStopPing() else onPingAll()
                },
                modifier = Modifier.size(38.dp)
            ) {
                if (isPinging) {
                    Icon(
                        imageVector = Icons.Filled.Stop,
                        contentDescription = s.cancel,
                        tint = MaterialTheme.colorScheme.error
                    )
                } else {
                    Icon(
                        imageVector = Icons.Filled.NetworkCheck,
                        contentDescription = s.pingAll,
                        tint = MaterialTheme.colorScheme.primary
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
                        onShowProperties = { sub -> propertiesSub = sub },
                        onEditSubscription = { sub -> editingSubscription = sub },
                        onExportAll = { subId ->
                            val text = onExportSubscriptionText(subId)
                            if (text.isNotBlank()) onShareText(text)
                        }
                    )
                }
                if (groupExpanded) {
                    item(key = "sub_info_" + selectedSubscription.id) {
                        // No rounding and no gap: the rows below continue the same tile.
                        // The bar itself no longer draws the announcement; the provider
                        // card below lays it out where it can wrap over several lines.
                        SubscriptionInfoBar(
                            sub = selectedSubscription,
                            pullUp = 0.dp,
                            roundedBottom = false
                        )
                    }
                    item(key = "sub_provider_" + selectedSubscription.id) {
                        // Renders nothing when the provider sent no announcement, banner
                        // or links, so a plain subscription keeps the card it always had.
                        SubscriptionProviderCard(
                            sub = selectedSubscription,
                            onOpenUrl = onOpenUrl
                        )
                    }
                }
            } else if (selectedCustomGroup != null) {
                // Same header as a subscription, minus the traffic block: ping, export,
                // rename and delete for a group the user made.
                item(key = "custom_head_" + selectedCustomGroup.id) {
                    SubscriptionHeaderTile(
                        group = HomeServerGroup(
                            id = selectedCustomGroup.id,
                            title = selectedCustomGroup.name,
                            nodes = groupNodes,
                            isCustom = true
                        ),
                        isExpanded = groupExpanded,
                        onToggleExpand = { groupExpanded = !groupExpanded },
                        onRefreshSubscription = {},
                        onDeleteSubscription = {},
                        onPingGroup = { if (groupNodes.isNotEmpty()) onPingNodes(groupNodes) },
                        onExportAll = {
                            val text = onExportNodesText(groupNodes.mapTo(mutableSetOf()) { it.id })
                            if (text.isNotBlank()) onShareText(text)
                        },
                        onRenameGroup = { renamingGroup = selectedCustomGroup },
                        onDeleteGroup = { deletingGroup = selectedCustomGroup }
                    )
                }
            }
            val showRows = (selectedSubscription == null && selectedCustomGroup == null) || groupExpanded
            if (filtered.isEmpty() && showRows) {
                item(key = "empty") {
                    EmptyState(text = if (groupNodes.isEmpty()) s.noServers else s.nothingFound)
                }
            } else if (showRows) {
                // rowItems is either the plain node list or nodes interleaved with
                // country headers when "group by country" is enabled.
                itemsIndexed(
                    rowItems,
                    key = { _, entry ->
                        when (entry) {
                            is NodeUiModel -> entry.id
                            is CountrySection -> "country_" + entry.code
                            else -> entry.hashCode()
                        }
                    }
                ) { index, entry ->
                    when (entry) {
                        is CountrySection -> CountrySectionHeader(
                            section = entry,
                            unknownLabel = s.otherLabel
                        )
                        is NodeUiModel -> {
                            // The same continuous rail is used for All servers,
                            // manual servers and subscriptions, matching every
                            // dashboard style.
                            val isFirst = entry.id == firstVisibleNodeId
                            val isLast = index == rowItems.lastIndex
                            // A subscription and a custom group both put a header tile
                            // above the rail, so the rows continue it instead of opening
                            // their own rounded card.
                            val hasGroupHeader =
                                selectedSubscription != null || selectedCustomGroup != null
                            val railShape = when {
                                hasGroupHeader && isLast ->
                                    RoundedCornerShape(bottomStart = 18.dp, bottomEnd = 18.dp)
                                hasGroupHeader -> RoundedCornerShape(0.dp)
                                isFirst && entry.id == lastVisibleNodeId -> RoundedCornerShape(18.dp)
                                isFirst -> RoundedCornerShape(topStart = 18.dp, topEnd = 18.dp)
                                entry.id == lastVisibleNodeId ->
                                    RoundedCornerShape(bottomStart = 18.dp, bottomEnd = 18.dp)
                                else -> RoundedCornerShape(0.dp)
                            }
                            Box(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clip(railShape)
                                    .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.38f))
                                    .padding(
                                        start = 8.dp,
                                        end = 8.dp,
                                        top = if (isFirst && !hasGroupHeader) 10.dp else 4.dp,
                                        bottom = if (entry.id == lastVisibleNodeId) 10.dp else 4.dp
                                    )
                            ) {
                                ServerTileRow(
                                    node = entry,
                                    isSelectionMode = isSelectionMode,
                                    isNodeSelected = entry.id in selectedNodeIds,
                                    isPinging = testingNodeId == entry.id || entry.id in pingingNodeIds,
                                    supportingText = serverTestResults[entry.id],
                                    onClick = {
                                        tick(HapticFeedbackType.LongPress)
                                        if (isSelectionMode) {
                                            selectedNodeIds = if (entry.id in selectedNodeIds) {
                                                selectedNodeIds - entry.id
                                            } else {
                                                selectedNodeIds + entry.id
                                            }
                                            if (selectedNodeIds.isEmpty()) isSelectionMode = false
                                        } else {
                                            onSelectNode(entry)
                                        }
                                    },
                                    onLongClick = {
                                        tick(HapticFeedbackType.LongPress)
                                        isSelectionMode = true
                                        selectedNodeIds = selectedNodeIds + entry.id
                                    },
                                    onEditNode = { onEditNode(entry) },
                                    onDeleteNode = { onDeleteNode(entry) },
                                    onPingNode = { onPingNode(entry) },
                                    onCopyLink = { onCopyNodeLink(entry) },
                                    onExportQr = { onExportQrCode(entry) },
                                    onMoveToGroup = { assigningNodes = listOf(entry) }
                                )
                            }
                        }
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

    if (showCreateGroup) {
        GroupNameDialog(
            title = s.newGroup,
            confirmText = s.createAction,
            initialName = "",
            onDismiss = {
                showCreateGroup = false
                pendingAssignment = emptyList()
            },
            onConfirm = { name ->
                showCreateGroup = false
                // Same normalisation the view model applies, so the new group is
                // recognised when it comes back.
                pendingGroupName = name.trim().take(64)
                onCreateGroup(name)
            }
        )
    }

    renamingGroup?.let { target ->
        GroupNameDialog(
            title = s.renameGroup,
            confirmText = s.saveAction,
            initialName = target.name,
            onDismiss = { renamingGroup = null },
            onConfirm = { name ->
                renamingGroup = null
                onRenameGroup(target.id, name)
            }
        )
    }

    deletingGroup?.let { target ->
        LumenDialog(
            title = "${s.deleteGroup}: ${target.name}",
            // Spelled out because the wording is the whole point: the servers stay.
            message = s.deleteGroupConfirm,
            onDismissRequest = { deletingGroup = null },
            confirmText = s.delete,
            destructive = true,
            onConfirm = {
                deletingGroup = null
                if (group == target.id) group = GROUP_ALL
                onDeleteGroup(target.id)
            },
            dismissText = s.cancel,
            onDismiss = { deletingGroup = null }
        )
    }

    if (assigningNodes.isNotEmpty()) {
        val targets = assigningNodes
        GroupPickerDialog(
            groups = serverGroups,
            // Only meaningful for a single server; a mixed selection shows no tick.
            currentGroupId = targets.singleOrNull()?.groupId,
            onDismiss = { assigningNodes = emptyList() },
            onCreateGroup = {
                assigningNodes = emptyList()
                // Carried through the creation so the servers land in the new group.
                pendingAssignment = targets
                isSelectionMode = false
                selectedNodeIds = emptySet()
                showCreateGroup = true
            },
            onPick = { groupId ->
                assigningNodes = emptyList()
                isSelectionMode = false
                selectedNodeIds = emptySet()
                onAssignNodesToGroup(targets, groupId)
            }
        )
    }

    // "Subscription properties" from the header menu; the URL row is left out when the
    // provider asked for it with hide-url.
    editingSubscription?.let { sub ->
        EditSubscriptionDialog(
            subscription = sub,
            onDismiss = { editingSubscription = null },
            onConfirm = { name, url ->
                onUpdateSubscription(sub, name, url).also { saved ->
                    if (saved) editingSubscription = null
                }
            }
        )
    }

    propertiesSub?.let { sub ->
        SubscriptionDetailsDialog(
            sub = sub,
            onDismiss = { propertiesSub = null },
            onCopyUrl = onCopyText,
            onOpenUrl = onOpenUrl
        )
    }
}

/** Subscription groups own both a display name and a source URL. */
@Composable
private fun EditSubscriptionDialog(
    subscription: SubscriptionUiModel,
    onDismiss: () -> Unit,
    onConfirm: (String, String) -> Boolean
) {
    val s = LocalStrings.current
    var name by remember(subscription.id) { mutableStateOf(subscription.name) }
    var url by remember(subscription.id) { mutableStateOf(subscription.url) }
    var invalid by remember(subscription.id) { mutableStateOf(false) }
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
                    text = "${s.edit}: ${subscription.name}",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis
                )
                Spacer(Modifier.height(14.dp))
                OutlinedTextField(
                    value = name,
                    onValueChange = {
                        name = it
                        invalid = false
                    },
                    label = { Text(s.groupNameLabel) },
                    singleLine = true,
                    isError = invalid && name.isBlank(),
                    shape = RoundedCornerShape(14.dp),
                    modifier = Modifier.fillMaxWidth()
                )
                Spacer(Modifier.height(10.dp))
                OutlinedTextField(
                    value = url,
                    onValueChange = {
                        url = it
                        invalid = false
                    },
                    label = { Text(s.url) },
                    singleLine = true,
                    isError = invalid,
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
                        onClick = {
                            invalid = !onConfirm(name.trim(), url.trim())
                        },
                        enabled = name.isNotBlank() && url.isNotBlank()
                    ) { Text(s.saveAction) }
                }
            }
        }
    }
}

/** Create / rename dialog for a custom group. */
@Composable
private fun GroupNameDialog(
    title: String,
    confirmText: String,
    initialName: String,
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit
) {
    val s = LocalStrings.current
    var name by remember(initialName) { mutableStateOf(initialName) }
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
                    text = title,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.onSurface
                )
                Spacer(Modifier.height(14.dp))
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text(s.groupNameLabel) },
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
                        onClick = { onConfirm(name.trim()) },
                        enabled = name.isNotBlank()
                    ) { Text(confirmText) }
                }
            }
        }
    }
}

/**
 * Group chooser for one server or for the whole selection. "No group" clears the
 * assignment, which returns the servers to Default, or to their subscription when
 * they came from one.
 */
@Composable
private fun GroupPickerDialog(
    groups: List<ServerGroupUiModel>,
    currentGroupId: String?,
    onDismiss: () -> Unit,
    onCreateGroup: () -> Unit,
    onPick: (String?) -> Unit
) {
    val s = LocalStrings.current
    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = RoundedCornerShape(22.dp),
            color = MaterialTheme.colorScheme.surface
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(vertical = 18.dp)
            ) {
                Text(
                    text = s.moveToGroup,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier.padding(horizontal = 20.dp)
                )
                Spacer(Modifier.height(10.dp))
                DropdownMenuItem(
                    text = { Text(s.groupNoGroup, color = MaterialTheme.colorScheme.onSurface) },
                    trailingIcon = if (currentGroupId == null) {
                        { Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) }
                    } else null,
                    onClick = { onPick(null) }
                )
                groups.forEach { entry ->
                    DropdownMenuItem(
                        text = { Text(entry.name, color = MaterialTheme.colorScheme.onSurface) },
                        trailingIcon = if (currentGroupId == entry.id) {
                            { Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp)) }
                        } else null,
                        onClick = { onPick(entry.id) }
                    )
                }
                DropdownMenuItem(
                    text = { Text(s.newGroup, color = MaterialTheme.colorScheme.primary) },
                    trailingIcon = {
                        Icon(
                            Icons.Filled.CreateNewFolder,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.size(18.dp)
                        )
                    },
                    onClick = onCreateGroup
                )
                Spacer(Modifier.height(6.dp))
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 20.dp),
                    horizontalArrangement = Arrangement.End
                ) {
                    TextButton(onClick = onDismiss) { Text(s.cancel) }
                }
            }
        }
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
            // Every entry here starts adding a server, so each one confirms with a tick.
            DropdownMenuItem(
                text = { Text(s.subscriptionLink) },
                leadingIcon = { Icon(Icons.Filled.Link, contentDescription = null) },
                onClick = { expanded = false; tick(HapticFeedbackType.LongPress); onAddSubscription() }
            )
            DropdownMenuItem(
                text = { Text(s.importFromClipboard) },
                leadingIcon = { Icon(Icons.Filled.ContentPaste, contentDescription = null) },
                onClick = { expanded = false; tick(HapticFeedbackType.LongPress); onImportClipboard() }
            )
            DropdownMenuItem(
                text = { Text(s.importQrCode) },
                leadingIcon = { Icon(Icons.Filled.QrCodeScanner, contentDescription = null) },
                onClick = { expanded = false; tick(HapticFeedbackType.LongPress); onImportQr() }
            )
            DropdownMenuItem(
                text = { Text(s.importFromFile) },
                leadingIcon = { Icon(Icons.Filled.UploadFile, contentDescription = null) },
                onClick = { expanded = false; tick(HapticFeedbackType.LongPress); onImportFile() }
            )
            DropdownMenuItem(
                text = { Text(s.importManually) },
                leadingIcon = { Icon(Icons.Filled.Edit, contentDescription = null) },
                onClick = { expanded = false; tick(HapticFeedbackType.LongPress); onAddNode() }
            )
        }
    }
}

/** Header of a country bucket: flag, location name, server count and best ping. */
@Composable
private fun CountrySectionHeader(section: CountrySection, unknownLabel: String) {
    val title = when {
        section.code.isBlank() -> unknownLabel
        else -> CountryFlagHelper.countryDisplayName(section.code).ifBlank { section.code }
    }
    val flag = if (section.code.length == 2) CountryFlagHelper.getFlagEmoji(section.code) else ""
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 12.dp, bottom = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp)
    ) {
        if (flag.isNotBlank()) {
            Text(text = flag, style = MaterialTheme.typography.bodyMedium)
        }
        Text(
            text = title,
            style = MaterialTheme.typography.labelLarge,
            fontWeight = FontWeight.Bold,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
        Text(
            text = section.count.toString(),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
        Spacer(Modifier.weight(1f))
        val best = section.bestPing
        if (best != null) {
            Text(
                text = "$best ms",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.primary
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
