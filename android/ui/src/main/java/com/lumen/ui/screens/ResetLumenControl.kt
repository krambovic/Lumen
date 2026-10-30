package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import com.lumen.ui.components.ExpressiveAction
import com.lumen.ui.components.LumenDialog
import com.lumen.ui.components.selectionShape

@Composable
internal fun ResetLumenControl(onReset: (LumenResetTarget) -> Unit) {
    val s = LocalStrings.current
    var open by remember { mutableStateOf(false) }
    var target by remember { mutableStateOf(LumenResetTarget.SETTINGS) }
    SectionHeader(s.resetLumen)
    ExpressiveAction(s.resetLumen, { open = true }, Modifier.fillMaxWidth(),
        container = MaterialTheme.colorScheme.errorContainer, content = MaterialTheme.colorScheme.onErrorContainer)
    if (open) LumenDialog(s.resetLumen, onDismissRequest = { open = false },
        message = s.resetWarning, confirmText = s.resetAction, dismissText = s.cancel,
        destructive = true, onConfirm = { open = false; onReset(target) }, onDismiss = { open = false }, content = {
            LumenResetTarget.entries.forEach { option ->
                val selected = option == target
                Row(Modifier.fillMaxWidth().padding(vertical = 4.dp).clip(selectionShape(selected))
                    .background(if (selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainer)
                    .clickable { target = option }.padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
                    RadioButton(selected, { target = option })
                    Column(Modifier.weight(1f).padding(start = 6.dp)) {
                        Text(if (option == LumenResetTarget.SETTINGS) s.resetSettings else s.resetServers,
                            style = MaterialTheme.typography.titleSmall)
                        Text(if (option == LumenResetTarget.SETTINGS) s.resetSettingsHint else s.resetServersHint,
                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
        })
}
