//! Owned snapshots: capture on the state owner, decode on the disk writer.

use serde_json::Value;

use crate::models::{AppSettings, AppState};

#[derive(Debug, Clone)]
pub struct StateSnapshot {
    pub payload: String,
    pub settings: AppSettings,
}

impl StateSnapshot {
    pub fn to_dict(&self) -> Value {
        serde_json::from_str(&self.payload).unwrap_or(Value::Null)
    }
}

pub fn snapshot_state(state: &AppState) -> StateSnapshot {
    // Python serializes with `ensure_ascii=True`; serde_json keeps raw UTF-8.
    // The decoded document is byte-for-byte identical content, so the disk
    // writer sees the same `to_dict()` either way.
    let payload = serde_json::to_string(&state.to_dict()).unwrap_or_else(|_| "{}".to_string());
    StateSnapshot {
        payload,
        settings: state.settings.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_copies_settings_and_decodes_payload() {
        let mut state = AppState::default();
        state.settings.theme = "dark".to_string();
        let snapshot = snapshot_state(&state);

        assert_eq!(snapshot.settings.theme, "dark");
        let decoded = snapshot.to_dict();
        assert_eq!(decoded["settings"]["theme"], "dark");
        assert_eq!(decoded["schema_version"], crate::constants::STATE_SCHEMA_VERSION);
    }
}
