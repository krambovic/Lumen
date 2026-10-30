package com.lumen.ui.screens

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.FilterList
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** Groups and protocol are facets; filters retain the released client's semantics. */
@Composable
internal fun ServerFilterBar(
    group: String, groups: List<Pair<String, String>>, onGroup: (String) -> Unit,
    protocol: String, protocols: List<String>, onProtocol: (String) -> Unit,
    groupByCountry: Boolean, onGroupByCountry: (Boolean) -> Unit,
    reachable: Boolean, onReachable: (Boolean) -> Unit, maxPing: Int, onMaxPing: (Int) -> Unit,
    onReset: () -> Unit, onNewGroup: () -> Unit, onRenameGroup: (() -> Unit)?,
    onDeleteGroup: (() -> Unit)?, onDeleteManual: (() -> Unit)?
) {
    val s = LocalStrings.current
    var groupMenu by remember { mutableStateOf(false) }
    var protocolMenu by remember { mutableStateOf(false) }
    var filterMenu by remember { mutableStateOf(false) }
    var actions by remember { mutableStateOf(false) }
    val count = (if (groupByCountry) 1 else 0) + (if (reachable) 1 else 0) + (if (maxPing > 0) 1 else 0)
    Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Box(Modifier.weight(1f)) {
            FilledTonalButton({ groupMenu = true }, Modifier.fillMaxWidth(),
                contentPadding = PaddingValues(horizontal = 12.dp), shape = com.lumen.ui.components.selectionShape(groupMenu)) {
                Text(groups.firstOrNull { it.first == group }?.second ?: s.allGroups,
                    Modifier.weight(1f), maxLines = 1, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                Icon(Icons.Default.KeyboardArrowDown, null, Modifier.size(18.dp))
            }
            LumenMenu(groupMenu, { groupMenu = false }) {
                groups.forEach { (id, label) -> DropdownMenuItem(text = { Text(label) },
                    trailingIcon = { if (id == group) Icon(Icons.Default.Check, null) },
                    onClick = { onGroup(id); groupMenu = false }) }
            }
        }
        Box {
            OutlinedButton({ protocolMenu = true }, contentPadding = PaddingValues(horizontal = 10.dp),
                shape = com.lumen.ui.components.selectionShape(protocolMenu || protocol.isNotBlank())) {
                Text(protocol.ifBlank { s.protocolField }, maxLines = 1)
                Icon(Icons.Default.KeyboardArrowDown, null, Modifier.size(18.dp))
            }
            LumenMenu(protocolMenu, { protocolMenu = false }) {
                (listOf("" to s.allProtocols) + protocols.map { it to it.uppercase() }).forEach { (id, label) ->
                    DropdownMenuItem(text = { Text(label) }, trailingIcon = { if (protocol == id) Icon(Icons.Default.Check, null) },
                        onClick = { onProtocol(id); protocolMenu = false })
                }
            }
        }
        Box {
            FilledTonalIconButton({ filterMenu = true }) {
                BadgedBox(badge = { if (count > 0) Badge { Text(count.toString()) } }) {
                    Icon(Icons.Default.FilterList, s.filtersLabel)
                }
            }
            LumenMenu(filterMenu, { filterMenu = false }) {
                DropdownMenuItem(text = { Text(s.groupByCountry) }, onClick = { onGroupByCountry(!groupByCountry) },
                    trailingIcon = { if (groupByCountry) Icon(Icons.Default.Check, null) })
                DropdownMenuItem(text = { Text(s.onlyReachable) }, onClick = { onReachable(!reachable) },
                    trailingIcon = { if (reachable) Icon(Icons.Default.Check, null) })
                HorizontalDivider()
                listOf(0, 100, 200, 500).forEach { value ->
                    DropdownMenuItem(text = { Text("${s.maxPingLabel}: ${if (value == 0) s.offLabel else "$value ms"}") },
                        onClick = { onMaxPing(value) }, trailingIcon = { if (maxPing == value) Icon(Icons.Default.Check, null) })
                }
                DropdownMenuItem(text = { Text(s.resetFilters) }, onClick = { onReset(); filterMenu = false })
            }
        }
        Box {
            IconButton({ actions = true }) { Icon(Icons.Default.MoreVert, s.groupsAndServers) }
            LumenMenu(actions, { actions = false }) {
                DropdownMenuItem(text = { Text(s.newGroup) }, onClick = { actions = false; onNewGroup() })
                onRenameGroup?.let { callback -> DropdownMenuItem(text = { Text(s.renameGroup) }, onClick = { actions = false; callback() }) }
                onDeleteGroup?.let { callback -> DropdownMenuItem(text = { Text(s.deleteGroup) }, onClick = { actions = false; callback() }) }
                onDeleteManual?.let { callback -> DropdownMenuItem(text = { Text(s.deleteAllServers) }, onClick = { actions = false; callback() }) }
            }
        }
    }
}
