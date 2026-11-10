//! Text and parsed snapshots of the selected sing-box configuration.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Result, ensure};
use lumen_core::config::json_document::{config_has_proxy_outbound, format_json_error_message};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct SingboxDocumentState {
    pub source_path: PathBuf,
    pub text: String,
    pub text_hash: String,
    pub has_proxy_outbound: bool,
    pub file_modified: Option<SystemTime>,
    pub file_size: Option<u64>,
}

pub struct ParsedSingboxDocument {
    pub source_path: PathBuf,
    pub text: String,
    pub text_hash: String,
    pub payload: Value,
    pub has_proxy_outbound: bool,
}

pub fn inspect_singbox_document_text(source_path: &Path, text: &str) -> SingboxDocumentState {
    SingboxDocumentState {
        source_path: source_path.into(),
        text: text.into(),
        text_hash: format!("{:x}", Sha256::digest(text.as_bytes())),
        has_proxy_outbound: serde_json::from_str::<Value>(text)
            .is_ok_and(|payload| config_has_proxy_outbound(&payload)),
        file_modified: None,
        file_size: None,
    }
}

pub fn parse_singbox_document(source_path: &Path, text: &str) -> Result<ParsedSingboxDocument> {
    let payload: Value = serde_json::from_str(text).map_err(|error| {
        anyhow::anyhow!(
            "{}: {}",
            source_path
                .file_name()
                .unwrap_or(source_path.as_os_str())
                .to_string_lossy(),
            format_json_error_message(text, &error)
        )
    })?;
    ensure!(
        payload.is_object(),
        "Корень sing-box config должен быть JSON-объектом."
    );
    Ok(ParsedSingboxDocument {
        source_path: source_path.into(),
        text: text.into(),
        text_hash: format!("{:x}", Sha256::digest(text.as_bytes())),
        has_proxy_outbound: config_has_proxy_outbound(&payload),
        payload,
    })
}
