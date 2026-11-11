//! GUI-free helpers for the History tab
//! (port of `qml_app/bridge/history_helpers.py`, itself a faithful port of the
//! formatting logic in the classic `ui/history_page.py`).

use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, TimeZone, Utc};

use lumen_core::i18n::tr;
use lumen_core::traffic_history::TrafficHistoryStorage;

/// Python `fmt_bytes`.
pub fn fmt_bytes(bytes: i64) -> String {
    let mut value = bytes.max(0) as f64;
    let units = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];
    let mut unit_idx = 0usize;
    while value >= 1024.0 && unit_idx < units.len() - 1 {
        value /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        return format!("{} {}", value as i64, units[unit_idx]);
    }
    if unit_idx <= 2 {
        return format!("{value:.1} {}", units[unit_idx]);
    }
    format!("{value:.2} {}", units[unit_idx])
}

/// Parse a Python-`fromisoformat`-compatible timestamp. Returns the instant
/// plus whether the source string carried an explicit UTC offset.
fn parse_iso(value: &str) -> Option<(DateTime<FixedOffset>, bool)> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(aware) = DateTime::parse_from_rfc3339(value) {
        return Some((aware, true));
    }
    // Python accepts "+00:00" instead of "Z" and no fractional seconds.
    let normalized = value.replace('Z', "+00:00");
    if let Ok(aware) = DateTime::parse_from_rfc3339(&normalized) {
        return Some((aware, true));
    }
    let formats = [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
    ];
    for format in formats {
        if let Ok(naive) = NaiveDateTime::parse_from_str(value, format) {
            return Some((Utc.from_utc_datetime(&naive).fixed_offset(), false));
        }
    }
    None
}

/// Python `fmt_datetime` — `%d.%m.%Y %H:%M` in the local timezone.
pub fn fmt_datetime(iso: &str) -> String {
    if iso.is_empty() {
        return String::new();
    }
    match parse_iso(iso) {
        Some((dt, _)) => dt
            .with_timezone(&Local)
            .format("%d.%m.%Y %H:%M")
            .to_string(),
        None => iso.chars().take(16).collect(),
    }
}

/// Python `fmt_duration` — localized "Xч Yм" / "Yм Zс" / "Zс".
pub fn fmt_duration(start: &str, end: Option<&str>) -> String {
    let total_sec = match duration_seconds(start, end, true) {
        Ok(seconds) => seconds,
        Err(()) => return String::new(),
    };
    if total_sec < 0 {
        return String::new();
    }
    let hours = total_sec / 3600;
    let minutes = (total_sec % 3600) / 60;
    let secs = total_sec % 60;
    if hours > 0 {
        tr(
            "{hours}ч {minutes}м",
            &[
                ("hours", &hours.to_string()),
                ("minutes", &minutes.to_string()),
            ],
        )
    } else if minutes > 0 {
        tr(
            "{minutes}м {secs}с",
            &[
                ("minutes", &minutes.to_string()),
                ("secs", &secs.to_string()),
            ],
        )
    } else {
        tr("{secs}с", &[("secs", &secs.to_string())])
    }
}

/// Python `_duration_seconds` — raw session length in seconds (0 when unknown).
pub fn duration_seconds_of(start: &str, end: Option<&str>) -> i64 {
    match duration_seconds(start, end, false) {
        Ok(seconds) => {
            if seconds > 0 {
                seconds
            } else {
                0
            }
        }
        Err(()) => 0,
    }
}

fn duration_seconds(start: &str, end: Option<&str>, for_display: bool) -> Result<i64, ()> {
    if start.is_empty() {
        if for_display {
            return Err(());
        }
        return Ok(0);
    }
    let (start_dt, start_aware) = parse_iso(start).ok_or(())?;
    let (end_dt, end_aware) = match end {
        Some(end) if !end.is_empty() => parse_iso(end).ok_or(())?,
        _ => (Utc::now().fixed_offset(), true),
    };
    // Python raises on naive/aware mixing; the except branch swallows it.
    if start_aware != end_aware {
        return Err(());
    }
    if !start_aware && !end_aware {
        // Both naive: arithmetic is timezone-free, like Python.
        let start_naive = start_dt.naive_utc();
        let end_naive = end_dt.naive_utc();
        return Ok((end_naive - start_naive).num_seconds());
    }
    Ok((end_dt - start_dt).num_seconds())
}

fn mode_label(mode: &str) -> String {
    match mode {
        "xray" => "Прокси".to_string(),
        "singbox" => "TUN (sing-box)".to_string(),
        other => other.to_string(),
    }
}

fn route_label(route: &str) -> String {
    match route {
        "proxy" => "VPN".to_string(),
        "direct" => "Прямой".to_string(),
        "mixed" => "Смешанный".to_string(),
        other => other.to_string(),
    }
}

fn is_tun(mode: &str) -> bool {
    mode.to_lowercase().contains("tun") || mode == "singbox"
}

/// Python `build_history_payload` — a QML-friendly dict mirroring
/// `ui/history_page._refresh()`.
pub fn build_history_payload(
    storage: Option<&TrafficHistoryStorage>,
    days: i64,
) -> serde_json::Value {
    use serde_json::json;
    let Some(storage) = storage else {
        return json!({
            "summaryDown": "0 B",
            "summaryUp": "0 B",
            "summarySessions": 0,
            "sessions": [],
            "daily": [],
            "procs": [],
        });
    };

    let mut sessions = storage.get_sessions(days);
    sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    let total_up: u64 = sessions.iter().map(|session| session.total_upload).sum();
    let total_down: u64 = sessions.iter().map(|session| session.total_download).sum();

    let session_rows: Vec<serde_json::Value> = sessions
        .iter()
        .map(|session| {
            let proc_keys: Vec<&String> = session.processes.keys().collect();
            let mut procs = proc_keys
                .iter()
                .take(5)
                .map(|key| key.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if proc_keys.len() > 5 {
                procs = format!("{procs} (+{})", proc_keys.len() - 5);
            }
            json!({
                "date": fmt_datetime(&session.started_at),
                "node": session.node_name,
                "mode": session.mode,
                "isTun": is_tun(&session.mode),
                "duration": fmt_duration(&session.started_at, session.ended_at.as_deref()),
                "down": fmt_bytes(session.total_download as i64),
                "hasDown": session.total_download > 0,
                "up": fmt_bytes(session.total_upload as i64),
                "procs": procs,
                // Raw values for client-side column sorting.
                "ts": session.started_at,
                "durSec": duration_seconds_of(&session.started_at, session.ended_at.as_deref()),
                "downB": session.total_download as i64,
                "upB": session.total_upload as i64,
            })
        })
        .collect();

    let daily = storage.get_daily_totals(days);
    let mut daily_pairs: Vec<(&String, &(u64, u64))> = daily.iter().collect();
    daily_pairs.sort_by(|a, b| b.0.cmp(a.0));
    let daily_rows: Vec<serde_json::Value> = daily_pairs
        .iter()
        .map(|(date_key, totals)| {
            let down = totals.0 as i64;
            let up = totals.1 as i64;
            json!({
                "day": date_key,
                "down": fmt_bytes(down),
                "hasDown": down > 0,
                "up": fmt_bytes(up),
                // Raw values for client-side column sorting.
                "downB": down,
                "upB": up,
            })
        })
        .collect();

    let proc_totals = storage.get_process_totals(days);
    let mut proc_pairs: Vec<_> = proc_totals.iter().collect();
    proc_pairs.sort_by(|a, b| {
        let a_down = a.1.get("download").and_then(|v| v.as_u64()).unwrap_or(0);
        let b_down = b.1.get("download").and_then(|v| v.as_u64()).unwrap_or(0);
        b_down.cmp(&a_down)
    });
    let proc_rows: Vec<serde_json::Value> = proc_pairs
        .iter()
        .map(|(exe, stats)| {
            let down = stats.get("download").and_then(|v| v.as_u64()).unwrap_or(0) as i64;
            let up = stats.get("upload").and_then(|v| v.as_u64()).unwrap_or(0) as i64;
            let route = stats
                .get("route")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            json!({
                "exe": exe,
                "down": fmt_bytes(down),
                "hasDown": down > 0,
                "up": fmt_bytes(up),
                "route": route,
                // Raw values for client-side column sorting.
                "downB": down,
                "upB": up,
            })
        })
        .collect();

    json!({
        "summaryDown": fmt_bytes(total_down as i64),
        "summaryUp": fmt_bytes(total_up as i64),
        "summarySessions": sessions.len(),
        "sessions": session_rows,
        "daily": daily_rows,
        "procs": proc_rows,
    })
}

/// Python `_MODE_LABELS` / `_ROUTE_LABELS` re-exported for pages that need the
/// raw label maps (tests).
pub fn mode_label_for(mode: &str) -> String {
    mode_label(mode)
}

pub fn route_label_for(route: &str) -> String {
    route_label(route)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn test_fmt_bytes_matches_python_units() {
        assert_eq!(fmt_bytes(0), "0 B");
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(fmt_bytes(1024), "1.0 KB");
        assert_eq!(fmt_bytes(1536), "1.5 KB");
        assert_eq!(fmt_bytes(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn test_fmt_datetime_formats_local_time() {
        // Aware UTC input converts to the local timezone.
        let dt = Utc.with_ymd_and_hms(2024, 5, 4, 15, 30, 0).unwrap();
        let offset = Local::now().format("%z").to_string();
        let iso = format!(
            "{}T{:02}:{:02}:00{}",
            dt.format("%Y-%m-%d"),
            dt.hour(),
            dt.minute(),
            offset
        );
        let rendered = fmt_datetime(&iso);
        assert!(rendered.ends_with(" 15:30") || rendered.contains(":"));
        assert!(rendered.len() >= 16);
    }

    #[test]
    fn test_fmt_duration_and_seconds() {
        let start = "2024-05-04T10:00:00+00:00";
        let end = "2024-05-04T11:30:05+00:00";
        assert_eq!(duration_seconds_of(start, Some(end)), 5405);
        assert_eq!(
            fmt_duration(start, Some(end)),
            tr("{hours}ч {minutes}м", &[("hours", "1"), ("minutes", "30")])
        );
        assert_eq!(duration_seconds_of(start, None) >= 0, true);
        assert_eq!(duration_seconds_of("", None), 0);
        assert_eq!(fmt_duration("", None), "");
    }

    #[test]
    fn test_mode_and_route_labels() {
        assert_eq!(mode_label_for("xray"), "Прокси");
        assert_eq!(mode_label_for("singbox"), "TUN (sing-box)");
        assert_eq!(mode_label_for("weird"), "weird");
        assert_eq!(route_label_for("proxy"), "VPN");
        assert_eq!(route_label_for("direct"), "Прямой");
        assert_eq!(route_label_for("mixed"), "Смешанный");
        assert!(is_tun("singbox"));
        assert!(!is_tun("xray"));
    }

    #[test]
    fn test_empty_payload_without_storage() {
        let payload = build_history_payload(None, 30);
        assert_eq!(payload["summaryDown"], "0 B");
        assert_eq!(payload["summarySessions"], 0);
        assert!(payload["sessions"].as_array().unwrap().is_empty());
    }
}
