package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp

@Composable
internal fun ServerLayoutPicker(state: SettingsUiState, onUpdate: (SettingsUiState) -> Unit) {
    val s = LocalStrings.current
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(s.serverLayout, style = MaterialTheme.typography.titleMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            listOf(false to s.serverLayoutList, true to s.serverLayoutGrid).forEach { (grid, title) ->
                OutlinedCard(onClick = { onUpdate(state.copy(compactServerGrid = grid)) }, Modifier.weight(1f),
                    colors = CardDefaults.outlinedCardColors(containerColor = if (state.compactServerGrid == grid)
                        MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainerLow)) {
                    Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        repeat(2) { Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                            repeat(if (grid) 3 else 1) { Box(Modifier.weight(1f).height(18.dp)
                                .clip(RoundedCornerShape(5.dp)).background(MaterialTheme.colorScheme.primary.copy(alpha = .25f))) }
                        } }
                        Text(title, style = MaterialTheme.typography.labelLarge)
                        RadioButton(state.compactServerGrid == grid, onClick = null)
                    }
                }
            }
        }
    }
}
