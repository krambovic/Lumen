package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Layers
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lumen.ui.components.CountryFlagHelper
import com.lumen.ui.components.expressiveButtonSemantics
import com.lumen.ui.theme.LumenMono

/** The slider layout selects directly from this table, without opening a picker. */
@Composable
internal fun DashboardServerTable(
    nodes: List<NodeUiModel>, groupTitle: String, pinging: Set<String>, onSelect: (NodeUiModel) -> Unit,
    onOpenServers: () -> Unit, showSpeed: Boolean, connected: Boolean, speedValues: () -> DashboardSpeeds,
    onPingAll: () -> Unit, onPingNode: (NodeUiModel) -> Unit, onStopPing: () -> Unit
) {
    val s = LocalStrings.current
    var query by remember { mutableStateOf("") }
    val visible by rememberDashboardServers(nodes, query)
    val art = com.lumen.ui.theme.expressivePalette()
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 16.dp)) {
        item(key = "search", contentType = "controls") {
            Row(Modifier.fillMaxWidth().padding(bottom = 10.dp)
                .expressiveButtonSemantics(groupTitle, onOpenServers)
                .clip(RoundedCornerShape(12.dp, 30.dp, 12.dp, 30.dp)).background(art.stage)
                .clickable(onClick = onOpenServers).padding(18.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(s.serverGroup.uppercase(), style = MaterialTheme.typography.labelSmall,
                        color = art.onStage.copy(alpha = .7f))
                    Text(groupTitle, style = MaterialTheme.typography.headlineMedium,
                        color = art.onStage, maxLines = 2, overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(top = 8.dp))
                    Text(s.serverCountLabel(nodes.size), style = MaterialTheme.typography.bodySmall,
                        color = art.onStage.copy(alpha = .8f), modifier = Modifier.padding(top = 4.dp))
                }
                Icon(Icons.Default.Layers, null, Modifier.size(30.dp), tint = art.onStage)
                Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.size(20.dp), tint = art.onStage)
            }
            if (showSpeed) DashboardTableSpeedCards(connected, speedValues)
            ExpressiveTextField(query, { query = it }, Modifier.fillMaxWidth(), singleLine = true,
                placeholder = { Text(s.searchServers) }, leadingIcon = { Icon(Icons.Default.Search, null) })
            Row(Modifier.fillMaxWidth().padding(start = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("${s.serverLabel} · ${visible.size}", Modifier.weight(1f), style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
                DashboardPingAllButton(nodes.any { it.id in pinging }, nodes.isNotEmpty(), onPingAll, onStopPing)
            }
        }
        if (visible.isEmpty()) item(key = "empty", contentType = "empty") {
            Text(if (nodes.isEmpty()) s.noServers else s.nothingFound, Modifier.padding(20.dp),
                color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        items(visible, key = { it.id }, contentType = { "server" }) { node ->
            Row(Modifier.fillMaxWidth().animateItem(fadeInSpec = null, fadeOutSpec = null).padding(bottom = 3.dp)
                .semantics { contentDescription = node.name + ". " + node.displayProtocol }
                .clip(com.lumen.ui.components.selectionShape(node.isSelected, 8.dp, 18.dp))
                .background(if (node.isSelected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainerLow)
                .clickable { onSelect(node) }.padding(horizontal = 12.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically) {
                Text(CountryFlagHelper.getServerFlagEmoji(node.countryCode), fontSize = 20.sp)
                Column(Modifier.weight(1f).padding(horizontal = 12.dp)) {
                    Text(node.name, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold,
                        maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(node.displayProtocol, style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                DashboardNodePing(node, node.id in pinging) { onPingNode(node) }
                Box(Modifier.width(26.dp), contentAlignment = Alignment.CenterEnd) {
                    if (node.isSelected) Icon(Icons.Default.Check, null, Modifier.size(18.dp), tint = MaterialTheme.colorScheme.primary)
                }
            }
        }
    }
}


@Composable
private fun DashboardTableSpeedCards(connected: Boolean, values: () -> DashboardSpeeds) {
    val s = LocalStrings.current
    val art = com.lumen.ui.theme.expressivePalette()
    val speeds = if (connected) values() else DashboardSpeeds()

                Row(Modifier.fillMaxWidth().padding(bottom = 14.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf(speeds.download, speeds.upload).forEachIndexed { index, speed ->
                        val value = formatDashboardSpeed(speed)
                        Column(Modifier.weight(1f).clip(RoundedCornerShape(if (index == 0) 28.dp else 12.dp, 12.dp, 28.dp, 12.dp))
                            .background(if (index == 0) art.blue else art.lavender).padding(14.dp)) {
                            val ink = if (index == 0) art.onBlue else art.onLavender
                            Text((if (index == 0) "↓ " else "↑ ") + s.speedLabel,
                                style = MaterialTheme.typography.labelSmall, color = ink.copy(alpha = .75f))
                            Row(verticalAlignment = Alignment.Bottom, modifier = Modifier.padding(top = 12.dp)) {
                                Text(value.first, style = MaterialTheme.typography.headlineLarge, fontFamily = LumenMono, color = ink)
                                Text(value.second, style = MaterialTheme.typography.labelSmall, color = ink,
                                    modifier = Modifier.padding(start = 5.dp, bottom = 5.dp))
                            }
                        }
                    }
                }
            
}
