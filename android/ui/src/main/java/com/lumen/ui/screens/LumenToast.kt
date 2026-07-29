package com.lumen.ui.screens

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
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
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay

/**
 * In-app toast queue. Replaces android.widget.Toast so notices are drawn by Lumen
 * itself: the system toast ignores the app theme, cannot be placed above the nav
 * pill, and is silently dropped when notifications are disabled.
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
            enter = slideInVertically(tween(260)) { it / 2 } + fadeIn(tween(220)),
            exit = slideOutVertically(tween(200)) { it / 2 } + fadeOut(tween(180))
        ) {
            // Shaped like a stock Android toast: a translucent wrap-content pill that
            // lets the screen show through, but painted with Lumen's own palette.
            val shape = RoundedCornerShape(percent = 50)
            Box(
                modifier = Modifier
                    .clip(shape)
                    .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.78f))
                    .border(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.28f), shape)
                    .padding(horizontal = 20.dp, vertical = 11.dp)
            ) {
                Text(
                    text = shown,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.92f),
                    textAlign = TextAlign.Center
                )
            }
        }
    }
}
