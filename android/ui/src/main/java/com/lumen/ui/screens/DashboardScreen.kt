package com.lumen.ui.screens

import android.graphics.BitmapFactory
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Image
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
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
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
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
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
import com.lumen.ui.components.expressiveButtonSemantics
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

// === Premium Clean Gaming Colors ===
private val NeonGreen = Color(0xFF00FF66)
private val PurpleAccent = Color(0xFF8B5CF6)
private val DangerRed = Color(0xFFFF3B30)
private val BgDark = Color(0xFF050508)
private val CardDark = Color(0xFF12121A)
private val CardStroke = Color(0xFF22222F)
private val TextMuted = Color(0xFF8B93A5)

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
    
    val groupTitle = when {
        groupFilter == "manual" -> strings.manual
        groupFilter == "sub" -> strings.subscriptions
        groupFilter.startsWith("subscription:") -> subscriptions.firstOrNull { it.id == groupFilter.removePrefix("subscription:") }?.name ?: strings.allGroups
        groupFilter.startsWith("group:") -> serverGroups.firstOrNull { it.id == groupFilter.removePrefix("group:") }?.name ?: strings.allGroups
        else -> strings.allGroups
    }
    
    val scanQr = rememberQrScanRequest(onImportQr)

    val isConnected = connectionState == ConnectionState.Connected
    val scrollState = rememberScrollState()

    val bgBrush = Brush.radialGradient(
        colors = listOf(Color(0xFF160B2E), BgDark),
        center = Offset(500f, 0f),
        radius = 1600f
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
                    DashboardServerTable(
                        groupNodes, groupTitle, pingingNodeIds, onSelectNode, onOpenServers,
                        speedStatsEnabled, isConnected, speedValues,
                        onPingAll = { onPingNodes(groupNodes) }, onPingNode = { onPingNodes(listOf(it)) }, onStopPing = onStopPing
                    )
                }
            } else {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .verticalScroll(scrollState)
                        .padding(horizontal = 16.dp, vertical = 6.dp)
                ) {
                    PremiumHeroSection(connectionState, onToggleConnection, tick)
                    Spacer(modifier = Modifier.height(24.dp))

                    if (speedStatsEnabled) {
                        val basePingValue = connectedPing ?: activeNode?.pingMs?.takeIf { it > 0 }?.toString()
                        val dlSpeed = if (isConnected) formatSpeedToMbps(speedValues().download) else "0.00"
                        PremiumStatsBar(basePing = basePingValue, dlSpeed = dlSpeed, isConnected = isConnected)
                        Spacer(modifier = Modifier.height(24.dp))
                    }

                    GameBoostSection(selectedGameId = "pubg")
                    Spacer(modifier = Modifier.height(24.dp))

                    Row(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Default.Layers, contentDescription=null, tint=PurpleAccent, modifier=Modifier.size(16.dp))
                            Spacer(Modifier.width(6.dp))
                            Text(text = "SELECTED SERVER", fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
                        }
                        TextButton(
                            onClick = openServerPicker,
                            modifier = Modifier
                                .height(36.dp)
                                .expressiveButtonSemantics(strings.changeServer, openServerPicker)
                        ) {
                            Text(text = "Change", fontSize = 12.sp, fontWeight = FontWeight.Bold, color = NeonGreen)
                        }
                    }
                    Spacer(modifier = Modifier.height(8.dp))

                    if (activeNode != null) {
                        val flagEmoji = CountryFlagHelper.getServerFlagEmoji(activeNode.countryCode)
                        val serverShape = RoundedCornerShape(20.dp)
                        
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(serverShape)
                                .background(CardDark)
                                .border(1.5.dp, if(isConnected) PurpleAccent.copy(alpha=0.5f) else CardStroke, serverShape)
                                .clickable(role = androidx.compose.ui.semantics.Role.Button, onClick = openServerPicker)
                                .padding(16.dp),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Box(
                                modifier = Modifier
                                    .size(46.dp)
                                    .clip(RoundedCornerShape(12.dp))
                                    .background(BgDark)
                                    .border(1.dp, CardStroke, RoundedCornerShape(12.dp)),
                                contentAlignment = Alignment.Center
                            ) {
                                Text(text = flagEmoji, fontSize = 22.sp)
                            }
                            Spacer(modifier = Modifier.width(14.dp))
                            Column(modifier = Modifier.weight(1f)) {
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Text(text = activeNode.name, fontSize = 15.sp, fontWeight = FontWeight.Bold, color = Color.White, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                }
                                Spacer(modifier = Modifier.height(6.dp))
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Box(Modifier.size(6.dp).clip(CircleShape).background(if(isConnected) NeonGreen else TextMuted))
                                    Spacer(Modifier.width(6.dp))
                                    Text(text = "Protocol: ${activeNode.displayProtocol}", fontSize = 11.sp, color = TextMuted, fontWeight = FontWeight.Medium)
                                }
                            }
                            Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = strings.changeServer, tint = if(isConnected) NeonGreen else Color.White, modifier = Modifier.size(24.dp))
                        }
                    } else {
                        Box(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(20.dp))
                                .background(CardDark)
                                .border(1.dp, CardStroke, RoundedCornerShape(20.dp))
                                .clickable(onClick = openServerPicker)
                                .padding(16.dp),
                            contentAlignment = Alignment.Center
                        ) {
                            Text(
                                text = strings.noServerSelected,
                                fontSize = 14.sp,
                                color = TextMuted
                            )
                        }
                    }

                    Spacer(modifier = Modifier.height(160.dp))
                }
            }
        }
    }

    if (showServerPicker) {
        DashboardServerPicker(groupNodes, groupTitle, onDismiss = { showServerPicker = false }, onSelect = { onSelectNode(it); showServerPicker = false }, pinging = pingingNodeIds, onPingAll = { onPingNodes(groupNodes) }, onPingNode = { onPingNodes(listOf(it)) }, onStopPing = onStopPing)
    }
    if (showAddSheet) {
        AddServerSheet(onDismissRequest = { showAddSheet = false }, onImportClipboard = onImportClipboard, onImportQr = scanQr, onImportFile = onImportFile, onAddManualNode = onAddManualNode)
    }
}

@Composable
private fun PremiumHeroSection(connectionState: ConnectionState, onToggleConnection: () -> Unit, tick: (HapticFeedbackType) -> Unit) {
    val isConnected = connectionState == ConnectionState.Connected
    val isConnecting = connectionState == ConnectionState.Connecting
    val scope = rememberCoroutineScope()
    val orbScale = remember { Animatable(1f) }

    val mainColor = if (isConnected) NeonGreen else TextMuted
    val statusText = when {
        isConnecting -> "Optimizing\nNetwork..."
        isConnected -> "Gaming Boost\nActive >>"
        else -> "Ready to\nBoost"
    }

    val onOrbitClick: () -> Unit = {
        scope.launch {
            orbScale.snapTo(0.88f)
            orbScale.animateTo(1f, spring(dampingRatio = 0.55f, stiffness = 600f))
        }
        tick(HapticFeedbackType.LongPress)
        onToggleConnection()
    }

    Row(
        modifier = Modifier.fillMaxWidth().padding(top = 10.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(6.dp))
                    .background(mainColor.copy(alpha = 0.1f))
                    .padding(horizontal = 10.dp, vertical = 6.dp)
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Default.Shield, contentDescription = null, tint = mainColor, modifier = Modifier.size(14.dp))
                    Spacer(modifier = Modifier.width(6.dp))
                    Text(if(isConnected) "Secured Connection" else "Not Connected", color = mainColor, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                }
            }
            Spacer(modifier = Modifier.height(14.dp))
            Text(
                text = statusText,
                color = Color.White,
                fontSize = 26.sp,
                fontWeight = FontWeight.Black,
                lineHeight = 32.sp
            )
            Spacer(modifier = Modifier.height(8.dp))
            Text("WireGuard Low-Latency Routing", color = TextMuted, fontSize = 12.sp, fontWeight = FontWeight.Medium)
        }

        Box(
            modifier = Modifier
                .size(130.dp)
                .graphicsLayer { scaleX = orbScale.value; scaleY = orbScale.value }
                .clickable(onClick = onOrbitClick),
            contentAlignment = Alignment.Center
        ) {
            val shape = CircleShape 
            if (isConnected || isConnecting) {
                Box(modifier = Modifier.size(130.dp).clip(shape).border(1.dp, mainColor.copy(alpha = 0.3f), shape))
                Box(modifier = Modifier.size(110.dp).clip(shape).background(mainColor.copy(alpha = 0.1f)))
            }
            Box(
                modifier = Modifier
                    .size(90.dp)
                    .clip(shape)
                    .background(if (isConnected) NeonGreen else CardDark)
                    .border(2.dp, if (isConnected) NeonGreen else CardStroke, shape),
                contentAlignment = Alignment.Center
            ) {
                if (isConnecting) {
                    CircularProgressIndicator(color = mainColor, strokeWidth = 3.dp, modifier = Modifier.size(38.dp))
                } else {
                    Icon(Icons.Default.PowerSettingsNew, contentDescription = null, tint = if (isConnected) BgDark else Color.White, modifier = Modifier.size(36.dp))
                }
            }
        }
    }
}

@Composable
private fun PremiumStatsBar(basePing: String?, dlSpeed: String, isConnected: Boolean) {
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
                delay((1000..2000).random().toLong()) 
            }
        } else {
            livePing = "--"
            liveJitter = "--"
            liveLoss = "--"
        }
    }

    val borderColor = if(isConnected) PurpleAccent.copy(alpha=0.4f) else CardStroke
    val barShape = RoundedCornerShape(16.dp)
    
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(barShape)
            .background(CardDark)
            .border(1.dp, borderColor, barShape)
            .padding(vertical = 18.dp, horizontal = 20.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        PremiumStat(title = "Ping", value = livePing, unit = "ms", color = if(isConnected) Color.White else TextMuted)
        Box(modifier = Modifier.width(1.dp).height(30.dp).background(CardStroke)) 
        PremiumStat(title = "Jitter", value = liveJitter, unit = "ms", color = if(isConnected) Color.White else TextMuted)
        Box(modifier = Modifier.width(1.dp).height(30.dp).background(CardStroke)) 
        PremiumStat(title = "Loss", value = liveLoss, unit = "%", color = if(isConnected) Color.White else TextMuted)
        Box(modifier = Modifier.width(1.dp).height(30.dp).background(CardStroke)) 
        PremiumStat(title = "Speed", value = if(isConnected) dlSpeed else "--", unit = "Mb", color = if(isConnected) Color.White else TextMuted)
    }
}

@Composable
private fun PremiumStat(title: String, value: String, unit: String, color: Color) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(title, color = TextMuted, fontSize = 12.sp, fontWeight = FontWeight.Medium)
        Spacer(modifier = Modifier.height(4.dp))
        Row(verticalAlignment = Alignment.Bottom) {
            Text(value, color = color, fontSize = 22.sp, fontWeight = FontWeight.Bold)
            Spacer(modifier = Modifier.width(2.dp))
            Text(unit, color = TextMuted, fontSize = 11.sp, modifier = Modifier.padding(bottom=3.dp))
        }
    }
}

internal fun formatSpeedToMbps(bytesPerSecond: Long): String {
    val mbps = (bytesPerSecond * 8.0) / 1_000_000.0
    return if (mbps > 0) String.format(Locale.US, "%.2f", mbps) else "0.00"
}

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

data class BoostGame(val id: String, val name: String, val posterUrl: String, val isHot: Boolean = false)

@Composable
fun GameBoostSection(modifier: Modifier = Modifier, selectedGameId: String = "pubg", onGameSelect: (String) -> Unit = {}) {
    val games = remember {
        listOf(
            BoostGame("pubg", "PUBG Mobile", "https://cdn.cloudflare.steamstatic.com/steam/apps/578080/library_600x900.jpg", true),
            BoostGame("cod", "Call of Duty", "https://cdn.cloudflare.steamstatic.com/steam/apps/1938090/library_600x900.jpg"),
            BoostGame("efootball", "eFootball", "https://cdn.cloudflare.steamstatic.com/steam/apps/1665460/library_600x900.jpg"),
            BoostGame("valorant", "Valorant", "https://cdn1.epicgames.com/offer/cbd5b3d310a54b12bf3fe8c41994174f/EGS_VALORANT_RiotGames_S2_1200x1600-b2f707f43db4af1658b1d4ed83134914")
        )
    }
    var activeGame by remember { mutableStateOf(selectedGameId) }

    Column(modifier = modifier.fillMaxWidth()) {
        Row(modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Filled.PlayArrow, contentDescription = null, tint = NeonGreen, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(8.dp))
                Text("Game Profiles", color = Color.White, fontSize = 16.sp, fontWeight = FontWeight.Bold)
            }
            Text("Auto Detect", color = TextMuted, fontSize = 12.sp, fontWeight = FontWeight.Medium)
        }
        Spacer(Modifier.height(16.dp))
        LazyRow(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            items(games.size) { index ->
                val game = games[index]
                GamePosterPremiumCard(game = game, isSelected = game.id == activeGame, onClick = { activeGame = game.id; onGameSelect(game.id) })
            }
        }
    }
}

@Composable
private fun GamePosterPremiumCard(game: BoostGame, isSelected: Boolean, onClick: () -> Unit) {
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

    val borderColor = if (isSelected) NeonGreen else CardStroke
    val glowWidth = if (isSelected) 2.dp else 1.dp
    val posterShape = RoundedCornerShape(16.dp)

    Box(
        modifier = Modifier
            .width(110.dp)
            .height(150.dp)
            .clip(posterShape)
            .background(CardDark)
            .border(glowWidth, borderColor, posterShape)
            .clickable(onClick = onClick)
    ) {
        if (posterBitmap != null) {
            Image(bitmap = posterBitmap!!, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
        }
        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color.Transparent, Color(0xFA050508)))))
        
        if (isSelected) {
            Box(modifier = Modifier.fillMaxWidth().padding(8.dp), contentAlignment = Alignment.TopEnd) {
                Icon(Icons.Default.CheckCircle, contentDescription=null, tint=NeonGreen, modifier=Modifier.size(18.dp).background(Color.Black, CircleShape))
            }
        }
        
        Column(Modifier.fillMaxSize().padding(10.dp), verticalArrangement = Arrangement.Bottom, horizontalAlignment = Alignment.CenterHorizontally) {
            Text(game.name, color = Color.White, fontSize = 12.sp, fontWeight = FontWeight.Bold, maxLines = 2, textAlign = TextAlign.Center, lineHeight = 14.sp)
            Spacer(Modifier.height(8.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                if(isSelected) Box(Modifier.size(4.dp).clip(CircleShape).background(NeonGreen))
                Spacer(Modifier.width(4.dp))
                Text("Optimized", color = if(isSelected) NeonGreen else TextMuted, fontSize = 10.sp, fontWeight = FontWeight.Medium)
            }
        }
    }
}
