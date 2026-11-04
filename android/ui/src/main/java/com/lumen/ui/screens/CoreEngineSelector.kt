package com.lumen.ui.screens

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@Composable
internal fun CoreEngineSelector(selected: String, onSelect: (String) -> Unit) {
    val s = LocalStrings.current
    SectionHeader(LumenProfileText.CORE.inLanguage(s.localeCode))
    LumenDropdown(
        label = "",
        options = listOf("HYBRID", "SINGBOX", "XRAY"),
        selected = selected,
        onSelected = onSelect,
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
        optionLabel = { when (it) { "SINGBOX" -> "sing-box"; "XRAY" -> "Xray"; else -> InterfaceText.HYBRID.inLanguage(s.localeCode) } }
    )
    Text(
        when (selected) {
            "XRAY" -> LumenProfileText.XRAY
            "SINGBOX" -> LumenProfileText.SINGBOX
            else -> LumenProfileText.HYBRID
        }.inLanguage(s.localeCode),
        modifier = Modifier.padding(horizontal = 14.dp, vertical = 4.dp),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant
    )
}

@Composable
internal fun SettingsSetupAction(onOpenWelcome: (() -> Unit)?) {
    val s = LocalStrings.current
    if (onOpenWelcome == null) return
    Button(onClick = onOpenWelcome, modifier = Modifier.fillMaxWidth().heightIn(min = 80.dp),
        shape = MaterialTheme.shapes.extraLarge, contentPadding = PaddingValues(20.dp)) {
        Icon(Icons.Default.Settings, null, Modifier.size(28.dp))
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(5.dp)) {
            Text(WelcomeText.REOPEN.inLanguage(s.localeCode), style = MaterialTheme.typography.titleMedium)
            Text(InterfaceText.SETUP_HINT.inLanguage(s.localeCode), style = MaterialTheme.typography.bodySmall)
        }
        Spacer(Modifier.width(8.dp))
        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.size(22.dp))
    }
}

@Composable
internal fun RecipientKeySetting(onClick: () -> Unit) {
    val s = LocalStrings.current
    SectionHeader(InterfaceText.PROTECTED_PROFILES.inLanguage(s.localeCode))
    OutlinedCard(onClick = onClick, modifier = Modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large,
        colors = CardDefaults.outlinedCardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
        Row(Modifier.padding(18.dp), verticalAlignment = Alignment.CenterVertically) {
            Surface(shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.secondaryContainer) {
                Icon(Icons.Default.Key, null, Modifier.padding(12.dp).size(24.dp),
                    tint = MaterialTheme.colorScheme.onSecondaryContainer)
            }
            Spacer(Modifier.width(14.dp))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(5.dp)) {
                Text(LumenProfileText.KEY.inLanguage(s.localeCode), style = MaterialTheme.typography.titleMedium)
                Text(InterfaceText.RECIPIENT_HINT.inLanguage(s.localeCode), style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.size(20.dp), tint = MaterialTheme.colorScheme.primary)
        }
    }
}
