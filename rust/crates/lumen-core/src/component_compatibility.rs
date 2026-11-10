//! Embedded compatibility policy for downloadable Lumen components.

use std::sync::LazyLock;

use anyhow::{Context, Result, ensure};
use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};

const MANIFEST: &str = concat!(
    r#"{"components":{"singbox":{"minimum_version":"1.12.0"},"xray":{"minimum_version":"25.12.8"}},"geodata":"#,
    r#"{"required_geoip":["ru-blocked","ru-blocked-community","ru"],"required_geosite":["ru-blocked","category-ru"]},"policy_version":"2026-07-14","schema":1}"#,
);
const TRUSTED_SHA256: &str = "eb221483ad75554d699f8f48a6bcc11bfe13410b21788dd6da49200d0ecccaa3";
static VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([0-9]+)\.([0-9]+)\.([0-9]+)").expect("valid version regex"));

pub fn verify_compatibility_manifest() -> Result<()> {
    ensure!(
        format!("{:x}", Sha256::digest(MANIFEST.as_bytes())) == TRUSTED_SHA256,
        "манифест совместимости Lumen поврежден или изменен"
    );
    Ok(())
}

fn policy() -> Result<Value> {
    verify_compatibility_manifest()?;
    Ok(serde_json::from_str(MANIFEST)?)
}

fn version_tuple(value: &str) -> Option<(u64, u64, u64)> {
    let fields = VERSION.captures(value)?;
    Some((
        fields[1].parse().ok()?,
        fields[2].parse().ok()?,
        fields[3].parse().ok()?,
    ))
}

pub fn ensure_component_compatible(component: &str, version: &str) -> Result<()> {
    let policy = policy()?;
    let entry = policy["components"]
        .get(component)
        .filter(|entry| entry.is_object())
        .with_context(|| format!("неизвестный компонент в манифесте совместимости: {component}"))?;
    let current = version_tuple(version)
        .with_context(|| format!("не удалось определить версию {component}"))?;
    let minimum_text = entry["minimum_version"].as_str().unwrap_or_default();
    if let Some(minimum) = version_tuple(minimum_text) {
        ensure!(
            current >= minimum,
            "{component} {version} несовместим с этой версией Lumen; требуется {minimum_text} или новее"
        );
    }
    Ok(())
}

pub fn required_geodata_codes() -> Result<(Vec<String>, Vec<String>)> {
    let policy = policy()?;
    let geodata = &policy["geodata"];
    Ok((
        serde_json::from_value(geodata["required_geosite"].clone())?,
        serde_json::from_value(geodata["required_geoip"].clone())?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_the_pinned_manifest_and_enforces_minimum_versions() -> anyhow::Result<()> {
        verify_compatibility_manifest()?;
        ensure_component_compatible("xray", "Xray 26.7.11")?;
        ensure_component_compatible("xray", "25.12.8-rc.1")?;
        let error = ensure_component_compatible("singbox", "sing-box version 1.11.0").unwrap_err();
        assert!(error.to_string().contains("требуется 1.12.0"));
        Ok(())
    }

    #[test]
    fn supplies_the_lumen_geodata_codes() -> anyhow::Result<()> {
        let (geosite, geoip) = required_geodata_codes()?;
        assert_eq!(geosite, ["ru-blocked", "category-ru"]);
        assert_eq!(geoip, ["ru-blocked", "ru-blocked-community", "ru"]);
        Ok(())
    }
}
