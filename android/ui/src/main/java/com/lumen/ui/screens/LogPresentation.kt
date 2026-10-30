package com.lumen.ui.screens

internal fun filterLogEntries(entries: List<LogEntryUi>, level: String, query: String): List<LogEntryUi> =
    entries.filter { entry ->
        (level == LOG_FILTER_ALL || logLevelRank(entry.level) >= logLevelRank(level)) &&
            (query.isBlank() || entry.message.contains(query, true) || entry.component.contains(query, true) ||
                entry.time.contains(query, true) || entry.level.contains(query, true))
    }

/** Identity is stable when older pages are prepended; repeated events still get distinct keys. */
internal fun logIdentityKeys(entries: List<LogEntryUi>): List<String> {
    val occurrences = mutableMapOf<String, Int>()
    return entries.map { entry ->
        val identity = "${entry.timestamp}|${entry.level}|${entry.component}|${entry.message}"
        val occurrence = occurrences.getOrDefault(identity, 0)
        occurrences[identity] = occurrence + 1
        "$identity|$occurrence"
    }
}
