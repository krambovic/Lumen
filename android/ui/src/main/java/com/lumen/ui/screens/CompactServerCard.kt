package com.lumen.ui.screens

import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDirection
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lumen.ui.components.*
import com.lumen.ui.theme.expressivePalette

@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun CompactServerCard(node: NodeUiModel, checked: Boolean, pinging: Boolean,
    onClick: () -> Unit, onLongClick: () -> Unit, onEdit: () -> Unit, onPing: () -> Unit,
    onCopy: () -> Unit, onDelete: () -> Unit, modifier: Modifier = Modifier) {
    val s = LocalStrings.current
    val art = expressivePalette()
    var menu by remember { mutableStateOf(false) }
    val bg = if (checked) art.blue else if (node.isSelected) art.stage else MaterialTheme.colorScheme.surfaceContainerLow
    val ink = if (checked) art.onBlue else if (node.isSelected) art.onStage else MaterialTheme.colorScheme.onSurface
    val shape = selectionShape(checked || node.isSelected, 12.dp, 22.dp)
    Column(modifier.heightIn(min = 112.dp).clip(shape).background(bg)
        .combinedClickable(onClick = onClick, onLongClick = onLongClick).padding(9.dp),
        verticalArrangement = Arrangement.spacedBy(5.dp)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(CountryFlagHelper.getServerFlagEmoji(node.countryCode), Modifier.weight(1f), fontSize = 18.sp)
            if (checked) Icon(Icons.Default.CheckBox, s.selected, Modifier.size(18.dp), tint = ink)
            Box {
                IconButton({ menu = true }, Modifier.size(28.dp)) { Icon(Icons.Default.MoreVert, node.name, Modifier.size(18.dp), tint = ink) }
                LumenMenu(menu, { menu = false }) {
                    listOf(s.edit to onEdit, s.ping to onPing, s.copyLink to onCopy, s.delete to onDelete).forEach { (label, action) ->
                        DropdownMenuItem(text = { Text(label) }, onClick = { menu = false; action() })
                    }
                }
            }
        }
        Text(node.name, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.SemiBold,
            color = ink, maxLines = 2, overflow = TextOverflow.Ellipsis)
        val ping = node.pingMs
        Text(if (pinging) "…" else if (ping == null) "—" else if (ping > 0) "$ping ms" else s.pingUnreachable,
            style = MaterialTheme.typography.labelSmall.copy(textDirection = TextDirection.Ltr),
            color = if (pinging || ping == null) ink.copy(alpha = .7f) else pingColor(ping, bg))
    }
}
