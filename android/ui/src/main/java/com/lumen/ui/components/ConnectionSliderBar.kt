package com.lumen.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import com.lumen.ui.screens.LocalStrings
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

@Composable
fun ConnectionSliderBar(
    connectionState: ConnectionState,
    onToggleConnection: () -> Unit,
    modifier: Modifier = Modifier
) {
    val s = LocalStrings.current
    // Slider gestures buzz on grab and on the toggle that follows the release.
    // Routed through the shared tick so the vibrator fires even when the system
    // touch-feedback switch is off.
    val tick = com.lumen.ui.screens.rememberHapticTick()
    fun buzz(type: HapticFeedbackType) { tick(type) }

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
    ) {
        val density = LocalDensity.current
        val insetPx = with(density) { 4.dp.toPx() }
        val knobSizePx = with(density) { 56.dp.toPx() }
        // Travel is calculated from the real outer track width. Previously the
        // parent padding was omitted from this calculation, so the thumb escaped
        // the track or stopped short depending on screen width.
        val maxOffsetPx = (
            constraints.maxWidth.toFloat() - knobSizePx - insetPx * 2f
        ).coerceAtLeast(0f)

        var isDragging by remember { mutableStateOf(false) }
        var dragPx by remember {
            mutableFloatStateOf(if (connectionState == ConnectionState.Connected) maxOffsetPx else 0f)
        }
        val offsetAnimation = remember {
            Animatable(if (connectionState == ConnectionState.Connected) maxOffsetPx else 0f)
        }
        val sliderScope = rememberCoroutineScope()

        // Keep the visual target in sync with settled external state. Connecting
        // is intentionally ignored here: a successful swipe has already chosen
        // the destination, while the service is still transitioning to it.
        val settledStateOffsetPx = when (connectionState) {
            ConnectionState.Connected -> maxOffsetPx
            ConnectionState.Disconnected,
            ConnectionState.Error -> 0f
            ConnectionState.Connecting -> null
        }
        androidx.compose.runtime.LaunchedEffect(connectionState, maxOffsetPx) {
            if (!isDragging) {
                settledStateOffsetPx?.let { offsetPx ->
                    dragPx = offsetPx
                    offsetAnimation.animateTo(
                        targetValue = offsetPx,
                        animationSpec = tween(durationMillis = 220, easing = FastOutSlowInEasing)
                    )
                }
            }
        }

        // While dragging, follow the finger exactly. Once released, the
        // Animatable starts from the actual release point rather than from the
        // old connection-state position.
        val effectiveOffsetPx = if (isDragging) dragPx else offsetAnimation.value

        val draggableState = rememberDraggableState { delta ->
            isDragging = true
            dragPx = (dragPx + delta).coerceIn(0f, maxOffsetPx)
        }

        val fillBoundsPx = sliderFillBounds(
            trackWidthPx = constraints.maxWidth.toFloat(),
            thumbOffsetPx = effectiveOffsetPx,
            thumbSizePx = knobSizePx,
            insetPx = insetPx
        )
        if (fillBoundsPx != null) {
            val fillWidthDp = with(density) {
                (fillBoundsPx.second - fillBoundsPx.first).toDp()
            }
            Box(
                modifier = Modifier
                    .height(56.dp)
                    .width(fillWidthDp)
                    .align(Alignment.CenterStart)
                    .offset { IntOffset(fillBoundsPx.first.roundToInt(), 0) }
                    .clip(RoundedCornerShape(28.dp))
                    .background(animatedKnobBg)
            )
        }
        val labelSafePadding = with(density) { (insetPx + knobSizePx + 8.dp.toPx()).toDp() }
        Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = Alignment.Center
        ) {
            Text(
                text = labelText,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
                color = if (effectiveOffsetPx >= maxOffsetPx * 0.5f) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface,
                fontSize = 17.sp,
                textAlign = TextAlign.Center,
                maxLines = 1,
                softWrap = false,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = labelSafePadding)
            )
        }

        // Draggable Knob / Thumb
        Box(
            modifier = Modifier
                .offset {
                    IntOffset(
                        (insetPx + effectiveOffsetPx).roundToInt(),
                        insetPx.roundToInt()
                    )
                }
                .size(56.dp)
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
                        val release = sliderReleaseTarget(
                            connectionState = connectionState,
                            dragOffsetPx = dragPx,
                            maxOffsetPx = maxOffsetPx
                        )
                        val releaseOffsetPx = dragPx
                        // Keep drag mode active until the animation has captured
                        // the release point. This prevents a one-frame jump to
                        // the previous state while the service changes state.
                        sliderScope.launch {
                            offsetAnimation.snapTo(releaseOffsetPx)
                            isDragging = false
                            offsetAnimation.animateTo(
                                targetValue = release.targetOffsetPx,
                                animationSpec = tween(
                                    durationMillis = 220,
                                    easing = FastOutSlowInEasing
                                )
                            )
                        }
                        if (release.togglesConnection) {
                            buzz(HapticFeedbackType.LongPress)
                            onToggleConnection()
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

/**
 * Returns the inner fill bounds in pixels. The fill shares the same 4.dp inset
 * as the thumb, so the outer track border remains visible on every side.
 */
internal fun sliderFillBounds(
    trackWidthPx: Float,
    thumbOffsetPx: Float,
    thumbSizePx: Float,
    insetPx: Float
): Pair<Float, Float>? {
    if (thumbOffsetPx <= 0.5f) return null

    val startPx = insetPx
    val endPx = (startPx + thumbOffsetPx + thumbSizePx)
        .coerceAtMost(trackWidthPx - insetPx)
    return startPx to endPx
}

internal data class SliderReleaseTarget(
    val targetOffsetPx: Float,
    val togglesConnection: Boolean
)

internal fun sliderReleaseTarget(
    connectionState: ConnectionState,
    dragOffsetPx: Float,
    maxOffsetPx: Float
): SliderReleaseTarget = when (connectionState) {
    ConnectionState.Disconnected -> {
        if (dragOffsetPx > maxOffsetPx * 0.4f) {
            SliderReleaseTarget(maxOffsetPx, togglesConnection = true)
        } else {
            SliderReleaseTarget(0f, togglesConnection = false)
        }
    }
    ConnectionState.Connected -> {
        if (dragOffsetPx < maxOffsetPx * 0.6f) {
            SliderReleaseTarget(0f, togglesConnection = true)
        } else {
            SliderReleaseTarget(maxOffsetPx, togglesConnection = false)
        }
    }
    ConnectionState.Connecting,
    ConnectionState.Error -> SliderReleaseTarget(0f, togglesConnection = false)
}
