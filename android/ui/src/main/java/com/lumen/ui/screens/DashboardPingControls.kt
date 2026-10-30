package com.lumen.ui.screens

import androidx.compose.foundation.layout.*
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
import com.lumen.ui.components.expressiveButtonSemantics
import com.lumen.ui.theme.LumenMono

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
