package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.platform.testTag
import com.lumen.ui.components.*
import com.lumen.ui.theme.expressivePalette
import com.lumen.ui.theme.LumenMono

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun DashboardServerPicker(
    nodes: List<NodeUiModel>, groupTitle: String, onDismiss: () -> Unit,
    onSelect: (NodeUiModel) -> Unit, pinging: Set<String>,
    onPingAll: () -> Unit, onPingNode: (NodeUiModel) -> Unit, onStopPing: () -> Unit
) {
    val s = LocalStrings.current
    val art = expressivePalette()
    var query by remember { mutableStateOf("") }
    val listState = rememberLazyListState()
    val visible by rememberDashboardServers(nodes, query, listState)
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = MaterialTheme.colorScheme.background,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        // Scroll the list first; at its top, a downward swipe dismisses the sheet.
        sheetGesturesEnabled = !listState.canScrollBackward,
        dragHandle = { BottomSheetDefaults.DragHandle() }) {
        Column(Modifier.fillMaxWidth().fillMaxHeight(.9f).padding(start = 16.dp, end = 16.dp, top = 12.dp)) {
            Row(Modifier.fillMaxWidth().testTag(LumenMotionTags.PickerHeader), verticalAlignment = Alignment.CenterVertically) {
                Text(s.serverLabel, style = MaterialTheme.typography.headlineLarge, modifier = Modifier.weight(1f))
                IconButton(onClick = onDismiss, modifier = Modifier.size(48.dp)) {
                    Icon(Icons.Default.Close, contentDescription = s.close)
                }
            }
            Spacer(Modifier.height(12.dp))
            ExpressiveTextField(query, { query = it }, Modifier.fillMaxWidth().semantics { contentDescription = s.searchServers }, singleLine = true,
                placeholder = { Text(s.searchServers) }, leadingIcon = { Icon(Icons.Default.Search, null) })
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(groupTitle, style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.weight(1f))
                DashboardPingAllButton(nodes.any { it.id in pinging }, nodes.isNotEmpty(), onPingAll, onStopPing)
            }
            LazyColumn(Modifier.weight(1f).testTag(LumenMotionTags.PickerList), state = listState, verticalArrangement = Arrangement.spacedBy(6.dp),
                contentPadding = PaddingValues(bottom = 24.dp)) {
                if (visible.isEmpty()) item(key = "empty", contentType = "empty") {
                    Text(s.nothingFound, Modifier.padding(20.dp), color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                items(visible, key = { it.id }, contentType = { "server" }) { node ->
                    val selected = node.isSelected
                    Row(Modifier.fillMaxWidth().animateItem(fadeInSpec = null, fadeOutSpec = null)
                        .semantics { contentDescription = node.name + ". " + node.displayProtocol }
                        .clip(com.lumen.ui.components.selectionShape(selected, 14.dp, 26.dp))
                        .background(if (selected) art.lavender else MaterialTheme.colorScheme.surfaceContainerLow)
                        .clickable { onSelect(node) }.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(44.dp).clip(RoundedCornerShape(6.dp, 18.dp, 6.dp, 18.dp))
                            .background(MaterialTheme.colorScheme.background), contentAlignment = Alignment.Center) {
                            Text(CountryFlagHelper.getServerFlagEmoji(node.countryCode), fontSize = 22.sp)
                        }
                        Column(Modifier.weight(1f).padding(horizontal = 12.dp)) {
                            Text(node.name, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(node.displayProtocol, style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                        DashboardNodePing(node, node.id in pinging) { onPingNode(node) }
                        if (selected) Icon(Icons.Default.Check, s.selected, Modifier.padding(start = 8.dp).size(20.dp))
                    }
                }
            }
        }
    }
}

