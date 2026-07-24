package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

@Composable
fun RoutingScreen(
    mode: SplitModeUi,
    apps: List<AppEntryUiModel>,
    isLoadingApps: Boolean,
    onModeChange: (SplitModeUi) -> Unit,
    onToggleApp: (AppEntryUiModel) -> Unit,
    onAutoSelect: () -> Unit,
    onClearSelection: () -> Unit,
    onBack: (() -> Unit)? = null,
    modifier: Modifier = Modifier
) {
    val strings = LocalStrings.current
    var query by remember { mutableStateOf("") }
    var hideSystem by remember { mutableStateOf(true) }
    var showOptions by remember { mutableStateOf(false) }
    val filteredApps = remember(apps, query, hideSystem) {
        apps.asSequence()
            .filter { !hideSystem || !it.isSystem }
            .filter {
                query.isBlank() || it.label.contains(query, true) ||
                    it.packageName.contains(query, true)
            }
            .toList()
    }

    Column(modifier.fillMaxSize().padding(horizontal = 16.dp)) {
        LumenScreenHeader(
            title = strings.appRouting,
            onBack = onBack,
            actions = {
                if (mode != SplitModeUi.DISABLED) {
                    Box {
                        IconButton(onClick = { showOptions = true }) {
                            Icon(Icons.Filled.MoreVert, contentDescription = strings.settings, tint = MaterialTheme.colorScheme.primary)
                        }
                        val menuShape = RoundedCornerShape(20.dp)
                        DropdownMenu(
                            expanded = showOptions,
                            onDismissRequest = { showOptions = false },
                            modifier = Modifier
                                .widthIn(min = 220.dp)
                                .shadow(8.dp, menuShape)
                                .clip(menuShape)
                                .background(MaterialTheme.colorScheme.surfaceContainerHigh, menuShape)
                                .border(androidx.compose.foundation.BorderStroke(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.35f)), menuShape)
                        ) {
                            Column(Modifier.padding(vertical = 4.dp)) {
                                DropdownMenuItem(
                                    text = { Text(strings.hideSystem) },
                                    trailingIcon = {
                                        Switch(checked = hideSystem, onCheckedChange = { hideSystem = it })
                                    },
                                    onClick = { hideSystem = !hideSystem }
                                )
                                DropdownMenuItem(
                                    text = { Text(strings.autoSelect) },
                                    onClick = { showOptions = false; onAutoSelect() }
                                )
                                DropdownMenuItem(
                                    text = { Text(strings.clear) },
                                    onClick = { showOptions = false; onClearSelection() }
                                )
                            }
                        }
                    }
                }
            }
        )

        SectionHeader(strings.splitTunneling)
        ModeOption(strings.off, strings.offDescription, mode == SplitModeUi.DISABLED) {
            onModeChange(SplitModeUi.DISABLED)
        }
        ModeOption(strings.vpnOnly, strings.vpnOnlyDescription, mode == SplitModeUi.ALLOW_LIST) {
            onModeChange(SplitModeUi.ALLOW_LIST)
        }
        ModeOption(strings.exclude, strings.excludeDescription, mode == SplitModeUi.DISALLOW_LIST) {
            onModeChange(SplitModeUi.DISALLOW_LIST)
        }

        if (mode != SplitModeUi.DISABLED) {
            SectionHeader(if (mode == SplitModeUi.ALLOW_LIST) strings.appsUsingVpn else strings.appsBypassingVpn)
            OutlinedTextField(
                value = query,
                onValueChange = { query = it },
                modifier = Modifier.fillMaxWidth(),
                placeholder = { Text(strings.searchApps) },
                leadingIcon = { Icon(Icons.Filled.Search, null, tint = MaterialTheme.colorScheme.onSurfaceVariant) },
                singleLine = true
            )
            if (isLoadingApps) {
                Text(strings.loadingApps, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(top = 12.dp))
            } else {
                LazyColumn(Modifier.weight(1f).padding(top = 8.dp)) {
                    items(filteredApps, key = { it.packageName }) { app ->
                        Surface(
                            modifier = Modifier.fillMaxWidth().padding(vertical = 3.dp),
                            shape = RoundedCornerShape(14.dp),
                            color = MaterialTheme.colorScheme.surface,
                            tonalElevation = if (app.isSelected) 2.dp else 0.dp
                        ) {
                            Row(
                                Modifier.fillMaxWidth().clickable { onToggleApp(app) }
                                    .padding(horizontal = 10.dp, vertical = 8.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                app.icon?.let {
                                    Image(bitmap = it, contentDescription = null, modifier = Modifier.size(40.dp))
                                    Spacer(Modifier.width(10.dp))
                                }
                                Column(Modifier.weight(1f)) {
                                    Text(app.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                    Text(
                                        app.packageName,
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis
                                    )
                                }
                                Checkbox(
                                    checked = app.isSelected,
                                    onCheckedChange = { onToggleApp(app) }
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
private fun ModeOption(title: String, description: String, selected: Boolean, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        RadioButton(selected = selected, onClick = onClick)
        Spacer(Modifier.width(4.dp))
        Column {
            Text(title)
            Text(description, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}