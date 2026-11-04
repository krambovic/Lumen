package com.lumen.ui.screens

import android.graphics.BitmapFactory
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

import com.lumen.ui.components.*
import com.lumen.ui.theme.*

// === Premium Yellow Gaming Colors ===
private val CyberYellow = Color(0xFFFFD600)
private val NeonGreen = Color(0xFF00FF66)
private val BgDark = Color(0xFF09090E)
private val CardDark = Color(0xFF15151E)
private val CardStroke = Color(0xFF252536)
private val TextMuted = Color(0xFF8B93A5)

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
    var renamingGroup by remember { mutableStateOf<ServerGroupUiModel?>(null) }
    var deletingGroup by remember { mutableStateOf<ServerGroupUiModel?>(null) }
    var movingNodes by remember { mutableStateOf<List<NodeUiModel>?>(null) }
    var selectedIds by remember { mutableStateOf(setOf<String>()) }
    var pendingSelectedId by remember { mutableStateOf<String?>(null) }
    val selectServer: (NodeUiModel) -> Unit = { node -> pendingSelectedId = node.id; onSelectNode(node) }
    var showDeleteAll by remember { mutableStateOf(false) }
    var showSubscriptionMenu by remember { mutableStateOf(false) }
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
    val filteredNodes = if (prepared.query.group == selectedFilter) prepared.visible else emptyList()
    
    LaunchedEffect(prepared.source, pendingSelectedId) {
        if (pendingSelectedId != null && (prepared.source.any { it.id == pendingSelectedId && it.isSelected } ||
                (prepared.source === nodes && nodes.none { it.id == pendingSelectedId }))) pendingSelectedId = null
    }

    val activeSub = subscriptionForServerFilter(selectedFilter, subscriptions)

    val bgBrush = Brush.radialGradient(
        colors = listOf(Color(0xFF2A1C00), BgDark),
        center = Offset(500f, 0f),
        radius = 1600f
    )

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(bgBrush)
            .statusBarsPadding()
    ) {
        BrandTopBar(onAddClick = { showAddSheet = true }, title = strings.servers)

        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 16.dp)
        ) {
            if (selectedIds.isNotEmpty()) {
                Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                    horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("${strings.selected}: ${selectedIds.size}", style = MaterialTheme.typography.labelLarge, color = CyberYellow)
                    TextButton(onClick = { onPingNodes(nodes.filter { it.id in selectedIds }) }) { Text(strings.ping, color = Color.White) }
                    TextButton(onClick = {
                        onExportNodesText(selectedIds).takeIf { it.isNotBlank() }?.let(onShareText)
                    }) { Text(strings.exportSelected, color = Color.White) }
                    TextButton(onClick = { movingNodes = nodes.filter { it.id in selectedIds } }) { Text(strings.moveToGroup, color = Color.White) }
                    TextButton(onClick = { selectedIds = emptySet() }) { Text(strings.cancel, color = Color.Red) }
                }
            }
            
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
                            color = TextMuted
                        )
                    },
                    leadingIcon = {
                        Icon(
                            imageVector = Icons.Default.Search,
                            contentDescription = null,
                            tint = TextMuted,
                            modifier = Modifier.size(20.dp)
                        )
                    },
                    trailingIcon = {
                        if (searchQuery.isNotEmpty()) {
                            IconButton(onClick = { searchQuery = "" }) {
                                Icon(
                                    imageVector = Icons.Default.Close,
                                    contentDescription = strings.clear,
                                    tint = TextMuted,
                                    modifier = Modifier.size(18.dp)
                                )
                            }
                        }
                    },
                    singleLine = true,
                    shape = RoundedCornerShape(20.dp),
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedContainerColor = CardDark,
                        unfocusedContainerColor = CardDark,
                        disabledContainerColor = CardDark,
                        focusedBorderColor = CyberYellow,
                        unfocusedBorderColor = CardStroke
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

            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(bottom = 120.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                item(key = "server_overview", contentType = "overview") {
                    
                    if (activeSub != null && selectedFilter != "manual") {
                        Box(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(vertical = 6.dp)
                                .clip(RoundedCornerShape(20.dp))
                                .background(CardDark)
                                .border(1.dp, CardStroke, RoundedCornerShape(20.dp))
                                .padding(16.dp)
                        ) {
                            Column {
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    horizontalArrangement = Arrangement.SpaceBetween,
                                    verticalAlignment = Alignment.CenterVertically
                                ) {
                                    Column(Modifier.weight(1f)) {
                                        Text(activeSub.name.uppercase(), maxLines = 1, overflow = TextOverflow.Ellipsis,
                                            fontSize = 15.sp, fontWeight = FontWeight.Black, color = CyberYellow, fontFamily = FontFamily.Monospace)
                                        Spacer(Modifier.height(4.dp))
                                        Text(strings.serverCountLabel(activeSub.nodeCount),
                                            fontSize = 11.sp, color = TextMuted)
                                    }
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        Box(
                                            modifier = Modifier
                                                .size(40.dp)
                                                .clip(RoundedCornerShape(10.dp))
                                                .background(BgDark)
                                                .border(1.dp, CardStroke, RoundedCornerShape(10.dp))
                                                .clickable { onRefreshSubscription(activeSub) },
                                            contentAlignment = Alignment.Center
                                        ) {
                                            if (refreshingIds.contains(activeSub.id)) {
                                                CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp, color = CyberYellow)
                                            } else {
                                                Icon(Icons.Default.Refresh, contentDescription = strings.refreshSubscription, tint = CyberYellow, modifier = Modifier.size(18.dp))
                                            }
                                        }
                                        Spacer(Modifier.width(8.dp))
                                        Box {
                                            IconButton(onClick = { showSubscriptionMenu = true }, modifier = Modifier.size(40.dp)) {
                                                Icon(Icons.Default.MoreVert, contentDescription = activeSub.name, tint = TextMuted)
                                            }
                                            if (showSubscriptionMenu) LumenMenu(expanded = true, onDismissRequest = { showSubscriptionMenu = false }) {
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
                                    Spacer(modifier = Modifier.height(16.dp))
                                    val trafficText = activeSub.trafficSummary ?: "—"
                                    val daysText = activeSub.expiryDaysLeft?.let { "$it ${strings.daysRemaining}" }

                                    Row(
                                        modifier = Modifier.fillMaxWidth(),
                                        horizontalArrangement = Arrangement.SpaceBetween
                                    ) {
                                        Text(text = "DATA USAGE", color = TextMuted, fontSize = 10.sp, fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold)
                                        Text(text = trafficText, color = Color.White, fontSize = 11.sp, fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold)
                                    }
                                    Spacer(modifier = Modifier.height(8.dp))

                                    val ratio = trafficRatio.coerceIn(0f, 1f)
                                    Box(
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .height(4.dp)
                                            .clip(RoundedCornerShape(2.dp))
                                            .background(BgDark)
                                    ) {
                                        Box(
                                            modifier = Modifier
                                                .fillMaxWidth(ratio)
                                                .height(4.dp)
                                                .clip(RoundedCornerShape(2.dp))
                                                .background(CyberYellow)
                                        )
                                    }
                                    if (daysText != null) {
                                        Spacer(modifier = Modifier.height(8.dp))
                                        Text(text = daysText, fontSize = 11.sp, color = CyberYellow)
                                    }
                                }
                            }
                        }
                    }

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
                                    color = TextMuted
                                )
                                Icon(
                                    Icons.Filled.KeyboardArrowDown,
                                    contentDescription = null,
                                    modifier = Modifier.size(18.dp),
                                    tint = TextMuted
                                )
                            }

                            if (showSortMenu) LumenMenu(
                                expanded = true,
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
                    androidx.compose.material3.LinearProgressIndicator(Modifier.fillMaxWidth(), color = CyberYellow, trackColor = CardDark)
                }
                if (filteredNodes.isEmpty() && !preparing) {
                    item(key = "empty_servers", contentType = "empty") {
                        Column(
                            modifier = Modifier.fillMaxWidth()
                                .clip(RoundedCornerShape(24.dp))
                                .background(CardDark)
                                .border(1.dp, CardStroke, RoundedCornerShape(24.dp))
                                .padding(horizontal = 20.dp, vertical = 24.dp),
                            horizontalAlignment = Alignment.CenterHorizontally
                        ) {
                            Icon(Icons.Default.Search, contentDescription = null,
                                tint = TextMuted,
                                modifier = Modifier.size(28.dp))
                            Spacer(Modifier.height(12.dp))
                            Text(if (nodes.isEmpty()) strings.noServers else strings.nothingFound,
                                style = MaterialTheme.typography.bodyMedium,
                                textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                                color = TextMuted)
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
                            }) { Text(if (nodes.isEmpty()) strings.add else strings.resetFilters, color = CyberYellow) }
                        }
                    }
                }
                val sections = if (groupByCountry) filteredNodes.groupBy { countryCodeOf(it).ifBlank { "—" } }
                    else mapOf("" to filteredNodes)
                sections.forEach { (country, sectionNodes) ->
                    if (country.isNotBlank()) item(key = "country:$country", contentType = "country") {
                        Text("${CountryFlagHelper.getFlagEmoji(country)}  $country · ${sectionNodes.size}",
                            Modifier.padding(horizontal = 8.dp, vertical = 10.dp), style = MaterialTheme.typography.labelLarge, color = Color.White)
                    }
                    items(sectionNodes, key = { it.id }, contentType = { "server" }) { node ->
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
                            onMove = { movingNodes = listOf(it) }, onDeleteNode = { deletingNode = it })
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

internal fun subscriptionForServerFilter(filter: String, subscriptions: List<SubscriptionUiModel>): SubscriptionUiModel? =
    filter.takeIf { it.startsWith("subscription:") }?.removePrefix("subscription:")
        ?.let { id -> subscriptions.firstOrNull { it.id == id } }

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
    var showNodeMenu by remember(node.id) { mutableStateOf(false) }
    
    val isSelected = node.isSelected || selectionChecked
    
    // انیمیشن تپش نئونی (Pulse Glow Effect)
    val infiniteTransition = rememberInfiniteTransition()
    val glowAlpha by infiniteTransition.animateFloat(
        initialValue = if (isSelected) 0.3f else 1.0f,
        targetValue = 1.0f,
        animationSpec = infiniteRepeatable(
            animation = tween(1000),
            repeatMode = RepeatMode.Reverse
        ),
        label = "pulse_glow"
    )

    // استفاده از آلفا متحرک برای ایجاد حس تپش وقتی سرور انتخاب شده
    val borderColor = if (isSelected) CyberYellow.copy(alpha = glowAlpha) else CardStroke
    val borderWidth = if (isSelected) 2.dp else 1.dp
    val cardShape = RoundedCornerShape(16.dp)
    
    val flagEmoji = remember(node.countryCode) {
        CountryFlagHelper.getServerFlagEmoji(node.countryCode)
    }
    
    val pingMs = node.pingMs
    val pingText = when {
        pinging -> "..."
        pingMs != null && pingMs > 0 -> "${pingMs}ms"
        pingMs != null -> "FAIL"
        else -> "--"
    }

    Box(
        modifier = modifier
            .fillMaxWidth()
            .clip(cardShape)
            .background(CardDark)
            .border(borderWidth, borderColor, cardShape)
            .combinedClickable(
                onClick = { onClick(node) },
                onLongClick = { onLongClick(node) }
            )
            .padding(16.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically
        ) {
            // Circle Flag Box - انیمیشن تپش روی کادر پرچم
            Box(
                modifier = Modifier
                    .size(48.dp)
                    .clip(CircleShape)
                    .background(BgDark)
                    .border(
                        width = if (isSelected) 1.5.dp else 1.dp,
                        color = if (isSelected) CyberYellow.copy(alpha = glowAlpha) else CardStroke,
                        shape = CircleShape
                    )
                    .graphicsLayer {
                        // یک بزرگ‌نمایی (Scale) خیلی ریز و جذاب وقتی می‌تپه
                        val scale = if (isSelected) 0.97f + (0.03f * glowAlpha) else 1f
                        scaleX = scale
                        scaleY = scale
                    },
                contentAlignment = Alignment.Center
            ) {
                Text(text = flagEmoji, fontSize = 28.sp)
            }

            Spacer(modifier = Modifier.width(14.dp))

            // Server Info
            Column(modifier = Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        text = node.name,
                        color = Color.White,
                        fontSize = 15.sp,
                        fontWeight = FontWeight.Bold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )
                    if (isSelected) {
                        Spacer(modifier = Modifier.width(6.dp))
                        Icon(
                            Icons.Default.CheckCircle,
                            contentDescription = null,
                            tint = CyberYellow.copy(alpha = glowAlpha),
                            modifier = Modifier.size(16.dp)
                        )
                    }
                }
                Spacer(modifier = Modifier.height(6.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(
                        modifier = Modifier
                            .size(6.dp)
                            .clip(CircleShape)
                            .background(if (isSelected) CyberYellow else TextMuted)
                    )
                    Spacer(modifier = Modifier.width(6.dp))
                    Text(
                        text = "PROTOCOL: ${node.displayProtocol.uppercase()}",
                        color = TextMuted,
                        fontSize = 10.sp,
                        fontFamily = FontFamily.Monospace,
                        fontWeight = FontWeight.Bold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )
                }
            }

            // Ping Badge & Menu
            Column(horizontalAlignment = Alignment.End) {
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(6.dp))
                        .background(if (pingMs != null && pingMs > 0 && !pinging) NeonGreen.copy(alpha = 0.15f) else CardStroke)
                        .padding(horizontal = 6.dp, vertical = 4.dp)
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            text = pingText,
                            color = if (pingMs != null && pingMs > 0 && !pinging) NeonGreen else TextMuted,
                            fontSize = 11.sp,
                            fontWeight = FontWeight.Bold,
                            fontFamily = FontFamily.Monospace
                        )
                    }
                }
                Box {
                    IconButton(onClick = { showNodeMenu = true }, modifier = Modifier.size(32.dp).padding(top = 4.dp)) {
                        Icon(Icons.Default.MoreVert, contentDescription = null, tint = TextMuted)
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
    }
}
