package com.lumen.ui.screens

import androidx.compose.foundation.layout.*
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.ui.graphics.lerp
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.NetworkPing
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import com.lumen.ui.theme.LumenMono
import androidx.compose.ui.platform.testTag
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDirection

@Composable
internal fun DashboardConnectionLatencyTile(
    node: NodeUiModel?, checking: Boolean, result: String?, onCheck: () -> Unit,
    visible: Boolean
) {
    val s = LocalStrings.current
    if (node == null) return
    val colors = MaterialTheme.colorScheme
    val label = LumenProfileText.PING.inLanguage(s.localeCode)
    val displayedPing = result ?: node.pingMs?.takeIf { it > 0 }?.let { "$it ms" } ?: "—"
    AnimatedVisibility(
        visible = visible,
        enter = expandVertically(spring(dampingRatio = .86f, stiffness = 280f), expandFrom = Alignment.Top) +
            fadeIn(tween(220, delayMillis = 70)),
        exit = shrinkVertically(spring(dampingRatio = 1f, stiffness = 380f), shrinkTowards = Alignment.Top) +
            fadeOut(tween(150))
    ) {
    Surface(shape = RoundedCornerShape(topStart = 12.dp, topEnd = 28.dp, bottomStart = 28.dp, bottomEnd = 12.dp),
        color = lerp(colors.surfaceContainerLow, colors.primaryContainer, .18f), contentColor = colors.onSurface,
        border = BorderStroke(1.dp, colors.outlineVariant.copy(alpha = .45f)),
        modifier = Modifier.fillMaxWidth().padding(bottom = 10.dp)
            .testTag("dashboard_connected_ping").semantics {
                contentDescription = "$label: ${node.name}"
                stateDescription = if (checking) LumenProfileText.MEASURING.inLanguage(s.localeCode) else displayedPing
            }) {
        Row(Modifier.heightIn(min = 100.dp).padding(16.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            Column(Modifier.weight(1f)) {
                Text(label, style = MaterialTheme.typography.labelMedium)
                Text(displayedPing, fontFamily = LumenMono, fontWeight = FontWeight.SemiBold,
                    style = MaterialTheme.typography.headlineMedium.copy(textDirection = TextDirection.Ltr),
                    modifier = Modifier.padding(top = 4.dp), maxLines = 1)
                Text(if (checking) LumenProfileText.MEASURING.inLanguage(s.localeCode) else node.name,
                    style = MaterialTheme.typography.bodySmall,
                    color = colors.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            FilledIconButton(onClick = onCheck, modifier = Modifier.size(52.dp),
                shape = com.lumen.ui.components.selectionShape(checking, 26.dp, 16.dp),
                colors = IconButtonDefaults.filledIconButtonColors(containerColor = colors.primaryContainer,
                    contentColor = colors.onPrimaryContainer)) {
                Icon(if (checking) Icons.Default.Stop else Icons.Default.NetworkPing,
                    if (checking) s.cancel else LumenProfileText.CHECK.inLanguage(s.localeCode), Modifier.size(24.dp))
            }
        }
    }
    }
}

@Composable
internal fun DashboardPingAllButton(pinging: Boolean, enabled: Boolean, onPing: () -> Unit, onStop: () -> Unit) {
    val s = LocalStrings.current
    TextButton(if (pinging) onStop else onPing, enabled = enabled) {
        Icon(if (pinging) Icons.Default.Stop else Icons.Default.NetworkPing, null, Modifier.size(18.dp))
        Text(if (pinging) s.cancel else s.ping, Modifier.padding(start = 6.dp))
    }
}

@Composable
internal fun DashboardNodePing(node: NodeUiModel, pinging: Boolean, onPing: () -> Unit) {
    val s = LocalStrings.current
    TextButton(onPing, enabled = !pinging,
        modifier = Modifier.semantics { contentDescription = "${s.ping}: ${node.name}" },
        contentPadding = PaddingValues(horizontal = 6.dp)) {
        Box(Modifier.widthIn(min = 44.dp).height(24.dp), contentAlignment = Alignment.Center) {
            if (pinging) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
            else Text(node.pingMs?.takeIf { it > 0 }?.let { "$it ms" } ?: "—",
                fontFamily = LumenMono,
                color = node.pingMs?.let(::pingColor) ?: MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.labelSmall.copy(textDirection = androidx.compose.ui.text.style.TextDirection.Ltr))
        }
    }
}
