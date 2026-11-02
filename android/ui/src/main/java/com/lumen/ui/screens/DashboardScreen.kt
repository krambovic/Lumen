
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
import androidx.compose.foundation.shape.CutCornerShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Layers
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.PowerSettingsNew
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
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

// === تم سایبرپانک و گیمینگ ===
private val NeonCyan = Color(0xFF00FFCC)
private val NeonYellow = Color(0xFFFFD600)
private val DarkBg = Color(0xFF07080B)
private val CardBg = Color(0xFF10121B)
private val StrokeColor = Color(0xFF1E2235)

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

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(DarkBg)
            .statusBarsPadding()
    ) {
        BrandTopBar(onAddClick = { showAddSheet = true })

        Box(modifier = Modifier.weight(1f)) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(scrollState)
                    .padding(horizontal = 16.dp, vertical = 6.dp)
            ) {
                // کارت اتصال سایبرپانک
                OrbitConnectionCard(connectionState, onToggleConnection)
                Spacer(modifier = Modifier.height(16.dp))

                // بخش بازی‌ها
                GameBoostSection(selectedGameId = "pubg")
                Spacer(modifier = Modifier.height(16.dp))
                
                // کادر پینگ
                val displayPing = if (isConnected) connectedPing ?: activeNode?.pingMs?.takeIf { it > 0 }?.toString() ?: "148" else null
                com.lumen.ui.components.LivePingCard(ping = displayPing, isConnected = isConnected, modifier = Modifier.fillMaxWidth())
                Spacer(modifier = Modifier.height(16.dp))

                // هدر سرور
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(text = "SERVER_LINK //", fontSize = 14.sp, fontWeight = FontWeight.Black, color = Color.White, fontFamily = FontFamily.Monospace)
                    TextButton(onClick = openServerPicker, modifier = Modifier.height(36.dp)) {
                        Text(text = "SELECT", fontSize = 12.sp, fontWeight = FontWeight.Bold, color = NeonYellow, fontFamily = FontFamily.Monospace)
                    }
                }
                Spacer(modifier = Modifier.height(8.dp))

                // کارت سرور با استایل HUD زاویه‌دار
                if (activeNode != null) {
                    val flagEmoji = CountryFlagHelper.getServerFlagEmoji(activeNode.countryCode)
                    val serverShape = CutCornerShape(topStart = 16.dp, bottomEnd = 16.dp)
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(serverShape)
                            .background(CardBg)
                            .border(1.5.dp, StrokeColor, serverShape)
                            .clickable(onClick = openServerPicker)
                            .padding(14.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Box(
                            modifier = Modifier
                                .size(40.dp)
                                .clip(CutCornerShape(topStart = 10.dp, bottomEnd = 10.dp))
                                .background(DarkBg)
                                .border(1.dp, StrokeColor, CutCornerShape(topStart = 10.dp, bottomEnd = 10.dp)),
                            contentAlignment = Alignment.Center
                        ) {
                            Text(text = flagEmoji, fontSize = 20.sp)
                        }
                        Spacer(modifier = Modifier.width(12.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            Text(text = activeNode.name.uppercase(), fontSize = 14.sp, fontWeight = FontWeight.Black, color = Color.White, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Spacer(modifier = Modifier.height(4.dp))
                            Text(text = activeNode.displayProtocol, fontSize = 11.sp, color = NeonCyan, fontFamily = FontFamily.Monospace)
                        }
                        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null, tint = NeonYellow, modifier = Modifier.size(24.dp))
                    }
                }

                Spacer(modifier = Modifier.height(16.dp))

                // کارت‌های سرعت
                if (speedStatsEnabled) DashboardSpeedCards(isConnected, speedValues)

                // کارت اشتراک
                activeSub?.let { sub ->
                    val subShape = CutCornerShape(topEnd = 16.dp, bottomStart = 16.dp)
                    Spacer(modifier = Modifier.height(8.dp))
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(subShape)
                            .background(CardBg)
                            .border(1.5.dp, StrokeColor, subShape)
                            .clickable(onClick = onOpenServers)
                            .padding(14.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Icon(Icons.Default.Layers, contentDescription = null, tint = NeonYellow, modifier = Modifier.size(22.dp))
                        Spacer(Modifier.width(12.dp))
                        Column(Modifier.weight(1f)) {
                            Text(sub.name.uppercase(), fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }

                // ==========================================
                // این اسپیسر نامرئی کلید حل مشکل رفتن زیر منوعه!
                // با این کار کل محتوا کشیده میشه بالا و هیچی گیر نمیکنه
                Spacer(modifier = Modifier.height(160.dp))
                // ==========================================
            }
        }
    }

    if (showServerPicker) DashboardServerPicker(groupNodes, groupTitle, onDismiss = { showServerPicker = false }, onSelect = { onSelectNode(it); showServerPicker = false }, pinging = pingingNodeIds, onPingAll = { onPingNodes(groupNodes) }, onPingNode = { onPingNodes(listOf(it)) }, onStopPing = onStopPing)
    if (showAddSheet) AddServerSheet(onDismissRequest = { showAddSheet = false }, onImportClipboard = onImportClipboard, onImportQr = scanQr, onImportFile = onImportFile, onAddManualNode = onAddManualNode)
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

@Composable
private fun DashboardSpeedCards(isConnected: Boolean, values: () -> DashboardSpeeds) {
    val speeds = if (isConnected) values() else DashboardSpeeds()
    val cardShape = CutCornerShape(topStart = 16.dp, bottomEnd = 16.dp)

    Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        // کارت دانلود
        val (downVal, downUnit) = formatDashboardSpeed(speeds.download)
        Box(
            modifier = Modifier
                .weight(1f)
                .clip(cardShape)
                .background(CardBg)
                .border(1.5.dp, StrokeColor, cardShape)
                .padding(16.dp)
        ) {
            Column {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Default.ArrowDownward, contentDescription = null, tint = NeonYellow, modifier = Modifier.size(16.dp))
                    Spacer(modifier = Modifier.width(6.dp))
                    Text("DL_RATE", fontSize = 11.sp, color = Color(0xFF757D8A), fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold)
                }
                Spacer(modifier = Modifier.height(12.dp))
                Row(verticalAlignment = Alignment.Bottom) {
                    Text(downVal, fontSize = 34.sp, fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Black, color = NeonYellow)
                    Spacer(modifier = Modifier.width(4.dp))
                    Text(downUnit, fontSize = 12.sp, color = Color(0xFF757D8A), fontFamily = FontFamily.Monospace, modifier = Modifier.padding(bottom = 6.dp))
                }
            }
        }

        // کارت آپلود
        val (upVal, upUnit) = formatDashboardSpeed(speeds.upload)
        Box(
            modifier = Modifier
                .weight(1f)
                .clip(cardShape)
                .background(CardBg)
                .border(1.5.dp, StrokeColor, cardShape)
                .padding(16.dp)
        ) {
            Column {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Default.ArrowUpward, contentDescription = null, tint = NeonCyan, modifier = Modifier.size(16.dp))
                    Spacer(modifier = Modifier.width(6.dp))
                    Text("UP_RATE", fontSize = 11.sp, color = Color(0xFF757D8A), fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold)
                }
                Spacer(modifier = Modifier.height(12.dp))
                Row(verticalAlignment = Alignment.Bottom) {
                    Text(upVal, fontSize = 34.sp, fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Black, color = NeonCyan)
                    Spacer(modifier = Modifier.width(4.dp))
                    Text(upUnit, fontSize = 12.sp, color = Color(0xFF757D8A), fontFamily = FontFamily.Monospace, modifier = Modifier.padding(bottom = 6.dp))
                }
            }
        }
    }
}

@Composable
private fun OrbitConnectionCard(connectionState: ConnectionState, onToggleConnection: () -> Unit) {
    val isConnected = connectionState == ConnectionState.Connected
    val isConnecting = connectionState == ConnectionState.Connecting

    val orbBg = if (isConnected) NeonCyan else DarkBg
    val orbFg = if (isConnected) Color.Black else Color.White
    val orbScale = remember { Animatable(1f) }
    val scope = rememberCoroutineScope()
    
    val onOrbitClick: () -> Unit = {
        scope.launch {
            orbScale.snapTo(0.88f)
            orbScale.animateTo(1f, spring(dampingRatio = 0.55f, stiffness = 600f))
        }
        onToggleConnection()
    }

    val boxShape = CutCornerShape(topEnd = 24.dp, bottomStart = 24.dp)

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(boxShape)
            .background(CardBg)
            .border(1.5.dp, StrokeColor, boxShape)
            .padding(16.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Box(
            modifier = Modifier
                .size(110.dp)
                .graphicsLayer { scaleX = orbScale.value; scaleY = orbScale.value }
                .clip(CutCornerShape(16.dp))
                .border(2.dp, if(isConnected) NeonCyan else StrokeColor, CutCornerShape(16.dp))
                .clickable(onClick = onOrbitClick)
                .background(if (isConnected) NeonCyan.copy(alpha = 0.1f) else DarkBg),
            contentAlignment = Alignment.Center
        ) {
            if (isConnecting) {
                CircularProgressIndicator(color = NeonCyan, strokeWidth = 3.dp, modifier = Modifier.size(40.dp))
            } else {
                Icon(Icons.Default.PowerSettingsNew, contentDescription = null, tint = if(isConnected) NeonCyan else Color.White, modifier = Modifier.size(46.dp))
            }
        }

        Spacer(Modifier.width(20.dp))
        Column(Modifier.weight(1f)) {
            Text("SYSTEM_LINK //", fontSize = 12.sp, fontWeight = FontWeight.Bold, color = Color(0xFF757D8A), fontFamily = FontFamily.Monospace)
            Spacer(Modifier.height(8.dp))
            Text(
                text = when {
                    isConnecting -> "CONNECTING..."
                    isConnected -> "CONNECTED"
                    else -> "STANDBY"
                },
                fontSize = 24.sp, fontWeight = FontWeight.Black, color = if(isConnected) NeonCyan else Color.White, fontFamily = FontFamily.Monospace
            )
        }
    }
}

// ==================== GAME BOOST SECTION ====================
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
    val shape = CutCornerShape(topStart = 16.dp, bottomEnd = 16.dp)

    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(CardBg)
            .border(1.5.dp, StrokeColor, shape)
            .padding(12.dp)
    ) {
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Filled.PlayArrow, contentDescription = null, tint = NeonCyan, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(6.dp))
                Text(text = "BOOST_ROUTER", color = Color.White, fontSize = 13.sp, fontWeight = FontWeight.Black, fontFamily = FontFamily.Monospace)
            }
            Text(text = "AUTO", color = NeonYellow, fontSize = 11.sp, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace)
        }
        Spacer(Modifier.height(12.dp))
        LazyRow(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            items(games.size) { index ->
                val game = games[index]
                GamePosterCard(game = game, isSelected = game.id == activeGame, onClick = { activeGame = game.id; onGameSelect(game.id) })
            }
        }
    }
}

@Composable
private fun GamePosterCard(game: BoostGame, isSelected: Boolean, onClick: () -> Unit) {
    var posterBitmap by remember { mutableStateOf(posterMemoryCache[game.id]) }
    var isLoading by remember { mutableStateOf(posterBitmap == null) }

    LaunchedEffect(game.id) {
        if (posterMemoryCache.containsKey(game.id)) { posterBitmap = posterMemoryCache[game.id]; isLoading = false; return@LaunchedEffect }
        withContext(Dispatchers.IO) {
            try {
                val conn = (URL(game.posterUrl).openConnection() as HttpURLConnection).apply { setRequestProperty("User-Agent", "Mozilla/5.0"); connectTimeout = 10000; readTimeout = 10000 }
                if (conn.responseCode in 200..299) {
                    val bytes = conn.inputStream.readBytes()
                    BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.let { decoded -> val img = decoded.asImageBitmap(); posterMemoryCache[game.id] = img; posterBitmap = img }
                }
            } catch (_: Exception) {} finally { isLoading = false }
        }
    }

    val posterShape = CutCornerShape(topStart = 8.dp, bottomEnd = 8.dp)
    Box(
        modifier = Modifier
            .width(96.dp)
            .height(128.dp)
            .clip(posterShape)
            .background(DarkBg)
            .border(width = if (isSelected) 2.dp else 1.dp, color = if (isSelected) NeonCyan else StrokeColor, shape = posterShape)
            .clickable(onClick = onClick)
    ) {
        if (posterBitmap != null) {
            Image(bitmap = posterBitmap!!, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
        } else {
            Box(Modifier.fillMaxSize().background(DarkBg))
        }
        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color(0x22000000), Color(0xCC07080B)))))
        Column(Modifier.fillMaxSize().padding(8.dp), verticalArrangement = Arrangement.SpaceBetween, horizontalAlignment = Alignment.CenterHorizontally) {
            Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.TopEnd) {
                if (game.isHot) {
                    Box(Modifier.clip(CutCornerShape(4.dp)).background(NeonYellow).padding(horizontal = 4.dp, vertical = 2.dp)) {
                        Text("HOT", color = Color.Black, fontSize = 8.sp, fontWeight = FontWeight.Black)
                    }
                }
            }
            Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.fillMaxWidth()) {
                Text(game.name.uppercase(), color = Color.White, fontSize = 10.sp, fontWeight = FontWeight.Black, maxLines = 1, overflow = TextOverflow.Ellipsis, fontFamily = FontFamily.Monospace)
                Spacer(Modifier.height(4.dp))
                Text(if(isSelected) "ACTIVE" else "READY", color = if(isSelected) NeonCyan else Color(0xFF757D8A), fontSize = 8.sp, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace)
            }
        }
    }
}
