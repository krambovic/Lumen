package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.scrollBy
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
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.ContentPaste
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.DragHandle
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import com.lumen.ui.components.LumenDialog
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

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

private val LAN_PRESET = listOf(
    DomainRuleItem("10.0.0.0/8", "direct"),
    DomainRuleItem("172.16.0.0/12", "direct"),
    DomainRuleItem("192.168.0.0/16", "direct"),
    DomainRuleItem("127.0.0.0/8", "direct"),
    DomainRuleItem("169.254.0.0/16", "direct"),
    DomainRuleItem("224.0.0.0/4", "direct"),
    DomainRuleItem("::1/128", "direct"),
    DomainRuleItem("fc00::/7", "direct"),
    DomainRuleItem("fe80::/10", "direct"),
    DomainRuleItem("ff00::/8", "direct")
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
    val art = com.lumen.ui.theme.expressivePalette()
    val clipboard = LocalClipboardManager.current
    val density = LocalDensity.current
    var newAddressInput by remember { mutableStateOf("") }
    var selectedAction by remember { mutableStateOf("direct") }
    var editingRule by remember { mutableStateOf<DomainRuleItem?>(null) }
    var rules by remember(directDomains, directIpCidrs) {
        mutableStateOf(
            (parseRulesString(directDomains) + parseRulesString(directIpCidrs))
                .distinctBy { it.address.lowercase() }
        )
    }
    val listState = rememberLazyListState()
    val dragScope = rememberCoroutineScope()
    var draggedRuleKey by remember { mutableStateOf<String?>(null) }
    var draggedOffsetY by remember { mutableStateOf(0f) }
    var dragPointerY by remember { mutableStateOf(0f) }
    var dragStartRules by remember { mutableStateOf<List<DomainRuleItem>?>(null) }
    var autoScrollJob by remember { mutableStateOf<Job?>(null) }

    fun ruleKey(rule: DomainRuleItem): String = "rule:${rule.address.lowercase()}"

    fun updateRules(newRules: List<DomainRuleItem>) {
        // Both fields are accepted for migration, but one ordered stream is persisted.
        // Splitting IP and domain entries made cross-type ordering impossible.
        rules = newRules
        onDirectRulesChange(serializeRules(newRules), "")
    }

    fun isPresetActive(preset: List<DomainRuleItem>): Boolean =
        preset.all { wanted ->
            rules.any { it.address.equals(wanted.address, true) && it.action == wanted.action }
        }

    fun togglePreset(preset: List<DomainRuleItem>) {
        val addresses = preset.map { it.address.lowercase() }.toSet()
        val withoutPreset = rules.filterNot { it.address.lowercase() in addresses }
        // A newly enabled preset should take effect before broader custom rules.
        updateRules(if (isPresetActive(preset)) withoutPreset else preset + withoutPreset)
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

    fun startRuleDrag(activeKey: String, localPointerY: Float) {
        val item = listState.layoutInfo.visibleItemsInfo.firstOrNull { it.key == activeKey } ?: return
        draggedRuleKey = activeKey
        draggedOffsetY = 0f
        dragPointerY = item.offset + localPointerY
        dragStartRules = rules
    }

    fun dragRuleBy(activeKey: String, amountY: Float) {
        if (draggedRuleKey != activeKey) return
        draggedOffsetY += amountY
        dragPointerY += amountY

        val layoutInfo = listState.layoutInfo
        val currentInfo = layoutInfo.visibleItemsInfo.firstOrNull { it.key == activeKey } ?: return
        val draggedCenter = currentInfo.offset + currentInfo.size / 2f + draggedOffsetY
        val targetInfo = layoutInfo.visibleItemsInfo.firstOrNull { info ->
            val key = info.key as? String
            key?.startsWith("rule:") == true && key != activeKey &&
                draggedCenter >= info.offset && draggedCenter <= info.offset + info.size
        }
        if (targetInfo != null) {
            val fromIndex = rules.indexOfFirst { ruleKey(it) == activeKey }
            val toIndex = rules.indexOfFirst { ruleKey(it) == targetInfo.key }
            if (fromIndex >= 0 && toIndex >= 0 && fromIndex != toIndex) {
                draggedOffsetY += (currentInfo.offset - targetInfo.offset).toFloat()
                rules = rules.move(fromIndex, toIndex)
            }
        }

        val edge = with(density) { 56.dp.toPx() }
        val scrollDelta = when {
            dragPointerY < layoutInfo.viewportStartOffset + edge -> -18f
            dragPointerY > layoutInfo.viewportEndOffset - edge -> 18f
            else -> 0f
        }
        if (scrollDelta != 0f && autoScrollJob?.isActive != true) {
            autoScrollJob = dragScope.launch {
                val consumed = listState.scrollBy(scrollDelta)
                draggedOffsetY += consumed
            }
        }
    }

    fun finishRuleDrag(commit: Boolean) {
        autoScrollJob?.cancel()
        if (commit && draggedRuleKey != null) {
            updateRules(rules)
        } else if (!commit) {
            dragStartRules?.let { rules = it }
        }
        draggedRuleKey = null
        draggedOffsetY = 0f
        dragStartRules = null
    }

    LazyColumn(
        state = listState,
        modifier = modifier
            .fillMaxSize().statusBarsPadding(),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 100.dp),
        verticalArrangement = Arrangement.spacedBy(0.dp)
    ) {
        item(key = "header") {
            LumenScreenHeader(title = s.domainIpRouting, onBack = onBack, applyStatusBarPadding = false)
        }
        item(key = "notice") {
            SettingsPanel(s.routing, rules.size.toString().padStart(2, '0'), art.stage, art.onStage) {
                Text(s.directRulesNotice, style = MaterialTheme.typography.bodySmall,
                    color = art.onStage.copy(alpha = .8f))
            }
        }
        item(key = "presets_title") {
            SectionHeader(s.routingPresets)
        }
        item(key = "presets") {
            Column {
                Column(Modifier.padding(vertical = 4.dp)) {
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
        item(key = "custom_title") { Spacer(Modifier.height(14.dp)) }
        item(key = "composer") {
            SettingsPanel(s.customRules, "+", art.lavender, art.onLavender) {
                Column {
                    ExpressiveTextField(
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
                key = { _, rule -> ruleKey(rule) }
            ) { index, rule ->
                val key = ruleKey(rule)
                RuleRow(
                    rule = rule,
                    actionLabel = when (rule.action) {
                        "proxy" -> s.proxyAction
                        "block" -> s.blockAction
                        else -> s.directAction
                    },
                    isFirst = index == 0,
                    isLast = index == rules.lastIndex,
                    isDragging = draggedRuleKey == key,
                    dragOffsetY = if (draggedRuleKey == key) draggedOffsetY else 0f,
                    onDragStart = { pointerY -> startRuleDrag(key, pointerY) },
                    onDrag = { amountY -> dragRuleBy(key, amountY) },
                    onDragEnd = { finishRuleDrag(commit = true) },
                    onDragCancel = { finishRuleDrag(commit = false) },
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
                ExpressiveTextField(
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
    label: String, selected: Boolean, modifier: Modifier = Modifier,
    destructive: Boolean = false, onClick: () -> Unit
) {
    val art = com.lumen.ui.theme.expressivePalette()
    val container = if (destructive) art.coral else art.blue
    val ink = if (destructive) art.onCoral else art.onBlue
    Box(modifier.height(124.dp).clip(com.lumen.ui.components.selectionShape(selected, 18.dp, 30.dp))
        .background(container).clickable(onClick = onClick)) {
        com.lumen.ui.components.ExpressiveMotif(
            if (destructive) com.lumen.ui.components.Motif.Sun else com.lumen.ui.components.Motif.Fan,
            ink.copy(alpha = .08f), Modifier.align(Alignment.BottomEnd).size(94.dp))
        Column(Modifier.fillMaxSize().padding(14.dp), verticalArrangement = Arrangement.SpaceBetween) {
            Box(Modifier.size(26.dp).clip(androidx.compose.foundation.shape.CircleShape)
                .background(if (selected) ink else androidx.compose.ui.graphics.Color.Transparent)
                .border(1.5.dp, ink.copy(alpha = .65f), androidx.compose.foundation.shape.CircleShape),
                contentAlignment = Alignment.Center) {
                if (selected) Icon(Icons.Filled.Check, null, Modifier.size(16.dp), tint = container)
            }
            Text(label, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Bold,
                color = ink, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
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
    val shape = com.lumen.ui.components.selectionShape(selected, 12.dp, 24.dp)
    Box(
        modifier = modifier
            .clip(shape)
            .background(
                if (selected) accent
                else MaterialTheme.colorScheme.primary.copy(alpha = .08f)
            )
            .clickable(onClick = onClick)
            .height(48.dp),
        contentAlignment = Alignment.Center
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodySmall,
            fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium,
            color = if (selected) {
                if (destructive) MaterialTheme.colorScheme.onError else MaterialTheme.colorScheme.onPrimary
            } else MaterialTheme.colorScheme.onSurfaceVariant,
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
    isDragging: Boolean,
    dragOffsetY: Float,
    onDragStart: (Float) -> Unit,
    onDrag: (Float) -> Unit,
    onDragEnd: () -> Unit,
    onDragCancel: () -> Unit,
    onEdit: () -> Unit,
    onDelete: () -> Unit
) {
    // Saving replaces the parent's rules state. A long-lived pointer handler must
    // use the new callbacks on subsequent gestures, not the pre-save list.
    val currentOnDragStart by rememberUpdatedState(onDragStart)
    val currentOnDrag by rememberUpdatedState(onDrag)
    val currentOnDragEnd by rememberUpdatedState(onDragEnd)
    val currentOnDragCancel by rememberUpdatedState(onDragCancel)
    val accent = when (rule.action) {
        "block" -> MaterialTheme.colorScheme.error
        "proxy" -> MaterialTheme.colorScheme.tertiary
        else -> MaterialTheme.colorScheme.primary
    }
    val shape = RoundedCornerShape(16.dp)
    Row(
        modifier = Modifier
            .zIndex(if (isDragging) 1f else 0f)
            .graphicsLayer {
                translationY = dragOffsetY
                shadowElevation = if (isDragging) 12.dp.toPx() else 0f
            }
            .fillMaxWidth().padding(bottom = 6.dp)
            .clip(shape)
            .background(accent.copy(alpha = .08f))
            .pointerInput(rule.address) {
                detectDragGesturesAfterLongPress(
                    onDragStart = { currentOnDragStart(it.y) },
                    onDrag = { change, amount ->
                        change.consume()
                        currentOnDrag(amount.y)
                    },
                    onDragEnd = { currentOnDragEnd() },
                    onDragCancel = { currentOnDragCancel() }
                )
            }
            .clickable(onClick = onEdit)
            .padding(start = 14.dp, top = 10.dp, bottom = 10.dp, end = 6.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
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
        Icon(
            Icons.Filled.DragHandle,
            contentDescription = "${LocalStrings.current.moveRuleUp} / ${LocalStrings.current.moveRuleDown}",
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(24.dp)
        )
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

internal fun <T> List<T>.move(from: Int, to: Int): List<T> {
    if (from !in indices || to !in indices || from == to) return this
    return toMutableList().apply {
        add(to, removeAt(from))
    }
}
