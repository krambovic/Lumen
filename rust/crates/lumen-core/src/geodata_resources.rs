//! Sing-box binary rule-set locations for regional routing presets.
//!
//! Port of `geodata_resources.py`: RuNetFreedom release archive entries and
//! the raw.githubusercontent rule-set feeds for China and Iran.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::constants::xray_path_default;

pub const RUNETFREEDOM_RULES_BASE_URL: &str =
    "https://github.com/runetfreedom/russia-v2ray-rules-dat/releases/latest/download";
pub const SINGBOX_RULES_ARCHIVE_URL: &str =
    "https://github.com/runetfreedom/russia-v2ray-rules-dat/releases/latest/download/sing-box.zip";

/// `SINGBOX_RULE_SET_DIR = XRAY_PATH_DEFAULT.parent / "rule-sets"`.
pub fn singbox_rule_set_dir() -> PathBuf {
    let mut dir = xray_path_default();
    dir.pop();
    dir.join("rule-sets")
}

// Keys are the labels used by the common routing model. Values are paths in
// RuNetFreedom's sing-box.zip release archive.
pub const SINGBOX_BINARY_RULE_SETS: [(&str, &str); 6] = [
    // Kept for custom/legacy rules created before regional presets.
    ("geosite:category-ru", "rule-set-geosite/geosite-category-ru.srs"),
    ("geosite:ru-available-only-inside", "rule-set-geosite/geosite-ru-available-only-inside.srs"),
    ("geosite:ru-blocked", "rule-set-geosite/geosite-ru-blocked.srs"),
    ("geoip:ru", "rule-set-geoip/geoip-ru.srs"),
    ("geoip:ru-blocked", "rule-set-geoip/geoip-ru-blocked.srs"),
    ("geoip:ru-blocked-community", "rule-set-geoip/geoip-ru-blocked-community.srs"),
];

const CHINA_RULE_SET_SOURCES: [(&str, &str, &str); 11] = [
    ("geosite:cn", "rule-set-geosite", "geosite-cn"),
    ("geosite:gfw", "rule-set-geosite", "geosite-gfw"),
    ("geosite:greatfire", "rule-set-geosite", "geosite-greatfire"),
    ("geosite:google", "rule-set-geosite", "geosite-google"),
    ("geoip:cn", "rule-set-geoip", "geoip-cn"),
    ("geoip:facebook", "rule-set-geoip", "geoip-facebook"),
    ("geoip:fastly", "rule-set-geoip", "geoip-fastly"),
    ("geoip:google", "rule-set-geoip", "geoip-google"),
    ("geoip:netflix", "rule-set-geoip", "geoip-netflix"),
    ("geoip:telegram", "rule-set-geoip", "geoip-telegram"),
    ("geoip:twitter", "rule-set-geoip", "geoip-twitter"),
];

const IRAN_RULE_SET_SOURCES: [(&str, &str); 2] =
    [("geosite:ir", "geosite-ir"), ("geoip:ir", "geoip-ir")];

pub fn china_singbox_rule_sets() -> BTreeMap<String, String> {
    CHINA_RULE_SET_SOURCES
        .iter()
        .map(|(key, kind, tag)| {
            (
                key.to_string(),
                format!("https://raw.githubusercontent.com/2dust/sing-box-rules/{kind}/{tag}.srs"),
            )
        })
        .collect()
}

pub fn iran_singbox_rule_sets() -> BTreeMap<String, String> {
    IRAN_RULE_SET_SOURCES
        .iter()
        .map(|(key, tag)| {
            (
                key.to_string(),
                format!("https://raw.githubusercontent.com/chocolate4u/Iran-sing-box-rules/rule-set/{tag}.srs"),
            )
        })
        .collect()
}

pub fn regional_singbox_rule_sets() -> BTreeMap<String, BTreeMap<String, String>> {
    let mut regional: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    regional.insert(
        "russia".to_string(),
        SINGBOX_BINARY_RULE_SETS
            .iter()
            .map(|(key, path)| (key.to_string(), format!("archive:{path}")))
            .collect(),
    );
    regional.insert("china".to_string(), china_singbox_rule_sets());
    regional.insert("iran".to_string(), iran_singbox_rule_sets());
    regional
}

pub fn all_singbox_binary_rule_sets() -> BTreeMap<String, String> {
    let mut all: BTreeMap<String, String> = SINGBOX_BINARY_RULE_SETS
        .iter()
        .map(|(key, path)| (key.to_string(), path.to_string()))
        .collect();
    // The stored value is the local file name ("geosite:cn" -> "geosite-cn.srs");
    // the download URL itself lives in china_singbox_rule_sets().
    for (key, _) in china_singbox_rule_sets() {
        all.insert(key.clone(), format!("{}.srs", key.replace(':', "-")));
    }
    for (key, _) in iran_singbox_rule_sets() {
        all.insert(key.clone(), format!("{}.srs", key.replace(':', "-")));
    }
    all
}

pub fn singbox_rule_set_path(key: &str) -> Option<PathBuf> {
    let archive_path = all_singbox_binary_rule_sets()
        .get(key.trim().to_lowercase().as_str())?
        .clone();
    Some(singbox_rule_set_dir().join(archive_path.rsplit('/').next().unwrap_or(&archive_path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_rule_sets_cover_regional_labels() {
        let all = all_singbox_binary_rule_sets();
        assert_eq!(all.get("geosite:ru-blocked").map(String::as_str), Some("rule-set-geosite/geosite-ru-blocked.srs"));
        assert_eq!(all.get("geosite:cn").map(String::as_str), Some("geosite-cn.srs"));
        assert_eq!(all.get("geoip:ir").map(String::as_str), Some("geoip-ir.srs"));
    }

    #[test]
    fn regional_presets_map_to_archive_or_feeds() {
        let regional = regional_singbox_rule_sets();
        assert_eq!(
            regional["russia"]["geoip:ru"],
            "archive:rule-set-geoip/geoip-ru.srs"
        );
        assert!(regional["china"]["geoip:telegram"].starts_with("https://raw.githubusercontent.com/2dust/"));
        assert!(regional["iran"]["geosite:ir"].contains("Iran-sing-box-rules"));
    }

    #[test]
    fn rule_set_path_lands_in_rule_set_dir() {
        let path = singbox_rule_set_path("GEOIP:RU").expect("known label");
        assert!(path.ends_with("geoip-ru.srs"));
        assert!(path.starts_with(singbox_rule_set_dir()));
        assert!(singbox_rule_set_path("geosite:unknown").is_none());
    }
}
