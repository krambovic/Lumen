package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import java.text.DateFormat
import java.util.Date

private val GEO_SOURCES = listOf(
    "https://github.com/Loyalsoldier/v2ray-rules-dat/",
    "https://github.com/runetfreedom/russia-v2ray-rules-dat/",
    "https://github.com/Chocolate4U/Iran-v2ray-rules/"
)

@Composable
fun GeoResourcesScreen(
    resources: List<GeoResourceUiModel>,
    source: String,
    isUpdating: Boolean,
    onSourceChange: (String) -> Unit,
    onDownload: () -> Unit,
    onBack: (() -> Unit)? = null,
    modifier: Modifier = Modifier
) {
    val s = LocalStrings.current
    Column(
        modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp)
    ) {
        LumenScreenHeader(
            title = s.geoResourceFiles,
            onBack = onBack,
            actions = {
                if (isUpdating) CircularProgressIndicator(Modifier.padding(8.dp))
                else TextButton(onClick = onDownload) { Text(s.refresh) }
            }
        )
        Spacer(Modifier.height(8.dp))
        LumenDropdown(
            label = s.geoSourceLabel,
            options = GEO_SOURCES,
            selected = source,
            onSelected = onSourceChange
        )
        SectionHeader(s.geoResourceFiles.uppercase())
        if (resources.isEmpty()) {
            Text(
                s.geoNotLoaded,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(vertical = 12.dp)
            )
        }
        resources.forEach { resource ->
            GeoResourceCard(resource)
            Spacer(Modifier.height(10.dp))
        }
    }
}

@Composable
private fun GeoResourceCard(resource: GeoResourceUiModel) {
    val shape = RoundedCornerShape(20.dp)
    Column(
        Modifier.fillMaxWidth().clip(shape)
            .background(MaterialTheme.colorScheme.surface)
            .border(1.dp, MaterialTheme.colorScheme.outline.copy(alpha = 0.35f), shape)
            .padding(horizontal = 18.dp, vertical = 16.dp)
    ) {
        Text(resource.name, style = MaterialTheme.typography.titleLarge)
        Spacer(Modifier.height(8.dp))
        Text(
            "${formatGeoBytes(resource.sizeBytes)} • ${DateFormat.getDateTimeInstance().format(Date(resource.modifiedAt))}",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    }
}

internal fun formatGeoBytes(bytes: Long): String {
    if (bytes < 1024L) return "$bytes B"
    val units = arrayOf("KB", "MB", "GB")
    var value = bytes.toDouble()
    var unit = -1
    while (value >= 1024.0 && unit < units.lastIndex) {
        value /= 1024.0
        unit++
    }
    return String.format(java.util.Locale.US, "%.1f %s", value, units[unit.coerceAtLeast(0)])
}