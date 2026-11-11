//! Structured, filterable ring-buffer model for the logs page
//! (port of `qml_app/bridge/log_model.py`).

use std::collections::VecDeque;

use lumen_core::log_utils::{LogEntry, parse_log_line};

/// Python `LogModel` (QAbstractListModel) as a plain struct.
#[derive(Default)]
pub struct LogModel {
    max_lines: usize,
    lines: VecDeque<LogEntry>,
}

impl LogModel {
    pub fn new(max_lines: usize) -> Self {
        Self {
            max_lines,
            lines: VecDeque::new(),
        }
    }

    pub fn row_count(&self) -> usize {
        self.lines.len()
    }

    pub fn entry_at(&self, row: usize) -> Option<&LogEntry> {
        self.lines.get(row)
    }

    pub fn lines(&self) -> &VecDeque<LogEntry> {
        &self.lines
    }

    /// Python `LogModel._localized_entry` — translate the friendly summaries
    /// produced by log humanization (raw log lines arrive pre-localized).
    fn localized_entry(entry: LogEntry, translate: Option<&dyn Fn(&str) -> String>) -> LogEntry {
        let Some(translate) = translate else {
            return entry;
        };
        let mut changed = false;
        let mut message = entry.message.clone();
        let mut details = entry.details.clone();
        if entry.message != entry.details {
            message = translate(&entry.message);
            details = translate(&entry.details);
            changed = true;
        }
        let mut source = entry.source.clone();
        if !entry.source.is_empty() && !entry.source.is_ascii() {
            source = translate(&entry.source);
            changed = true;
        }
        let mut action_label = entry.action_label.clone();
        if !entry.action_label.is_empty() {
            action_label = translate(&entry.action_label);
            changed = true;
        }
        if !changed {
            return entry;
        }
        let mut result = entry;
        result.message = message;
        result.details = details;
        result.source = source;
        result.action_label = action_label;
        result.search_text =
            format!("{} {} {}", result.source, result.message, result.details).to_lowercase();
        result
    }

    /// Python `LogModel.append_lines` — append a burst, keeping at most
    /// `max_lines` entries.
    pub fn append_lines(
        &mut self,
        lines: impl IntoIterator<Item = String>,
        translate: Option<&dyn Fn(&str) -> String>,
    ) {
        let entries: Vec<LogEntry> = lines
            .into_iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| Self::localized_entry(parse_log_line(&line, None), translate))
            .collect();
        if entries.is_empty() {
            return;
        }
        if entries.len() >= self.max_lines {
            self.lines.clear();
            let start = entries.len() - self.max_lines;
            self.lines.extend(entries.into_iter().skip(start));
            return;
        }
        let overflow = (self.lines.len() + entries.len()).saturating_sub(self.max_lines);
        for _ in 0..overflow {
            self.lines.pop_front();
        }
        self.lines.extend(entries);
    }

    /// Python `LogModel.append_line`.
    pub fn append_line(&mut self, line: &str, translate: Option<&dyn Fn(&str) -> String>) {
        self.append_lines([line.to_string()], translate);
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }
}

/// Python `LogFilterModel` (QSortFilterProxyModel) state.
#[derive(Debug, Clone)]
pub struct LogFilter {
    pub level: String,
    pub search: String,
}

impl Default for LogFilter {
    fn default() -> Self {
        Self {
            level: "all".into(),
            search: String::new(),
        }
    }
}

impl LogFilter {
    /// Python `LogFilterModel.setLevelFilter`.
    pub fn set_level_filter(&mut self, value: &str) {
        let mut value = (if value.is_empty() { "all" } else { value })
            .trim()
            .to_lowercase();
        if !matches!(
            value.as_str(),
            "all" | "error" | "warning" | "success" | "info"
        ) {
            value = "all".to_string();
        }
        self.level = value;
    }

    /// Python `LogFilterModel.setSearchText`.
    pub fn set_search_text(&mut self, value: &str) {
        self.search = value.trim().to_lowercase();
    }

    /// Python `LogFilterModel.filterAcceptsRow`.
    pub fn filter_accepts(&self, entry: &LogEntry) -> bool {
        let level = if entry.level.is_empty() {
            "info"
        } else {
            entry.level.as_str()
        };
        if self.level != "all" && level != self.level {
            return false;
        }
        if !self.search.is_empty() && !entry.search_text.contains(&self.search) {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn translator() -> impl Fn(&str) -> String {
        |message: &str| {
            if message.starts_with("Локальный порт 10808") {
                "Local port 10808 is busy".to_string()
            } else {
                message.to_string()
            }
        }
    }

    #[test]
    fn test_append_lines_parses_and_translates_friendly_summaries() {
        let mut model = LogModel::new(10);
        let translate = translator();
        model.append_line("[xray] Порт 10808 занят другим процессом", Some(&translate));
        let entry = model.entry_at(0).unwrap();
        assert_eq!(entry.message, "Local port 10808 is busy");
    }

    #[test]
    fn test_ring_buffer_evicts_oldest_lines() {
        let mut model = LogModel::new(3);
        model.append_lines((0..5).map(|index| format!("[xray] line-{index}")), None);
        assert_eq!(model.row_count(), 3);
        assert_eq!(model.entry_at(0).unwrap().message, "line-2");
        assert_eq!(model.entry_at(2).unwrap().message, "line-4");
    }

    #[test]
    fn test_level_filter_matches_python_levels() {
        let mut filter = LogFilter::default();
        filter.set_level_filter("ERROR");
        assert_eq!(filter.level, "error");
        filter.set_level_filter("nonsense");
        assert_eq!(filter.level, "all");
        filter.set_level_filter("warning");
        let mut entry = LogEntry::default();
        entry.level = "error".into();
        assert!(!filter.filter_accepts(&entry));
        entry.level = "warning".into();
        entry.search_text = "tunnel ready".into();
        assert!(filter.filter_accepts(&entry));
        filter.set_search_text("  TUNNEL ");
        assert!(filter.filter_accepts(&entry));
        filter.set_search_text("missing");
        assert!(!filter.filter_accepts(&entry));
    }
}
