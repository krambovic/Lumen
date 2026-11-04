package com.lumen.ui.components

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.PowerSettingsNew
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lumen.ui.theme.LocalLumenColors
import com.lumen.ui.screens.LocalStrings
import kotlinx.coroutines.launch
import kotlin.math.PI
import kotlin.math.roundToInt
import kotlin.math.sin

/**
 * Connection slider inspired by Material's wavy progress track.
 * A static idle wave becomes an active track while dragging; tapping is an equivalent action.
 */
@Composable
fun ConnectionSliderBar(
    connectionState: ConnectionState,
    onToggleConnection: () -> Unit,
    modifier: Modifier = Modifier
) {
    val lumenColors = LocalLumenColors.current
    val strings = LocalStrings.current
    val tick = com.lumen.ui.screens.rememberHapticTick()

    val isConnected = connectionState == ConnectionState.Connected
    val isConnecting = connectionState == ConnectionState.Connecting
    val atRight = sliderAtRight(connectionState)
    val actionLabel = if (isConnecting) strings.cancel else if (isConnected) strings.centerDisconnect else strings.centerConnect

    val art = com.lumen.ui.theme.expressivePalette()
    val activeColor = art.onLime
    val trackInactiveColor = art.onLime.copy(alpha = 0.20f)
    val cardBg = art.lime
    val shape = RoundedCornerShape(24.dp)

    val interactionSource = remember { MutableInteractionSource() }

    Surface(
        modifier = modifier
            .expressiveButtonSemantics(actionLabel, onToggleConnection)
            .fillMaxWidth()
            .height(76.dp)
            .clip(shape)
            .clickable(
                interactionSource = interactionSource,
                role = androidx.compose.ui.semantics.Role.Button,
                onClickLabel = actionLabel,
                indication = null,
                onClick = {
                    tick(HapticFeedbackType.LongPress)
                    onToggleConnection()
                }
            ),
        shape = shape,
        color = cardBg
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 16.dp, vertical = 6.dp),
            verticalArrangement = Arrangement.SpaceBetween
        ) {
            // Top Status & Instruction Row
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                // Status dot + label
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(
                        modifier = Modifier
                            .size(7.dp)
                            .clip(CircleShape)
                            .background(if (isConnected) lumenColors.hero else if (isConnecting) lumenColors.accent else lumenColors.muted.copy(alpha = 0.5f))
                    )
                    Spacer(modifier = Modifier.size(6.dp))
                    Text(
                        text = when {
                            isConnected -> strings.centerConnected
                            isConnecting -> strings.centerConnecting
                            else -> strings.off
                        },
                        fontSize = 14.sp,
                        fontWeight = FontWeight.Bold,
                        color = art.onLime
                    )
                }

                Text(
                    text = when {
                        isConnected -> strings.slideToDisconnect
                        isConnecting -> strings.centerConnecting
                        else -> strings.slideToConnect
                    },
                    fontSize = 11.sp,
                    fontWeight = FontWeight.Medium,
                    color = art.onLime.copy(alpha = 0.75f)
                )
            }

            // Bottom Track Row: Wavy Canvas + Draggable Thumb
            BoxWithConstraints(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(48.dp),
                contentAlignment = Alignment.CenterStart
            ) {
                val density = LocalDensity.current
                val thumbSizePx = with(density) { 38.dp.toPx() }
                val strokeWidthPx = with(density) { 4.5.dp.toPx() }
                val amplitudePx = with(density) { 4.5.dp.toPx() }
                val wavelengthPx = with(density) { 32.dp.toPx() }
                val gapPx = with(density) { 6.dp.toPx() }
                val dotRadiusPx = with(density) { 3.dp.toPx() }

                val maxOffsetPx = (constraints.maxWidth.toFloat() - thumbSizePx).coerceAtLeast(0f)

                var isDragging by remember { mutableStateOf(false) }
                var dragPx by remember {
                    mutableFloatStateOf(if (atRight) maxOffsetPx else 0f)
                }
                val offsetAnimation = remember {
                    Animatable(if (atRight) maxOffsetPx else 0f)
                }
                val scope = rememberCoroutineScope()

                val settledOffsetPx = if (atRight) maxOffsetPx else 0f
                LaunchedEffect(atRight, maxOffsetPx) {
                    if (!isDragging) {
                        dragPx = settledOffsetPx
                        offsetAnimation.animateTo(
                            targetValue = settledOffsetPx,
                            animationSpec = spring(dampingRatio = .9f, stiffness = 420f)
                        )
                    }
                }

                val effectiveOffsetPx = { if (isDragging) dragPx else offsetAnimation.value }

                val draggableState = rememberDraggableState { delta ->
                    isDragging = true
                    dragPx = (dragPx + delta).coerceIn(0f, maxOffsetPx)
                }

                // One continuous phase: connecting settles into a calmer connected wave
                // without replacing the transition or snapping its phase back to zero.
                val waveSpeed = animateFloatAsState(
                    if (isConnecting) 327f else if (isConnected) 105f else 0f,
                    spring(dampingRatio = 1f, stiffness = 90f), label = "wave_speed"
                )
                val waveAmplitude = animateFloatAsState(
                    if (isConnected) .55f else 1f,
                    spring(dampingRatio = .9f, stiffness = 120f), label = "wave_amplitude"
                )
                val travelingAngle = rememberForegroundAngle(enabled = atRight || isDragging) { waveSpeed.value }

                val wavePath = remember { Path() }
                val idlePath = remember { Path() }
                val waveStroke = remember(strokeWidthPx) {
                    Stroke(width = strokeWidthPx, cap = StrokeCap.Round, join = StrokeJoin.Round)
                }

                // Canvas drawing the exact M3 Expressive Wavy Progress Track
                Canvas(modifier = Modifier.fillMaxSize()) {
                    val offset = effectiveOffsetPx()
                    val progress = if (maxOffsetPx > 0f) (offset / maxOffsetPx).coerceIn(0f, 1f) else 0f
                    val centerY = size.height / 2f
                    val totalWidth = size.width

                    // 1. Stop indicator dot at far right
                    val stopX = totalWidth - dotRadiusPx
                    if (progress < .96f) drawCircle(
                        color = activeColor,
                        radius = dotRadiusPx,
                        center = Offset(stopX, centerY)
                    )

                    val trackRightLimit = stopX - dotRadiusPx - gapPx

                    val phase = travelingAngle.value * (PI / 180f).toFloat()
                    val waveStartX = thumbSizePx / 2f
                    val waveEndX = (offset - gapPx).coerceAtLeast(waveStartX)
                    if (progress > .02f && waveEndX > waveStartX) {
                        wavePath.rewind()
                        var x = waveStartX
                        var first = true
                        while (x <= waveEndX) {
                            val y = centerY + amplitudePx * waveAmplitude.value *
                                sin((2 * PI * (x - waveStartX) / wavelengthPx).toFloat() - phase)
                            if (first) { wavePath.moveTo(x, y); first = false }
                            else wavePath.lineTo(x, y)
                            x += 2f
                        }
                        drawPath(wavePath, activeColor, style = waveStroke)
                    }

                    // The unused track remains a wave, with no straight rail underneath.
                    val inactiveStartX = (offset + thumbSizePx + gapPx).coerceAtMost(trackRightLimit)
                    if (inactiveStartX < trackRightLimit) {
                        idlePath.rewind()
                        var x = inactiveStartX
                        while (x <= trackRightLimit) {
                            val y = centerY + amplitudePx * .7f *
                                sin((2 * PI * (x - inactiveStartX) / wavelengthPx).toFloat())
                            if (x == inactiveStartX) idlePath.moveTo(x, y) else idlePath.lineTo(x, y)
                            x += 2f
                        }
                        drawPath(idlePath, trackInactiveColor, style = waveStroke)
                    }
                }

                // Draggable Thumb Knob (M3 Expressive rounded knob with power icon)
                Box(
                    modifier = Modifier
                        .offset {
                            IntOffset(
                                effectiveOffsetPx().roundToInt(),
                                0 // BoxWithConstraints already centers the thumb vertically.
                            )
                        }
                        .size(with(density) { thumbSizePx.toDp() })
                        .clip(expressiveShape(Motif.Clover))
                        .background(activeColor)
                        .clickable(
                            interactionSource = remember { MutableInteractionSource() },
                            indication = null,
                            onClick = {
                                tick(HapticFeedbackType.LongPress)
                                onToggleConnection()
                            }
                        )
                        .draggable(
                            state = draggableState,
                            orientation = Orientation.Horizontal,
                            onDragStarted = {
                                dragPx = effectiveOffsetPx()
                                isDragging = true
                                tick(HapticFeedbackType.TextHandleMove)
                            },
                            onDragStopped = {
                                val releasedOffset = dragPx
                                val finalProgress = if (maxOffsetPx > 0f) releasedOffset / maxOffsetPx else 0f
                                scope.launch {
                                    offsetAnimation.snapTo(releasedOffset)
                                    isDragging = false
                                    if (sliderDragShouldToggle(connectionState, finalProgress)) {
                                        tick(HapticFeedbackType.LongPress)
                                        onToggleConnection()
                                    } else {
                                        offsetAnimation.animateTo(settledOffsetPx,
                                            spring(dampingRatio = 0.75f, stiffness = 500f))
                                    }
                                }
                            }
                        ),
                    contentAlignment = Alignment.Center
                ) {
                    Icon(
                        imageVector = Icons.Default.PowerSettingsNew,
                        contentDescription = actionLabel,
                        tint = art.lime,
                        modifier = Modifier.size(19.dp)
                    )
                }
            }
        }
    }
}

internal fun sliderAtRight(state: ConnectionState): Boolean =
    state == ConnectionState.Connected || state == ConnectionState.Connecting

internal fun sliderDragShouldToggle(state: ConnectionState, progress: Float): Boolean =
    if (sliderAtRight(state)) progress < .35f else progress > .65f
