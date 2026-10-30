package com.lumen.ui.screens

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.lumen.ui.components.*
import com.lumen.ui.theme.expressivePalette

/** Read-only status: all connect, cancel and disconnect actions belong to the dock. */
@Composable
internal fun DockDashboardStatus(state: ConnectionState, protocol: String?) {
    val s = LocalStrings.current
    val art = expressivePalette()
    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(32.dp, 14.dp, 32.dp, 14.dp))
        .background(art.stage).padding(22.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(20.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(s.connection.uppercase(), color = art.onStage.copy(alpha = .65f), style = MaterialTheme.typography.labelMedium)
            AnimatedContent(state, modifier = Modifier.fillMaxWidth(),
                transitionSpec = { expressivePageEnter().togetherWith(expressivePageExit()).using(null) },
                label = "dock_dashboard_status") { current ->
                Text(when (current) {
                    ConnectionState.Connected -> s.centerConnected
                    ConnectionState.Connecting -> s.centerConnecting
                    ConnectionState.Error -> s.centerError
                    else -> s.off
                }, color = art.onStage, style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold,
                    maxLines = 1, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
            }
            if (!protocol.isNullOrBlank()) Text(protocol, color = art.onStage.copy(alpha = .75f),
                style = MaterialTheme.typography.bodyMedium)
        }
        Box(Modifier.size(64.dp).clip(expressiveShape(Motif.Pebble)).background(art.lime), contentAlignment = Alignment.Center) {
            LumenMark(art.onLime, Modifier.size(28.dp))
        }
    }
}
