package com.lumen.ui.screens

import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.fillMaxHeight
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Apps
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.Language
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

@Composable
fun RoutingHubScreen(
    onOpenDomainIp: () -> Unit, onOpenApps: () -> Unit, onOpenGeoResources: () -> Unit,
    onBack: (() -> Unit)? = null, appsSummary: String? = null, modifier: Modifier = Modifier
) {
    val s = LocalStrings.current
    val art = com.lumen.ui.theme.expressivePalette()
    Column(modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp).padding(bottom = 100.dp)) {
        SettingsDetailHeader(s.routing, s.settingsRoutingSummary, onBack, applyStatusBarPadding = true)
        Spacer(Modifier.height(12.dp))
        com.lumen.ui.components.ExpressiveNavigationTile(s.domainIpRouting, "", Icons.Filled.Language, "01",
            art.stage, art.onStage, art.onStage, com.lumen.ui.components.Motif.Fan,
            onOpenDomainIp, Modifier.fillMaxWidth().height(208.dp), prominent = true)
        Spacer(Modifier.height(8.dp))
        Row(Modifier.fillMaxWidth().height(210.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            com.lumen.ui.components.ExpressiveNavigationTile(s.appRouting, appsSummary.orEmpty(), Icons.Filled.Apps, "02",
                art.lime, art.onLime, art.onLime, com.lumen.ui.components.Motif.Pebble,
                onOpenApps, Modifier.weight(1.1f).fillMaxHeight())
            com.lumen.ui.components.ExpressiveNavigationTile(s.geoResourceFiles, "sing-box extended", Icons.Filled.Description, "03",
                art.lavender, art.onLavender, art.onLavender, com.lumen.ui.components.Motif.Sun,
                onOpenGeoResources, Modifier.weight(.9f).fillMaxHeight())
        }
    }
}
