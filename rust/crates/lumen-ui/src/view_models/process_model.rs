//! Per-process traffic stats model for the Dashboard page
//! (port of `qml_app/bridge/process_model.py`).

use serde_json::Value;

/// One normalized row (the Python `_normalize` dict, typed).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcessRow {
    pub name: String,
    pub down: f64,
    pub up: f64,
    pub download_total: f64,
    pub upload_total: f64,
    pub pid: i64,
    pub proxy_bytes: f64,
    pub direct_bytes: f64,
    pub unknown_bytes: f64,
    pub connections: i64,
    pub total_connections: i64,
    pub top_host: String,
    pub total: f64,
    pub route: String,
}

impl ProcessRow {
    /// Python `_get(d, *keys, default=...)` over a JSON payload.
    fn get(item: &Value, keys: &[&str], default: f64) -> f64 {
        for key in keys {
            if let Some(value) = item.get(*key) {
                if let Some(number) = value.as_f64() {
                    return number;
                }
                if let Some(number) = value.as_i64() {
                    return number as f64;
                }
            }
        }
        default
    }

    fn get_str(item: &Value, keys: &[&str], default: &str) -> String {
        for key in keys {
            if let Some(value) = item.get(*key)
                && let Some(text) = value.as_str()
            {
                return text.to_string();
            }
        }
        default.to_string()
    }

    fn get_int(item: &Value, keys: &[&str], default: i64) -> i64 {
        for key in keys {
            if let Some(value) = item.get(*key)
                && let Some(number) = value.as_i64()
            {
                return number;
            }
        }
        default
    }

    /// Python `ProcessModel._normalize` row builder.
    fn from_item(item: &Value) -> ProcessRow {
        let upload = Self::get(item, &["upload"], 0.0);
        let download = Self::get(item, &["download"], 0.0);
        let mut total = Self::get(item, &["total"], 0.0);
        if total == 0.0 {
            total = upload + download;
        }
        let proxy_bytes = Self::get(item, &["proxy_bytes"], 0.0);
        let direct_bytes = Self::get(item, &["direct_bytes"], 0.0);
        let unknown_default = (total - proxy_bytes - direct_bytes).max(0.0);
        ProcessRow {
            name: Self::get_str(item, &["name", "process", "image", "exe"], ""),
            down: Self::get(item, &["down_bps", "down", "rx", "down_speed"], 0.0),
            up: Self::get(item, &["up_bps", "up", "tx", "up_speed"], 0.0),
            download_total: download,
            upload_total: upload,
            pid: Self::get_int(item, &["pid"], 0),
            proxy_bytes,
            direct_bytes,
            unknown_bytes: Self::get(item, &["unknown_bytes"], unknown_default),
            connections: Self::get_int(item, &["connections"], 0),
            total_connections: Self::get_int(item, &["total_connections"], 0),
            top_host: Self::get_str(item, &["top_host", "host"], ""),
            total,
            route: Self::get_str(item, &["route"], "unknown"),
        }
    }
}

/// Python `ProcessModel` (QAbstractListModel) as a plain struct.
#[derive(Debug, Default)]
pub struct ProcessModel {
    rows: Vec<ProcessRow>,
    sort_key: String,
    sort_ascending: bool,
}

impl ProcessModel {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            sort_key: "total".into(),
            sort_ascending: false,
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn row_at(&self, row: usize) -> Option<&ProcessRow> {
        self.rows.get(row)
    }

    pub fn rows(&self) -> &[ProcessRow] {
        &self.rows
    }

    pub fn sort_key(&self) -> &str {
        &self.sort_key
    }

    pub fn sort_ascending(&self) -> bool {
        self.sort_ascending
    }

    /// Python `ProcessModel.set_stats`.
    pub fn set_stats(&mut self, stats: &[Value]) {
        let normalized = Self::normalize(stats);
        // Same length: update in place (cheapest, the common case).
        if normalized.len() == self.rows.len() {
            for (index, row) in normalized.into_iter().enumerate() {
                if row != self.rows[index] {
                    self.rows[index] = row;
                }
            }
            return;
        }
        // Length changed: reset (process list rarely changes size).
        self.rows = normalized;
    }

    /// Python `ProcessModel.set_sort`.
    pub fn set_sort(&mut self, key: &str, ascending: bool) {
        let normalized_key = match key {
            "download" | "upload" | "total" => key.to_string(),
            _ => "total".to_string(),
        };
        let normalized_ascending = ascending;
        if normalized_key == self.sort_key && normalized_ascending == self.sort_ascending {
            return;
        }
        self.sort_key = normalized_key;
        self.sort_ascending = normalized_ascending;
        let mut rows = std::mem::take(&mut self.rows);
        self.sort_rows(&mut rows);
        self.rows = rows;
    }

    /// Python `ProcessModel._sort_rows`.
    fn sort_rows(&self, rows: &mut [ProcessRow]) {
        let field = match self.sort_key.as_str() {
            "download" => |row: &ProcessRow| row.download_total,
            "upload" => |row: &ProcessRow| row.upload_total,
            _ => |row: &ProcessRow| row.total,
        };
        rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        // Stable sort keeps the casefolded name order as the tiebreak, matching
        // the two-pass Python `list.sort` sequence.
        if self.sort_ascending {
            rows.sort_by(|a, b| field(a).total_cmp(&field(b)));
        } else {
            rows.sort_by(|a, b| field(b).total_cmp(&field(a)));
        }
    }

    /// Python `ProcessModel._normalize`.
    fn normalize(stats: &[Value]) -> Vec<ProcessRow> {
        let model = ProcessModel::default();
        let mut rows: Vec<ProcessRow> = stats.iter().map(ProcessRow::from_item).collect();
        // The collector normally sends the busiest processes first, but not
        // every metrics backend guarantees that order. Keep the selected UI
        // order stable.
        model.sort_rows(&mut rows);
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_set_stats_normalizes_and_sorts_by_total() {
        let mut model = ProcessModel::new();
        assert_eq!(model.sort_key(), "total");
        assert!(!model.sort_ascending());
        model.set_stats(&[
            json!({"name": "chrome.exe", "upload": 10.0, "download": 20.0}),
            json!({"name": "steam.exe", "upload": 100.0, "download": 900.0}),
        ]);
        assert_eq!(model.row_count(), 2);
        // Busiest first (total descending by default).
        assert_eq!(model.row_at(0).unwrap().name, "steam.exe");
        assert_eq!(model.row_at(1).unwrap().name, "chrome.exe");
        let first = model.row_at(1).unwrap();
        assert_eq!(first.total, 30.0);
        assert_eq!(first.download_total, 20.0);
        assert_eq!(first.upload_total, 10.0);
    }

    #[test]
    fn test_unknown_bytes_defaults_to_remaining_total() {
        let row = ProcessRow::from_item(&json!({
            "name": "app.exe",
            "upload": 5.0,
            "download": 15.0,
            "proxy_bytes": 4.0,
            "direct_bytes": 6.0,
        }));
        assert_eq!(row.total, 20.0);
        assert_eq!(row.unknown_bytes, 10.0);
    }

    #[test]
    fn test_set_sort_validates_the_key() {
        let mut model = ProcessModel::new();
        model.set_stats(&[
            json!({"name": "a.exe", "upload_total": 0.0, "upload": 1.0}),
            json!({"name": "b.exe", "upload_total": 0.0, "upload": 9.0}),
        ]);
        model.set_sort("upload", true);
        assert_eq!(model.sort_key(), "upload");
        assert!(model.sort_ascending());
        assert_eq!(model.row_at(0).unwrap().name, "a.exe");
        model.set_sort("bogus", false);
        assert_eq!(model.sort_key(), "total");
    }
}
