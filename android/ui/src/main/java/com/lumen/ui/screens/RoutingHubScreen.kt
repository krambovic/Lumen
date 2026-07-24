package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp

@Composable
fun RoutingHubScreen(
    onOpenDomainIp: () -> Unit,
    onOpenApps: () -> Unit,
    onOpenGeoResources: () -> Unit,
    onBack: (() -> Unit)? = null,
    modifier: Modifier = Modifier
) {
    val s = LocalStrings.current
    Column(modifier.fillMaxSize().padding(horizontal = 16.dp)) {
        LumenScreenHeader(title = s.routing, onBack = onBack)
        Spacer(Modifier.height(8.dp))
        RoutingMenuCard(s.domainIpRouting, onOpenDomainIp)
        Spacer(Modifier.height(10.dp))
        RoutingMenuCard(s.appRouting, onOpenApps)
        Spacer(Modifier.height(10.dp))
        RoutingMenuCard(s.geoResourceFiles, onOpenGeoResources)
    }
}

@Composable
private fun RoutingMenuCard(title: String, onClick: () -> Unit) {
    val shape = RoundedCornerShape(20.dp)
    Row(
        Modifier.fillMaxWidth()
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f), shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 18.dp, vertical = 18.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(title, style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
        Text("›", style = MaterialTheme.typography.headlineMedium, color = MaterialTheme.colorScheme.primary)
    }
}