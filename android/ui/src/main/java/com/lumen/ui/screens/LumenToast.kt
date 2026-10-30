package com.lumen.ui.screens

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Info
import androidx.compose.material3.Icon
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay

private val notices = Channel<String>(capacity = 16, onBufferOverflow = kotlinx.coroutines.channels.BufferOverflow.DROP_OLDEST)

/** Non-composable entry point for UI notices, including activity intent errors. */
fun showLumenNotice(text: String) { if (text.isNotBlank()) notices.trySend(text) }

/**
 * In-app toast queue. Replaces android.widget.Toast so notices are drawn by Lumen
 * itself, using the app theme and a position above its bottom controls.
 */
@Stable
class LumenToastState {
    internal var message by mutableStateOf<String?>(null)
        private set

    private val queue = Channel<String>(capacity = Channel.BUFFERED)

    /** Queues a notice; messages never overlap, they are shown one after another. */
    suspend fun show(text: String) {
        if (text.isNotBlank()) queue.send(text.trim())
    }

    internal suspend fun consume() {
        for (text in queue) {
            message = text
            // Long summaries (subscription results) need more reading time.
            delay(if (text.length > 60) 4200L else 2600L)
            message = null
            delay(240L)
        }
    }
}

@Composable
fun rememberLumenToastState(): LumenToastState {
    val state = remember { LumenToastState() }
    LaunchedEffect(state) { state.consume() }
    LaunchedEffect(state) { for (message in notices) state.show(message) }
    return state
}

/**
 * Draws the queued notice as a themed card above the bottom navigation pill.
 * Purely decorative overlay: it never consumes touches.
 */
@Composable
fun LumenToastHost(state: LumenToastState, modifier: Modifier = Modifier) {
    val current = state.message
    // Keep the last text while the exit animation plays.
    var shown by remember { mutableStateOf("") }
    LaunchedEffect(current) { if (current != null) shown = current }
    Box(
        modifier = modifier.fillMaxWidth(),
        contentAlignment = Alignment.BottomCenter
    ) {
        AnimatedVisibility(
            visible = current != null,
            enter = slideInVertically(spring(dampingRatio = .82f, stiffness = 400f)) { it / 2 } + fadeIn(spring(stiffness = 600f)),
            exit = slideOutVertically(spring(dampingRatio = 1f, stiffness = 500f)) { it / 2 } + fadeOut(spring(stiffness = 800f))
        ) {
            val shape = RoundedCornerShape(24.dp, 24.dp, 24.dp, 8.dp)
            Row(
                modifier = Modifier
                    .widthIn(max = 480.dp)
                    .semantics { liveRegion = LiveRegionMode.Polite }
                    .clip(shape)
                    .background(MaterialTheme.colorScheme.inverseSurface)
                    .border(1.dp, MaterialTheme.colorScheme.inversePrimary.copy(alpha = 0.3f), shape)
                    .padding(horizontal = 16.dp, vertical = 14.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Icon(Icons.Default.Info, null, Modifier.size(22.dp), tint = MaterialTheme.colorScheme.inversePrimary)
                Text(
                    text = shown,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.inverseOnSurface,
                    textAlign = TextAlign.Start,
                    modifier = Modifier.padding(start = 12.dp)
                )
            }
        }
    }
}
