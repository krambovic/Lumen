//! Pure helpers for the Zapret tab (port of `qml_app/bridge/zapret_helpers.py`).

use serde_json::Value;

use lumen_engines::zapret_manager::PresetInfo;

/// Python `_fmt_date` — 'YYYY-MM-DD HH:MM' (same as the classic UI).
fn fmt_date(iso: &str) -> String {
    if iso.is_empty() {
        return String::new();
    }
    match chrono::DateTime::parse_from_rfc3339(iso) {
        Ok(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        Err(_) => match chrono::NaiveDateTime::parse_from_str(iso, "%Y-%m-%dT%H:%M:%S%.f") {
            Ok(naive) => naive.format("%Y-%m-%d %H:%M").to_string(),
            Err(_) => iso.to_string(),
        },
    }
}

/// Python `preset_to_map` — a single PresetInfo as a QML-friendly map.
pub fn preset_to_map(info: &PresetInfo) -> Value {
    serde_json::json!({
        "name": info.name,
        "description": if info.description.is_empty() { Value::String(String::new()) } else { Value::String(info.description.clone()) },
        "argCount": info.arg_count,
        "created": fmt_date(&info.created),
        "modified": fmt_date(&info.modified),
        "createdRaw": info.created,
        "modifiedRaw": info.modified,
    })
}

/// Python `list_preset_maps` — the full preset list (empty list on failure).
pub fn list_preset_maps() -> Vec<Value> {
    std::panic::catch_unwind(|| lumen_engines::zapret_manager::list_preset_infos())
        .map(|infos| infos.iter().map(preset_to_map).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_to_map_fields() {
        let info = PresetInfo {
            name: "General".into(),
            description: "Base strategy".into(),
            arg_count: 7,
            created: "2024-05-04T10:00:00+00:00".into(),
            modified: "2024-05-04T11:20:00+00:00".into(),
            file_path: std::path::PathBuf::from("zapret/General.txt"),
        };
        let map = preset_to_map(&info);
        assert_eq!(map["name"], "General");
        assert_eq!(map["description"], "Base strategy");
        assert_eq!(map["argCount"], 7);
        assert_eq!(map["createdRaw"], "2024-05-04T10:00:00+00:00");
        let created = map["created"].as_str().unwrap();
        assert!(created.starts_with("2024-05-04 10:"));
    }

    #[test]
    fn test_fmt_date_passthrough_on_garbage() {
        assert_eq!(fmt_date(""), "");
        assert_eq!(fmt_date("not a date"), "not a date");
    }
}
