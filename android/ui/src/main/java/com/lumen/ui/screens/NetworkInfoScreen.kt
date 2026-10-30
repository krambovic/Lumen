package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.OpenInNew
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.outlined.Public
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lumen.ui.components.*
import com.lumen.ui.theme.LumenMono
import com.lumen.ui.theme.expressivePalette
import java.text.DateFormat
import java.util.Date
import java.util.Locale

@Composable
fun NetworkInfoScreen(
    state: NetworkInfoUiState, serverName: String?, onRefresh: () -> Unit,
    onCopy: (String) -> Unit, onOpenLink: (String) -> Unit, modifier: Modifier = Modifier
) {
    val strings = LocalStrings.current
    val language = strings.localeCode
    fun t(value: NetworkInfoText) = value.inLanguage(language)
    val art = expressivePalette()
    val data = state.data
    Column(modifier.fillMaxSize().statusBarsPadding()) {
        BrandTopBar(title = t(NetworkInfoText.TITLE), actions = {
            FilledTonalIconButton(onClick = onRefresh, enabled = !state.loading,
                modifier = Modifier.size(48.dp)) { Icon(Icons.Default.Refresh, strings.refresh) }
        })
        LazyColumn(Modifier.weight(1f).fillMaxWidth(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 108.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item("ip") {
                Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(32.dp, 14.dp, 32.dp, 14.dp))
                    .background(art.stage)) {
                    val rotation = rememberDecorativeRotation(42000)
                    ExpressiveMotif(Motif.Sun, art.onStage.copy(alpha = .08f),
                        Modifier.align(Alignment.TopEnd).offset(30.dp, (-24).dp).size(160.dp)
                            .graphicsLayer { rotationZ = rotation.value })
                    Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                            Text(t(NetworkInfoText.PUBLIC_IP), Modifier.weight(1f), style = MaterialTheme.typography.labelMedium,
                                color = art.onStage.copy(alpha = .7f))
                            Surface(color = art.lime, contentColor = art.onLime, shape = RoundedCornerShape(16.dp)) {
                                Text(t(if (state.throughVpn) NetworkInfoText.EXIT else NetworkInfoText.CURRENT),
                                    Modifier.padding(horizontal = 12.dp, vertical = 7.dp), style = MaterialTheme.typography.labelMedium)
                            }
                        }
                        SelectionContainer {
                            Text(data?.ip ?: "—", color = art.onStage, fontFamily = LumenMono,
                                style = MaterialTheme.typography.headlineLarge.copy(textDirection = TextDirection.Ltr),
                                fontWeight = FontWeight.Bold, fontSize = if (data?.ip?.contains(':') == true) 25.sp else 32.sp,
                                lineHeight = 36.sp, modifier = Modifier.fillMaxWidth())
                        }
                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Outlined.Public, null, tint = art.lime, modifier = Modifier.size(20.dp))
                            Text(when {
                                state.waiting -> t(NetworkInfoText.WAITING)
                                state.throughVpn -> serverName ?: t(NetworkInfoText.EXIT)
                                else -> t(NetworkInfoText.CURRENT)
                            }, Modifier.weight(1f).padding(start = 8.dp), color = art.onStage,
                                style = MaterialTheme.typography.bodyMedium)
                            if (data != null) IconButton(onClick = { onCopy(data.ip) }) {
                                Icon(Icons.Default.ContentCopy, t(NetworkInfoText.COPY), tint = art.lime)
                            }
                        }
                        if (state.loading) {
                            LinearProgressIndicator(Modifier.fillMaxWidth(), color = art.lime,
                                trackColor = art.onStage.copy(alpha = .12f))
                            if (!state.waiting) Text(t(NetworkInfoText.LOOKING), color = art.onStage.copy(alpha = .7f),
                                style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
            }
            if (state.failed) item("error") {
                Surface(shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.errorContainer) {
                    Column(Modifier.padding(16.dp)) {
                        Text(t(NetworkInfoText.FAILED), color = MaterialTheme.colorScheme.onErrorContainer)
                        TextButton(onClick = onRefresh) { Text(strings.refresh) }
                    }
                }
            }
            item("location") {
                Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    NetworkInfoTile(t(NetworkInfoText.LOCATION), art.blue, art.onBlue, Modifier.weight(1f).fillMaxHeight()) {
                        val flag = CountryFlagHelper.getFlagEmoji(data?.countryCode)
                        if (flag.isNotEmpty()) Text(flag, fontSize = 28.sp)
                        Text(data?.country ?: "—", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                        Text(listOfNotNull(data?.city, data?.region).distinct().joinToString(" · ").ifBlank { "—" },
                            style = MaterialTheme.typography.bodyMedium)
                    }
                    NetworkInfoTile(t(NetworkInfoText.TIMEZONE), art.lavender, art.onLavender, Modifier.weight(1f).fillMaxHeight()) {
                        Text(data?.timezone?.substringAfterLast('/')?.replace('_', ' ') ?: "—",
                            style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                        Text(data?.timezone ?: "—", style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
            item("provider") {
                NetworkInfoTile(t(NetworkInfoText.PROVIDER), art.coral, art.onCoral) {
                    Text(data?.isp ?: data?.organization ?: "—", style = MaterialTheme.typography.titleLarge,
                        fontWeight = FontWeight.Bold)
                    if (data?.asn != null) Text(data.asn, fontFamily = LumenMono, style = MaterialTheme.typography.titleMedium)
                }
            }
            item("details") {
                SettingsPanel(t(NetworkInfoText.DETAILS), if (data?.ip?.contains(':') == true) "IPv6" else if (data != null) "IPv4" else "—",
                    MaterialTheme.colorScheme.surfaceContainerLow, MaterialTheme.colorScheme.onSurface) {
                    NetworkInfoRow(t(NetworkInfoText.COUNTRY), data?.country)
                    NetworkInfoRow(t(NetworkInfoText.REGION), data?.region)
                    NetworkInfoRow(t(NetworkInfoText.CITY), data?.city)
                    NetworkInfoRow(t(NetworkInfoText.POSTAL), data?.postal)
                    NetworkInfoRow(t(NetworkInfoText.ORGANIZATION), data?.organization)
                    NetworkInfoRow("ASN", data?.asn)
                    val coordinates = remember(data?.latitude, data?.longitude) {
                        if (data?.latitude != null && data.longitude != null)
                            String.format(Locale.US, "%.4f, %.4f", data.latitude, data.longitude) else null
                    }
                    NetworkInfoRow(t(NetworkInfoText.LOCATION), coordinates)
                    if (coordinates != null) TextButton(onClick = { onOpenLink("geo:${data?.latitude},${data?.longitude}?q=${data?.latitude},${data?.longitude}") }) {
                        Icon(Icons.Default.OpenInNew, null, Modifier.size(16.dp))
                        Text(t(NetworkInfoText.MAP), Modifier.padding(start = 8.dp))
                    }
                    Text(t(NetworkInfoText.APPROXIMATE), style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            item("classification") {
                SettingsPanel(t(NetworkInfoText.CLASSIFICATION), "IP", art.lavender, art.onLavender) {
                    val values = listOf(NetworkInfoText.VPN to data?.vpn, NetworkInfoText.PROXY to data?.proxy,
                        NetworkInfoText.TOR to data?.tor, NetworkInfoText.DATACENTER to data?.datacenter,
                        NetworkInfoText.MOBILE to data?.mobile)
                    values.forEach { (label, value) -> NetworkInfoRow(t(label), t(when (value) {
                        true -> NetworkInfoText.YES; false -> NetworkInfoText.NO; null -> NetworkInfoText.UNKNOWN
                    })) }
                    Text(t(NetworkInfoText.CLASSIFICATION_HINT), style = MaterialTheme.typography.bodySmall)
                }
            }
            if (data != null) item("source") {
                val checked = remember(state.updatedAt, language) {
                    state.updatedAt?.let { DateFormat.getTimeInstance(DateFormat.SHORT, Locale.forLanguageTag(language)).format(Date(it)) }
                }
                TextButton(onClick = { onOpenLink(data.sourceUrl) }) { Text("${t(NetworkInfoText.SOURCE)} · ${data.source}") }
                if (checked != null) Text("${t(NetworkInfoText.UPDATED)} $checked", Modifier.padding(horizontal = 12.dp),
                    style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}

@Composable
private fun NetworkInfoTile(title: String, container: Color, ink: Color, modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit) {
    Surface(modifier.fillMaxWidth(), color = container, contentColor = ink, shape = RoundedCornerShape(26.dp, 12.dp, 26.dp, 12.dp)) {
        Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(title, style = MaterialTheme.typography.labelMedium)
            content()
        }
    }
}

@Composable
private fun NetworkInfoRow(label: String, value: String?) {
    Row(Modifier.fillMaxWidth().padding(vertical = 5.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        Text(label, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
        SelectionContainer(Modifier.weight(1.25f)) {
            Text(value ?: "—", style = MaterialTheme.typography.bodyMedium.copy(textDirection = TextDirection.ContentOrLtr),
                fontWeight = FontWeight.Medium)
        }
    }
}
