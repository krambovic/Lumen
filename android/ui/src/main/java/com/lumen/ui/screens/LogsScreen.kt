package com.lumen.ui.screens

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Live log viewer rendered as separate tiles; long-press a tile to copy it.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun LogsScreen(
    logs: List<String>,
    onClear: () -> Unit,
    onExport: () -> Unit,
    onBack: (() -> Unit)? = null,
    modifier: Modifier = Modifier
) {
    val strings = LocalStrings.current
    val clipboard = LocalClipboardManager.current
    val listState = rememberLazyListState()
    LaunchedEffect(logs.size) {
        if (logs.isNotEmpty()) {
            listState.scrollToItem(logs.size - 1)
        }
    }

    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp)
    ) {
        LumenScreenHeader(
            title = strings.logs,
            onBack = onBack,
            actions = {
                Row {
                    TextButton(onClick = onExport, enabled = logs.isNotEmpty()) { Text(strings.export) }
                    TextButton(onClick = onClear, enabled = logs.isNotEmpty()) { Text(strings.clear) }
                }
            }
        )
        if (logs.isEmpty()) {
            Box(Modifier.weight(1f).fillMaxWidth()) {
                Text(
                    strings.noLogs,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.align(Alignment.Center)
                )
            }
        } else {
            LazyColumn(
                state = listState,
                modifier = Modifier.weight(1f).fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(6.dp)
            ) {
                itemsIndexed(logs, key = { index, _ -> index }) { _, line ->
                    LogTile(line) {
                        clipboard.setText(AnnotatedString(line))
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun LogTile(line: String, onCopy: () -> Unit) {
    val shape = RoundedCornerShape(10.dp)
    Text(
        line,
        fontFamily = FontFamily.Monospace,
        fontSize = 11.sp,
        lineHeight = 15.sp,
        color = MaterialTheme.colorScheme.onSurface,
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(MaterialTheme.colorScheme.surface)
            .border(1.dp, MaterialTheme.colorScheme.outline.copy(alpha = 0.35f), shape)
            .combinedClickable(onClick = {}, onLongClick = onCopy)
            .padding(horizontal = 10.dp, vertical = 8.dp)
    )
}