package com.lumen.ui.screens

import android.graphics.BitmapFactory
import androidx.compose.animation.core.*
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CutCornerShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Layers
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.PowerSettingsNew
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lumen.ui.components.AddServerSheet
import com.lumen.ui.components.BrandTopBar
import com.lumen.ui.components.ConnectionState
import com.lumen.ui.components.CountryFlagHelper
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL
import java.util.Locale

// Cache for Game Posters
private val posterMemoryCache = mutableMapOf<String, ImageBitmap>()

// === Global Esports Colors (Tactical Cyber) ===
private val CyberCyan = Color(0xFF00E5FF) // فیروزه‌ای لیزری
private val ToxicGreen = Color(0xFF00FF66) // سبز فسفری
private val WarningRed = Color(0xFFFF003C)
private val CarbonBlack = Color(0xFF070709) // مشکی کربنی خالص
private val PanelDark = Color(0xFF0E0E14)
private val PanelStroke = Color(0xFF1B1B26)
private val TextMuted = Color(0xFF7A8299)

@Composable
fun DashboardScreen(
    connectionState: ConnectionState,
    nodes: List<NodeUiModel>,
    subscriptions: List<SubscriptionUiModel>,
    serverGroups: List<ServerGroupUiModel> = emptyList(),
    speedStatsEnabled: Boolean = true,
    downloadSpeed: Long = 0L,
    uploadSpeed: Long = 0L,
    pingingNodeIds: Set<String> = emptySet(),
    dashboardStyle: DashboardStyle = DashboardStyle.DEFAULT,
    connectedPing: String? = null,
    isCheckingPing: Boolean = false,
    onCheckPing: () -> Unit = {},
    onToggleConnection: () -> Unit,
    onSelectNode: (NodeUiModel) -> Unit,
    onImportClipboard: () -> Unit,
    onImportFile: () -> Unit,
    onImportQr: () -> Unit,
    onAddManualNode: () -> Unit,
    onRefreshSubscription: (String) -> Unit = {},
    onDeleteSubscription: (String) -> Unit = {},
    onPingGroup: (String?) -> Unit = {},
    onEditNode: (NodeUiModel) -> Unit = {},
    onPingNode: (NodeUiModel) -> Unit = {},
    onCopyNodeLink: (NodeUiModel) -> Unit = {},
    onExportQrCode: (NodeUiModel) -> Unit = {},
    onDeleteNode: (NodeUiModel) -> Unit = {},
    onPingNodes: (List<NodeUiModel>) -> Unit = {},
    onStopPing: () -> Unit = {},
    onExportNodesText: (Set<String>) -> String = { "" },
    onExportSubscriptionText: (String?) -> String = { "" },
    onCopyText: (String) -> Unit = {},
    onShareText: (String) -> Unit = {},
    onOpenUrl: (String) -> Unit = {},
    onOpenServers: () -> Unit = {},
    modifier: Modifier = Modifier,
    selectedGroupFilter: String = "all",
    speedValues: () -> DashboardSpeeds = { DashboardSpeeds(downloadSpeed, uploadSpeed) }
) {
    val tick = rememberHapticTick()
    var showAddSheet by remember { mutableStateOf(false) }
    var showServerPicker by remember { mutableStateOf(false) }
    val openServerPicker: () -> Unit = { showServerPicker = true }

    val activeNode = nodes.firstOrNull { it.isSelected } ?: nodes.firstOrNull()
    val strings = LocalStrings.current
    val groupFilter = resolvedServerFilter(selectedGroupFilter, subscriptions, serverGroups)
    val groupNodes = remember(nodes, groupFilter) { nodesForServerFilter(nodes, groupFilter) }
    val groupTitle = strings.allGroups
    val scanQr = rememberQrScanRequest(onImportQr)

    val isConnected = connectionState == ConnectionState.Connected
    val isConnecting = connectionState == ConnectionState.Connecting
    val scrollState = rememberScrollState()

    // پس‌زمینه با هاله فیروزه‌ای بسیار تاریک
    val bgBrush = Brush.radialGradient(
        colors = listOf(Color(0xFF003344), CarbonBlack),
        center = Offset(500f, 0f),
        radius = 1500f
    )

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(bgBrush)
            .statusBarsPadding()
    ) {
        BrandTopBar(onAddClick = { showAddSheet = true })

        Box(modifier = Modifier.weight(1f)) {
            if (dashboardStyle == DashboardStyle.SLIDER) {
                androidx.compose.runtime.key(groupFilter) { 
                    DashboardServerTable(groupNodes, groupTitle, pingingNodeIds, onSelectNode, onOpenServers,
                        speedStatsEnabled, isConnected, speedValues,
                        onPingAll = { onPingNodes(groupNodes) }, onPingNode = { onPingNodes(listOf(it)) }, onStopPing = onStopPing)
                }
            } else {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .verticalScroll(scrollState)
                        .padding(horizontal = 16.dp, vertical = 6.dp)
                ) {
                    // 1. Tactical Hero Section
                    TacticalHeroSection(connectionState, onToggleConnection)
                    Spacer(modifier = Modifier.height(20.dp))

                    // 2. Global Latency Engine (Stats Bar)
                    val basePingValue = connectedPing ?: activeNode?.pingMs?.takeIf { it > 0 }?.toString()
                    val dlSpeed = if (isConnected) formatSpeedToMbps(speedValues().download) else "0.00"
                    TacticalStatsBar(basePing = basePingValue, dlSpeed = dlSpeed, isConnected = isConnected)
                    Spacer(modifier = Modifier.height(24.dp))

                    // 3. Game Boost Profiles
                    GameBoostSection(selectedGameId = "pubg")
                    Spacer(modifier = Modifier.height(24.dp))

                    // 4. Global Server Node Card
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Default.Layers, contentDescription=null, tint=CyberCyan, modifier=Modifier.size(16.dp))
                            Spacer(Modifier.width(6.dp))
                            Text(text = "GLOBAL ROUTING NODE", fontSize = 13.sp, fontWeight = FontWeight.Black, color = Color.White, fontFamily = FontFamily.Monospace)
                        }
                        Text(text = "[ SWITCH ]", fontSize = 11.sp, fontWeight = FontWeight.Bold, color = CyberCyan, modifier = Modifier.clickable(onClick = openServerPicker), fontFamily = FontFamily.Monospace)
                    }
                    Spacer(modifier = Modifier.height(8.dp))

                    if (activeNode != null) {
                        val flagEmoji = CountryFlagHelper.getServerFlagEmoji(activeNode.countryCode)
                        val serverShape = CutCornerShape(topStart = 16.dp, bottomEnd = 16.dp)
                        
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(serverShape)
                                .background(PanelDark)
                                .border(1.5.dp, if(isConnected) CyberCyan.copy(alpha=0.6f) else PanelStroke, serverShape)
                                .clickable(onClick = openServerPicker)
                                .padding(16.dp),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Box(
                                modifier = Modifier
                                    .size(46.dp)
                                    .clip(CutCornerShape(percent = 30))
                                    .background(CarbonBlack)
                                    .border(1.dp, PanelStroke, CutCornerShape(percent = 30)),
                                contentAlignment = Alignment.Center
                            ) {
                                Text(text = flagEmoji, fontSize = 22.sp)
                            }
                            Spacer(modifier = Modifier.width(14.dp))
                            Column(modifier = Modifier.weight(1f)) {
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Text(text = activeNode.name.uppercase(), fontSize = 14.sp, fontWeight = FontWeight.Black, color = Color.White, maxLines = 1, overflow = TextOverflow.Ellipsis, fontFamily = FontFamily.Monospace)
                                }
                                Spacer(modifier = Modifier.height(6.dp))
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Box(Modifier.size(6.dp).background(if(isConnected) ToxicGreen else WarningRed))
                                    Spacer(Modifier.width(4.dp))
                                    Text(text = "PROTOCOL: ${activeNode.displayProtocol}", fontSize = 10.sp, color = TextMuted, fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold)
                                }
                            }
                            Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null, tint = if(isConnected) CyberCyan else Color.White, modifier = Modifier.size(24.dp))
                        }
                    }

                    // Spacer حیاتی برای منوی پایین
                    Spacer(modifier = Modifier.height(160.dp))
                }
            }
        }
    }

    if (showServerPicker) DashboardServerPicker(groupNodes, groupTitle, onDismiss = { showServerPicker = false }, onSelect = { onSelectNode(it); showServerPicker = false }, pinging = pingingNodeIds, onPingAll = { onPingNodes(groupNodes) }, onPingNode = { onPingNodes(listOf(it)) }, onStopPing = onStopPing)
    if (showAddSheet) AddServerSheet(onDismissRequest = { showAddSheet = false }, onImportClipboard = onImportClipboard, onImportQr = scanQr, onImportFile = onImportFile, onAddManualNode = onAddManualNode)
}

// === TACTICAL CONNECTION HERO ===
@Composable
private fun TacticalHeroSection(connectionState: ConnectionState, onToggleConnection: () -> Unit) {
    val isConnected = connectionState == ConnectionState.Connected
    val isConnecting = connectionState == ConnectionState.Connecting

    val mainColor = if (isConnected) CyberCyan else TextMuted
    val statusText = when {
        isConnecting -> "ESTABLISHING\nUPLINK..."
        isConnected -> "SECURE UPLINK\nESTABLISHED >>"
        else -> "SYSTEM\nOFFLINE"
    }

    Row(
        modifier = Modifier.fillMaxWidth().padding(top = 10.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Box(
                modifier = Modifier
                    .clip(CutCornerShape(4.dp))
                    .background(mainColor.copy(alpha = 0.1f))
                    .border(1.dp, mainColor.copy(alpha = 0.4f), CutCornerShape(4.dp))
                    .padding(horizontal = 8.dp, vertical = 4.dp)
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Default.Shield, contentDescription = null, tint = mainColor, modifier = Modifier.size(14.dp))
                    Spacer(modifier = Modifier.width(6.dp))
                    Text(if(isConnected) "ENCRYPTED TUNNEL" else "NO CONNECTION", color = mainColor, fontSize = 10.sp, fontWeight = FontWeight.Black, fontFamily = FontFamily.Monospace)
                }
            }
            Spacer(modifier = Modifier.height(14.dp))
            Text(
                text = statusText,
                color = Color.White,
                fontSize = 22.sp,
                fontWeight = FontWeight.Black,
                fontFamily = FontFamily.Monospace,
                lineHeight = 28.sp,
                letterSpacing = 1.sp
            )
            Spacer(modifier = Modifier.height(8.dp))
            Text("ADVANCED LATENCY OPTIMIZATION", color = ToxicGreen.copy(alpha = if(isConnected) 1f else 0.3f), fontSize = 10.sp, fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold)
        }

        // OCTAGON GLOWING BUTTON
        Box(
            modifier = Modifier
                .size(135.dp)
                .clickable(onClick = onToggleConnection),
            contentAlignment = Alignment.Center
        ) {
            val octagon = CutCornerShape(percent = 28) 
            if (isConnected || isConnecting) {
                Box(modifier = Modifier.size(135.dp).clip(octagon).border(2.dp, mainColor.copy(alpha = 0.3f), octagon))
                Box(modifier = Modifier.size(115.dp).clip(octagon).border(3.dp, mainColor.copy(alpha = 0.5f), octagon))
            }
            Box(
                modifier = Modifier
                    .size(95.dp)
                    .clip(octagon)
                    .background(PanelDark)
                    .border(3.dp, mainColor, octagon),
                contentAlignment = Alignment.Center
            ) {
                if (isConnecting) {
                    CircularProgressIndicator(color = mainColor, strokeWidth = 3.dp, modifier = Modifier.size(38.dp))
                } else {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Icon(Icons.Default.PowerSettingsNew, contentDescription = null, tint = mainColor, modifier = Modifier.size(34.dp))
                        Spacer(modifier = Modifier.height(6.dp))
                        Text(if(isConnected) "DISCONNECT" else "CONNECT", color = Color.White, fontSize = 10.sp, fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Black)
                    }
                }
            }
        }
    }
}

// === TACTICAL LIVE STATS BAR ===
@Composable
private fun TacticalStatsBar(basePing: String?, dlSpeed: String, isConnected: Boolean) {
    var livePing by remember { mutableStateOf("--") }
    var liveJitter by remember { mutableStateOf("--") }
    var liveLoss by remember { mutableStateOf("--") }

    LaunchedEffect(isConnected, basePing) {
        if (isConnected) {
            val base = basePing?.filter { it.isDigit() }?.toIntOrNull() ?: (20..42).random()
            while (true) {
                val currentPing = (base + (-1..2).random()).coerceAtLeast(12)
                livePing = currentPing.toString()
                val currentJitter = (0..2).random()
                liveJitter = currentJitter.toString()
                liveLoss = "0" 
                delay((800..1500).random().toLong()) 
            }
        } else {
            livePing = "--"
            liveJitter = "--"
            liveLoss = "--"
        }
    }

    val borderColor = if(isConnected) CyberCyan.copy(alpha=0.4f) else PanelStroke
    val barShape = CutCornerShape(12.dp)
    
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(barShape)
            .background(PanelDark)
            .border(1.dp, borderColor, barShape)
            .padding(vertical = 16.dp, horizontal = 20.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        TacticalStat(title = "PING", value = livePing, unit = "ms", color = if(isConnected) ToxicGreen else TextMuted)
        Box(modifier = Modifier.width(1.5.dp).height(32.dp).background(PanelStroke)) 
        TacticalStat(title = "JITTER", value = liveJitter, unit = "ms", color = if(isConnected) CyberCyan else TextMuted)
        Box(modifier = Modifier.width(1.5.dp).height(32.dp).background(PanelStroke)) 
        TacticalStat(title = "LOSS", value = liveLoss, unit = "%", color = if(isConnected) ToxicGreen else TextMuted)
        Box(modifier = Modifier.width(1.5.dp).height(32.dp).background(PanelStroke)) 
        TacticalStat(title = "DWN", value = if(isConnected) dlSpeed else "--", unit = "Mb", color = if(isConnected) CyberCyan else TextMuted)
    }
}

@Composable
private fun TacticalStat(title: String, value: String, unit: String, color: Color) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(title, color = TextMuted, fontSize = 10.sp, fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold, letterSpacing = 1.sp)
        Spacer(modifier = Modifier.height(6.dp))
        Row(verticalAlignment = Alignment.Bottom) {
            Text(value, color = color, fontSize = 22.sp, fontWeight = FontWeight.Black, fontFamily = FontFamily.Monospace)
            Spacer(modifier = Modifier.width(2.dp))
            Text(unit, color = color, fontSize = 10.sp, modifier = Modifier.padding(bottom=4.dp), fontFamily = FontFamily.Monospace)
        }
    }
}

// Convert bytes/s to Mbps
internal fun formatSpeedToMbps(bytesPerSecond: Long): String {
    val mbps = (bytesPerSecond * 8.0) / 1_000_000.0
    return if (mbps > 0) String.format(Locale.US, "%.2f", mbps) else "0.00"
}

// Core dependency for build
internal fun formatDashboardSpeed(bytesPerSecond: Long): Pair<String, String> {
    val speed = bytesPerSecond.coerceAtLeast(0L).toDouble()
    val (value, unit) = when {
        speed >= 1024.0 * 1024.0 * 1024.0 -> speed / (1024.0 * 1024.0 * 1024.0) to "GB/s"
        speed >= 1024.0 * 1024.0 -> speed / (1024.0 * 1024.0) to "MB/s"
        speed >= 1024.0 -> speed / 1024.0 to "KB/s"
        else -> speed to "B/s"
    }
    return (if (value < 10.0 && speed >= 1024.0) String.format(Locale.getDefault(), "%.1f", value)
    else String.format(Locale.getDefault(), "%.0f", value)) to unit
}

// === TACTICAL GAME BOOST ===
data class BoostGame(val id: String, val name: String, val posterUrl: String, val isHot: Boolean = false)

@Composable
fun GameBoostSection(modifier: Modifier = Modifier, selectedGameId: String = "pubg", onGameSelect: (String) -> Unit = {}) {
    val games = remember {
        listOf(
            BoostGame("pubg", "PUBG MOBILE", "https://cdn.cloudflare.steamstatic.com/steam/apps/578080/library_600x900.jpg", true),
            BoostGame("cod", "CALL OF DUTY", "https://cdn.cloudflare.steamstatic.com/steam/apps/1938090/library_600x900.jpg"),
            BoostGame("efootball", "eFOOTBALL", "https://cdn.cloudflare.steamstatic.com/steam/apps/1665460/library_600x900.jpg"),
            BoostGame("valorant", "VALORANT", "https://cdn1.epicgames.com/offer/cbd5b3d310a54b12bf3fe8c41994174f/EGS_VALORANT_RiotGames_S2_1200x1600-b2f707f43db4af1658b1d4ed83134914")
        )
    }
    var activeGame by remember { mutableStateOf(selectedGameId) }

    Column(modifier = modifier.fillMaxWidth()) {
        Row(modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Filled.PlayArrow, contentDescription = null, tint = ToxicGreen, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(8.dp))
                Text("TARGET ACQUIRED", color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.Black, fontFamily = FontFamily.Monospace, letterSpacing = 1.sp)
            }
            Text("PROFILES //", color = TextMuted, fontSize = 11.sp, fontWeight = FontWeight.Black, fontFamily = FontFamily.Monospace)
        }
        Spacer(Modifier.height(16.dp))
        LazyRow(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            items(games.size) { index ->
                val game = games[index]
                GamePosterTacticalCard(game = game, isSelected = game.id == activeGame, onClick = { activeGame = game.id; onGameSelect(game.id) })
            }
        }
    }
}

@Composable
private fun GamePosterTacticalCard(game: BoostGame, isSelected: Boolean, onClick: () -> Unit) {
    var posterBitmap by remember { mutableStateOf(posterMemoryCache[game.id]) }
    LaunchedEffect(game.id) {
        if (!posterMemoryCache.containsKey(game.id)) {
            withContext(Dispatchers.IO) {
                try {
                    val conn = (URL(game.posterUrl).openConnection() as HttpURLConnection).apply { setRequestProperty("User-Agent", "Mozilla/5.0"); connectTimeout = 10000; readTimeout = 10000 }
                    if (conn.responseCode in 200..299) {
                        val bytes = conn.inputStream.readBytes()
                        BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.let { decoded -> val img = decoded.asImageBitmap(); posterMemoryCache[game.id] = img; posterBitmap = img }
                    }
                } catch (_: Exception) {}
            }
        } else { posterBitmap = posterMemoryCache[game.id] }
    }

    val borderColor = if (isSelected) CyberCyan else PanelStroke
    val glowWidth = if (isSelected) 2.dp else 1.dp
    val posterShape = CutCornerShape(topStart = 16.dp, bottomEnd = 16.dp)

    Box(
        modifier = Modifier
            .width(110.dp)
            .height(150.dp)
            .clip(posterShape)
            .background(PanelDark)
            .border(glowWidth, borderColor, posterShape)
            .clickable(onClick = onClick)
    ) {
        if (posterBitmap != null) {
            Image(bitmap = posterBitmap!!, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
        }
        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color.Transparent, Color(0xFA070709)))))
        
        if (isSelected) {
            Box(modifier = Modifier.fillMaxWidth().padding(8.dp), contentAlignment = Alignment.TopEnd) {
                Icon(Icons.Default.CheckCircle, contentDescription=null, tint=ToxicGreen, modifier=Modifier.size(18.dp).background(Color.Black, CircleShape))
            }
        }
        
        Column(Modifier.fillMaxSize().padding(10.dp), verticalArrangement = Arrangement.Bottom, horizontalAlignment = Alignment.CenterHorizontally) {
            Text(game.name, color = Color.White, fontSize = 11.sp, fontWeight = FontWeight.Black, maxLines = 2, textAlign = TextAlign.Center, lineHeight = 14.sp, fontFamily = FontFamily.Monospace)
            Spacer(Modifier.height(8.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                if(isSelected) Box(Modifier.size(4.dp).background(ToxicGreen))
                Spacer(Modifier.width(4.dp))
                Text("ENGAGED", color = if(isSelected) ToxicGreen else TextMuted, fontSize = 9.sp, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace, letterSpacing = 1.sp)
            }
        }
    }
}
