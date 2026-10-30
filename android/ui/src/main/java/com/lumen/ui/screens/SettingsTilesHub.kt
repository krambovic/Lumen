package com.lumen.ui.screens

import androidx.activity.compose.BackHandler
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.VectorConverter
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.automirrored.filled.AltRoute
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.AbsoluteAlignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import com.lumen.ui.components.BrandTopBar
import com.lumen.ui.components.ExpressiveNavigationTile
import com.lumen.ui.components.Motif
import com.lumen.ui.components.expressiveShape
import com.lumen.ui.theme.expressivePalette
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

/** A local draft owns editing. Only Done commits the layout to application settings. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun SettingsTilesHub(
    state: SettingsUiState, onUpdate: (SettingsUiState) -> Unit,
    onOpen: (SettingsTileId) -> Unit, modifier: Modifier = Modifier, resetSignal: Int = 0
) {
    val s = LocalStrings.current
    val art = expressivePalette()
    var editing by rememberSaveable { mutableStateOf(false) }
    var draft by rememberSaveable { mutableStateOf("") }
    var adding by rememberSaveable { mutableStateOf(false) }
    val tiles = remember(editing, draft, state.settingsTileLayout) {
        decodeSettingsTiles(if (editing) draft else state.settingsTileLayout)
    }
    fun cancel() { editing = false; adding = false }
    BackHandler(editing && !adding) { cancel() }
    LaunchedEffect(resetSignal) { if (resetSignal > 0) cancel() }
    val scrollState = rememberScrollState()
    var viewport by remember { mutableStateOf(Rect.Zero) }

    Column(modifier.fillMaxSize().statusBarsPadding()) {
        BrandTopBar(title = s.settings, actions = {
            if (editing) {
                FilledTonalButton(onClick = {
                    onUpdate(state.copy(settingsTileLayout = encodeSettingsTiles(tiles)))
                    cancel()
                }, contentPadding = PaddingValues(horizontal = 16.dp, vertical = 12.dp)) {
                    Icon(Icons.Default.Check, null, Modifier.size(18.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(s.done)
                }
            } else {
                IconButton(onClick = {
                    draft = encodeSettingsTiles(tiles)
                    editing = true
                }, modifier = Modifier.size(48.dp).clip(expressiveShape(Motif.Clover)).background(art.stage)) {
                    Icon(Icons.Default.Edit, s.settingsEditTiles, tint = art.onStage, modifier = Modifier.size(23.dp))
                }
            }
        })
        Text(if (editing) s.settingsTileEditHint else s.settingsAll.uppercase(),
            style = if (editing) MaterialTheme.typography.bodySmall else MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp))
        Column(Modifier.weight(1f).fillMaxWidth().onGloballyPositioned { viewport = it.boundsInWindow() }
            .verticalScroll(scrollState).padding(horizontal = 16.dp)) {
            SettingsTileGrid(tiles, editing, state, viewport, scrollState,
                onChange = { draft = encodeSettingsTiles(it) }, onOpen = onOpen)
            if (tiles.isEmpty()) Text(s.settingsNoTiles,
                modifier = Modifier.fillMaxWidth().padding(vertical = 40.dp),
                textAlign = TextAlign.Center, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (!editing) Text("Lumen · ${LumenVersion.appVersion}",
                style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth().padding(top = 24.dp, bottom = 16.dp))
            Spacer(Modifier.height(if (editing) 20.dp else 96.dp))
        }
        if (editing) {
            Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 88.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { adding = true }, modifier = Modifier.weight(1f).heightIn(min = 48.dp)) {
                    Icon(Icons.Default.Add, null, Modifier.size(20.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(s.settingsAddTile)
                }
                TextButton(onClick = { draft = encodeSettingsTiles(defaultSettingsTiles()) },
                    modifier = Modifier.heightIn(min = 48.dp)) { Text(s.settingsResetTiles) }
            }
        }
    }

    if (adding && editing) {
        val hidden = SettingsTileId.entries.filter { id -> tiles.none { it.id == id } }
        ModalBottomSheet(onDismissRequest = { adding = false },
            sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
            Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState())
                .padding(start = 20.dp, end = 20.dp, bottom = 32.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(s.settingsAddTile, style = MaterialTheme.typography.headlineSmall,
                    modifier = Modifier.padding(bottom = 8.dp))
                if (hidden.isEmpty()) Text(s.settingsAllTilesAdded,
                    style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                hidden.forEach { id ->
                    FilledTonalButton(onClick = {
                        val default = defaultSettingsTiles().first { it.id == id }
                        draft = encodeSettingsTiles(tiles + default)
                    }, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp), shape = RoundedCornerShape(18.dp)) {
                        Text(settingsTileTitle(id, s), Modifier.weight(1f), textAlign = TextAlign.Start)
                        Icon(Icons.Default.Add, s.add, Modifier.size(22.dp))
                    }
                }
            }
        }
    }
}

@Composable
private fun SettingsTileGrid(
    tiles: List<SettingsTileSpec>, editing: Boolean, state: SettingsUiState, viewport: Rect,
    scroll: androidx.compose.foundation.ScrollState, onChange: (List<SettingsTileSpec>) -> Unit,
    onOpen: (SettingsTileId) -> Unit
) {
    val s = LocalStrings.current
    val haptic = LocalHapticFeedback.current
    val density = LocalDensity.current
    val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    val latestTiles by rememberUpdatedState(tiles)
    val latestChange by rememberUpdatedState(onChange)
    var dragged by remember(editing) { mutableStateOf<SettingsTileId?>(null) }
    var dragTopLeft by remember { mutableStateOf(Offset.Zero) }
    var grabPoint by remember { mutableStateOf(Offset.Zero) }
    var gridBounds by remember { mutableStateOf(Rect.Zero) }
    val latestViewport by rememberUpdatedState(viewport)

    BoxWithConstraints(Modifier.fillMaxWidth().onGloballyPositioned { gridBounds = it.boundsInWindow() }) {
        val columns = if (maxWidth >= 600.dp) 4 else 2
        val gap = 8.dp
        val cellWidth = (maxWidth - gap * (columns - 1)) / columns
        val cellHeight = (cellWidth * .9f).coerceIn(140.dp, 168.dp)
        val stepX = with(density) { (cellWidth + gap).toPx() }
        val stepY = with(density) { (cellHeight + gap).toPx() }
        val placements = remember(tiles, columns) { placeSettingsTiles(tiles, columns) }
        val rows = placements.maxOfOrNull { it.row + it.tile.size.rows } ?: 0
        val latestPlacements by rememberUpdatedState(placements)

        fun moveAtPointer() {
            val id = dragged ?: return
            val pointer = dragTopLeft + grabPoint
            val target = latestPlacements.firstOrNull { placement ->
                val column = if (rtl) columns - placement.column - placement.tile.size.columns else placement.column
                val left = column * stepX
                val top = placement.row * stepY
                val width = placement.tile.size.columns * stepX - with(density) { gap.toPx() }
                val height = placement.tile.size.rows * stepY - with(density) { gap.toPx() }
                placement.tile.id != id && pointer.x in left..(left + width) && pointer.y in top..(top + height)
            } ?: return
            val current = latestTiles
            latestChange(moveSettingsTile(current, current.indexOfFirst { it.id == id },
                current.indexOfFirst { it.id == target.tile.id }))
        }
        val latestMoveAtPointer by rememberUpdatedState(::moveAtPointer)
        // Keep the pointer anchored while the scroll container moves under the dragged tile.
        LaunchedEffect(dragged, columns) {
            if (dragged == null) return@LaunchedEffect
            val edge = with(density) { 64.dp.toPx() }
            var previousFrame = 0L
            while (dragged != null) {
                withFrameNanos { frame ->
                    val elapsed = if (previousFrame == 0L) 0f else ((frame - previousFrame) / 1_000_000_000f).coerceAtMost(.05f)
                    previousFrame = frame
                    val pointerY = gridBounds.top + dragTopLeft.y + grabPoint.y
                    val bounds = latestViewport
                    val strength = when {
                        bounds.height <= 0f -> 0f
                        pointerY < bounds.top + edge -> -((bounds.top + edge - pointerY) / edge).coerceIn(0f, 1f)
                        pointerY > bounds.bottom - edge -> ((pointerY - bounds.bottom + edge) / edge).coerceIn(0f, 1f)
                        else -> 0f
                    }
                    // The actual suspend scroll runs below; this callback only computes velocity.
                    strength * with(density) { 540.dp.toPx() } * elapsed
                }.let { distance ->
                    if (distance != 0f) {
                        val consumed = scroll.scrollBy(distance)
                        dragTopLeft += Offset(0f, consumed)
                        latestMoveAtPointer()
                    }
                }
            }
        }

        Box(Modifier.fillMaxWidth().height(if (rows == 0) 0.dp else (cellHeight + gap) * rows - gap),
            contentAlignment = AbsoluteAlignment.TopLeft) {
            placements.forEach { placement -> key(placement.tile.id) {
                val tile = placement.tile
                val column = if (rtl) columns - placement.column - tile.size.columns else placement.column
                val anchor = IntOffset((column * stepX).roundToInt(), (placement.row * stepY).roundToInt())
                val title = settingsTileTitle(tile.id, s)
                fun updateSize() {
                    onChange(tiles.map { if (it.id == tile.id) it.copy(size = it.size.next()) else it })
                }
                SettingsTileCell(tile, anchor, cellWidth * tile.size.columns + gap * (tile.size.columns - 1),
                    cellHeight * tile.size.rows + gap * (tile.size.rows - 1), editing, dragged == tile.id,
                    dragPosition = { dragTopLeft },
                    onStart = { point ->
                        dragged = tile.id
                        grabPoint = point
                        dragTopLeft = Offset(anchor.x.toFloat(), anchor.y.toFloat())
                        if (state.hapticsEnabled) haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                    },
                    onDrag = { delta -> dragTopLeft += delta; moveAtPointer() },
                    onStop = { dragged = null },
                    customActions = listOf(
                        CustomAccessibilityAction(s.settingsMoveEarlier) {
                            val index = tiles.indexOf(tile)
                            if (index > 0) { onChange(moveSettingsTile(tiles, index, index - 1)); true } else false
                        },
                        CustomAccessibilityAction(s.settingsMoveLater) {
                            val index = tiles.indexOf(tile)
                            if (index < tiles.lastIndex) { onChange(moveSettingsTile(tiles, index, index + 1)); true } else false
                        },
                        CustomAccessibilityAction(s.settingsResizeTile) { updateSize(); true },
                        CustomAccessibilityAction(s.settingsHideTile) { onChange(tiles.filterNot { it.id == tile.id }); true }
                    ), title = title, sizeLabel = settingsTileSizeLabel(tile.size, s)
                ) {
                    SettingsTileArtwork(tile, Modifier.fillMaxSize(), interactive = !editing) { onOpen(tile.id) }
                    if (editing) {
                        IconButton(onClick = { onChange(tiles.filterNot { it.id == tile.id }) },
                            modifier = Modifier.align(Alignment.TopStart).size(48.dp)) {
                            Icon(Icons.Default.Remove, s.settingsHideTile + ": " + title,
                                Modifier.size(26.dp).background(MaterialTheme.colorScheme.errorContainer, CircleShape).padding(3.dp),
                                tint = MaterialTheme.colorScheme.onErrorContainer)
                        }
                        IconButton(onClick = { updateSize() },
                            modifier = Modifier.align(Alignment.BottomEnd).size(48.dp)) {
                            Icon(Icons.Default.OpenInFull, s.settingsResizeTile + ": " + title,
                                Modifier.size(26.dp).background(MaterialTheme.colorScheme.surface, RoundedCornerShape(8.dp)).padding(5.dp),
                                tint = MaterialTheme.colorScheme.onSurface)
                        }
                    }
                }
            } }
        }
    }
}

@Composable
private fun SettingsTileCell(
    tile: SettingsTileSpec, anchor: IntOffset, width: androidx.compose.ui.unit.Dp, height: androidx.compose.ui.unit.Dp,
    editing: Boolean, dragging: Boolean, dragPosition: () -> Offset,
    onStart: (Offset) -> Unit, onDrag: (Offset) -> Unit, onStop: () -> Unit,
    customActions: List<CustomAccessibilityAction>, title: String, sizeLabel: String,
    content: @Composable BoxScope.() -> Unit
) {
    val animated = remember(tile.id) { Animatable(Offset(anchor.x.toFloat(), anchor.y.toFloat()), Offset.VectorConverter) }
    val scope = rememberCoroutineScope()
    val currentStart by rememberUpdatedState(onStart)
    val currentDrag by rememberUpdatedState(onDrag)
    val currentStop by rememberUpdatedState(onStop)
    val currentPosition by rememberUpdatedState(dragPosition)
    val currentEditing by rememberUpdatedState(editing)
    val currentDragging by rememberUpdatedState(dragging)
    LaunchedEffect(anchor, dragging, editing) {
        val target = Offset(anchor.x.toFloat(), anchor.y.toFloat())
        if (!editing) animated.snapTo(target)
        else if (!dragging) animated.animateTo(target,
            spring(stiffness = 500f, dampingRatio = .85f))
    }
    fun finishDrag() {
        // Removing pointerInput on exit can cancel its gesture. Only the active
        // editing tile may seed a drop spring; other tiles keep their own anchors.
        if (!currentEditing || !currentDragging) return
        scope.launch {
            if (!currentEditing || !currentDragging) return@launch
            animated.snapTo(currentPosition())
            currentStop()
        }
    }
    Box(Modifier.absoluteOffset {
        val position = if (dragging) currentPosition() else animated.value
        IntOffset(position.x.roundToInt(), position.y.roundToInt())
    }
        .animateContentSize(spring(dampingRatio = .8f, stiffness = 360f))
        .size(width, height).zIndex(if (dragging) 1f else 0f)
        .graphicsLayer {
            if (dragging) {
                scaleX = 1.025f; scaleY = 1.025f
            }
        }
        .then(if (editing) Modifier.semantics {
            contentDescription = title
            stateDescription = sizeLabel
            this.customActions = customActions
        }.pointerInput(tile.id, editing) {
            detectDragGesturesAfterLongPress(
                onDragStart = { currentStart(it) }, onDragEnd = { finishDrag() }, onDragCancel = { finishDrag() }
            ) { change, amount -> change.consume(); currentDrag(amount) }
        } else Modifier), content = content)
}

internal fun settingsTileTitle(id: SettingsTileId, s: LumenStrings): String = when (id) {
    SettingsTileId.APPEARANCE -> s.appearance
    SettingsTileId.ROUTING -> s.routing
    SettingsTileId.DNS -> "DNS"
    SettingsTileId.PING -> s.ping
    SettingsTileId.SUBSCRIPTIONS -> s.subscriptions
    SettingsTileId.APP -> s.settingsAppTile
    SettingsTileId.TRAFFIC -> s.settingsTrafficTile
    SettingsTileId.LOGS -> s.settingsJournal
    SettingsTileId.UPDATES -> s.updates
    SettingsTileId.COMMUNITY -> s.community
}

private fun settingsTileSizeLabel(size: SettingsTileSize, s: LumenStrings): String = when (size) {
    SettingsTileSize.SMALL -> s.settingsTileSmall
    SettingsTileSize.WIDE -> s.settingsTileWide
    SettingsTileSize.TALL -> s.settingsTileTall
    SettingsTileSize.FULL -> s.settingsTileFull
}

@Composable
private fun SettingsTileArtwork(
    tile: SettingsTileSpec, modifier: Modifier, interactive: Boolean, onClick: () -> Unit
) {
    val s = LocalStrings.current
    val art = expressivePalette()
    val id = tile.id
    val container = when (id) {
        SettingsTileId.APPEARANCE -> art.stage
        SettingsTileId.ROUTING, SettingsTileId.UPDATES -> art.lime
        SettingsTileId.DNS, SettingsTileId.TRAFFIC, SettingsTileId.COMMUNITY -> art.blue
        SettingsTileId.PING, SettingsTileId.APP -> art.coral
        else -> art.lavender
    }
    val ink = when (id) {
        SettingsTileId.APPEARANCE -> art.onStage
        SettingsTileId.ROUTING, SettingsTileId.UPDATES -> art.onLime
        SettingsTileId.DNS, SettingsTileId.TRAFFIC, SettingsTileId.COMMUNITY -> art.onBlue
        SettingsTileId.PING, SettingsTileId.APP -> art.onCoral
        else -> art.onLavender
    }
    val description = if (tile.size == SettingsTileSize.SMALL) "" else when (id) {
        SettingsTileId.APPEARANCE -> s.settingsAppearanceSummary
        SettingsTileId.ROUTING -> s.settingsRoutingSummary
        SettingsTileId.DNS -> s.settingsDnsSummary
        SettingsTileId.SUBSCRIPTIONS -> s.settingsSubscriptionsSummary
        SettingsTileId.APP -> s.settingsAppSummary
        SettingsTileId.TRAFFIC -> s.settingsTrafficSummary
        SettingsTileId.LOGS -> s.settingsLogsSummary
        SettingsTileId.UPDATES -> s.settingsUpdatesSummary
        SettingsTileId.PING -> s.settingsPingSummary
        SettingsTileId.COMMUNITY -> s.communitySummary
    }
    val icon = when (id) {
        SettingsTileId.APPEARANCE -> Icons.Default.Palette
        SettingsTileId.ROUTING -> Icons.AutoMirrored.Filled.AltRoute
        SettingsTileId.DNS -> Icons.Default.Dns
        SettingsTileId.PING -> Icons.Default.Speed
        SettingsTileId.SUBSCRIPTIONS -> Icons.Default.CloudDownload
        SettingsTileId.APP -> Icons.Default.PhoneAndroid
        SettingsTileId.TRAFFIC -> Icons.AutoMirrored.Filled.Send
        SettingsTileId.LOGS -> Icons.Default.Description
        SettingsTileId.UPDATES -> Icons.Default.SystemUpdate
        SettingsTileId.COMMUNITY -> Icons.Default.Forum
    }
    val motif = when (id) {
        SettingsTileId.APPEARANCE -> Motif.Flower
        SettingsTileId.ROUTING, SettingsTileId.TRAFFIC -> Motif.Fan
        SettingsTileId.DNS, SettingsTileId.UPDATES -> Motif.Clover
        SettingsTileId.PING, SettingsTileId.LOGS -> Motif.Sun
        SettingsTileId.SUBSCRIPTIONS -> Motif.Arch
        SettingsTileId.APP -> Motif.Pebble
        SettingsTileId.COMMUNITY -> Motif.Flower
    }
    ExpressiveNavigationTile(settingsTileTitle(id, s), description, icon, (id.ordinal + 1).toString().padStart(2, '0'),
        container, ink, if (id == SettingsTileId.APPEARANCE) art.lime else ink, motif, onClick, modifier,
        prominent = id == SettingsTileId.APPEARANCE && tile.size == SettingsTileSize.TALL, interactive = interactive)
}
