package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.CheckBox
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextFieldDefaults
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
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lumen.ui.components.AddServerSheet
import com.lumen.ui.components.BrandTopBar
import com.lumen.ui.components.ConnectionState
import com.lumen.ui.components.CountryFlagHelper
import com.lumen.ui.components.LumenDialog
import com.lumen.ui.components.SubscriptionDetailsDialog
import com.lumen.ui.theme.LocalLumenColors
import com.lumen.ui.theme.TileBlueDark
import com.lumen.ui.theme.TileBlueDarkOn
import com.lumen.ui.theme.TileBlueLight
import com.lumen.ui.theme.TileBlueLightOn

enum class ServerSort { PING, NAME, NAME_DESC, COUNTRY, PROTOCOL }

const val SERVERS_SORT_PREF = "servers_last_sort"

fun serverSortOptions(s: LumenStrings): List<Pair<ServerSort, String>> = listOf(
    ServerSort.PING to s.sortByPing,
    ServerSort.NAME to s.sortByName,
    ServerSort.NAME_DESC to s.sortByNameDesc,
    ServerSort.COUNTRY to s.sortByCountry,
    ServerSort.PROTOCOL to s.sortByProtocol
)

fun serverSortLabel(sort: ServerSort, s: LumenStrings): String =
    serverSortOptions(s).firstOrNull { it.first == sort }?.second ?: s.sortByPing

fun serverSortOf(key: String): ServerSort =
    runCatching { ServerSort.valueOf(key) }.getOrDefault(ServerSort.PING)

fun serverSortComparator(sort: ServerSort): Comparator<NodeUiModel> = when (sort) {
    ServerSort.NAME -> compareBy(String.CASE_INSENSITIVE_ORDER) { it.name }
    ServerSort.NAME_DESC -> compareByDescending(String.CASE_INSENSITIVE_ORDER) { it.name }
    ServerSort.PING -> compareBy<NodeUiModel> { it.pingMs?.takeIf { ping -> ping > 0 } ?: Int.MAX_VALUE }
        .thenBy(String.CASE_INSENSITIVE_ORDER) { it.name }.thenBy { it.id }
    ServerSort.COUNTRY -> compareBy<NodeUiModel> { countryCodeOf(it).ifEmpty { "zzz" } }
        .thenBy { it.name.lowercase() }
    ServerSort.PROTOCOL -> compareBy<NodeUiModel> { protocolKey(it) }
        .thenBy { it.name.lowercase() }
}

internal fun protocolKey(node: NodeUiModel): String =
    node.displayProtocol.substringBefore('/').trim().lowercase()
        .ifEmpty { node.protocol.trim().lowercase() }

internal fun countryCodeOf(node: NodeUiModel): String =
    node.countryCode.trim().ifEmpty { CountryFlagHelper.detectCountryStrict(node.name, node.server) }
        .uppercase()

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ServerListScreen(
    nodes: List<NodeUiModel>,
    subscriptions: List<SubscriptionUiModel>,
    serverGroups: List<ServerGroupUiModel> = emptyList(),
    refreshingIds: Set<String>,
    isPinging: Boolean,
    pingingNodeIds: Set<String> = emptySet(),
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
    onUpdateSubscription: (SubscriptionUiModel, String, String) -> Boolean = { _, _, _ -> false },
    onRefreshSubscription: (SubscriptionUiModel) -> Unit,
    onDeleteSubscription: (SubscriptionUiModel) -> Unit,
    onImportFile: () -> Unit = {},
    onImportQr: () -> Unit = {},
    onPingGroup: (String?) -> Unit = {},
    onCreateGroup: (String) -> Unit = {},
    onRenameGroup: (String, String) -> Unit = { _, _ -> },
    onDeleteGroup: (String) -> Unit = {},
    onAssignNodesToGroup: (List<NodeUiModel>, String?) -> Unit = { _, _ -> },
    onExportNodesText: (Set<String>) -> String = { "" },
    onExportSubscriptionText: (String?) -> String = { "" },
    onShareText: (String) -> Unit = {},
    onCopyText: (String) -> Unit = {},
    onOpenUrl: (String) -> Unit = {},
    onOpen: () -> Unit = {},
    modifier: Modifier = Modifier,
    persistPreferences: Boolean = true,
    compactGrid: Boolean = false,
    selectedGroupFilter: String? = null,
    onSelectedGroupFilter: (String) -> Unit = {}
) {
    val lumenColors = LocalLumenColors.current
    val art = com.lumen.ui.theme.expressivePalette()
    val strings = LocalStrings.current
    val scanQr = rememberQrScanRequest(onImportQr)

    var searchQuery by remember { mutableStateOf("") }
    var localFilter by remember { mutableStateOf("all") }
    val selectedFilter = resolvedServerFilter(selectedGroupFilter ?: localFilter, subscriptions, serverGroups)
    val chooseFilter: (String) -> Unit = { localFilter = it; onSelectedGroupFilter(it) }
    var sortKey by if (persistPreferences) rememberUiPreference(SERVERS_SORT_PREF, ServerSort.PING.name)
        else remember { mutableStateOf(ServerSort.PING.name) }
    val currentSort = serverSortOf(sortKey)
    var showSortMenu by remember { mutableStateOf(false) }
    var showAddSheet by remember { mutableStateOf(false) }
    var editingSubscription by remember { mutableStateOf<SubscriptionUiModel?>(null) }
    var deletingSubscription by remember { mutableStateOf<SubscriptionUiModel?>(null) }
    var detailsSubscription by remember { mutableStateOf<SubscriptionUiModel?>(null) }
    var deletingNode by remember { mutableStateOf<NodeUiModel?>(null) }
    var showCreateGroup by remember { mutableStateOf(false) }
    var showGroupMenu by remember { mutableStateOf(false) }
    var renamingGroup by remember { mutableStateOf<ServerGroupUiModel?>(null) }
    var deletingGroup by remember { mutableStateOf<ServerGroupUiModel?>(null) }
    var movingNodes by remember { mutableStateOf<List<NodeUiModel>?>(null) }
    var selectedIds by remember { mutableStateOf(setOf<String>()) }
    var pendingSelectedId by remember { mutableStateOf<String?>(null) }
    val selectServer: (NodeUiModel) -> Unit = { node -> pendingSelectedId = node.id; onSelectNode(node) }
    var showDeleteAll by remember { mutableStateOf(false) }
    var showSubscriptionMenu by remember { mutableStateOf(false) }
    var showFilterMenu by remember { mutableStateOf(false) }
    var protocol by remember { mutableStateOf("") }
    var reachablePref by if (persistPreferences) rememberUiPreference("servers_filter_reachable", "0") else remember { mutableStateOf("0") }
    val onlyReachable = reachablePref == "1"
    var countryPref by if (persistPreferences) rememberUiPreference("servers_group_by_country", "0") else remember { mutableStateOf("0") }
    var maxPingPref by if (persistPreferences) rememberUiPreference("servers_filter_max_ping", "0") else remember { mutableStateOf("0") }
    val maxPing = maxPingPref.toIntOrNull()?.coerceAtLeast(0) ?: 0
    val groupByCountry = countryPref == "1"
    var autoPingStarted by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { onOpen() }
    LaunchedEffect(autoPingOnOpen, nodes.isNotEmpty()) {
        if (autoPingOnOpen && nodes.isNotEmpty() && !autoPingStarted) {
            autoPingStarted = true
            onPingAll()
        }
    }
    BackHandler(enabled = selectedIds.isNotEmpty()) { selectedIds = emptySet() }

    val query = ServerListQuery(selectedFilter, searchQuery, protocol, onlyReachable, currentSort, maxPing, groupByCountry)
    val prepared by rememberPreparedServerList(nodes, query)
    val preparing = prepared.source !== nodes || prepared.query != query
    val manualNodesCount = prepared.manualCount
    // Keep rows during a ping refresh; never display another group's rows after switching groups.
    val filteredNodes = if (prepared.query.group == selectedFilter) prepared.visible else emptyList()
    LaunchedEffect(prepared.source, pendingSelectedId) {
        if (pendingSelectedId != null && (prepared.source.any { it.id == pendingSelectedId && it.isSelected } ||
                (prepared.source === nodes && nodes.none { it.id == pendingSelectedId }))) pendingSelectedId = null
    }

    val activeSub = subscriptionForServerFilter(selectedFilter, subscriptions)

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .statusBarsPadding()
    ) {
        // Top Bar
        BrandTopBar(onAddClick = { showAddSheet = true }, title = strings.servers)

        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 16.dp)
        ) {
            if (selectedIds.isNotEmpty()) {
                Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                    horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("${strings.selected}: ${selectedIds.size}", style = MaterialTheme.typography.labelLarge)
                    TextButton(onClick = { onPingNodes(nodes.filter { it.id in selectedIds }) }) { Text(strings.ping) }
                    TextButton(onClick = {
                        onExportNodesText(selectedIds).takeIf { it.isNotBlank() }?.let(onShareText)
                    }) { Text(strings.exportSelected) }
                    TextButton(onClick = { movingNodes = nodes.filter { it.id in selectedIds } }) { Text(strings.moveToGroup) }
                    TextButton(onClick = { selectedIds = emptySet() }) { Text(strings.cancel) }
                }
            }
            // 1. Search Bar
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(vertical = 4.dp)
            ) {
                ExpressiveTextField(
                    value = searchQuery,
                    onValueChange = { searchQuery = it },
                    placeholder = {
                        Text(
                            text = strings.searchServers,
                            fontSize = 14.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    },
                    leadingIcon = {
                        Icon(
                            imageVector = Icons.Default.Search,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.size(20.dp)
                        )
                    },
                    trailingIcon = {
                        if (searchQuery.isNotEmpty()) {
                            IconButton(onClick = { searchQuery = "" }) {
                                Icon(
                                    imageVector = Icons.Default.Close,
                                    contentDescription = strings.clear,
                                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                                    modifier = Modifier.size(18.dp)
                                )
                            }
                        }
                    },
                    singleLine = true,
                    shape = RoundedCornerShape(28.dp),
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedContainerColor = lumenColors.raised,
                        unfocusedContainerColor = lumenColors.raised,
                        disabledContainerColor = lumenColors.raised,
                        focusedBorderColor = Color.Transparent,
                        unfocusedBorderColor = Color.Transparent
                    ),
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(50.dp)
                )
            }

            val currentGroup = serverGroups.firstOrNull { selectedFilter == "group:${it.id}" }
            ServerFilterBar(selectedFilter,
                serverFilterChoices(nodes, serverGroups, subscriptions, strings),
                chooseFilter, protocol, prepared.protocols, { protocol = it },
                groupByCountry, { countryPref = if (it) "1" else "0" },
                onlyReachable, { reachablePref = if (it) "1" else "0" }, maxPing, { maxPingPref = it.toString() },
                onReset = { protocol = ""; reachablePref = "0"; countryPref = "0"; maxPingPref = "0" },
                onNewGroup = { showCreateGroup = true },
                onRenameGroup = currentGroup?.let { group -> { renamingGroup = group } },
                onDeleteGroup = currentGroup?.let { group -> { deletingGroup = group } },
                onDeleteManual = if (selectedFilter == "manual" && manualNodesCount > 0) ({ showDeleteAll = true }) else null)

            // 5. Server List
            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(bottom = 100.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp)
            ) {
                item(key = "server_overview", contentType = "overview") {
            // 3. Subscription Provider Banner (if present and filter matches)
            if (activeSub != null && selectedFilter != "manual") {
                val isDark = MaterialTheme.colorScheme.background.luminance() < 0.5f
                val bannerBg = art.blue
                val bannerText = art.onBlue
                val btnBg = art.onLavender.copy(alpha = .1f)

                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(vertical = 6.dp)
                        .clip(RoundedCornerShape(topStart = 30.dp, topEnd = 30.dp, bottomEnd = 12.dp, bottomStart = 30.dp))
                        .background(bannerBg)
                        .padding(horizontal = 14.dp, vertical = 10.dp)
                ) {
                    com.lumen.ui.components.ExpressiveMotif(com.lumen.ui.components.Motif.Fan, bannerText.copy(alpha = .08f),
                        Modifier.align(Alignment.BottomEnd).size(140.dp))
                    Column {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Column(Modifier.weight(1f)) {
                                Text(activeSub.name, maxLines = 1, overflow = TextOverflow.Ellipsis,
                                    style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold, color = bannerText)
                                Text(strings.serverCountLabel(activeSub.nodeCount),
                                    style = MaterialTheme.typography.bodySmall, color = bannerText.copy(alpha = .7f))
                            }
                            Row(verticalAlignment = Alignment.CenterVertically) {
                            Box(
                                modifier = Modifier
                                    .size(48.dp)
                                    .clip(com.lumen.ui.components.expressiveShape(com.lumen.ui.components.Motif.Clover))
                                    .background(btnBg)
                                    .clickable { onRefreshSubscription(activeSub) },
                                contentAlignment = Alignment.Center
                            ) {
                                if (refreshingIds.contains(activeSub.id)) {
                                    CircularProgressIndicator(
                                        modifier = Modifier.size(16.dp),
                                        strokeWidth = 2.dp,
                                        color = bannerText
                                    )
                                } else {
                                    Icon(
                                        imageVector = Icons.Default.Refresh,
                                        contentDescription = strings.refreshSubscription,
                                        tint = bannerText,
                                        modifier = Modifier.size(18.dp)
                                    )
                                }
                            }
                            Box {
                                IconButton(onClick = { showSubscriptionMenu = true }) {
                                    Icon(Icons.Default.MoreVert, contentDescription = activeSub.name, tint = bannerText)
                                }
                                LumenMenu(expanded = showSubscriptionMenu, onDismissRequest = { showSubscriptionMenu = false }) {
                                    DropdownMenuItem(text = { Text(strings.subscriptionProperties) }, onClick = {
                                        showSubscriptionMenu = false; detailsSubscription = activeSub
                                    })
                                    DropdownMenuItem(text = { Text(strings.edit) }, onClick = {
                                        showSubscriptionMenu = false; editingSubscription = activeSub
                                    })
                                    DropdownMenuItem(text = { Text(strings.exportAll) }, onClick = {
                                        showSubscriptionMenu = false
                                        onExportSubscriptionText(activeSub.id).takeIf { it.isNotBlank() }?.let(onShareText)
                                    })
                                    DropdownMenuItem(text = { Text(strings.delete) }, onClick = {
                                        showSubscriptionMenu = false; deletingSubscription = activeSub
                                    })
                                }
                            }
                            }
                        }

                        activeSub.trafficRatio?.let { trafficRatio ->
                        Spacer(modifier = Modifier.height(8.dp))
                        val ratio = trafficRatio.coerceIn(0f, 1f)
                        Box(
                            modifier = Modifier
                                .fillMaxWidth()
                                .height(6.dp)
                                .clip(RoundedCornerShape(8.dp))
                                .background(MaterialTheme.colorScheme.background)
                        ) {
                            Box(
                                modifier = Modifier
                                    .fillMaxWidth(ratio)
                                    .height(6.dp)
                                    .clip(RoundedCornerShape(8.dp))
                                    .background(MaterialTheme.colorScheme.primary)
                            )
                        }
                        }

                        Spacer(modifier = Modifier.height(6.dp))

                        val trafficText = activeSub.trafficSummary ?: "—"
                        val daysText = activeSub.expiryDaysLeft?.let { "$it ${strings.daysRemaining}" }

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween
                        ) {
                            Text(
                                text = trafficText,
                                fontSize = 11.sp,
                                color = bannerText.copy(alpha = 0.8f)
                            )
                            if (daysText != null) Text(
                                text = daysText,
                                fontSize = 11.sp,
                                color = bannerText.copy(alpha = 0.8f)
                            )
                        }
                        if (com.lumen.ui.components.hasProviderCard(activeSub)) {
                            com.lumen.ui.components.SubscriptionProviderCard(activeSub, onOpenUrl,
                                contentColor = bannerText)
                        }
                    }
                }
            }

            // 4. List Toolbar: "Сначала быстрые" + "Проверить пинг"
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 4.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Box {
                    Row(
                        modifier = Modifier
                            .height(48.dp)
                            .clip(RoundedCornerShape(16.dp))
                            .clickable { showSortMenu = true }
                            .padding(horizontal = 8.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            text = serverSortLabel(currentSort, strings),
                            fontSize = 12.sp,
                            fontWeight = FontWeight.Medium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                        Icon(
                            Icons.Filled.KeyboardArrowDown,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                            tint = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }

                    LumenMenu(
                        expanded = showSortMenu,
                        onDismissRequest = { showSortMenu = false }
                    ) {
                        serverSortOptions(strings).forEach { (sort, label) ->
                            DropdownMenuItem(text = { Text(label) }, onClick = {
                                sortKey = sort.name
                                showSortMenu = false
                            })
                        }
                    }
                }

                DashboardPingAllButton(isPinging, filteredNodes.isNotEmpty(), { onPingNodes(filteredNodes) }, onStopPing)
            }

                }
                if (preparing && filteredNodes.isEmpty()) item(key = "preparing_servers", contentType = "progress") {
                    androidx.compose.material3.LinearProgressIndicator(Modifier.fillMaxWidth())
                }
                if (filteredNodes.isEmpty() && !preparing) {
                    item(key = "empty_servers", contentType = "empty") {
                        Column(
                            modifier = Modifier.fillMaxWidth()
                                .clip(RoundedCornerShape(24.dp))
                                .background(MaterialTheme.colorScheme.surface)
                                .padding(horizontal = 20.dp, vertical = 24.dp),
                            horizontalAlignment = Alignment.CenterHorizontally
                        ) {
                            Icon(Icons.Default.Search, contentDescription = null,
                                tint = MaterialTheme.colorScheme.primary,
                                modifier = Modifier.size(28.dp))
                            Spacer(Modifier.height(12.dp))
                            Text(if (nodes.isEmpty()) strings.noServers else strings.nothingFound,
                                style = MaterialTheme.typography.bodyMedium,
                                textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                                color = MaterialTheme.colorScheme.onSurfaceVariant)
                            Spacer(Modifier.height(8.dp))
                            TextButton(onClick = {
                                if (nodes.isEmpty()) showAddSheet = true else {
                                    searchQuery = ""
                                    chooseFilter("all")
                                    protocol = ""
                                    reachablePref = "0"
                                    countryPref = "0"
                                    maxPingPref = "0"
                                }
                            }) { Text(if (nodes.isEmpty()) strings.add else strings.resetFilters) }
                        }
                    }
                }
                val sections = if (groupByCountry) filteredNodes.groupBy { countryCodeOf(it).ifBlank { "—" } }
                    else mapOf("" to filteredNodes)
                sections.forEach { (country, sectionNodes) ->
                    if (country.isNotBlank()) item(key = "country:$country", contentType = "country") {
                        Text("${CountryFlagHelper.getFlagEmoji(country)}  $country · ${sectionNodes.size}",
                            Modifier.padding(horizontal = 8.dp, vertical = 10.dp), style = MaterialTheme.typography.labelLarge)
                    }
                if (compactGrid) {
                    items(sectionNodes.chunked(3), key = { row -> "grid:" + row.joinToString { it.id } }, contentType = { "server_grid" }) { row ->
                        Row(Modifier.fillMaxWidth().animateItem(fadeInSpec = null, fadeOutSpec = null),
                            horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            row.forEach { node ->
                                CompactServerCard(if (pendingSelectedId != null) node.copy(isSelected = node.id == pendingSelectedId) else node,
                                    node.id in selectedIds, node.id in pingingNodeIds || testingNodeId == node.id,
                                    onClick = {
                                        if (selectedIds.isEmpty()) selectServer(node)
                                        else selectedIds = if (node.id in selectedIds) selectedIds - node.id else selectedIds + node.id
                                    }, onLongClick = { selectedIds = selectedIds + node.id },
                                    onEdit = { onEditNode(node) }, onPing = { onPingNode(node) },
                                    onCopy = { onCopyNodeLink(node) }, onDelete = { deletingNode = node }, modifier = Modifier.weight(1f))
                            }
                            repeat(3 - row.size) { Spacer(Modifier.weight(1f)) }
                        }
                    }
                } else items(sectionNodes, key = { it.id }, contentType = { "server" }) { node ->
                    ServerListRow(if (pendingSelectedId != null) node.copy(isSelected = node.id == pendingSelectedId) else node, node.id in selectedIds,
                        node.id in pingingNodeIds || testingNodeId == node.id,
                        serverTestResults[node.id],
                        onClick = { clicked ->
                            if (selectedIds.isEmpty()) selectServer(clicked)
                            else selectedIds = if (clicked.id in selectedIds) selectedIds - clicked.id else selectedIds + clicked.id
                        },
                        onLongClick = { selectedIds = selectedIds + it.id },
                        onEditNode = onEditNode, onPingNode = onPingNode,
                        onCopyNodeLink = onCopyNodeLink, onExportQrCode = onExportQrCode,
                        onMove = { movingNodes = listOf(it) }, onDeleteNode = { deletingNode = it },
                        modifier = Modifier.animateItem(fadeInSpec = null, fadeOutSpec = null))
                }

                }
                item(key = "server_footer", contentType = "spacer") {
                    Spacer(modifier = Modifier.height(16.dp))
                }
            }
        }
    }

    if (showAddSheet) {
        AddServerSheet(
            onDismissRequest = { showAddSheet = false },
            onImportClipboard = onImportClipboard,
            onImportQr = scanQr,
            onImportFile = onImportFile,
            onAddManualNode = onAddNode
        )
    }
    editingSubscription?.let { original ->
        var name by remember(original.id) { mutableStateOf(original.name) }
        var url by remember(original.id) { mutableStateOf(original.url) }
        LumenDialog(
            title = strings.edit,
            onDismissRequest = { editingSubscription = null },
            confirmText = strings.saveAction,
            onConfirm = {
                if (url.isNotBlank()) {
                    val saved = onUpdateSubscription(original, name.trim(), url.trim())
                    if (saved) { editingSubscription = null }
                }
            },
            dismissText = strings.cancel,
            onDismiss = { editingSubscription = null }
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                ExpressiveTextField(shape = MaterialTheme.shapes.large, value = name, onValueChange = { name = it },
                    label = { Text(strings.nameOptional) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                ExpressiveTextField(shape = MaterialTheme.shapes.large, value = url, onValueChange = { url = it },
                    label = { Text(strings.url) }, singleLine = true, modifier = Modifier.fillMaxWidth())
            }
        }
    }
    detailsSubscription?.let { sub ->
        SubscriptionDetailsDialog(sub, onDismiss = { detailsSubscription = null },
            onCopyUrl = onCopyText, onOpenUrl = onOpenUrl)
    }
    deletingSubscription?.let { sub ->
        LumenDialog(title = strings.deleteSubscription, message = sub.name,
            onDismissRequest = { deletingSubscription = null }, confirmText = strings.delete,
            onConfirm = { onDeleteSubscription(sub); deletingSubscription = null },
            dismissText = strings.cancel, onDismiss = { deletingSubscription = null }, destructive = true)
    }
    deletingNode?.let { node ->
        LumenDialog(title = strings.delete, message = node.name,
            onDismissRequest = { deletingNode = null }, confirmText = strings.delete,
            onConfirm = { onDeleteNode(node); deletingNode = null },
            dismissText = strings.cancel, onDismiss = { deletingNode = null }, destructive = true)
    }
    if (showCreateGroup || renamingGroup != null) {
        val original = renamingGroup
        var name by remember(original?.id) { mutableStateOf(original?.name.orEmpty()) }
        LumenDialog(title = if (original == null) strings.newGroup else strings.renameGroup,
            onDismissRequest = { showCreateGroup = false; renamingGroup = null },
            confirmText = if (original == null) strings.createAction else strings.saveAction,
            onConfirm = {
                if (name.isNotBlank()) {
                    if (original == null) onCreateGroup(name.trim()) else onRenameGroup(original.id, name.trim())
                    showCreateGroup = false; renamingGroup = null
                }
            }, dismissText = strings.cancel,
            onDismiss = { showCreateGroup = false; renamingGroup = null }) {
            ExpressiveTextField(shape = MaterialTheme.shapes.large, value = name, onValueChange = { name = it },
                label = { Text(strings.nodeName) }, singleLine = true, modifier = Modifier.fillMaxWidth())
        }
    }
    deletingGroup?.let { group ->
        LumenDialog(title = strings.deleteGroup, message = strings.deleteGroupConfirm,
            onDismissRequest = { deletingGroup = null }, confirmText = strings.delete,
            onConfirm = {
                onDeleteGroup(group.id)
                if (selectedFilter == "group:${group.id}") chooseFilter("all")
                deletingGroup = null
            }, dismissText = strings.cancel, onDismiss = { deletingGroup = null }, destructive = true)
    }
    if (showDeleteAll) {
        LumenDialog(title = strings.deleteAllServers, message = strings.deleteAllServersConfirm,
            onDismissRequest = { showDeleteAll = false }, confirmText = strings.delete,
            onConfirm = { onDeleteAllNodes(); showDeleteAll = false },
            dismissText = strings.cancel, onDismiss = { showDeleteAll = false }, destructive = true)
    }
    movingNodes?.let { nodesToMove ->
        LumenDialog(title = strings.moveToGroup, onDismissRequest = { movingNodes = null },
            dismissText = strings.cancel, onDismiss = { movingNodes = null }) {
            Column {
                TextButton(onClick = { onAssignNodesToGroup(nodesToMove, null); movingNodes = null; selectedIds = emptySet() }) {
                    Text(strings.groupDefault)
                }
                serverGroups.forEach { group ->
                    TextButton(onClick = { onAssignNodesToGroup(nodesToMove, group.id); movingNodes = null; selectedIds = emptySet() }) {
                        Text(group.name)
                    }
                }
            }
        }
    }
}

@Composable
private fun FilterPill(label: String, isSelected: Boolean, onClick: () -> Unit) {
    LumenFilterChip(label, isSelected, onClick)
}

/** Aggregate filters deliberately have no provider owner. */
internal fun subscriptionForServerFilter(filter: String, subscriptions: List<SubscriptionUiModel>): SubscriptionUiModel? =
    filter.takeIf { it.startsWith("subscription:") }?.removePrefix("subscription:")
        ?.let { id -> subscriptions.firstOrNull { it.id == id } }


/** Per-row inputs keep unrelated latency/selection updates skippable. */
@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
@Composable
private fun ServerListRow(
    node: NodeUiModel, selectionChecked: Boolean, pinging: Boolean, testResult: String?,
    onClick: (NodeUiModel) -> Unit, onLongClick: (NodeUiModel) -> Unit,
    onEditNode: (NodeUiModel) -> Unit, onPingNode: (NodeUiModel) -> Unit,
    onCopyNodeLink: (NodeUiModel) -> Unit, onExportQrCode: (NodeUiModel) -> Unit,
    onMove: (NodeUiModel) -> Unit, onDeleteNode: (NodeUiModel) -> Unit,
    modifier: Modifier = Modifier
) {
    val strings = LocalStrings.current
    val art = com.lumen.ui.theme.expressivePalette()

                    var showNodeMenu by remember(node.id) { mutableStateOf(false) }
                    val isSelected = node.isSelected
                    val highlighted = isSelected || selectionChecked
                    val cardBg = androidx.compose.animation.animateColorAsState(
                        targetValue = if (selectionChecked) art.blue else if (isSelected) art.stage else MaterialTheme.colorScheme.surfaceContainerLow,
                        animationSpec = androidx.compose.animation.core.spring(), label = "server_color"
                    )
                    val rowCorner = androidx.compose.animation.core.animateDpAsState(
                        targetValue = if (highlighted) 22.dp else 14.dp,
                        animationSpec = androidx.compose.animation.core.spring(
                            dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy,
                            stiffness = androidx.compose.animation.core.Spring.StiffnessMedium
                        ), label = "server_shape"
                    )
                    val textPrimary = if (selectionChecked) art.onBlue else if (isSelected) art.onStage else MaterialTheme.colorScheme.onSurface
                    val textMuted = textPrimary.copy(alpha = .75f)

                    val flagEmoji = remember(node.countryCode) {
                        CountryFlagHelper.getServerFlagEmoji(node.countryCode)
                    }

                        // Latency
                        val pingMs = node.pingMs
                        val unreachable = pingMs != null && pingMs <= 0 && !pinging
                        val pingText = when {
                            pinging -> "..."
                            pingMs != null && pingMs > 0 -> "$pingMs ms"
                            pingMs != null -> strings.pingUnreachable
                            else -> "--"
                        }


                    Row(
                        modifier = modifier
                            .fillMaxWidth()
                            .graphicsLayer {
                                shape = RoundedCornerShape(topStart = rowCorner.value, topEnd = 12.dp,
                                    bottomEnd = rowCorner.value, bottomStart = 12.dp)
                                clip = true
                            }.drawBehind { drawRect(cardBg.value) }
                            .combinedClickable(
                                onClick = {
                                    onClick(node)
                                },
                                onLongClick = { onLongClick(node) }
                            )
                            .padding(horizontal = 12.dp, vertical = 8.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        // Flag squircle badge
                        Box(
                            modifier = Modifier
                                .size(34.dp)
                                .clip(RoundedCornerShape(topStart = 6.dp, topEnd = 18.dp, bottomEnd = 6.dp, bottomStart = 18.dp))
                                .background(if (highlighted) art.lime else MaterialTheme.colorScheme.background),
                            contentAlignment = Alignment.Center
                        ) {
                            Text(
                                text = flagEmoji,
                                fontSize = 18.sp,
                                fontWeight = FontWeight.Bold
                            )
                        }

                        Spacer(modifier = Modifier.width(12.dp))

                        // Info
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                text = node.name,
                                fontSize = 15.sp,
                                fontWeight = if (highlighted) FontWeight.Bold else FontWeight.SemiBold,
                                letterSpacing = if (strings.localeCode == "fa") 0.sp else (-0.5).sp,
                                color = textPrimary,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis
                            )
                            Spacer(modifier = Modifier.height(2.dp))
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(node.displayProtocol,
                                    style = MaterialTheme.typography.bodySmall, color = textMuted,
                                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                                Spacer(Modifier.width(8.dp))
                                Text(pingText, fontFamily = com.lumen.ui.theme.LumenMono,
                                    style = MaterialTheme.typography.labelSmall.copy(textDirection = androidx.compose.ui.text.style.TextDirection.Ltr),
                                    color = if (pinging || pingMs == null) textMuted else pingColor(pingMs, cardBg.value),
                                    maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                        }

                        if (isSelected || selectionChecked) {
                            Spacer(modifier = Modifier.width(10.dp))
                            Icon(
                                imageVector = if (selectionChecked) Icons.Default.CheckBox else Icons.Default.CheckCircle,
                                contentDescription = strings.selected,
                                tint = if (art.stage.luminance() > .5f) art.onStage else art.lime,
                                modifier = Modifier.size(20.dp)
                            )
                        }
                        Box {
                            IconButton(onClick = { showNodeMenu = true }, modifier = Modifier.size(48.dp)) {
                                Icon(Icons.Default.MoreVert, contentDescription = node.name, tint = textMuted)
                            }
                            if (showNodeMenu) LumenMenu(expanded = true, onDismissRequest = { showNodeMenu = false }) {
                                DropdownMenuItem(text = { Text(strings.edit) }, onClick = {
                                    showNodeMenu = false; onEditNode(node)
                                })
                                DropdownMenuItem(text = { Text(strings.ping) }, onClick = {
                                    showNodeMenu = false; onPingNode(node)
                                })
                                DropdownMenuItem(text = { Text(strings.copyLink) }, onClick = {
                                    showNodeMenu = false; onCopyNodeLink(node)
                                })
                                DropdownMenuItem(text = { Text(strings.exportQrCode) }, onClick = {
                                    showNodeMenu = false; onExportQrCode(node)
                                })
                                DropdownMenuItem(text = { Text(strings.moveToGroup) }, onClick = {
                                    showNodeMenu = false; onMove(node)
                                })
                                DropdownMenuItem(text = { Text(strings.delete) }, onClick = {
                                    showNodeMenu = false; onDeleteNode(node)
                                })
                            }
                        }
                    }

}
