package com.lumen.ui.screens

import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.produceState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Cached pattern only removes legacy technical outbound tags, never arbitrary name segments. */
private val technicalOutboundSuffix = Regex(
    "\\s+[·•.]\\s*(?:proxy(?:[-_ ]?\\d+)?|direct|block|reject|dns|outbound(?:[-_ ]?\\d+)?|bypass)\\s*$",
    RegexOption.IGNORE_CASE
)

fun cleanServerDisplayName(name: String): String {
    var clean = name.trim()
    while (true) {
        val shortened = clean.replace(technicalOutboundSuffix, "").trimEnd()
        if (shortened == clean || shortened.isBlank()) return clean
        clean = shortened
    }
}

/** A deterministic order keeps successful probes first and unknown/failed nodes at the end. */
internal fun dashboardServersByPing(nodes: List<NodeUiModel>, query: String): List<NodeUiModel> =
    nodes.filter { query.isBlank() || it.name.contains(query, true) || it.displayProtocol.contains(query, true) }
        .sortedWith(serverSortComparator(ServerSort.PING))

/** Each arriving latency immediately schedules a new order; stale jobs cannot publish later. */
@Composable
internal fun rememberDashboardServers(
    nodes: List<NodeUiModel>, query: String, listState: LazyListState? = null
): State<List<NodeUiModel>> =
    produceState(emptyList(), nodes, query, listState) {
        val sorted = withContext(Dispatchers.Default) { dashboardServersByPing(nodes, query) }
        // Lazy lists normally retain the first visible key. At the absolute top,
        // that would follow the old first server downward and hide faster results.
        // Override the key anchor BEFORE publishing the new order, in the same
        // remeasure. Never interrupt a drag/fling or move a viewport below the top.
        if (listState != null && !listState.isScrollInProgress && !listState.canScrollBackward &&
            value.firstOrNull()?.id != sorted.firstOrNull()?.id) {
            listState.requestScrollToItem(0)
        }
        value = sorted
    }

internal fun serverFilterChoices(
    nodes: List<NodeUiModel>, groups: List<ServerGroupUiModel>, subscriptions: List<SubscriptionUiModel>, s: LumenStrings
): List<Pair<String, String>> =
    listOf("all" to "${s.allGroups} · ${nodes.size}",
        "manual" to "${s.manual} · ${nodes.count { nodeInGroup(it, GROUP_MANUAL) }}") +
        groups.map { "group:${it.id}" to "${it.name} · ${it.nodeCount}" } +
        subscriptions.map { "subscription:${it.id}" to "${it.name} · ${it.nodeCount}" }
