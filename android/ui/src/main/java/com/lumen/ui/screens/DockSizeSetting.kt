package com.lumen.ui.screens

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.lumen.ui.components.DockSize

@Composable
internal fun DockSizeSetting(state: SettingsUiState, onUpdate: (SettingsUiState) -> Unit) {
    val s = LocalStrings.current
    val selected = DockSize.fromPercent(state.dockScalePercent)
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(InterfaceText.DOCK_SIZE.inLanguage(s.localeCode), style = MaterialTheme.typography.titleMedium)
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            DockSize.entries.forEach { size ->
                val title = when (size) {
                    DockSize.SMALL -> InterfaceText.SMALL
                    DockSize.DEFAULT -> InterfaceText.DEFAULT
                    DockSize.LARGE -> InterfaceText.LARGE
                }.inLanguage(s.localeCode)
                OutlinedCard(onClick = { onUpdate(state.copy(dockScalePercent = size.percent)) },
                    modifier = Modifier.weight(1f),
                    colors = CardDefaults.outlinedCardColors(containerColor = if (selected == size)
                        MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainerLow)) {
                    Column(Modifier.padding(horizontal = 10.dp, vertical = 14.dp),
                        verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(title, style = MaterialTheme.typography.labelLarge, maxLines = 1,
                            overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                        RadioButton(selected == size, onClick = null)
                    }
                }
            }
        }
    }
}
