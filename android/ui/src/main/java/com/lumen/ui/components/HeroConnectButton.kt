package com.lumen.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lumen.ui.theme.ConnectionConnecting
import com.lumen.ui.theme.ConnectionDanger
import com.lumen.ui.theme.ConnectionDisconnected
import com.lumen.ui.theme.ConnectionSuccess

enum class ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error
}

@Composable
fun HeroConnectButton(
    state: ConnectionState,
    onConnectClick: () -> Unit,
    modifier: Modifier = Modifier,
    buttonSize: Dp = 204.dp,
    statusText: String? = null
) {
    val cardBg = MaterialTheme.colorScheme.surface
    val surfaceVariantBg = MaterialTheme.colorScheme.surfaceVariant
    val onSurfaceText = MaterialTheme.colorScheme.onSurface
    val onSurfaceVariantText = MaterialTheme.colorScheme.onSurfaceVariant
    val primaryColor = MaterialTheme.colorScheme.primary
    val secondaryColor = MaterialTheme.colorScheme.secondary

    val interactionSource = remember { MutableInteractionSource() }
    val isPressed by interactionSource.collectIsPressedAsState()

    val scalePressed by animateFloatAsState(
        targetValue = if (isPressed) 0.93f else 1.0f,
        animationSpec = tween(durationMillis = 150),
        label = "hero_button_press_scale"
    )

    // Only animate continuous pulse/rotation while connecting to keep CPU/GPU idle when connected
    val shouldAnimate = state == ConnectionState.Connecting
    val pulseScale: Float
    val pulseAlpha: Float
    val rotationDegrees: Float
    if (shouldAnimate) {
        val infiniteTransition = rememberInfiniteTransition(label = "hero_button_motion")
        pulseScale = infiniteTransition.animateFloat(
            initialValue = 1.0f,
            targetValue = 1.22f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 1800, easing = FastOutSlowInEasing),
                repeatMode = RepeatMode.Reverse
            ),
            label = "pulse_scale"
        ).value
        pulseAlpha = infiniteTransition.animateFloat(
            initialValue = 0.28f,
            targetValue = 0f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 1800, easing = FastOutSlowInEasing),
                repeatMode = RepeatMode.Reverse
            ),
            label = "pulse_alpha"
        ).value
        rotationDegrees = infiniteTransition.animateFloat(
            initialValue = 0f,
            targetValue = 360f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 1200, easing = LinearEasing),
                repeatMode = RepeatMode.Restart
            ),
            label = "connecting_rotation"
        ).value
    } else {
        pulseScale = 1f
        pulseAlpha = 0f
        rotationDegrees = 0f
    }

    val targetColor = when (state) {
        ConnectionState.Disconnected -> ConnectionDisconnected
        ConnectionState.Connecting -> ConnectionConnecting
        ConnectionState.Connected -> ConnectionSuccess
        ConnectionState.Error -> ConnectionDanger
    }

    val animatedColor by animateColorAsState(
        targetValue = targetColor,
        animationSpec = tween(durationMillis = 400),
        label = "button_accent_color"
    )

    val strings = com.lumen.ui.screens.LocalStrings.current

    val resolvedStatusText = statusText ?: when (state) {
        ConnectionState.Disconnected -> strings.tapToConnect
        ConnectionState.Connecting -> strings.connectingStatus
        ConnectionState.Connected -> strings.connectedStatus
        ConnectionState.Error -> strings.connectionError
    }

    val centerText = when (state) {
        ConnectionState.Disconnected -> strings.centerConnect
        ConnectionState.Connecting -> strings.centerConnecting
        ConnectionState.Connected -> strings.centerConnected
        ConnectionState.Error -> strings.centerError
    }

    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = modifier
    ) {
        Box(
            contentAlignment = Alignment.Center,
            modifier = Modifier.size(buttonSize * 1.3f)
        ) {
            // Pulse outer glow canvas (MINIMAL GLOW)
            Canvas(modifier = Modifier.matchParentSize()) {
                val center = Offset(size.width / 2f, size.height / 2f)
                val baseRadius = (buttonSize.toPx() / 2f)

                if (state == ConnectionState.Connecting || state == ConnectionState.Connected || state == ConnectionState.Error) {
                    val minimalGlowAlpha = if (shouldAnimate) pulseAlpha.coerceAtMost(0.08f) else 0.04f
                    drawCircle(
                        color = animatedColor.copy(alpha = minimalGlowAlpha),
                        radius = baseRadius * (if (shouldAnimate) pulseScale else 1.04f),
                        center = center
                    )
                }
            }

            // Main interactive circle button with center text
            Box(
                contentAlignment = Alignment.Center,
                modifier = Modifier
                    .size(buttonSize)
                    .graphicsLayer {
                        scaleX = scalePressed
                        scaleY = scalePressed
                    }
                    .clickable(
                        interactionSource = interactionSource,
                        indication = null,
                        onClick = onConnectClick
                    )
            ) {
                Canvas(modifier = Modifier.matchParentSize()) {
                    val center = Offset(size.width / 2f, size.height / 2f)
                    val radius = size.width / 2f

                    // Outer container background
                    val bgBrush = when (state) {
                        ConnectionState.Connected -> Brush.radialGradient(
                            colors = listOf(primaryColor.copy(alpha = 0.18f), cardBg.copy(alpha = 0.88f)),
                            center = center,
                            radius = radius * 1.2f
                        )
                        ConnectionState.Connecting -> Brush.radialGradient(
                            colors = listOf(ConnectionConnecting.copy(alpha = 0.16f), cardBg.copy(alpha = 0.88f)),
                            center = center,
                            radius = radius
                        )
                        ConnectionState.Error -> Brush.radialGradient(
                            colors = listOf(ConnectionDanger.copy(alpha = 0.16f), cardBg.copy(alpha = 0.88f)),
                            center = center,
                            radius = radius
                        )
                        ConnectionState.Disconnected -> Brush.radialGradient(
                            colors = listOf(surfaceVariantBg.copy(alpha = 0.5f), cardBg.copy(alpha = 0.85f)),
                            center = center,
                            radius = radius
                        )
                    }

                    drawCircle(
                        brush = bgBrush,
                        radius = radius - 8.dp.toPx(),
                        center = center
                    )

                    // Liquid Glass outer stroke border
                    val strokeWidth = 2.5.dp.toPx()
                    val strokeRadius = radius - 8.dp.toPx() - (strokeWidth / 2f)

                    val glassBorder = Brush.verticalGradient(
                        colors = listOf(
                            animatedColor.copy(alpha = 0.90f),
                            secondaryColor.copy(alpha = 0.50f),
                            animatedColor.copy(alpha = 0.25f)
                        )
                    )

                    drawCircle(
                        brush = glassBorder,
                        radius = strokeRadius,
                        center = center,
                        style = Stroke(width = strokeWidth)
                    )

                    // Connecting state spinning dual arcs
                    if (state == ConnectionState.Connecting) {
                        val arcRadius = strokeRadius - 7.dp.toPx()
                        val arcStroke = 3.dp.toPx()

                        // Arc 1
                        drawArc(
                            brush = Brush.sweepGradient(
                                colors = listOf(primaryColor.copy(alpha = 0.1f), primaryColor, secondaryColor)
                            ),
                            startAngle = rotationDegrees,
                            sweepAngle = 110f,
                            useCenter = false,
                            topLeft = Offset(center.x - arcRadius, center.y - arcRadius),
                            size = Size(arcRadius * 2, arcRadius * 2),
                            style = Stroke(width = arcStroke, cap = StrokeCap.Round)
                        )

                        // Arc 2 (opposite side)
                        drawArc(
                            brush = Brush.sweepGradient(
                                colors = listOf(secondaryColor.copy(alpha = 0.1f), secondaryColor, primaryColor)
                            ),
                            startAngle = rotationDegrees + 180f,
                            sweepAngle = 110f,
                            useCenter = false,
                            topLeft = Offset(center.x - arcRadius, center.y - arcRadius),
                            size = Size(arcRadius * 2, arcRadius * 2),
                            style = Stroke(width = arcStroke, cap = StrokeCap.Round)
                        )
                    }
                }

                // Text inside the center of the circle in the color of its border
                Text(
                    text = centerText,
                    color = animatedColor,
                    fontSize = 18.sp,
                    fontWeight = FontWeight.Bold,
                    textAlign = androidx.compose.ui.text.style.TextAlign.Center
                )
            }
        }
    }
}
