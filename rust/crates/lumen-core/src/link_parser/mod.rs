//! Port of `windows/xray_fluent/link_parser.py`.
//!
//! Dispatch order in [`parse_links_text`] matches Python exactly: file
//! reference → JSON → Clash YAML → OpenVPN (dormant, see `openvpn_hook.rs`)
//! → WireGuard INI → base64 subscription blob → plain link list.

mod clash;
mod helpers;
mod json_import;
pub mod openvpn_hook;
mod schemes;
mod stream;
mod validate;
mod wireguard;

use std::path::PathBuf;

use serde_json::{json, Value};

pub use crate::models::Node;
pub use validate::{
    is_happ_auto_placeholder, normalize_node_outbound, normalize_outbound_for_runtime,
    repair_node_outbound_from_link, validate_node_outbound,
};

pub const MAX_IMPORT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_IMPORT_LINES: usize = 20_000;
pub const MAX_IMPORT_NODES: usize = 20_000;

/// Python `LinkParseError(ValueError)`.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct LinkParseError {
    pub message: String,
}

impl LinkParseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<String> for LinkParseError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

// ── public API ──

/// Python `parse_links_text(text, allow_file_reference=False)`.
pub fn parse_links_text(
    text: &str,
    allow_file_reference: bool,
) -> Result<(Vec<Node>, Vec<String>), LinkParseError> {
    if text.len() > MAX_IMPORT_BYTES {
        return Ok((
            Vec::new(),
            vec![format!("Import data exceeds the {MAX_IMPORT_BYTES}-byte limit")],
        ));
    }
    let mut stripped = text.trim().trim_start_matches('\u{feff}').trim().to_string();
    let mut source_path: Option<PathBuf> = None;
    if allow_file_reference {
        match read_import_file_reference(&stripped) {
            Ok(Some((file_text, path))) => {
                source_path = Some(path);
                stripped = file_text.trim().to_string();
            }
            Ok(None) => {}
            Err(exc) => return Ok((Vec::new(), vec![exc.message])),
        }
    }
    if stripped.starts_with('{') || stripped.starts_with('[') || stripped.starts_with('"') {
        match json_import::parse_json_nodes_text(&stripped) {
            Ok(result) => return Ok(result),
            Err(exc) => {
                if stripped.starts_with('{') {
                    if let Some(json_lines) = json_import::try_parse_json_lines(&stripped) {
                        return Ok(json_lines);
                    }
                    return Ok((Vec::new(), vec![format!("JSON: {exc}")]));
                }
                if !stripped.to_lowercase().starts_with("[interface]") {
                    return Ok((Vec::new(), vec![format!("JSON: {exc}")]));
                }
            }
        }
    }
    if clash::looks_like_clash_yaml(&stripped) {
        return match clash::parse_clash_yaml_nodes_text(&stripped) {
            Ok(result) => Ok(result),
            Err(exc) => Ok((Vec::new(), vec![format!("Clash YAML: {exc}")])),
        };
    }
    // OPENVPN-BRANCH: Python delegates `.ovpn` configs to
    // `openvpn_import.parse_openvpn_config` before the WireGuard INI check.
    let source_path_str = source_path.as_ref().map(|p| p.to_string_lossy().to_string());
    if let Some(result) = openvpn_hook::try_parse_openvpn_text(&stripped, source_path_str.as_deref()) {
        return match result {
            Ok((nodes, errors)) => Ok((nodes, errors)),
            Err(exc) => Ok((Vec::new(), vec![exc.message])),
        };
    }

    let lowered = stripped.to_lowercase();
    if lowered.contains("[interface]") && lowered.contains("[peer]") {
        return match wireguard::parse_wireguard_config(&stripped) {
            Ok(node) => Ok((vec![node], Vec::new())),
            Err(exc) => Ok((Vec::new(), vec![format!("Config: {exc}")])),
        };
    }

    if let Some(decoded) = helpers::decode_subscription_blob(&stripped) {
        return parse_links_text(&decoded, false);
    }
    let lines = helpers::split_subscription_entries(&stripped);
    if lines.len() > MAX_IMPORT_LINES {
        return Ok((
            Vec::new(),
            vec![format!("Import contains more than {MAX_IMPORT_LINES} non-empty lines")],
        ));
    }
    let mut nodes: Vec<Node> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        let index = index + 1;
        let result: Result<(), String> = (|| {
            if line.starts_with('{') || line.starts_with('[') || line.starts_with('"') {
                let (inner, inner_errors) = json_import::parse_json_nodes_text(line)
                    .map_err(|e| e.message)?;
                nodes.extend(inner);
                errors.extend(inner_errors.into_iter().map(|error| format!("Line {index}: {error}")));
            } else if let Some(decoded_line) = helpers::decode_subscription_blob(line) {
                let (inner, inner_errors) =
                    parse_links_text(&decoded_line, false).map_err(|e| e.message)?;
                nodes.extend(inner);
                errors.extend(inner_errors.into_iter().map(|error| format!("Line {index}: {error}")));
            } else {
                let mut node = parse_single(line).map_err(|e| e.message)?;
                schemes::apply_happ_server_metadata(&mut node, line);
                nodes.push(node);
            }
            if nodes.len() > MAX_IMPORT_NODES {
                return Err(format!("Import contains more than {MAX_IMPORT_NODES} nodes"));
            }
            Ok(())
        })();
        if let Err(message) = result {
            errors.push(format!("Line {index}: {message}"));
        }
    }

    Ok((nodes, errors))
}

/// Python `parse_single(raw)`.
pub fn parse_single(raw: &str) -> Result<Node, LinkParseError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(LinkParseError::new("empty input"));
    }

    if text.starts_with('{') {
        return json_import::parse_json_outbound_text(text);
    }
    if text.starts_with('[') {
        let (nodes, errors) = match json_import::parse_json_nodes_text(text) {
            Ok(result) => result,
            Err(_) => (Vec::new(), Vec::new()),
        };
        if let Some(first) = nodes.into_iter().next() {
            return Ok(first);
        }
        if !errors.is_empty() {
            let joined = errors
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ");
            return Err(LinkParseError::new(joined));
        }
        return wireguard::parse_wireguard_config(text);
    }

    if let Some((scheme_text, _)) = text.split_once("://") {
        if scheme_text.to_lowercase() == "ss" {
            return schemes::parse_shadowsocks(text);
        }
    }

    let parsed = helpers::PyUrlSplit::parse(text)
        .map_err(|e| LinkParseError::new(format!("invalid link: {}", e.message)))?;
    let scheme = parsed.scheme.to_lowercase();
    if scheme == "vless" {
        return schemes::parse_vless(text);
    }
    if scheme == "vmess" {
        return schemes::parse_vmess(text);
    }
    if scheme == "trojan" {
        return schemes::parse_trojan(text);
    }
    if scheme == "ss" {
        return schemes::parse_shadowsocks(text);
    }
    if scheme == "socks" || scheme == "socks5" {
        return schemes::parse_socks(text);
    }
    if scheme == "http" || scheme == "https" {
        return schemes::parse_http_proxy(text);
    }
    if matches!(scheme.as_str(), "naive" | "naive+https" | "naive+quic" | "quic") {
        return schemes::parse_naive_link(text, "");
    }
    if matches!(scheme.as_str(), "wireguard" | "wg" | "awg" | "warp") {
        return wireguard::parse_wireguard_like_link(text, &scheme);
    }
    if scheme == "hysteria" || scheme == "hy" {
        return schemes::parse_hysteria(text);
    }
    if scheme == "hysteria2" || scheme == "hy2" {
        return schemes::parse_hysteria2(text);
    }
    if scheme == "tuic" {
        return schemes::parse_tuic(text);
    }
    if scheme == "mieru" || scheme == "mierus" {
        return schemes::parse_mieru(text);
    }
    if scheme == "masque" {
        return schemes::parse_masque(text);
    }
    if scheme == "anytls" {
        return schemes::parse_anytls(text);
    }

    Err(LinkParseError::new(format!(
        "unsupported scheme: {}",
        if scheme.is_empty() { "unknown" } else { &scheme }
    )))
}

// ── file reference ──

const FILE_REFERENCE_EXTENSIONS: [&str; 6] = [".conf", ".ovpn", ".txt", ".json", ".yaml", ".yml"];

/// Python `_read_import_file_reference(text)` — accept paths copied from
/// Explorer/QML instead of treating them as links.
fn read_import_file_reference(text: &str) -> Result<Option<(String, PathBuf)>, LinkParseError> {
    if text.is_empty() || text.contains('\n') {
        return Ok(None);
    }
    let candidate_text = text.trim().trim_matches('"').to_string();
    let parsed = helpers::PyUrlSplit::parse(&candidate_text)?;
    let mut path_text = String::new();
    let candidate_bytes = candidate_text.as_bytes();
    if candidate_text.len() >= 3
        && candidate_bytes[1] == b':'
        && (candidate_bytes[2] == b'\\' || candidate_bytes[2] == b'/')
    {
        path_text = candidate_text.clone();
    } else if parsed.scheme.to_lowercase() == "file" {
        // A `file://host/share` target makes Windows open an SMB session and
        // authenticate to `host`, so remote hosts are never followed.
        if !parsed.netloc.is_empty() {
            return Ok(None);
        }
        path_text = helpers::url2pathname(&helpers::unquote(&parsed.path));
    } else if parsed.scheme.is_empty() {
        let candidate = candidate_text.clone();
        let lowered = candidate.to_lowercase();
        if FILE_REFERENCE_EXTENSIONS.iter().any(|ext| lowered.ends_with(ext)) {
            path_text = candidate;
        }
    }
    if path_text.is_empty() || path_text.starts_with("\\\\") || path_text.starts_with("//") {
        return Ok(None);
    }
    let path = PathBuf::from(&path_text);
    if !path.is_file() {
        return Ok(None);
    }
    let size = match std::fs::metadata(&path) {
        Ok(meta) => meta.len(),
        Err(exc) => {
            return Err(LinkParseError::new(format!(
                "Cannot inspect import file: {exc}"
            )))
        }
    };
    if size as usize > MAX_IMPORT_BYTES {
        return Err(LinkParseError::new(format!(
            "Import file exceeds the {MAX_IMPORT_BYTES}-byte limit"
        )));
    }
    let bytes = std::fs::read(&path).map_err(|exc| {
        LinkParseError::new(format!("Cannot inspect import file: {exc}"))
    })?;
    Ok(Some((String::from_utf8_lossy(&bytes).to_string(), path)))
}

// Silence unused imports until the openvpn hook lands.

/// Helper kept for `openvpn_hook.rs`: builds the single-node wrapper the
/// Python `.ovpn` branch produces (`_first_mapping(native.get("servers"))`).
#[allow(dead_code)]
pub(crate) fn openvpn_wrapper_node(
    native: &Value,
    dns_servers: &[String],
    profile_name: &str,
) -> Node {
    let first_server = json_import::first_mapping(native.get("servers"));
    let mut outbound = json!({
        "protocol": "openvpn",
        "singbox": native,
    });
    if !dns_servers.is_empty() {
        outbound["_dns"] = json!(dns_servers);
    }
    let mut node = Node::new();
    node.name = profile_name.to_string();
    node.scheme = "openvpn".to_string();
    node.server = helpers::py_str_or(first_server.get("server"), "");
    node.port = helpers::py_int_or(first_server.get("server_port"), 0).unwrap_or(0);
    node.link = serde_json::to_string(&outbound).unwrap_or_default();
    node.outbound = outbound;
    node
}

#[cfg(test)]
mod tests;
