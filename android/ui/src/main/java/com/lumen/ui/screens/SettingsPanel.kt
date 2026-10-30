package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.lumen.ui.theme.LumenMono

/** A section owns its colour; fields sit directly in it without another card shell. */
@Composable
internal fun SettingsPanel(
    title: String, index: String, container: Color, ink: Color,
    content: @Composable ColumnScope.() -> Unit
) {
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(28.dp, 12.dp, 28.dp, 12.dp))
        .background(container).padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text(title, style = MaterialTheme.typography.titleLarge, color = ink,
                modifier = Modifier.weight(1f))
            Text(index, fontFamily = LumenMono, style = MaterialTheme.typography.labelSmall,
                color = ink.copy(alpha = .65f), modifier = Modifier.padding(start = 12.dp, top = 4.dp))
        }
        content()
    }
}

internal fun dnsStrategyDisplay(value: String): String = when (value) {
    "prefer_ipv4" -> "IPv4 → IPv6"
    "prefer_ipv6" -> "IPv6 → IPv4"
    "ipv4_only" -> "IPv4"
    "ipv6_only" -> "IPv6"
    else -> value
}
