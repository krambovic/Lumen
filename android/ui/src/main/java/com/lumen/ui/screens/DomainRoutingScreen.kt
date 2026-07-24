package com.lumen.ui.screens

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.ContentPaste
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

data class DomainRuleItem(
    val address: String,
    val action: String // "direct", "proxy", "block"
)

private fun parseRulesString(raw: String): List<DomainRuleItem> {
    if (raw.isBlank()) return emptyList()
    return raw.split(Regex("[\\n,;]+")).mapNotNull { item ->
        val trimmed = item.trim()
        if (trimmed.isEmpty()) null
        else {
            val action = when {
                trimmed.startsWith("proxy:", true) -> "proxy"
                trimmed.startsWith("block:", true) || trimmed.startsWith("reject:", true) -> "block"
                trimmed.startsWith("direct:", true) -> "direct"
                else -> "direct"
            }
            val addr = if (trimmed.contains(":")) trimmed.substringAfter(":") else trimmed
            DomainRuleItem(addr.trim(), action)
        }
    }
}

private fun serializeRules(rules: List<DomainRuleItem>): String {
    return rules.joinToString("\n") { rule ->
        when (rule.action) {
            "proxy" -> "proxy:${rule.address}"
            "block" -> "block:${rule.address}"
            else -> "direct:${rule.address}"
        }
    }
}

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
    var showActionMenu by remember { mutableStateOf(false) }

    val rules = remember(directDomains) { parseRulesString(directDomains) }

    fun updateRules(newRules: List<DomainRuleItem>) {
        onDirectRulesChange(serializeRules(newRules), directIpCidrs)
    }

    fun addRule(address: String, action: String) {
        val trimmed = address.trim()
        if (trimmed.isNotBlank()) {
            val existing = rules.filterNot { it.address.equals(trimmed, ignoreCase = true) }
            updateRules(existing + DomainRuleItem(trimmed, action))
            newAddressInput = ""
        }
    }

    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp)
    ) {
        LumenScreenHeader(title = s.domainIpRouting, onBack = onBack)

        Text(
            s.directRulesNotice,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(bottom = 12.dp)
        )

        // Quick Input Row (+ Add bar)
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(16.dp))
                .border(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f), RoundedCornerShape(16.dp)),
            color = MaterialTheme.colorScheme.surfaceVariant,
            shape = RoundedCornerShape(16.dp)
        ) {
            Column(Modifier.padding(12.dp)) {
                OutlinedTextField(
                    value = newAddressInput,
                    onValueChange = { newAddressInput = it },
                    placeholder = { Text("example.com, geosite:ru, 192.168.1.0/24") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth()
                )
                Spacer(Modifier.height(10.dp))
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    // Action dropdown selector
                    Box {
                        Surface(
                            modifier = Modifier
                                .clip(RoundedCornerShape(12.dp))
                                .border(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.4f), RoundedCornerShape(12.dp))
                                .clickable { showActionMenu = true },
                            color = MaterialTheme.colorScheme.surfaceContainerHigh,
                            shape = RoundedCornerShape(12.dp)
                        ) {
                            Row(
                                modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                Text(
                                    text = when (selectedAction) {
                                        "proxy" -> s.proxyAction
                                        "block" -> s.blockAction
                                        else -> s.directAction
                                    },
                                    style = MaterialTheme.typography.bodyMedium,
                                    fontWeight = FontWeight.Bold,
                                    color = MaterialTheme.colorScheme.primary
                                )
                                Spacer(Modifier.width(6.dp))
                                Text("▾", color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold)
                            }
                        }

                        val menuShape = RoundedCornerShape(4.dp)
                        DropdownMenu(
                            expanded = showActionMenu,
                            onDismissRequest = { showActionMenu = false },
                            modifier = Modifier
                                .widthIn(min = 180.dp)
                                .clip(menuShape)
                                .background(MaterialTheme.colorScheme.surface)
                                .border(BorderStroke(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.35f)), menuShape)
                        ) {
                            DropdownMenuItem(
                                text = { Text(s.directAction, color = MaterialTheme.colorScheme.onSurface) },
                                onClick = { selectedAction = "direct"; showActionMenu = false }
                            )
                            DropdownMenuItem(
                                text = { Text(s.proxyAction, color = MaterialTheme.colorScheme.onSurface) },
                                onClick = { selectedAction = "proxy"; showActionMenu = false }
                            )
                            DropdownMenuItem(
                                text = { Text(s.blockAction, color = MaterialTheme.colorScheme.onSurface) },
                                onClick = { selectedAction = "block"; showActionMenu = false }
                            )
                        }
                    }

                    // Add Button
                    Surface(
                        modifier = Modifier
                            .clip(RoundedCornerShape(12.dp))
                            .clickable { addRule(newAddressInput, selectedAction) },
                        color = MaterialTheme.colorScheme.primary,
                        shape = RoundedCornerShape(12.dp)
                    ) {
                        Row(
                            modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Icon(Icons.Filled.Add, contentDescription = null, tint = MaterialTheme.colorScheme.onPrimary, modifier = Modifier.size(18.dp))
                            Spacer(Modifier.width(4.dp))
                            Text(s.add, color = MaterialTheme.colorScheme.onPrimary, style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)
                        }
                    }
                }
            }
        }

        Spacer(Modifier.height(10.dp))

        // Actions Row (Paste & Clear all)
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Surface(
                    modifier = Modifier
                        .clip(RoundedCornerShape(10.dp))
                        .border(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f), RoundedCornerShape(10.dp))
                        .clickable {
                            val text = clipboard.getText()?.text
                            if (!text.isNullOrBlank()) {
                                val pasted = parseRulesString(text)
                                if (pasted.isNotEmpty()) {
                                    updateRules((rules + pasted).distinctBy { it.address.lowercase() })
                                }
                            }
                        },
                    color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f)
                ) {
                    Row(Modifier.padding(horizontal = 10.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Filled.ContentPaste, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(4.dp))
                        Text(s.paste, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurface, fontWeight = FontWeight.SemiBold)
                    }
                }
            }

            if (rules.isNotEmpty()) {
                Surface(
                    modifier = Modifier
                        .clip(RoundedCornerShape(10.dp))
                        .clickable { updateRules(emptyList()) },
                    color = MaterialTheme.colorScheme.errorContainer.copy(alpha = 0.3f)
                ) {
                    Row(Modifier.padding(horizontal = 10.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Filled.Delete, contentDescription = null, tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(4.dp))
                        Text(s.clearAll, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error, fontWeight = FontWeight.SemiBold)
                    }
                }
            }
        }

        Spacer(Modifier.height(12.dp))

        // Table Card
        Surface(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .clip(RoundedCornerShape(18.dp))
                .border(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f), RoundedCornerShape(18.dp)),
            color = MaterialTheme.colorScheme.surfaceVariant,
            shape = RoundedCornerShape(18.dp)
        ) {
            Column(Modifier.fillMaxSize()) {
                // Table Header
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(MaterialTheme.colorScheme.surfaceContainerHigh.copy(alpha = 0.5f))
                        .padding(horizontal = 14.dp, vertical = 10.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        s.address,
                        style = MaterialTheme.typography.bodySmall,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.weight(1f)
                    )
                    Text(
                        s.action,
                        style = MaterialTheme.typography.bodySmall,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.width(110.dp)
                    )
                    Spacer(Modifier.width(36.dp))
                }

                if (rules.isEmpty()) {
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .padding(24.dp),
                        contentAlignment = Alignment.Center
                    ) {
                        Text(
                            s.noRulesYet,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                } else {
                    LazyColumn(
                        modifier = Modifier.fillMaxSize()
                    ) {
                        itemsIndexed(rules, key = { idx, item -> "${idx}_${item.address}" }) { index, item ->
                            RuleRowItem(
                                item = item,
                                onActionChange = { newAct ->
                                    val updated = rules.toMutableList()
                                    updated[index] = item.copy(action = newAct)
                                    updateRules(updated)
                                },
                                onDelete = {
                                    val updated = rules.toMutableList()
                                    updated.removeAt(index)
                                    updateRules(updated)
                                }
                            )
                        }
                    }
                }
            }
        }
        Spacer(Modifier.height(16.dp))
    }
}

@Composable
private fun RuleRowItem(
    item: DomainRuleItem,
    onActionChange: (String) -> Unit,
    onDelete: () -> Unit
) {
    val s = LocalStrings.current
    var showMenu by remember { mutableStateOf(false) }

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .border(
                0.5.dp,
                MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.2f),
                RoundedCornerShape(0.dp)
            )
            .padding(horizontal = 14.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        // Address
        Text(
            text = item.address,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f)
        )

        // Action Combo Button
        Box(modifier = Modifier.width(110.dp)) {
            Surface(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(8.dp))
                    .border(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.35f), RoundedCornerShape(8.dp))
                    .clickable { showMenu = true },
                color = MaterialTheme.colorScheme.surface,
                shape = RoundedCornerShape(8.dp)
            ) {
                Row(
                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 6.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        text = when (item.action) {
                            "proxy" -> s.proxyAction
                            "block" -> s.blockAction
                            else -> s.directAction
                        },
                        style = MaterialTheme.typography.bodySmall,
                        fontWeight = FontWeight.Bold,
                        color = when (item.action) {
                            "proxy" -> MaterialTheme.colorScheme.primary
                            "block" -> MaterialTheme.colorScheme.error
                            else -> MaterialTheme.colorScheme.onSurface
                        },
                        fontSize = 12.sp
                    )
                    Text("▾", fontSize = 10.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }

            val menuShape = RoundedCornerShape(4.dp)
            DropdownMenu(
                expanded = showMenu,
                onDismissRequest = { showMenu = false },
                modifier = Modifier
                    .widthIn(min = 150.dp)
                    .clip(menuShape)
                    .background(MaterialTheme.colorScheme.surface)
                    .border(BorderStroke(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.35f)), menuShape)
            ) {
                DropdownMenuItem(
                    text = { Text(s.directAction, color = MaterialTheme.colorScheme.onSurface) },
                    trailingIcon = if (item.action == "direct") {
                        { Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(16.dp)) }
                    } else null,
                    onClick = { onActionChange("direct"); showMenu = false }
                )
                DropdownMenuItem(
                    text = { Text(s.proxyAction, color = MaterialTheme.colorScheme.onSurface) },
                    trailingIcon = if (item.action == "proxy") {
                        { Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(16.dp)) }
                    } else null,
                    onClick = { onActionChange("proxy"); showMenu = false }
                )
                DropdownMenuItem(
                    text = { Text(s.blockAction, color = MaterialTheme.colorScheme.error) },
                    trailingIcon = if (item.action == "block") {
                        { Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(16.dp)) }
                    } else null,
                    onClick = { onActionChange("block"); showMenu = false }
                )
            }
        }

        Spacer(Modifier.width(8.dp))

        // Delete button
        IconButton(
            onClick = onDelete,
            modifier = Modifier.size(28.dp)
        ) {
            Icon(
                imageVector = Icons.Filled.Delete,
                contentDescription = "Delete",
                tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                modifier = Modifier.size(18.dp)
            )
        }
    }
}