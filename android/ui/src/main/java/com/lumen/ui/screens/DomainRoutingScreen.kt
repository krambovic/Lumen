package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.ContentPaste
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.lumen.ui.components.LumenDialog

data class DomainRuleItem(
    val address: String,
    val action: String // "direct", "proxy", "block"
)

private fun parseRulesString(raw: String): List<DomainRuleItem> {
    if (raw.isBlank()) return emptyList()
    return raw.split(Regex("[\\n,;]+")).mapNotNull { item ->
        val trimmed = item.trim()
        if (trimmed.isEmpty()) {
            null
        } else {
            val action = when {
                trimmed.startsWith("proxy:", true) -> "proxy"
                trimmed.startsWith("block:", true) || trimmed.startsWith("reject:", true) -> "block"
                else -> "direct"
            }
            val prefix = listOf("proxy:", "block:", "reject:", "direct:")
                .firstOrNull { trimmed.startsWith(it, true) }
            val address = if (prefix == null) trimmed else trimmed.substring(prefix.length).trim()
            DomainRuleItem(address.ifEmpty { trimmed }, action)
        }
    }
}

private fun serializeRules(rules: List<DomainRuleItem>): String =
    rules.joinToString("\n") { rule ->
        val prefix = when (rule.action) {
            "proxy" -> "proxy"
            "block" -> "block"
            else -> "direct"
        }
        "$prefix:${rule.address}"
    }

private fun isIpAddressRule(address: String): Boolean {
    val value = address.trim().lowercase()
    if (value.startsWith("geoip:") || value.startsWith("geosite:")) return false
    if (value.startsWith("full:") || value.startsWith("domain:") ||
        value.startsWith("keyword:") || value.startsWith("regexp:") ||
        value.startsWith("regex:")
    ) return false
    return value.contains('/') ||
        (value.isNotEmpty() && value.all { it.isDigit() || it == '.' || it == ':' })
}

private val LAN_PRESET = listOf(
    DomainRuleItem("10.0.0.0/8", "direct"),
    DomainRuleItem("172.16.0.0/12", "direct"),
    DomainRuleItem("192.168.0.0/16", "direct"),
    DomainRuleItem("127.0.0.0/8", "direct"),
    DomainRuleItem("fc00::/7", "direct")
)

private val ADS_PRESET = listOf(
    DomainRuleItem("geosite:category-ads-all", "block")
)

@Composable
fun DomainRoutingScreen(
    directDomains: String,
    directIpCidrs: String,
    onDirectRulesChange: (String, String) -> Unit,
    onBack: (() -> Unit)? = null,
    modifier: Modifier = Modifier
) {
    val s = LocalStrings.current
    val clipboard = LocalClipboardManager.current
    var newAddressInput by remember { mutableStateOf("") }
    var selectedAction by remember { mutableStateOf("direct") }
    var editingRule by remember { mutableStateOf<DomainRuleItem?>(null) }
    val rules = remember(directDomains, directIpCidrs) {
        (parseRulesString(directDomains) + parseRulesString(directIpCidrs))
            .distinctBy { it.address.lowercase() }
    }

    fun updateRules(newRules: List<DomainRuleItem>) {
        val (ipRules, domainRules) = newRules.partition { isIpAddressRule(it.address) }
        onDirectRulesChange(serializeRules(domainRules), serializeRules(ipRules))
    }

    fun isPresetActive(preset: List<DomainRuleItem>): Boolean =
        preset.all { wanted ->
            rules.any { it.address.equals(wanted.address, true) && it.action == wanted.action }
        }

    fun togglePreset(preset: List<DomainRuleItem>) {
        val addresses = preset.map { it.address.lowercase() }.toSet()
        val withoutPreset = rules.filterNot { it.address.lowercase() in addresses }
        updateRules(if (isPresetActive(preset)) withoutPreset else withoutPreset + preset)
    }

    fun addRule(address: String, action: String) {
        val trimmed = address.trim()
        if (trimmed.isEmpty()) return
        updateRules(
            rules.filterNot { it.address.equals(trimmed, ignoreCase = true) } +
                DomainRuleItem(trimmed, action)
        )
        newAddressInput = ""
    }

    LazyColumn(
        modifier = modifier
            .fillMaxSize(),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 20.dp),
        verticalArrangement = Arrangement.spacedBy(0.dp)
    ) {
        item(key = "header") {
            LumenScreenHeader(title = s.domainIpRouting, onBack = onBack)
        }
        item(key = "notice") {
            LumenCard {
                Text(
                    text = s.directRulesNotice,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
        }
        item(key = "presets_title") {
            SectionHeader(s.routingPresets)
        }
        item(key = "presets") {
            SettingsCard {
                Column(Modifier.padding(vertical = 12.dp)) {
                    Text(
                        text = s.routingPresetsDesc,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    Spacer(Modifier.height(10.dp))
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        PresetButton(
                            label = s.presetLan,
                            selected = isPresetActive(LAN_PRESET),
                            modifier = Modifier.weight(1f),
                            onClick = { togglePreset(LAN_PRESET) }
                        )
                        PresetButton(
                            label = s.presetAds,
                            selected = isPresetActive(ADS_PRESET),
                            destructive = true,
                            modifier = Modifier.weight(1f),
                            onClick = { togglePreset(ADS_PRESET) }
                        )
                    }
                }
            }
        }
        item(key = "custom_title") {
            SectionHeader(s.customRules)
        }
        item(key = "composer") {
            SettingsCard {
                Column(Modifier.padding(vertical = 12.dp)) {
                    OutlinedTextField(
                        value = newAddressInput,
                        onValueChange = { newAddressInput = it },
                        placeholder = { Text("example.com, geosite:ru, 192.168.1.0/24") },
                        singleLine = true,
                        shape = RoundedCornerShape(14.dp),
                        modifier = Modifier.fillMaxWidth()
                    )
                    Spacer(Modifier.height(10.dp))
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        ActionChip(
                            label = s.directAction,
                            selected = selectedAction == "direct",
                            modifier = Modifier.weight(1f)
                        ) { selectedAction = "direct" }
                        ActionChip(
                            label = s.proxyAction,
                            selected = selectedAction == "proxy",
                            modifier = Modifier.weight(1f)
                        ) { selectedAction = "proxy" }
                        ActionChip(
                            label = s.blockAction,
                            selected = selectedAction == "block",
                            destructive = true,
                            modifier = Modifier.weight(1f)
                        ) { selectedAction = "block" }
                    }
                    Spacer(Modifier.height(10.dp))
                    Button(
                        onClick = { addRule(newAddressInput, selectedAction) },
                        enabled = newAddressInput.isNotBlank(),
                        shape = RoundedCornerShape(14.dp),
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        Icon(Icons.Filled.Add, contentDescription = null, modifier = Modifier.size(18.dp))
                        Spacer(Modifier.width(6.dp))
                        Text(s.add, fontWeight = FontWeight.SemiBold)
                    }
                }
            }
        }
        item(key = "rule_actions") {
            Row(
                modifier = Modifier.fillMaxWidth().padding(top = 5.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                TextButton(
                    onClick = {
                        val pasted = clipboard.getText()?.text?.let(::parseRulesString).orEmpty()
                        if (pasted.isNotEmpty()) {
                            updateRules((rules + pasted).distinctBy { it.address.lowercase() })
                        }
                    }
                ) {
                    Icon(Icons.Filled.ContentPaste, contentDescription = null, modifier = Modifier.size(17.dp))
                    Spacer(Modifier.width(5.dp))
                    Text(s.paste)
                }
                if (rules.isNotEmpty()) {
                    TextButton(onClick = { updateRules(emptyList()) }) {
                        Icon(
                            Icons.Filled.Delete,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.error,
                            modifier = Modifier.size(17.dp)
                        )
                        Spacer(Modifier.width(5.dp))
                        Text(s.clearAll, color = MaterialTheme.colorScheme.error)
                    }
                }
            }
        }
        if (rules.isEmpty()) {
            item(key = "empty") {
                LumenCard {
                    Text(
                        text = s.noRulesYet,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }
        } else {
            itemsIndexed(
                rules,
                key = { _, rule -> rule.address.lowercase() }
            ) { index, rule ->
                RuleRow(
                    rule = rule,
                    actionLabel = when (rule.action) {
                        "proxy" -> s.proxyAction
                        "block" -> s.blockAction
                        else -> s.directAction
                    },
                    isFirst = index == 0,
                    isLast = index == rules.lastIndex,
                    onEdit = { editingRule = rule },
                    onDelete = {
                        updateRules(rules.filterIndexed { ruleIndex, _ -> ruleIndex != index })
                    }
                )
            }
        }
    }

    editingRule?.let { original ->
        var editorAddress by remember(original) { mutableStateOf(original.address) }
        var editorAction by remember(original) { mutableStateOf(original.action) }
        LumenDialog(
            title = s.edit,
            onDismissRequest = { editingRule = null },
            confirmText = s.saveAction,
            onConfirm = {
                val address = editorAddress.trim()
                if (address.isNotEmpty()) {
                    val withoutOriginal = rules.filterNot {
                        it.address.equals(original.address, ignoreCase = true)
                    }
                    updateRules(
                        withoutOriginal.filterNot {
                            it.address.equals(address, ignoreCase = true)
                        } + DomainRuleItem(address, editorAction)
                    )
                    editingRule = null
                }
            },
            dismissText = s.cancel,
            onDismiss = { editingRule = null }
        ) {
            Column {
                OutlinedTextField(
                    value = editorAddress,
                    onValueChange = { editorAddress = it },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                    shape = RoundedCornerShape(14.dp)
                )
                Spacer(Modifier.height(10.dp))
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    ActionChip(
                        label = s.directAction,
                        selected = editorAction == "direct",
                        modifier = Modifier.weight(1f)
                    ) { editorAction = "direct" }
                    ActionChip(
                        label = s.proxyAction,
                        selected = editorAction == "proxy",
                        modifier = Modifier.weight(1f)
                    ) { editorAction = "proxy" }
                    ActionChip(
                        label = s.blockAction,
                        selected = editorAction == "block",
                        destructive = true,
                        modifier = Modifier.weight(1f)
                    ) { editorAction = "block" }
                }
            }
        }
    }
}

@Composable
private fun PresetButton(
    label: String,
    selected: Boolean,
    modifier: Modifier = Modifier,
    destructive: Boolean = false,
    onClick: () -> Unit
) {
    val accent = if (destructive) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary
    val shape = RoundedCornerShape(14.dp)
    Row(
        modifier = modifier
            .clip(shape)
            .background(
                if (selected) accent.copy(alpha = 0.16f)
                else MaterialTheme.colorScheme.surface
            )
            .border(
                1.dp,
                if (selected) accent else MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.55f),
                shape
            )
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 11.dp),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (selected) {
            Icon(
                Icons.Filled.Check,
                contentDescription = null,
                tint = accent,
                modifier = Modifier.size(16.dp)
            )
            Spacer(Modifier.width(5.dp))
        }
        Text(
            text = label,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.SemiBold,
            color = if (selected || destructive) accent else MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
    }
}

@Composable
private fun ActionChip(
    label: String,
    selected: Boolean,
    modifier: Modifier = Modifier,
    destructive: Boolean = false,
    onClick: () -> Unit
) {
    val accent = if (destructive) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary
    val shape = RoundedCornerShape(12.dp)
    Box(
        modifier = modifier
            .clip(shape)
            .background(
                if (selected) accent.copy(alpha = 0.14f)
                else MaterialTheme.colorScheme.surface
            )
            .border(
                1.dp,
                if (selected) accent else MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.45f),
                shape
            )
            .clickable(onClick = onClick)
            .padding(vertical = 9.dp),
        contentAlignment = Alignment.Center
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodySmall,
            fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium,
            color = if (selected) accent else MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1
        )
    }
}

@Composable
private fun RuleRow(
    rule: DomainRuleItem,
    actionLabel: String,
    isFirst: Boolean,
    isLast: Boolean,
    onEdit: () -> Unit,
    onDelete: () -> Unit
) {
    val accent = when (rule.action) {
        "block" -> MaterialTheme.colorScheme.error
        "proxy" -> MaterialTheme.colorScheme.tertiary
        else -> MaterialTheme.colorScheme.primary
    }
    val shape = when {
        isFirst && isLast -> RoundedCornerShape(18.dp)
        isFirst -> RoundedCornerShape(topStart = 18.dp, topEnd = 18.dp)
        isLast -> RoundedCornerShape(bottomStart = 18.dp, bottomEnd = 18.dp)
        else -> RoundedCornerShape(0.dp)
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .clickable(onClick = onEdit)
            .padding(start = 14.dp, top = 10.dp, bottom = 10.dp, end = 6.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Box(
            modifier = Modifier
                .size(34.dp)
                .clip(RoundedCornerShape(11.dp))
                .background(accent.copy(alpha = 0.14f)),
            contentAlignment = Alignment.Center
        ) {
            Text(
                text = actionLabel.take(1).uppercase(),
                color = accent,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.ExtraBold
            )
        }
        Spacer(Modifier.width(11.dp))
        Column(Modifier.weight(1f)) {
            Text(
                text = rule.address,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Text(
                text = actionLabel,
                style = MaterialTheme.typography.bodySmall,
                color = accent,
                fontWeight = FontWeight.SemiBold
            )
        }
        IconButton(onClick = onEdit, modifier = Modifier.size(38.dp)) {
            Icon(
                Icons.Filled.Edit,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(18.dp)
            )
        }
        IconButton(onClick = onDelete, modifier = Modifier.size(38.dp)) {
            Icon(
                Icons.Filled.Delete,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.size(18.dp)
            )
        }
    }
}
