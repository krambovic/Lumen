package com.lumen.ui.screens

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lumen.ui.theme.LocalLumenColors
import com.lumen.ui.theme.LumenMono
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch

private const val LOGS_LEVEL_FILTER_PREF = "logs_level_filter"

/** All loaded records are searchable. Older disk pages load automatically at the top. */
@Composable
fun LogsScreen(
    logs: List<String>, onClear: () -> Unit, onExport: () -> Unit,
    onBack: (() -> Unit)? = null, entries: List<LogEntryUi> = emptyList(),
    onLoadMore: (() -> Unit)? = null, onExportText: ((String) -> Unit)? = null,
    modifier: Modifier = Modifier, persistPreferences: Boolean = true,
    isLoadingHistory: Boolean = false
) {
    val s = LocalStrings.current
    var level by if (persistPreferences) rememberUiPreference(LOGS_LEVEL_FILTER_PREF, LOG_FILTER_ALL)
        else remember { mutableStateOf(LOG_FILTER_ALL) }
    var query by remember { mutableStateOf("") }
    val structured = entries.isNotEmpty()
    val visible = remember(entries, level, query) { filterLogEntries(entries, level, query) }
    val plain = remember(logs, query) { logs.filter { query.isBlank() || it.contains(query, true) } }
    val list = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val atLatest by remember { derivedStateOf { !list.canScrollBackward } }
    val latest = visible.lastOrNull()
    LaunchedEffect(latest?.timestamp, latest?.message, visible.size) {
        if (atLatest) list.scrollToItem(0)
    }
    // This also searches older pages when a filter has no matches in the live tail.
    LaunchedEffect(entries.size, query, level, isLoadingHistory, onLoadMore != null) {
        if (!isLoadingHistory && onLoadMore != null && visible.size < 20) onLoadMore()
    }
    Column(modifier.fillMaxSize().statusBarsPadding().padding(horizontal = 16.dp)) {
        LumenScreenHeader(s.logs, onBack, applyStatusBarPadding = false, actions = {
            IconButton(onClick = {
                if (structured && onExportText != null && (level != LOG_FILTER_ALL || query.isNotBlank())) onExportText(visible.joinToString("\n") { it.formatted() })
                else onExport()
            }, enabled = entries.isNotEmpty() || logs.isNotEmpty()) { Icon(Icons.Default.Share, s.export) }
            IconButton(onClick = onClear, enabled = entries.isNotEmpty() || logs.isNotEmpty()) { Icon(Icons.Default.Delete, s.clear) }
        })
        ExpressiveTextField(query, { query = it }, Modifier.fillMaxWidth(), singleLine = true,
            placeholder = { Text(s.logSearch) }, leadingIcon = { Icon(Icons.Default.Search, null) })
        Spacer(Modifier.height(10.dp))
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            LumenFilterChip(s.logLevelAll, level == LOG_FILTER_ALL, { level = LOG_FILTER_ALL })
            LOG_STORE_LEVELS.forEach { value ->
                LumenFilterChip(logStoreLevelLabel(value, s), level == value, { level = value })
            }
        }
        Row(Modifier.fillMaxWidth().height(48.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("${if (structured) visible.size else plain.size} / ${if (structured) entries.size else logs.size}",
                style = MaterialTheme.typography.labelSmall, fontFamily = LumenMono,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.weight(1f))
            if (isLoadingHistory) {
                CircularProgressIndicator(Modifier.size(14.dp), strokeWidth = 2.dp)
                Text(s.logHistoryLoading, Modifier.padding(start = 6.dp), style = MaterialTheme.typography.labelSmall)
            } else if (!atLatest) TextButton(onClick = { scope.launch { list.scrollToItem(0) } }) { Text(s.logLatest) }
        }
        Box(Modifier.weight(1f).fillMaxWidth().clip(RoundedCornerShape(16.dp))
            .background(MaterialTheme.colorScheme.surfaceContainerLow)
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(16.dp))) {
            if ((if (structured) visible.isEmpty() else plain.isEmpty()) && !isLoadingHistory && onLoadMore == null) {
                Text(if (entries.isEmpty() && logs.isEmpty()) s.noLogs else s.nothingFound,
                    Modifier.align(Alignment.Center).padding(24.dp), style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
            } else if (structured || onLoadMore != null) {
                LogConsole(visible, list, onLoadMore, isLoadingHistory)
            } else {
                val clipboard = LocalClipboardManager.current
                LazyColumn(state = list, reverseLayout = true, contentPadding = PaddingValues(12.dp, 8.dp, 12.dp, 100.dp)) {
                    items(plain.size, key = { "plain:${plain.lastIndex - it}" }, contentType = { "plain" }) { index ->
                        val line = plain[plain.lastIndex - index]
                        PlainLogLine(line) { clipboard.setText(AnnotatedString(line)) }
                    }
                }
            }
        }
    }
}

@Composable
private fun LogConsole(entries: List<LogEntryUi>, state: LazyListState, onLoadMore: (() -> Unit)?, loading: Boolean) {
    val s = LocalStrings.current
    val clipboard = LocalClipboardManager.current
    val keys = remember(entries) { logIdentityKeys(entries) }
    val loadOlder by rememberUpdatedState(onLoadMore)
    LaunchedEffect(state, entries.size, loading, onLoadMore != null) {
        if (!loading && onLoadMore != null) {
            snapshotFlow { state.layoutInfo.visibleItemsInfo.maxOfOrNull { it.index } ?: -1 }
                .distinctUntilChanged().collect { index ->
                    if (index >= entries.size - 6 && index >= 0) loadOlder?.invoke()
                }
        }
    }
    LazyColumn(state = state, reverseLayout = true, contentPadding = PaddingValues(start = 8.dp, end = 8.dp, top = 8.dp, bottom = 100.dp)) {
        items(entries.size, key = { keys[keys.lastIndex - it] }, contentType = { "event" }) { index ->
            val entry = entries[entries.lastIndex - index]
            LogConsoleLine(entry) { clipboard.setText(AnnotatedString(entry.formatted())) }
        }
        item(key = "history", contentType = "history") {
            if (onLoadMore != null) TextButton(onClick = onLoadMore, enabled = !loading, modifier = Modifier.fillMaxWidth()) {
                Text(if (loading) s.logHistoryLoading else s.loadOlderLogs)
            } else if (entries.isNotEmpty()) Text(s.logHistoryEnd, style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(12.dp))
        }
    }
}

internal fun logStoreLevelLabel(level: String, s: LumenStrings): String = when (level) {
    "info" -> s.logLevelInfo
    "warning" -> s.logLevelWarning
    "error" -> s.logLevelError
    else -> s.logLevelDebug
}

@Composable
private fun logLevelColor(level: String): Color = when (level.lowercase()) {
    "error" -> MaterialTheme.colorScheme.error
    "warning" -> LocalLumenColors.current.warning
    "info" -> MaterialTheme.colorScheme.primary
    else -> MaterialTheme.colorScheme.onSurfaceVariant
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun LogConsoleLine(entry: LogEntryUi, onCopy: () -> Unit) {
    val accent = logLevelColor(entry.level)
    val highlighted = entry.level == "error" || entry.level == "warning"
    Column(Modifier.fillMaxWidth().background(if (highlighted) accent.copy(alpha = .07f) else Color.Transparent)
        .combinedClickable(onClick = {}, onLongClick = onCopy)) {
        Row(Modifier.padding(horizontal = 8.dp, vertical = 12.dp)) {
            // Reserve space in the timestamp's font units, including system font scaling.
            Column(Modifier.width(with(LocalDensity.current) { 80.sp.toDp() })) {
                Text(entry.time, fontFamily = LumenMono, fontSize = 10.sp, lineHeight = 14.sp,
                    maxLines = 1, softWrap = false,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(entry.level.uppercase(), fontFamily = LumenMono, fontSize = 9.sp,
                    color = accent, fontWeight = FontWeight.Bold, modifier = Modifier.padding(top = 4.dp))
            }
            Column(Modifier.weight(1f)) {
                if (entry.component.isNotBlank()) Text(entry.component,
                    style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold, color = accent)
                Text(entry.message, fontFamily = LumenMono, fontSize = 12.sp, lineHeight = 18.sp,
                    color = MaterialTheme.colorScheme.onSurface, modifier = Modifier.padding(top = 3.dp))
            }
        }
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = .6f))
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun PlainLogLine(line: String, onCopy: () -> Unit) {
    Text(line, fontFamily = LumenMono, fontSize = 12.sp, lineHeight = 18.sp,
        color = MaterialTheme.colorScheme.onSurface, modifier = Modifier.fillMaxWidth()
            .combinedClickable(onClick = {}, onLongClick = onCopy).padding(vertical = 10.dp))
}
