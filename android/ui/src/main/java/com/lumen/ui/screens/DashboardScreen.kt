package com.lumen.ui.screens

import android.graphics.BitmapFactory
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Layers
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.PowerSettingsNew
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.layout.ContentScale
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
// Neon Gaming Colors
private val GoldNeon = Color(0xFFFFB800)
private val DarkBg = Color(0xFF07080B)
private val CardBg = Color(0xFF10121A)
private val StrokeColor = Color(0xFF1B1E2B)

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
    val lumenColors = LocalLumenColors.current
    val tick = rememberHapticTick()
    var showAddSheet by remember { mutableStateOf(false) }
    var showServerPicker by remember { mutableStateOf(false) }
    val openServerPicker: () -> Unit = { showServerPicker = true }

    val activeNode = nodes.firstOrNull { it.isSelected } ?: nodes.firstOrNull()
    val activeSub = subscriptions.firstOrNull { it.id == activeNode?.subscriptionId }
        ?: subscriptions.firstOrNull()
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
    val isConnecting = connectionState == ConnectionState.Connecting

    val scrollState = rememberScrollState()

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(DarkBg)
            .statusBarsPadding()
    ) {
        BrandTopBar(
            onAddClick = { showAddSheet = true }
        )

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
                    when (dashboardStyle) {
                        DashboardStyle.SLIDER -> { }
                        DashboardStyle.DEFAULT -> {
                            com.lumen.ui.components.ConnectionStage(connectionState, activeNode?.name, {
                                tick(HapticFeedbackType.LongPress)
                                onToggleConnection()
                            })
                        }
                        DashboardStyle.CENTERED -> OrbitConnectionCard(connectionState, onToggleConnection)
                        DashboardStyle.DOCK -> DockDashboardStatus(connectionState, activeNode?.displayProtocol)
                    }

                    Spacer(modifier = Modifier.height(14.dp))

                    // ============================================
                    // BALOCH PING GAMING SECTION
                    // ============================================
                    GameBoostSection(selectedGameId = "pubg")
                    Spacer(modifier = Modifier.height(12.dp))
                    
                    // پینگ هوشمند: اگه وصل بود و پینگ لایو نداشت، پینگ سرور رو نشون میده
                    val displayPing = connectedPing ?: activeNode?.pingMs?.takeIf { it > 0 }?.let { "$it ms" }
                    com.lumen.ui.components.LivePingCard(
                        ping = displayPing,
                        isConnected = isConnected,
                        modifier = Modifier.fillMaxWidth()
                    )
                    Spacer(modifier = Modifier.height(14.dp))
                    // ============================================

                    // 3. Section Header
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 4.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            text = strings.serverLabel,
                            fontSize = 17.sp,
                            fontWeight = FontWeight.Bold,
                            color = Color.White
                        )
                        TextButton(
                            onClick = openServerPicker,
                            modifier = Modifier
                                .height(48.dp)
                                .expressiveButtonSemantics(strings.changeServer, openServerPicker)
                        ) {
                            Text(
                                text = strings.changeServer,
                                fontSize = 13.sp,
                                fontWeight = FontWeight.SemiBold,
                                color = GoldNeon
                            )
                        }
                    }

                    Spacer(modifier = Modifier.height(8.dp))

                    // 4. Selected Server Row Card (Neon Themed)
                    if (activeNode != null) {
                        val flagEmoji = CountryFlagHelper.getServerFlagEmoji(activeNode.countryCode)

                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(24.dp))
                                .background(CardBg)
                                .border(1.dp, StrokeColor, RoundedCornerShape(24.dp))
                                .expressiveButtonSemantics(activeNode.name + ". " + activeNode.displayProtocol, openServerPicker)
                                .clickable(role = androidx.compose.ui.semantics.Role.Button, onClick = openServerPicker)
                                .padding(horizontal = 14.dp, vertical = 12.dp),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Box(
                                modifier = Modifier
                                    .size(38.dp)
                                    .clip(RoundedCornerShape(topStart = 6.dp, topEnd = 18.dp, bottomEnd = 6.dp, bottomStart = 18.dp))
                                    .background(DarkBg),
                                contentAlignment = Alignment.Center
                            ) {
                                Text(
                                    text = flagEmoji,
                                    fontSize = 18.sp,
                                    fontWeight = FontWeight.Bold
                                )
                            }

                            Spacer(modifier = Modifier.width(12.dp))

                            Column(modifier = Modifier.weight(1f)) {
                                Text(
                                    text = activeNode.name,
                                    fontSize = 15.sp,
                                    fontWeight = FontWeight.Bold,
                                    color = Color.White,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis
                                )
                                Spacer(modifier = Modifier.height(2.dp))
                                Text(
                                    text = activeNode.displayProtocol,
                                    fontSize = 12.sp,
                                    color = Color(0xFF757D8A),
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis
                                )
                            }

                            val pingMs = activeNode.pingMs
                            val pingText = when {
                                activeNode.id in pingingNodeIds -> "..."
                                pingMs != null && pingMs > 0 -> "$pingMs ms"
                                connectedPing != null && isConnected -> connectedPing
                                else -> "--"
                            }

                            val pColor = pingMs?.let(::pingColor) ?: Color(0xFF757D8A)
                            Box(
                                modifier = Modifier
                                    .clip(RoundedCornerShape(10.dp))
                                    .background(pColor.copy(alpha = .16f))
                                    .padding(horizontal = 9.dp, vertical = 6.dp)
                            ) {
                                Text(
                                    text = pingText,
                                    fontSize = 12.sp,
                                    fontWeight = FontWeight.SemiBold,
                                    color = pColor,
                                    style = MaterialTheme.typography.labelSmall.copy(textDirection = androidx.compose.ui.text.style.TextDirection.Ltr)
                                )
                            }

                            Spacer(modifier = Modifier.width(10.dp))

                            Icon(
                                imageVector = Icons.AutoMirrored.Filled.KeyboardArrowRight,
                                contentDescription = strings.changeServer,
                                tint = GoldNeon,
                                modifier = Modifier.size(20.dp)
                            )
                        }
                    } else {
                        Box(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(20.dp))
                                .background(CardBg)
                                .border(1.dp, StrokeColor, RoundedCornerShape(20.dp))
                                .clickable(onClick = openServerPicker)
                                .padding(16.dp),
                            contentAlignment = Alignment.Center
                        ) {
                            Text(
                                text = strings.noServerSelected,
                                fontSize = 14.sp,
                                color = Color(0xFF757D8A)
                            )
                        }
                    }

                    Spacer(modifier = Modifier.height(10.dp))

                    if (speedStatsEnabled) DashboardSpeedCards(isConnected, speedValues)

                    activeSub?.let { sub ->
                        val summary = listOfNotNull(
                            sub.trafficSummary,
                            sub.expiryDaysLeft?.let { "$it ${strings.daysRemaining}" }
                        ).joinToString(" · ")
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(20.dp))
                                .background(CardBg)
                                .border(1.dp, StrokeColor, RoundedCornerShape(20.dp))
                                .clickable(onClick = onOpenServers)
                                .padding(horizontal = 14.dp, vertical = 12.dp),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Icon(Icons.Default.Layers, contentDescription = null,
                                tint = GoldNeon, modifier = Modifier.size(22.dp))
                            Spacer(Modifier.width(12.dp))
                            Column(Modifier.weight(1f)) {
                                Text(sub.name, style = MaterialTheme.typography.labelLarge, color = Color.White,
                                    maxLines = 1, overflow = TextOverflow.Ellipsis)
                                if (summary.isNotBlank()) Text(summary, style = MaterialTheme.typography.bodySmall,
                                    color = Color(0xFF757D8A),
                                    maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                            sub.trafficRatio?.let { ratio ->
                                Box(Modifier.width(50.dp).height(4.dp).clip(RoundedCornerShape(6.dp))
                                    .background(DarkBg)) {
                                    Box(Modifier.fillMaxWidth(ratio.coerceIn(0f, 1f)).height(4.dp)
                                        .background(GoldNeon))
                                }
                            }
                        }
                    }

                    // فاصله بسیار زیاد برای اطمینان صد در صد از نرفتن زیر منو
                    Spacer(modifier = Modifier.height(200.dp))
                }
            }
        }
    }

    if (showServerPicker) DashboardServerPicker(groupNodes, groupTitle,
        onDismiss = { showServerPicker = false },
        onSelect = { onSelectNode(it); showServerPicker = false },
        pinging = pingingNodeIds, onPingAll = { onPingNodes(groupNodes) },
        onPingNode = { onPingNodes(listOf(it)) }, onStopPing = onStopPing)

    if (showAddSheet) {
        AddServerSheet(
            onDismissRequest = { showAddSheet = false },
            onImportClipboard = onImportClipboard,
            onImportQr = scanQr,
            onImportFile = onImportFile,
            onAddManualNode = onAddManualNode
        )
    }
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
    val strings = LocalStrings.current
    val speeds = if (isConnected) values() else DashboardSpeeds()

    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(9.dp)
    ) {
        val (downVal, downUnit) = formatDashboardSpeed(speeds.download)
        Box(
            modifier = Modifier
                .weight(1.12f)
                .clip(RoundedCornerShape(topStart = 30.dp, topEnd = 30.dp, bottomEnd = 30.dp, bottomStart = 10.dp))
                .background(CardBg)
                .border(1.dp, StrokeColor, RoundedCornerShape(topStart = 30.dp, topEnd = 30.dp, bottomEnd = 30.dp, bottomStart = 10.dp))
                .padding(14.dp)
        ) {
            Column {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(
                        modifier = Modifier
                            .size(24.dp)
                            .clip(RoundedCornerShape(8.dp))
                            .background(DarkBg),
                        contentAlignment = Alignment.Center
                    ) {
                        Icon(
                            imageVector = Icons.Default.ArrowDownward,
                            contentDescription = null,
                            tint = GoldNeon,
                            modifier = Modifier
                                .size(14.dp)
                                .graphicsLayer { rotationZ = 45f }
                        )
                    }
                    Spacer(modifier = Modifier.width(7.dp))
                    Text(
                        text = "↓ ${strings.speedLabel}",
                        fontSize = 12.sp,
                        color = Color(0xFF757D8A)
                    )
                }
                Spacer(modifier = Modifier.height(8.dp))
                Row(verticalAlignment = Alignment.Bottom) {
                    Text(
                        text = downVal,
                        fontSize = 38.sp,
                        fontFamily = com.lumen.ui.theme.LumenMono,
                        letterSpacing = (-2).sp,
                        fontWeight = FontWeight.Bold,
                        color = GoldNeon
                    )
                    Spacer(modifier = Modifier.width(4.dp))
                    Text(
                        text = downUnit,
                        fontSize = 12.sp,
                        color = Color(0xFF757D8A)
                    )
                }
            }
        }

        val (upVal, upUnit) = formatDashboardSpeed(speeds.upload)
        Box(
            modifier = Modifier
                .weight(.88f)
                .clip(RoundedCornerShape(topStart = 30.dp, topEnd = 10.dp, bottomEnd = 30.dp, bottomStart = 30.dp))
                .background(CardBg)
                .border(1.dp, StrokeColor, RoundedCornerShape(topStart = 30.dp, topEnd = 10.dp, bottomEnd = 30.dp, bottomStart = 30.dp))
                .padding(14.dp)
        ) {
            Column {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(
                        modifier = Modifier
                            .size(24.dp)
                            .clip(RoundedCornerShape(8.dp))
                            .background(DarkBg),
                        contentAlignment = Alignment.Center
                    ) {
                        Icon(
                            imageVector = Icons.Default.ArrowUpward,
                            contentDescription = null,
                            tint = Color(0xFF00E676),
                            modifier = Modifier
                                .size(14.dp)
                                .graphicsLayer { rotationZ = 45f }
                        )
                    }
                    Spacer(modifier = Modifier.width(7.dp))
                    Text(
                        text = "↑ ${strings.speedLabel}",
                        fontSize = 12.sp,
                        color = Color(0xFF757D8A)
                    )
                }
                Spacer(modifier = Modifier.height(8.dp))
                Row(verticalAlignment = Alignment.Bottom) {
                    Text(
                        text = upVal,
                        fontSize = 38.sp,
                        fontFamily = com.lumen.ui.theme.LumenMono,
                        letterSpacing = (-2).sp,
                        fontWeight = FontWeight.Bold,
                        color = Color(0xFF00E676)
                    )
                    Spacer(modifier = Modifier.width(4.dp))
                    Text(
                        text = upUnit,
                        fontSize = 12.sp,
                        color = Color(0xFF757D8A)
                    )
                }
            }
        }
    }
    Spacer(modifier = Modifier.height(18.dp))
}

@Composable
private fun OrbitConnectionCard(connectionState: ConnectionState, onToggleConnection: () -> Unit) {
    val strings = LocalStrings.current
    val tick = rememberHapticTick()
    val isConnected = connectionState == ConnectionState.Connected
    val isConnecting = connectionState == ConnectionState.Connecting

    val orbBg = if (isConnected) GoldNeon else DarkBg
    val orbFg = if (isConnected) Color.Black else Color.White
    val orbScale = remember { Animatable(1f) }
    val scope = rememberCoroutineScope()
    val rotationSpeed = animateFloatAsState(
        targetValue = when { isConnecting -> 200f; isConnected -> 24f; else -> 0f },
        animationSpec = tween(900, easing = FastOutSlowInEasing),
        label = "orbit_speed"
    )
    var orbRotation by remember { mutableFloatStateOf(-6f) }
    val spinActive by remember(isConnecting, isConnected) {
        androidx.compose.runtime.derivedStateOf { isConnecting || isConnected || rotationSpeed.value > 0.1f }
    }
    LaunchedEffect(spinActive) {
        if (spinActive) {
            var lastFrame = withFrameNanos { it }
            while (true) {
                val frame = withFrameNanos { it }
                val elapsed = ((frame - lastFrame) / 1_000_000_000f).coerceAtMost(0.05f)
                orbRotation = (orbRotation + rotationSpeed.value * elapsed) % 360f
                lastFrame = frame
            }
        }
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
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(topStart = 36.dp, topEnd = 12.dp, bottomEnd = 36.dp, bottomStart = 36.dp))
            .background(CardBg)
            .border(1.dp, StrokeColor, RoundedCornerShape(topStart = 36.dp, topEnd = 12.dp, bottomEnd = 36.dp, bottomStart = 36.dp))
            .padding(16.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Box(
            modifier = Modifier
                .expressiveButtonSemantics(when { isConnecting -> strings.cancel; isConnected -> strings.centerDisconnect; else -> strings.centerConnect }, onOrbitClick)
                .size(168.dp)
                .graphicsLayer {
                    scaleX = orbScale.value
                    scaleY = orbScale.value
                }
                .clip(CircleShape)
                .clickable(role = androidx.compose.ui.semantics.Role.Button, onClick = onOrbitClick),
            contentAlignment = Alignment.Center
        ) {
            Box(
                Modifier
                    .matchParentSize()
                    .graphicsLayer { rotationZ = orbRotation }
                    .clip(com.lumen.ui.components.expressiveShape(com.lumen.ui.components.Motif.Flower))
                    .background(orbBg)
            )
            if (isConnecting) {
                CircularProgressIndicator(
                    modifier = Modifier.size(56.dp),
                    color = orbFg,
                    strokeWidth = 4.dp
                )
            } else {
                Icon(
                    imageVector = Icons.Default.PowerSettingsNew,
                    contentDescription = null,
                    tint = orbFg,
                    modifier = Modifier.size(54.dp)
                )
            }
        }

        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Text(strings.connection.uppercase(), style = MaterialTheme.typography.labelSmall,
                color = Color(0xFF757D8A))
            Spacer(Modifier.height(12.dp))
            Text(
                text = when {
                    isConnecting -> strings.centerConnecting
                    isConnected -> strings.centerDisconnect
                    else -> strings.centerConnect
                },
                style = MaterialTheme.typography.headlineMedium.copy(fontSize = 22.sp, lineHeight = 26.sp),
                color = Color.White, maxLines = 2
            )
        }
    }
}

// ==================== GAME BOOST SECTION ====================
data class BoostGame(
    val id: String,
    val name: String,
    val posterUrl: String,
    val isHot: Boolean = false
)

@Composable
fun GameBoostSection(
    modifier: Modifier = Modifier,
    selectedGameId: String = "pubg",
    onGameSelect: (String) -> Unit = {}
) {
    val strings = LocalStrings.current
    val games = remember {
        listOf(
            BoostGame("pubg", "PUBG Mobile", "https://cdn.cloudflare.steamstatic.com/steam/apps/578080/library_600x900.jpg", true),
            BoostGame("cod", "Call of Duty", "https://cdn.cloudflare.steamstatic.com/steam/apps/1938090/library_600x900.jpg"),
            BoostGame("efootball", "eFootball", "https://cdn.cloudflare.steamstatic.com/steam/apps/1665460/library_600x900.jpg"),
            BoostGame("valorant", "Valorant", "https://cdn1.epicgames.com/offer/cbd5b3d310a54b12bf3fe8c41994174f/EGS_VALORANT_RiotGames_S2_1200x1600-b2f707f43db4af1658b1d4ed83134914"),
            BoostGame("genshin", "Genshin", "https://cdn1.epicgames.com/salesEvent/salesEvent/EGS_GenshinImpact_miHoYoLimited_S2_1200x1600-c12cd2583aeab61e766ed2fb5257376f")
        )
    }

    var activeGame by remember { mutableStateOf(selectedGameId) }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .background(CardBg)
            .border(1.dp, StrokeColor, RoundedCornerShape(18.dp))
            .padding(vertical = 12.dp, horizontal = 12.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    imageVector = Icons.Filled.PlayArrow,
                    contentDescription = null,
                    tint = GoldNeon,
                    modifier = Modifier.size(16.dp)
                )
                Spacer(Modifier.width(6.dp))
                Text(text = "Game Boost", color = Color.White, fontSize = 13.sp, fontWeight = FontWeight.Bold)
            }
            Text(text = "Auto-Optimize", color = Color(0xFF6B7280), fontSize = 11.sp)
        }

        Spacer(Modifier.height(10.dp))

        LazyRow(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            items(games.size) { index ->
                val game = games[index]
                GamePosterCard(
                    game = game,
                    isSelected = game.id == activeGame,
                    onClick = {
                        activeGame = game.id
                        onGameSelect(game.id)
                    }
                )
            }
        }
    }
}

@Composable
private fun GamePosterCard(game: BoostGame, isSelected: Boolean, onClick: () -> Unit) {
    var posterBitmap by remember { mutableStateOf(posterMemoryCache[game.id]) }
    var isLoading by remember { mutableStateOf(posterBitmap == null) }

    LaunchedEffect(game.id) {
        if (posterMemoryCache.containsKey(game.id)) {
            posterBitmap = posterMemoryCache[game.id]
            isLoading = false
            return@LaunchedEffect
        }
        withContext(Dispatchers.IO) {
            try {
                val conn = (URL(game.posterUrl).openConnection() as HttpURLConnection).apply {
                    setRequestProperty("User-Agent", "Mozilla/5.0")
                    connectTimeout = 10000; readTimeout = 10000
                }
                if (conn.responseCode in 200..299) {
                    val bytes = conn.inputStream.readBytes()
                    BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.let { decoded ->
                        val img = decoded.asImageBitmap()
                        posterMemoryCache[game.id] = img
                        posterBitmap = img
                    }
                }
            } catch (_: Exception) {} finally { isLoading = false }
        }
    }

    Box(
        modifier = Modifier
            .width(104.dp)
            .height(138.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(DarkBg)
            .border(
                width = if (isSelected) 2.2.dp else 1.dp,
                color = if (isSelected) GoldNeon else StrokeColor,
                shape = RoundedCornerShape(14.dp)
            )
            .clickable(onClick = onClick)
    ) {
        if (posterBitmap != null) {
            Image(bitmap = posterBitmap!!, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
        } else if (isLoading) {
            Box(Modifier.fillMaxSize().background(DarkBg), contentAlignment = Alignment.Center) {
                CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp, color = GoldNeon.copy(alpha = 0.6f))
            }
        } else {
            Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color(0xFF242A38), DarkBg))), contentAlignment = Alignment.Center) {
                Icon(Icons.Filled.PlayArrow, contentDescription = null, tint = GoldNeon.copy(alpha = 0.7f), modifier = Modifier.size(28.dp))
            }
        }

        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color(0x35000000), Color.Transparent, Color(0xCC07080B), Color(0xF507080B)))))

        Column(Modifier.fillMaxSize().padding(8.dp), verticalArrangement = Arrangement.SpaceBetween, horizontalAlignment = Alignment.CenterHorizontally) {
            Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.TopEnd) {
                if (game.isHot) {
                    Box(Modifier.clip(RoundedCornerShape(4.dp)).background(GoldNeon).padding(horizontal = 4.dp, vertical = 1.dp)) {
                        Text("HOT", color = Color.Black, fontSize = 8.sp, fontWeight = FontWeight.Black)
                    }
                }
            }
            Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.fillMaxWidth()) {
                Text(game.name, color = Color.White, fontSize = 11.sp, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Spacer(Modifier.height(3.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(5.dp).clip(CircleShape).background(Color(0xFF00E676)))
                    Spacer(Modifier.width(4.dp))
                    Text("Optimized", color = Color(0xFF00E676), fontSize = 8.sp, fontWeight = FontWeight.Medium)
                }
            }
        }
    }
}
