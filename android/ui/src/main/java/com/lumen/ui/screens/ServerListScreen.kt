package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.lumen.ui.components.CountryFlagIcon
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

@Composable
fun ServerListScreen(
    nodes: List<NodeUiModel>,
    subscriptions: List<SubscriptionUiModel>,
    refreshingIds: Set<String>,
    isPinging: Boolean,
    testingNodeId: String?,
    serverTestResults: Map<String, String>,
    onSelectNode: (NodeUiModel) -> Unit,
    onEditNode: (NodeUiModel) -> Unit,
    onDeleteNode: (NodeUiModel) -> Unit,
    onAddNode: () -> Unit,
    onPingAll: () -> Unit,
    onPingNode: (NodeUiModel) -> Unit,
    onImportClipboard: () -> Unit,
    onAddSubscription: (String, String) -> Unit,
    onRefreshSubscription: (SubscriptionUiModel) -> Unit,
    onDeleteSubscription: (SubscriptionUiModel) -> Unit,
    onToggleAutoUpdate: (SubscriptionUiModel, Boolean) -> Unit,
    modifier: Modifier = Modifier
) {
    val s = LocalStrings.current
    var query by remember { mutableStateOf("") }
    var group by remember { mutableStateOf("all") }
    var showAddSubscription by remember { mutableStateOf(false) }
    val groups = remember(subscriptions) { listOf("all", "manual") + subscriptions.map { it.id } }
    if (group !in groups) group = "all"
    val selectedSubscription = subscriptions.firstOrNull { it.id == group }
    val filtered = remember(nodes, query, group) {
        nodes.filter { node ->
            val inGroup = when (group) {
                "all" -> true
                "manual" -> node.subscriptionId == null
                else -> node.subscriptionId == group
            }
            inGroup && (query.isBlank() || node.name.contains(query, true) ||
                node.server.contains(query, true) || node.protocol.contains(query, true))
        }
    }

    Column(modifier.fillMaxSize().padding(16.dp)) {
        Row(Modifier.fillMaxWidth(), Arrangement.SpaceBetween, Alignment.CenterVertically) {
            Text(s.servers, style = MaterialTheme.typography.headlineMedium)
            Row {
                IconButton(onClick = { showAddSubscription = true }) {
                    Text("🔗", color = MaterialTheme.colorScheme.primary)
                }
                IconButton(onClick = onAddNode) {
                    Icon(Icons.Filled.Add, s.addNode, tint = MaterialTheme.colorScheme.primary)
                }
            }
        }
        LumenDropdown(
            label = s.subscriptions,
            options = groups,
            selected = group,
            onSelected = { group = it },
            optionLabel = { id ->
                when (id) {
                    "all" -> s.allGroups
                    "manual" -> s.manual
                    else -> subscriptions.firstOrNull { it.id == id }?.name ?: id
                }
            }
        )
        selectedSubscription?.let { sub ->
            Spacer(Modifier.height(8.dp))
            SubscriptionCard(
                sub = sub,
                isRefreshing = sub.id in refreshingIds,
                onRefresh = { onRefreshSubscription(sub) },
                onDelete = { onDeleteSubscription(sub) },
                onAutoUpdate = { onToggleAutoUpdate(sub, it) }
            )
        }
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            value = query,
            onValueChange = { query = it },
            modifier = Modifier.fillMaxWidth(),
            placeholder = { Text(s.searchServers) },
            leadingIcon = { Icon(Icons.Filled.Search, null, tint = MaterialTheme.colorScheme.onSurfaceVariant) },
            singleLine = true
        )
        Spacer(Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onPingAll, enabled = !isPinging && filtered.isNotEmpty()) {
                Text(if (isPinging) s.pinging else s.pingAll)
            }
            OutlinedButton(onClick = onImportClipboard) { Text(s.importClipboard) }
        }
        Spacer(Modifier.height(8.dp))
        if (filtered.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text(s.noServers, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        } else {
            LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                items(filtered.sortedWith(compareByDescending<NodeUiModel> { it.isAutoNode }.thenBy { it.name }), key = { it.id }) { node ->
                    NodeRow(
                        node = node,
                        testResult = serverTestResults[node.id],
                        isTesting = testingNodeId == node.id,
                        onSelect = { onSelectNode(node) },
                        onEdit = { onEditNode(node) },
                        onDelete = { onDeleteNode(node) },
                        onTest = { onPingNode(node) }
                    )
                }
            }
        }
    }

    if (showAddSubscription) {
        AddSubscriptionDialog(
            onDismiss = { showAddSubscription = false },
            onAdd = { name, url ->
                onAddSubscription(name, url)
                showAddSubscription = false
            }
        )
    }
}

@Composable
private fun SubscriptionCard(
    sub: SubscriptionUiModel,
    isRefreshing: Boolean,
    onRefresh: () -> Unit,
    onDelete: () -> Unit,
    onAutoUpdate: (Boolean) -> Unit
) {
    val s = LocalStrings.current
    LumenCard {
        Column {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(sub.name, style = MaterialTheme.typography.titleSmall)
                    Text(sub.url, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
                }
                IconButton(onClick = onRefresh, enabled = !isRefreshing) {
                    Icon(Icons.Filled.Refresh, s.refresh, tint = if (isRefreshing) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.primary)
                }
                IconButton(onClick = onDelete) { Icon(Icons.Filled.Delete, s.delete, tint = MaterialTheme.colorScheme.onSurfaceVariant) }
            }
            Row(Modifier.fillMaxWidth(), Arrangement.SpaceBetween, Alignment.CenterVertically) {
                val date = if (sub.lastUpdated > 0) {
                    SimpleDateFormat("dd MMM HH:mm", Locale.getDefault()).format(Date(sub.lastUpdated))
                } else s.never
                Text("${sub.nodeCount} ${s.nodes} • ${s.updated} $date", color = MaterialTheme.colorScheme.onSurfaceVariant)
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(s.auto, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Spacer(Modifier.width(6.dp))
                    Switch(sub.autoUpdateEnabled, onCheckedChange = onAutoUpdate)
                }
            }
        }
    }
}

@Composable
private fun NodeRow(
    node: NodeUiModel,
    testResult: String?,
    isTesting: Boolean,
    onSelect: () -> Unit,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
    onTest: () -> Unit
) {
    val s = LocalStrings.current
    val shape = RoundedCornerShape(14.dp)
    Row(
        Modifier.fillMaxWidth().clip(shape).background(MaterialTheme.colorScheme.surface)
            .border(1.dp, if (node.isSelected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline, shape)
            .clickable(onClick = onSelect).padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (node.isAutoNode) Text("⚡", Modifier.width(24.dp))
        else CountryFlagIcon(
            countryCode = node.countryCode,
            fallbackText = node.displayProtocol.take(3)
        )
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(node.name, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                if (node.isAutoNode) "AUTO • url-test" else "${node.displayProtocol} • ${node.server}:${node.port}",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1
            )
            testResult?.let {
                Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.primary, maxLines = 1)
            }
        }
        node.pingMs?.let { ping ->
            Text(if (ping >= 0) "$ping ms" else "—", color = pingColor(ping))
        }
        if (isTesting) {
            CircularProgressIndicator(Modifier.width(24.dp), strokeWidth = 2.dp)
        } else {
            IconButton(onClick = onTest) { Icon(Icons.Filled.Refresh, s.pingAll, tint = MaterialTheme.colorScheme.primary) }
        }
        IconButton(onClick = onEdit) { Icon(Icons.Filled.Edit, s.edit, tint = MaterialTheme.colorScheme.onSurfaceVariant) }
        IconButton(onClick = onDelete) { Icon(Icons.Filled.Delete, s.delete, tint = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

@Composable
private fun AddSubscriptionDialog(onDismiss: () -> Unit, onAdd: (String, String) -> Unit) {
    val s = LocalStrings.current
    var name by remember { mutableStateOf("") }
    var url by remember { mutableStateOf("") }
    Dialog(onDismissRequest = onDismiss) {
        Surface(shape = RoundedCornerShape(18.dp), color = MaterialTheme.colorScheme.surface) {
            Column(Modifier.padding(20.dp)) {
                Text(s.addSubscription, style = MaterialTheme.typography.titleMedium)
                Spacer(Modifier.height(12.dp))
                OutlinedTextField(name, { name = it }, label = { Text(s.nameOptional) }, singleLine = true)
                Spacer(Modifier.height(8.dp))
                OutlinedTextField(url, { url = it }, label = { Text("URL") }, singleLine = true)
                Spacer(Modifier.height(16.dp))
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    TextButton(onClick = onDismiss) { Text(s.cancel) }
                    Spacer(Modifier.width(8.dp))
                    OutlinedButton(
                        onClick = { onAdd(name.trim(), url.trim()) },
                        enabled = url.trim().startsWith("http")
                    ) { Text(s.add) }
                }
            }
        }
    }
}
