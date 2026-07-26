package com.lumen.ui.screens

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lumen.ui.theme.ConnectionDanger
import com.lumen.ui.theme.ConnectionSuccess
import com.lumen.ui.theme.ConnectionWarning

/** How many lines are added every time the user asks for more history. */
private const val LOG_PAGE = 200

private const val LOGS_LEVEL_FILTER_PREF = "logs_level_filter"

/**
 * Log viewer. [entries] is the app's log store — persisted when the user enabled it,
 * otherwise the live session — and is rendered filtered by severity and in a bounded
 * window so a large history cannot be materialised into the list at once. [logs] is the
 * plain-text live tail used while no structured store is available.
 *
 * Long-press a tile to copy it.
 */
@Composable
fun LogsScreen(
    logs: List<String>,
    onClear: () -> Unit,
    onExport: () -> Unit,
    onBack: (() -> Unit)? = null,
    entries: List<LogEntryUi> = emptyList(),
    // Pull an older page from the store; null when everything already lives in [entries].
    onLoadMore: (() -> Unit)? = null,
    // Export exactly what the filter selected; falls back to [onExport] when absent.
    onExportText: ((String) -> Unit)? = null,
    modifier: Modifier = Modifier
) {
    val strings = LocalStrings.current
    val structured = entries.isNotEmpty()
    var levelFilter by rememberUiPreference(LOGS_LEVEL_FILTER_PREF, LOG_FILTER_ALL)
    val visible = remember(entries, levelFilter) {
        if (levelFilter == LOG_FILTER_ALL) {
            entries
        } else {
            val min = logLevelRank(levelFilter)
            entries.filter { logLevelRank(it.level) >= min }
        }
    }
    // Bounded window over the filtered history; grows on demand instead of composing
    // thousands of persisted lines at once. Reset whenever the filter changes.
    var windowSize by remember(levelFilter) { mutableIntStateOf(LOG_PAGE) }
    val window = remember(visible, windowSize) { visible.takeLast(windowSize) }
    val olderAvailable = window.size < visible.size || onLoadMore != null

    val isEmpty = if (structured) visible.isEmpty() else logs.isEmpty()

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
                    TextButton(
                        onClick = {
                            if (structured && onExportText != null) {
                                onExportText(visible.joinToString("\n") { it.formatted() })
                            } else {
                                onExport()
                            }
                        },
                        enabled = !isEmpty
                    ) { Text(strings.export) }
                    TextButton(onClick = onClear, enabled = !isEmpty) { Text(strings.clear) }
                }
            }
        )
        if (structured) {
            LogLevelFilterRow(
                selected = levelFilter,
                counts = remember(entries) { entries.groupingBy { it.level.lowercase() }.eachCount() },
                total = entries.size,
                onSelected = { levelFilter = it }
            )
            Spacer(Modifier.height(6.dp))
        }
        if (isEmpty) {
            Box(Modifier.weight(1f).fillMaxWidth()) {
                Text(
                    strings.noLogs,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.align(Alignment.Center)
                )
            }
        } else if (structured) {
            StructuredLogList(
                window = window,
                olderAvailable = olderAvailable,
                onLoadOlder = {
                    windowSize += LOG_PAGE
                    // Only ask the store for another page once the window has caught up
                    // with everything it already handed over.
                    if (windowSize > visible.size) onLoadMore?.invoke()
                },
                modifier = Modifier.weight(1f)
            )
        } else {
            PlainLogList(logs = logs, modifier = Modifier.weight(1f))
        }
    }
}

@Composable
private fun StructuredLogList(
    window: List<LogEntryUi>,
    olderAvailable: Boolean,
    onLoadOlder: () -> Unit,
    modifier: Modifier = Modifier
) {
    val strings = LocalStrings.current
    val clipboard = LocalClipboardManager.current
    val listState = rememberLazyListState()
    // Follow the tail only while the user is already at the bottom, otherwise every new
    // line yanks the list back and fights manual scrolling.
    val isAtBottom by remember {
        derivedStateOf {
            val last = listState.layoutInfo.visibleItemsInfo.lastOrNull()
            last == null || last.index >= listState.layoutInfo.totalItemsCount - 2
        }
    }
    LaunchedEffect(window.size, isAtBottom) {
        // Index within the list, which carries the "load older" header before the tiles.
        val last = window.size - 1 + if (olderAvailable) 1 else 0
        if (last >= 0 && isAtBottom) listState.scrollToItem(last)
    }
    LazyColumn(
        state = listState,
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(6.dp)
    ) {
        if (olderAvailable) {
            item(key = "load_older") {
                TextButton(onClick = onLoadOlder, modifier = Modifier.fillMaxWidth()) {
                    Text(strings.loadOlderLogs)
                }
            }
        }
        // Positional keys: two identical lines can share a millisecond, and a duplicate
        // key aborts the list.
        itemsIndexed(window) { _, entry ->
            LogEntryTile(entry) { clipboard.setText(AnnotatedString(entry.formatted())) }
        }
    }
}

@Composable
private fun PlainLogList(logs: List<String>, modifier: Modifier = Modifier) {
    val clipboard = LocalClipboardManager.current
    val listState = rememberLazyListState()
    val isAtBottom by remember {
        derivedStateOf {
            val last = listState.layoutInfo.visibleItemsInfo.lastOrNull()
            last == null || last.index >= listState.layoutInfo.totalItemsCount - 2
        }
    }
    LaunchedEffect(logs.size, isAtBottom) {
        if (logs.isNotEmpty() && isAtBottom) {
            listState.scrollToItem(logs.size - 1)
        }
    }
    LazyColumn(
        state = listState,
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(6.dp)
    ) {
        itemsIndexed(logs, key = { index, _ -> index }) { _, line ->
            LogTile(line) { clipboard.setText(AnnotatedString(line)) }
        }
    }
}

/** Severity chips; the count next to each one is how much history it selects. */
@Composable
private fun LogLevelFilterRow(
    selected: String,
    counts: Map<String, Int>,
    total: Int,
    onSelected: (String) -> Unit
) {
    val strings = LocalStrings.current
    Row(
        modifier = Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(6.dp)
    ) {
        LumenFilterChip(
            label = strings.logLevelAll,
            selected = selected == LOG_FILTER_ALL,
            badge = total.toString(),
            onClick = { onSelected(LOG_FILTER_ALL) }
        )
        LOG_STORE_LEVELS.forEach { level ->
            val min = logLevelRank(level)
            val matching = counts.entries.sumOf { (name, count) ->
                if (logLevelRank(name) >= min) count else 0
            }
            LumenFilterChip(
                label = logStoreLevelLabel(level, strings),
                selected = selected == level,
                badge = matching.toString(),
                onClick = { onSelected(level) }
            )
        }
    }
}

/** Translated name of a log store severity. */
internal fun logStoreLevelLabel(level: String, s: LumenStrings): String = when (level) {
    "info" -> s.logLevelInfo
    "warning" -> s.logLevelWarning
    "error" -> s.logLevelError
    else -> s.logLevelDebug
}

private fun logLevelColor(level: String): Color = when (level.lowercase()) {
    "error" -> ConnectionDanger
    "warning" -> ConnectionWarning
    "info" -> ConnectionSuccess
    else -> Color(0xFF9AA6B8)
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun LogEntryTile(entry: LogEntryUi, onCopy: () -> Unit) {
    val shape = RoundedCornerShape(10.dp)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(MaterialTheme.colorScheme.surface)
            .border(1.dp, MaterialTheme.colorScheme.outline.copy(alpha = 0.35f), shape)
            .combinedClickable(onClick = {}, onLongClick = onCopy)
            .padding(horizontal = 10.dp, vertical = 8.dp)
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                entry.level.uppercase(),
                fontFamily = FontFamily.Monospace,
                fontSize = 10.sp,
                fontWeight = FontWeight.Bold,
                color = logLevelColor(entry.level)
            )
            Spacer(Modifier.width(8.dp))
            Text(
                entry.time,
                fontFamily = FontFamily.Monospace,
                fontSize = 10.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            if (entry.component.isNotBlank()) {
                Spacer(Modifier.width(8.dp))
                Text(
                    entry.component,
                    fontFamily = FontFamily.Monospace,
                    fontSize = 10.sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
        }
        Text(
            entry.message,
            fontFamily = FontFamily.Monospace,
            fontSize = 11.sp,
            lineHeight = 15.sp,
            color = MaterialTheme.colorScheme.onSurface
        )
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
