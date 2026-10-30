package com.lumen.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.*
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.PowerSettingsNew
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.lumen.ui.screens.LocalStrings
import com.lumen.ui.screens.rememberHapticTick
import com.lumen.ui.theme.expressivePalette

/** The launcher mark, fitted to its real glyph bounds rather than the launcher safe zone. */
@Composable
fun LumenMark(color: Color, modifier: Modifier = Modifier) {
    Box(modifier.drawWithCache {
        val width = minOf(size.width, size.height * .827f)
        val height = width / .827f
        val left = (size.width - width) / 2f
        val top = (size.height - height) / 2f
        val path = Path().apply {
            moveTo(left + width * .21668f, top)
            lineTo(left + width * .21668f, top + height * .81219f)
            lineTo(left + width, top + height * .81219f)
            lineTo(left + width * .81683f, top + height)
            lineTo(left, top + height)
            lineTo(left, top + height * .18411f)
            close()
        }
        onDrawBehind { drawPath(path, color) }
    })
}

@Composable
fun DockConnectionButton(state: ConnectionState, onToggle: () -> Unit, modifier: Modifier = Modifier) {
    val s = LocalStrings.current
    val art = expressivePalette()
    val connected = state == ConnectionState.Connected
    val connecting = state == ConnectionState.Connecting
    val label = if (connecting) s.cancel else if (connected) s.centerDisconnect else s.centerConnect
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scale = animateFloatAsState(if (pressed) .88f else 1f,
        spring(dampingRatio = .6f, stiffness = 550f), label = "dock_connect_press")
    val color = animateColorAsState(if (connected) art.lime else art.stage,
        spring(stiffness = 450f), label = "dock_connect_color")
    val ink = animateColorAsState(if (connected) art.onLime else art.onStage,
        spring(stiffness = 450f), label = "dock_connect_ink")
    val speed = animateFloatAsState(if (connecting) 180f else if (connected) 18f else 0f,
        tween(900, easing = FastOutSlowInEasing), label = "dock_connect_speed")
    val angle = remember { mutableFloatStateOf(0f) }
    val spinning by remember(state) { derivedStateOf { connected || connecting || speed.value > .1f } }
    LaunchedEffect(spinning) {
        if (spinning) {
            var previous = withFrameNanos { it }
            while (true) {
                val now = withFrameNanos { it }
                angle.floatValue = (angle.floatValue + speed.value * ((now - previous) / 1_000_000_000f).coerceAtMost(.05f)) % 360f
                previous = now
            }
        }
    }
    val morph = animateFloatAsState(if (pressed) 1f else if (connecting) .65f else if (connected) .2f else 0f,
        spring(dampingRatio = .72f, stiffness = 260f), label = "dock_connect_shape")
    val tick = rememberHapticTick()
    Box(modifier.size(48.dp).testTag("dock_connection")
        .expressiveButtonSemantics(label, onToggle)
        .graphicsLayer { scaleX = scale.value; scaleY = scale.value }
        .clip(CircleShape).clickable(interactionSource = interaction, indication = androidx.compose.foundation.LocalIndication.current,
            role = Role.Button, onClick = { tick(HapticFeedbackType.LongPress); onToggle() }), contentAlignment = Alignment.Center) {
        MorphingExpressiveMotif(Motif.Sun, color.value, { morph.value },
            Modifier.matchParentSize().graphicsLayer { rotationZ = angle.floatValue })
        Icon(Icons.Default.PowerSettingsNew, null, Modifier.size(23.dp), tint = ink.value)
        if (connecting) CircularProgressIndicator(Modifier.size(38.dp), color = ink.value, strokeWidth = 2.dp)
    }
}
