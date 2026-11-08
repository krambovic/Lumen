//! Dormant OpenVPN dispatch for `link_parser.py`'s `.ovpn` branch.
//!
//! This file is NOT declared in `mod.rs` yet.  The Rust port of
//! `openvpn_import.py` is produced by a parallel agent and lands at
//! `crate::openvpn_import` with exactly these signatures:
//!
//! ```ignore
//! pub fn looks_like_openvpn_config(text: &str) -> bool;
//! pub fn parse_openvpn_config(
//!     text: &str,
//!     source_path: Option<&str>,
//! ) -> anyhow::Result<(serde_json::Value, Vec<String>, String)>;
//! pub fn openvpn_requires_user_auth(text: &str) -> bool;
//! ```
//!
//! To enable: uncomment `pub mod openvpn_hook;` in `mod.rs` and the
//! `OPENVPN-BRANCH` call in `parse_links_text`, then delete this notice.

use serde_json::Value;

use super::LinkParseError;

/// Python dispatch (link_parser.py `parse_links_text`, openvpn section):
///
/// ```python
/// if looks_like_openvpn_config(stripped):
///     try:
///         native, dns_servers, profile_name = parse_openvpn_config(stripped, source_path=source_path)
///         ...
///     except Exception as exc:
///         return [], [f"OpenVPN: {exc}"]
/// ```
///
/// Returns `None` when the text is not an OpenVPN config; `Some(Ok(...))`
/// with the parsed node, or `Some(Err(errors))` with the `OpenVPN:` prefixed
/// failure message.
pub fn try_parse_openvpn_text(
    stripped: &str,
    source_path: Option<&str>,
) -> Option<Result<(Vec<crate::models::Node>, Vec<String>), LinkParseError>> {
    if !crate::openvpn_import::looks_like_openvpn_config(stripped) {
        return None;
    }
    let source: Option<std::path::PathBuf> = source_path.map(std::path::PathBuf::from);
    match crate::openvpn_import::parse_openvpn_config(stripped, source.as_deref()) {
        Ok((native, dns_servers, profile_name)) => {
            Some(Ok((vec![super::openvpn_wrapper_node(&native, &dns_servers, &profile_name)], Vec::new())))
        }
        Err(exc) => Some(Err(LinkParseError::new(format!("OpenVPN: {exc}")))),
    }
}

/// Reserved: `parse_single` does not route OpenVPN in Python (the INI-style
/// config goes through `parse_links_text`); kept for parity tooling.
#[allow(dead_code)]
pub fn openvpn_requires_user_auth(text: &str) -> bool {
    crate::openvpn_import::openvpn_requires_user_auth(text)
}

/// Type anchor so this file keeps compiling against the agreed value shape.
#[allow(dead_code)]
type NativeOutbound = Value;
