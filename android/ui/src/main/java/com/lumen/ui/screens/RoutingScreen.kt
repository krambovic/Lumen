package com.lumen.ui.screens

import com.lumen.ui.components.expressiveControlLabel
import androidx.compose.foundation.Image
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.background
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
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Checkbox
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
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
import androidx.compose.ui.text.font.FontWeight
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
    val filteredApps = remember(apps, query, hideSystem) {
        apps.asSequence()
            // A selected system app must stay visible even while the general system-app
            // filter is enabled; otherwise it cannot be reviewed or deselected.
            .filter { !hideSystem || !it.isSystem || it.isSelected }
            .filter {
                query.isBlank() || it.label.contains(query, true) ||
                    it.packageName.contains(query, true)
            }
            .toList()
    }

    LazyColumn(
        modifier = modifier.fillMaxSize().statusBarsPadding(),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 100.dp),
        verticalArrangement = Arrangement.spacedBy(0.dp)
    ) {
        item(key = "header") {
            LumenScreenHeader(title = strings.appRouting, onBack = onBack, applyStatusBarPadding = false)
        }
        item(key = "mode_title") {
            SectionHeader(strings.splitTunneling)
        }
        item(key = "modes") {
            Column {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    ModeOption(strings.off, strings.offDescription, mode == SplitModeUi.DISABLED,
                        Modifier.weight(1f)) { onModeChange(SplitModeUi.DISABLED) }
                    ModeOption(strings.vpnOnly, strings.vpnOnlyDescription, mode == SplitModeUi.ALLOW_LIST,
                        Modifier.weight(1f)) { onModeChange(SplitModeUi.ALLOW_LIST) }
                    ModeOption(strings.exclude, strings.excludeDescription, mode == SplitModeUi.DISALLOW_LIST,
                        Modifier.weight(1f)) { onModeChange(SplitModeUi.DISALLOW_LIST) }
                }
                Text(when (mode) {
                    SplitModeUi.DISABLED -> strings.offDescription
                    SplitModeUi.ALLOW_LIST -> strings.vpnOnlyDescription
                    SplitModeUi.DISALLOW_LIST -> strings.excludeDescription
                }, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(start = 4.dp, top = 10.dp))
            }
        }

        if (mode != SplitModeUi.DISABLED) {
            item(key = "apps_title") {
                val selectedCount = apps.count { it.isSelected }
                Column(Modifier.fillMaxWidth()) {
                    SectionHeader(if (mode == SplitModeUi.ALLOW_LIST) strings.appsUsingVpn else strings.appsBypassingVpn)
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Text(selectedCount.toString().padStart(2, '0'),
                            style = MaterialTheme.typography.headlineLarge, color = MaterialTheme.colorScheme.primary)
                        Text(strings.selected, style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(start = 8.dp))
                        Spacer(Modifier.weight(1f))
                        TextButton(onClick = onAutoSelect) { Text(strings.autoSelect) }
                        TextButton(onClick = onClearSelection) { Text(strings.clear) }
                    }
                }
            }

            item(key = "filters") {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    ExpressiveTextField(
                        value = query,
                        onValueChange = { query = it },
                        modifier = Modifier.weight(1f),
                        placeholder = { Text(strings.searchApps) },
                        leadingIcon = {
                            Icon(
                                Icons.Filled.Search,
                                contentDescription = null,
                                tint = MaterialTheme.colorScheme.onSurfaceVariant
                            )
                        },
                        singleLine = true,
                        shape = RoundedCornerShape(14.dp)
                    )
                }
                Row(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically) {
                    Text("${filteredApps.size} / ${apps.size}", style = MaterialTheme.typography.labelSmall,
                        fontFamily = com.lumen.ui.theme.LumenMono, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    LumenFilterChip(
                        label = strings.hideSystem,
                        selected = hideSystem,
                        onClick = { hideSystem = !hideSystem }
                    )
                }
            }
            item(key = "apps_gap") { Spacer(Modifier.height(12.dp)) }

            when {
                isLoadingApps -> item(key = "loading") {
                    LumenCard {
                        Text(
                            strings.loadingApps,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                }
                filteredApps.isEmpty() -> item(key = "empty") {
                    LumenCard {
                        Text(
                            strings.nothingFound,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                }
                else -> itemsIndexed(
                    filteredApps,
                    key = { _, app -> app.packageName }
                ) { index, app ->
                    AppRoutingRow(
                        app = app,
                        isFirst = index == 0,
                        isLast = index == filteredApps.lastIndex,
                        onToggle = { onToggleApp(app) }
                    )
                }
            }
        }
    }
}

@Composable
private fun ModeOption(title: String, description: String, selected: Boolean, modifier: Modifier, onClick: () -> Unit) {
    val art = com.lumen.ui.theme.expressivePalette()
    val ink = if (selected) art.onStage else MaterialTheme.colorScheme.onSurface
    Column(modifier.expressiveControlLabel("$title. $description").height(138.dp).clip(com.lumen.ui.components.selectionShape(selected, 14.dp, 28.dp))
        .background(if (selected) art.stage else MaterialTheme.colorScheme.surfaceContainerLow)
        .selectable(selected, role = androidx.compose.ui.semantics.Role.RadioButton, onClick = onClick)
        .padding(12.dp), verticalArrangement = Arrangement.SpaceBetween) {
        RadioButton(selected, onClick = null, modifier = Modifier.size(24.dp),
            colors = androidx.compose.material3.RadioButtonDefaults.colors(
                selectedColor = art.onStage, unselectedColor = MaterialTheme.colorScheme.primary))
        Text(title, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Bold,
            color = ink, maxLines = 4)
    }
}

@Composable
private fun AppRoutingRow(
    app: AppEntryUiModel,
    isFirst: Boolean,
    isLast: Boolean,
    onToggle: () -> Unit
) {
    val shape = com.lumen.ui.components.selectionShape(app.isSelected, 14.dp, 24.dp)
    Row(
        modifier = Modifier
            .fillMaxWidth().padding(bottom = 6.dp)
            .clip(shape)
            .background(if (app.isSelected) MaterialTheme.colorScheme.primary.copy(alpha = .11f) else MaterialTheme.colorScheme.surfaceContainerLow)
            .then(Modifier.selectable(selected = app.isSelected, role = androidx.compose.ui.semantics.Role.Checkbox, onClick = onToggle))
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (app.icon != null) {
            Image(
                bitmap = app.icon,
                contentDescription = null,
                modifier = Modifier.size(40.dp)
            )
        } else {
            Box(
                modifier = Modifier
                    .size(40.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.14f)),
                contentAlignment = Alignment.Center
            ) {
                Text(
                    text = app.label.take(1).uppercase(),
                    color = MaterialTheme.colorScheme.primary,
                    fontWeight = FontWeight.ExtraBold
                )
            }
        }
        Spacer(Modifier.width(11.dp))
        Column(Modifier.weight(1f)) {
            Text(
                text = app.label,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Text(
                text = app.packageName,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        }
        Checkbox(checked = app.isSelected, onCheckedChange = null)
    }
}
