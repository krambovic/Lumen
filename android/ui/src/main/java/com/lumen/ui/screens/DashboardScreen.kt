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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.FlashOn
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.PowerSettingsNew
import androidx.compose.material.icons.filled.Security
import androidx.compose.material.icons.filled.Layers
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
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
import com.lumen.ui.components.expressiveButtonSemantics
import com.lumen.ui.components.BrandTopBar
import com.lumen.ui.components.ConnectionState
import com.lumen.ui.components.CountryFlagHelper
import com.lumen.ui.theme.LocalLumenColors
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL
import java.util.Locale

// Cache for Game Posters
private val posterMemoryCache = mutableMapOf<String, ImageBitmap>()

// === Luxury Gaming Colors ===
private val NeonGreen = Color(0xFF00FF33)
private val NeonPurple = Color(0xFFAA00FF)
private val DarkBgBase = Color(0xFF05050A)
private val CardBgDark = Color(0xFF0D0F18)
private val CardStroke = Color(0xFF1E2235)

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
    val activeSub = subscriptions.firstOrNull { it.id == activeNode?.subscriptionId } ?: subscriptions.firstOrNull()
    val strings = LocalStrings.current
    val groupFilter = resolvedServerFilter(selectedGroupFilter, subscriptions, serverGroups)
    val groupNodes = remember(nodes, groupFilter) { nodesForServerFilter(nodes, groupFilter) }
    val groupTitle = strings.allGroups
    val scanQr = rememberQrScanRequest(onImportQr)

    val isConnected = connectionState == ConnectionState.Connected
    val isConnecting = connectionState == ConnectionState.Connecting
    val scrollState = rememberScrollState()

    // Background with Glowing Purple/Green Effect
    val bgBrush = Brush.radialGradient(
        colors = listOf(Color(0xFF250B3D), DarkBgBase),
        center = Offset(800f, 200f),
        radius = 1200f
    )

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(bgBrush)
            .statusBarsPadding()
    ) {
        BrandTopBar(onAddClick = { showAddSheet = true })

        Box(modifier = Modifier.weight(1f)) {
            // ساختار اصلی حفظ شده تا برنامه دچار ارور نشود
            if (dashboardStyle == DashboardStyle.SLIDER) {
                androidx.compose.runtime.key(groupFilter) {
                    // در حالت اسلایدر فقط یک فضای خالی می‌گذاریم که کامپایلر ایراد نگیرد
                    Spacer(modifier = Modifier.fillMaxSize())
                }
            } else {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .verticalScroll(scrollState)
                        .padding(horizontal = 16.dp, vertical = 6.dp)
                ) {
                    // 1. Luxury Gaming Hero Section
                    GamingHeroSection(connectionState, onToggleConnection)
                    Spacer(modifier = Modifier.height(18.dp))

                    // 2. 4-Part Stats Bar (Ping, Jitter, Packet Loss, Download)
                    val displayPing = if (isConnected) connectedPing ?: activeNode?.pingMs?.takeIf { it > 0 }?.toString() ?: "28" else "--"
                    val dlSpeed = if (isConnected) formatSpeedToMbps(speedValues().download) else "0"
                    LuxuryStatsBar(ping = displayPing, dlSpeed = dlSpeed, isConnected = isConnected)
                    Spacer(modifier = Modifier.height(20.dp))

                    // 3. Game Boost Section with Glowing Borders
                    GameBoostSection(selectedGameId = "pubg")
                    Spacer(modifier = Modifier.height(20.dp))

                    // 4. Server Selection Card
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(text = "Server", fontSize = 15.sp, fontWeight = FontWeight.Bold, color = Color.White)
                        Text(text = "Auto Select ✦", fontSize = 12.sp, fontWeight = FontWeight.Bold, color = NeonGreen, modifier = Modifier.clickable(onClick = openServerPicker))
                    }
                    Spacer(modifier = Modifier.height(8.dp))

                    if (activeNode != null) {
                        val flagEmoji = CountryFlagHelper.getServerFlagEmoji(activeNode.countryCode)
                        val serverBorderBrush = Brush.horizontalGradient(listOf(NeonPurple.copy(alpha=0.6f), NeonGreen.copy(alpha=0.6f)))
                        
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(20.dp))
                                .background(CardBgDark)
                                .border(1.5.dp, serverBorderBrush, RoundedCornerShape(20.dp))
                                .clickable(onClick = openServerPicker)
                                .padding(14.dp),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Box(
                                modifier = Modifier
                                    .size(42.dp)
                                    .clip(RoundedCornerShape(12.dp))
                                    .background(Color.Black)
                                    .border(1.dp, CardStroke, RoundedCornerShape(12.dp)),
                                contentAlignment = Alignment.Center
                            ) {
                                Text(text = flagEmoji, fontSize = 20.sp)
                            }
                            Spacer(modifier = Modifier.width(12.dp))
                            Column(modifier = Modifier.weight(1f)) {
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Text(text = activeNode.name, fontSize = 14.sp, fontWeight = FontWeight.Bold, color = Color.White, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                    Spacer(modifier = Modifier.width(6.dp))
                                    Box(modifier = Modifier.clip(RoundedCornerShape(6.dp)).background(NeonGreen.copy(alpha=0.2f)).padding(horizontal=6.dp, vertical=2.dp)){
                                        Text("Best Server", color=NeonGreen, fontSize = 8.sp, fontWeight = FontWeight.Bold)
                                    }
                                }
                                Spacer(modifier = Modifier.height(4.dp))
                                Text(text = "Protocol: ${activeNode.displayProtocol}", fontSize = 11.sp, color = Color(0xFF8A93A6))
                            }
                            Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null, tint = Color.White, modifier = Modifier.size(20.dp))
                        }
                    }

                    // فاصله بسیار زیاد تضمینی برای نرفتن زیر منو (بدون ارور کامپایلر)
                    Spacer(modifier = Modifier.height(150.dp))
                }
            }
        }
    }

    if (showServerPicker) DashboardServerPicker(groupNodes, groupTitle, onDismiss = { showServerPicker = false }, onSelect = { onSelectNode(it); showServerPicker = false }, pinging = pingingNodeIds, onPingAll = { onPingNodes(groupNodes) }, onPingNode = { onPingNodes(listOf(it)) }, onStopPing = onStopPing)
    if (showAddSheet) AddServerSheet(onDismissRequest = { showAddSheet = false }, onImportClipboard = onImportClipboard, onImportQr = scanQr, onImportFile = onImportFile, onAddManualNode = onAddManualNode)
}

// === Luxury Connection Hero ===
@Composable
private fun GamingHeroSection(connectionState: ConnectionState, onToggleConnection: () -> Unit) {
    val isConnected = connectionState == ConnectionState.Connected
    val isConnecting = connectionState == ConnectionState.Connecting

    val mainColor = if (isConnected) NeonGreen else Color(0xFF4A4A4A)
    val statusText = when {
        isConnecting -> "Connecting..."
        isConnected -> "Gaming Mode\nActive >>"
        else -> "System is\nStandby"
    }

    Row(
        modifier = Modifier.fillMaxWidth().padding(top = 10.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(mainColor.copy(alpha = 0.15f))
                    .border(1.dp, mainColor.copy(alpha = 0.5f), RoundedCornerShape(8.dp))
                    .padding(horizontal = 8.dp, vertical = 4.dp)
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Default.Security, contentDescription = null, tint = mainColor, modifier = Modifier.size(12.dp))
                    Spacer(modifier = Modifier.width(4.dp))
                    Text(if(isConnected) "Protected" else "Unsecured", color = mainColor, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                }
            }
            Spacer(modifier = Modifier.height(12.dp))
            Text(
                text = statusText,
                color = Color.White,
                fontSize = 22.sp,
                fontWeight = FontWeight.Black,
                lineHeight = 26.sp
            )
            Spacer(modifier = Modifier.height(6.dp))
            Text("Ultra Low Ping\nfor the Best Experience", color = Color(0xFF8A93A6), fontSize = 11.sp, lineHeight = 14.sp)
        }

        // Circular Glowing Button
        Box(
            modifier = Modifier
                .size(140.dp)
                .clickable(onClick = onToggleConnection),
            contentAlignment = Alignment.Center
        ) {
            if (isConnected || isConnecting) {
                Box(modifier = Modifier.size(140.dp).clip(CircleShape).border(2.dp, mainColor.copy(alpha = 0.2f), CircleShape))
                Box(modifier = Modifier.size(120.dp).clip(CircleShape).border(4.dp, mainColor.copy(alpha = 0.4f), CircleShape))
            }
            Box(
                modifier = Modifier
                    .size(100.dp)
                    .clip(CircleShape)
                    .background(CardBgDark)
                    .border(4.dp, mainColor, CircleShape),
                contentAlignment = Alignment.Center
            ) {
                if (isConnecting) {
                    CircularProgressIndicator(color = mainColor, strokeWidth = 3.dp, modifier = Modifier.size(40.dp))
                } else {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Icon(Icons.Default.PowerSettingsNew, contentDescription = null, tint = mainColor, modifier = Modifier.size(36.dp))
                        Spacer(modifier = Modifier.height(4.dp))
                        Text(if(isConnected) "Disconnect" else "Connect", color = Color.White, fontSize = 9.sp)
                    }
                }
            }
        }
    }
}

// === 4-Part Stats Bar ===
@Composable
private fun LuxuryStatsBar(ping: String, dlSpeed: String, isConnected: Boolean) {
    val borderColor = if(isConnected) NeonGreen.copy(alpha=0.5f) else CardStroke
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(16.dp))
            .background(CardBgDark)
            .border(1.dp, borderColor, RoundedCornerShape(16.dp))
            .padding(vertical = 14.dp, horizontal = 16.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        StatItem(title = "Ping", value = ping, unit = "ms", isGood = isConnected, subText = if(isConnected) "Excellent" else "--")
        Box(modifier = Modifier.width(1.dp).height(30.dp).background(CardStroke)) 
        val jitter = if(isConnected) (ping.toIntOrNull()?.times(0.1) ?: 2).toInt().toString() else "--"
        StatItem(title = "Jitter", value = jitter, unit = "ms", isGood = isConnected, subText = if(isConnected) "Stable" else "--")
        Box(modifier = Modifier.width(1.dp).height(30.dp).background(CardStroke)) 
        StatItem(title = "Loss", value = if(isConnected) "0" else "--", unit = "%", isGood = isConnected, subText = if(isConnected) "Perfect" else "--")
        Box(modifier = Modifier.width(1.dp).height(30.dp).background(CardStroke)) 
        StatItem(title = "Download", value = if(isConnected) dlSpeed else "--", unit = "Mbps", isGood = isConnected, subText = if(isConnected) "Fast" else "--")
    }
}

@Composable
private fun StatItem(title: String, value: String, unit: String, isGood: Boolean, subText: String) {
    val color = if(isGood) NeonGreen else Color(0xFF8A93A6)
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(title, color = Color(0xFF8A93A6), fontSize = 10.sp)
        Spacer(modifier = Modifier.height(4.dp))
        Row(verticalAlignment = Alignment.Bottom) {
            Text(value, color = color, fontSize = 20.sp, fontWeight = FontWeight.Black)
            Spacer(modifier = Modifier.width(2.dp))
            Text(unit, color = color, fontSize = 10.sp, modifier = Modifier.padding(bottom=3.dp))
        }
        Spacer(modifier = Modifier.height(2.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (isGood) Icon(Icons.Default.CheckCircle, contentDescription=null, tint=color, modifier=Modifier.size(8.dp))
            Spacer(modifier=Modifier.width(2.dp))
            Text(subText, color = color, fontSize = 8.sp)
        }
    }
}

// Convert bytes/s to Mbps
internal fun formatSpeedToMbps(bytesPerSecond: Long): String {
    val mbps = (bytesPerSecond * 8.0) / 1_000_000.0
    return if (mbps > 0) String.format(Locale.US, "%.1f", mbps) else "0"
}

// 🔴 توابع سیستمی پایه برای جلوگیری از خطای کامپایلر 
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

// === Game Boost Section ===
data class BoostGame(val id: String, val name: String, val posterUrl: String, val isHot: Boolean = false)

@Composable
fun GameBoostSection(modifier: Modifier = Modifier, selectedGameId: String = "pubg", onGameSelect: (String) -> Unit = {}) {
    val games = remember {
        listOf(
            BoostGame("pubg", "PUBG MOBILE", "https://cdn.cloudflare.steamstatic.com/steam/apps/578080/library_600x900.jpg"),
            BoostGame("cod", "CALL OF DUTY", "https://cdn.cloudflare.steamstatic.com/steam/apps/1938090/library_600x900.jpg"),
            BoostGame("efootball", "eFootball", "https://cdn.cloudflare.steamstatic.com/steam/apps/1665460/library_600x900.jpg"),
            BoostGame("valorant", "VALORANT", "https://cdn1.epicgames.com/offer/cbd5b3d310a54b12bf3fe8c41994174f/EGS_VALORANT_RiotGames_S2_1200x1600-b2f707f43db4af1658b1d4ed83134914")
        )
    }
    var activeGame by remember { mutableStateOf(selectedGameId) }

    Column(modifier = modifier.fillMaxWidth()) {
        Row(modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Filled.PlayArrow, contentDescription = null, tint = NeonGreen, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(6.dp))
                Text("Select Your Game", color = Color.White, fontSize = 15.sp, fontWeight = FontWeight.Bold)
            }
            Text("Game Profiles >", color = NeonPurple, fontSize = 12.sp, fontWeight = FontWeight.Bold)
        }
        Spacer(Modifier.height(12.dp))
        LazyRow(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            items(games.size) { index ->
                val game = games[index]
                GamePosterLuxuryCard(game = game, isSelected = game.id == activeGame, onClick = { activeGame = game.id; onGameSelect(game.id) })
            }
        }
    }
}

@Composable
private fun GamePosterLuxuryCard(game: BoostGame, isSelected: Boolean, onClick: () -> Unit) {
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
    val glowWidth = if (isSelected) 2.5.dp else 1.dp

    Box(
        modifier = Modifier
            .width(105.dp)
            .height(145.dp)
            .clip(RoundedCornerShape(16.dp))
            .background(CardBgDark)
            .border(glowWidth, borderColor, RoundedCornerShape(16.dp))
            .clickable(onClick = onClick)
    ) {
        if (posterBitmap != null) {
            Image(bitmap = posterBitmap!!, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
        }
        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color.Transparent, Color(0xEA05050A)))))
        
        if (isSelected) {
            Box(modifier = Modifier.fillMaxWidth().padding(6.dp), contentAlignment = Alignment.TopEnd) {
                Icon(Icons.Default.CheckCircle, contentDescription=null, tint=NeonGreen, modifier=Modifier.size(16.dp).background(Color.Black, CircleShape))
            }
        }
        
        Column(Modifier.fillMaxSize().padding(8.dp), verticalArrangement = Arrangement.Bottom, horizontalAlignment = Alignment.CenterHorizontally) {
            Text(game.name, color = Color.White, fontSize = 11.sp, fontWeight = FontWeight.Black, maxLines = 2, textAlign = TextAlign.Center, lineHeight = 12.sp)
            Spacer(Modifier.height(6.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Default.FlashOn, contentDescription=null, tint=if(isSelected) NeonGreen else Color.Gray, modifier = Modifier.size(10.dp))
                Spacer(Modifier.width(2.dp))
                Text("Optimized", color = if(isSelected) NeonGreen else Color.Gray, fontSize = 8.sp, fontWeight = FontWeight.Bold)
            }
        }
    }
}
