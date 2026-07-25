package com.lumen.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import com.lumen.ui.screens.LocalHapticsEnabled
import com.lumen.ui.screens.LocalStrings
import kotlin.math.roundToInt

@Composable
fun ConnectionSliderBar(
    connectionState: ConnectionState,
    onToggleConnection: () -> Unit,
    modifier: Modifier = Modifier
) {
    val s = LocalStrings.current
    val haptics = LocalHapticFeedback.current
    val hapticsEnabled = LocalHapticsEnabled.current
    // Slider gestures buzz on grab and on the toggle that follows the release.
    fun buzz(type: HapticFeedbackType) { if (hapticsEnabled) haptics.performHapticFeedback(type) }

    // Keep primary theme color for ALL states (no changing to green/yellow/red)
    val primaryColor = MaterialTheme.colorScheme.primary

    val animatedKnobBg by animateColorAsState(
        targetValue = primaryColor,
        animationSpec = tween(220),
        label = "slider_knob_bg"
    )

    val trackBgColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.75f)

    val labelText = when (connectionState) {
        ConnectionState.Disconnected -> s.slideToConnect
        ConnectionState.Connecting -> s.connectingStatus
        ConnectionState.Connected -> s.slideToDisconnect
        ConnectionState.Error -> s.connectionError
    }

    val shape = RoundedCornerShape(32.dp)
    val interactionSource = remember { MutableInteractionSource() }

    BoxWithConstraints(
        modifier = modifier
            .fillMaxWidth()
            .height(64.dp)
            .clip(shape)
            .background(trackBgColor)
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.35f), shape)
            .clickable(
                interactionSource = interactionSource,
                indication = null,
                onClick = onToggleConnection
            )
            .padding(4.dp)
    ) {
        val density = LocalDensity.current
        val knobSizePx = with(density) { 52.dp.toPx() }
        val maxOffsetPx = (constraints.maxWidth.toFloat() - knobSizePx).coerceAtLeast(0f)

        var isDragging by remember { mutableStateOf(false) }
        var dragPx by remember {
            mutableFloatStateOf(if (connectionState == ConnectionState.Connected) maxOffsetPx else 0f)
        }

        // Keep dragPx in sync smoothly when connectionState changes externally
        androidx.compose.runtime.LaunchedEffect(connectionState, maxOffsetPx) {
            if (!isDragging) {
                dragPx = if (connectionState == ConnectionState.Connected) maxOffsetPx else 0f
            }
        }

        val animatedReleaseOffsetPx by animateFloatAsState(
            targetValue = if (connectionState == ConnectionState.Connected) maxOffsetPx else 0f,
            animationSpec = tween(durationMillis = 220, easing = FastOutSlowInEasing),
            label = "slider_release_offset"
        )

        // When dragging: follow finger 1:1 instantly. When released: animate smoothly to target
        val effectiveOffsetPx = if (isDragging) dragPx else animatedReleaseOffsetPx

        val draggableState = rememberDraggableState { delta ->
            isDragging = true
            dragPx = (dragPx + delta).coerceIn(0f, maxOffsetPx)
        }

        // Active fill bar ends precisely at knob's right edge
        val fillWidthDp = if (effectiveOffsetPx <= 0.5f) {
            0.dp
        } else {
            with(density) { (effectiveOffsetPx + knobSizePx).toDp() }
        }

        if (fillWidthDp > 0.dp) {
            Box(
                modifier = Modifier
                    .fillMaxHeight()
                    .width(fillWidthDp)
                    .clip(RoundedCornerShape(24.dp))
                    .background(animatedKnobBg)
            )
        }

        // Center Label Text
        Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = Alignment.Center
        ) {
            Text(
                text = labelText,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
                color = if (effectiveOffsetPx > maxOffsetPx * 0.5f) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface,
                fontSize = 17.sp,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(horizontal = 44.dp)
            )
        }

        // Draggable Knob / Thumb
        Box(
            modifier = Modifier
                .offset { IntOffset(effectiveOffsetPx.roundToInt(), 0) }
                .size(52.dp)
                .clip(CircleShape)
                .background(animatedKnobBg)
                .border(1.dp, Color.White.copy(alpha = 0.25f), CircleShape)
                .draggable(
                    state = draggableState,
                    orientation = Orientation.Horizontal,
                    onDragStarted = {
                        isDragging = true
                        dragPx = effectiveOffsetPx
                        buzz(HapticFeedbackType.TextHandleMove)
                    },
                    onDragStopped = {
                        isDragging = false
                        if (dragPx > maxOffsetPx * 0.4f && connectionState == ConnectionState.Disconnected) {
                            buzz(HapticFeedbackType.LongPress)
                            onToggleConnection()
                        } else if (dragPx < maxOffsetPx * 0.6f && connectionState == ConnectionState.Connected) {
                            buzz(HapticFeedbackType.LongPress)
                            onToggleConnection()
                        } else {
                            dragPx = if (connectionState == ConnectionState.Connected) maxOffsetPx else 0f
                        }
                    }
                ),
            contentAlignment = Alignment.Center
        ) {
            Icon(
                imageVector = when (connectionState) {
                    ConnectionState.Connected -> Icons.Filled.Check
                    ConnectionState.Connecting -> Icons.Filled.Refresh
                    ConnectionState.Error -> Icons.Filled.Refresh
                    ConnectionState.Disconnected -> Icons.AutoMirrored.Filled.KeyboardArrowRight
                },
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onPrimary,
                modifier = Modifier.size(26.dp)
            )
        }
    }
}
