//! OpenVPN client profile import (`openvpn_import.py`).
//!
//! Translates a client .ovpn profile to sing-box-extended OpenVPN options.
//! Only fields implemented by the extended core are emitted. Inline
//! ``<ca|cert|key|tls-auth|tls-crypt|tls-crypt-v2|auth-user-pass|askpass>``
//! blocks and ``<connection>`` blocks are extracted shlex-style, and
//! referenced file resources are restricted to the profile directory and
//! embedded so imported profiles remain portable.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{anyhow, bail, Result};
use regex::Regex;
use serde_json::{json, Map, Value};

use crate::openvpn_normalization::{
    normalize_auth_digest, normalize_data_cipher, normalize_tls_cipher_suites,
};

const INLINE_TAGS: [&str; 8] = [
    "ca",
    "cert",
    "key",
    "tls-auth",
    "tls-crypt",
    "tls-crypt-v2",
    "auth-user-pass",
    "askpass",
];

const UNSAFE_UNSUPPORTED_DIRECTIVES: [&str; 3] = ["http-proxy-user-pass", "pkcs12", "secret"];

// ── Python semantics helpers ──

/// Python `str.splitlines()` semantics (including \r, \v, \f and Unicode
/// line separators as boundaries, with \r\n consumed as a single break).
fn py_splitlines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let is_break = match c {
            '\n' | '\u{000B}' | '\u{000C}' | '\u{001C}' | '\u{001D}' | '\u{001E}'
            | '\u{0085}' | '\u{2028}' | '\u{2029}' => true,
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                true
            }
            _ => false,
        };
        if is_break {
            lines.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    lines.push(current);
    lines
}

// ── POSIX shlex tokenizer (mirrors CPython Lib/shlex.py, posix=True,
//    whitespace_split=True, comments disabled) ──

fn shlex_split_posix(input: &str) -> std::result::Result<Vec<String>, &'static str> {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Space,
        Word,
        Quote(char),
        Escape,
    }

    let is_ws = |c: char| matches!(c, ' ' | '\t' | '\r' | '\n');
    let mut tokens: Vec<String> = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    let mut state = State::Space;
    // escapedstate from CPython: 'a' (word) or the active double-quote char.
    let mut escaped_from: char = ' ';

    for c in input.chars() {
        match state {
            State::Space => {
                if is_ws(c) {
                    if !token.is_empty() || quoted {
                        tokens.push(std::mem::take(&mut token));
                        quoted = false;
                    }
                } else if c == '\\' {
                    escaped_from = 'a';
                    state = State::Escape;
                } else if c == '"' || c == '\'' {
                    quoted = true;
                    state = State::Quote(c);
                } else {
                    token.push(c);
                    state = State::Word;
                }
            }
            State::Word => {
                if is_ws(c) {
                    state = State::Space;
                    if !token.is_empty() || quoted {
                        tokens.push(std::mem::take(&mut token));
                        quoted = false;
                    }
                } else if c == '"' || c == '\'' {
                    quoted = true;
                    state = State::Quote(c);
                } else if c == '\\' {
                    escaped_from = 'a';
                    state = State::Escape;
                } else {
                    token.push(c);
                }
            }
            State::Quote(q) => {
                if c == q {
                    // posix: closing quote returns to word state, token continues.
                    state = State::Word;
                } else if q == '"' && c == '\\' {
                    // Only double quotes can escape; inside single quotes the
                    // backslash is literal.
                    escaped_from = '"';
                    state = State::Escape;
                } else {
                    token.push(c);
                }
            }
            State::Escape => {
                // In posix shells, only the quote itself or the escape
                // character may be escaped within quotes; any other char keeps
                // the quote literally before it.
                if escaped_from == '"' && c != '\\' && c != '"' {
                    token.push('"');
                }
                token.push(c);
                state = if escaped_from == '"' {
                    State::Quote('"')
                } else {
                    State::Word
                };
            }
        }
    }
    match state {
        State::Space => {
            if !token.is_empty() || quoted {
                tokens.push(token);
            }
        }
        State::Word => tokens.push(token),
        State::Quote(_) => return Err("No closing quotation"),
        State::Escape => return Err("No escaped character"),
    }
    Ok(tokens)
}

// ── Tagged block extraction ──

/// One line with its terminator flag (only '\n' terminates — matches the
/// regex `^`/`$` anchors used by the Python implementation).
fn split_lines_with_newlines(text: &str) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c == '\n' {
            out.push((std::mem::take(&mut current), true));
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        out.push((current, false));
    }
    out
}

/// Replace `^\\s*<tag>\\s*\\r?\\n(?P<body>.*?)^\\s*</tag>\\s*$` spans (one
/// combined left-to-right pass over all tags, mirroring the Python
/// `_INLINE_BLOCK_RE.sub` / `_CONNECTION_BLOCK_RE.sub` behavior).
///
/// For inline blocks the closing tag is matched case-sensitively against the
/// captured opening text (regex backreference semantics); the body is
/// `strip("\\r\\n")`ed and suffixed with a newline. For `<connection>` blocks
/// both sides are case-insensitive and the raw body is kept.
fn extract_tagged_blocks(
    text: &str,
    tags: &[&str],
    close_case_sensitive: bool,
) -> (String, Vec<(String, String)>) {
    let lines = split_lines_with_newlines(text);
    let mut kept: Vec<(String, bool)> = Vec::new();
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let (content, had_term) = &lines[i];
        let trimmed = content.trim();
        let mut matched_tag: Option<&str> = None;
        let mut captured: &str = "";
        if trimmed.len() >= 3 && trimmed.starts_with('<') && trimmed.ends_with('>') && *had_term {
            let inner = &trimmed[1..trimmed.len() - 1];
            for tag in tags {
                if inner.eq_ignore_ascii_case(tag) {
                    matched_tag = Some(tag);
                    captured = inner;
                    break;
                }
            }
        }
        if let Some(tag) = matched_tag {
            let close = format!("</{captured}>");
            let mut found: Option<usize> = None;
            for j in (i + 1)..lines.len() {
                let t2 = lines[j].0.trim();
                let ok = if close_case_sensitive {
                    t2 == close
                } else {
                    t2.eq_ignore_ascii_case(&close)
                };
                if ok {
                    found = Some(j);
                    break;
                }
            }
            if let Some(j) = found {
                let joined: String = lines[(i + 1)..j]
                    .iter()
                    .map(|(c, _)| c.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                let body = if close_case_sensitive {
                    let stripped = joined.trim_matches(|c| c == '\r' || c == '\n');
                    if stripped.is_empty() {
                        String::new()
                    } else {
                        format!("{stripped}\n")
                    }
                } else if j > i + 1 {
                    format!("{joined}\n")
                } else {
                    String::new()
                };
                blocks.push((tag.to_lowercase(), body));
                // The match ends before the closing line's terminator, so the
                // newline stays behind as a blank line (same as re.sub).
                if lines[j].1 {
                    kept.push((String::new(), true));
                }
                i = j + 1;
                continue;
            }
        }
        kept.push((content.clone(), *had_term));
        i += 1;
    }
    let leftover: String = kept
        .iter()
        .map(|(c, t)| if *t { format!("{c}\n") } else { c.clone() })
        .collect();
    (leftover, blocks)
}

// ── Directive parsing ──

fn parse_directives(text: &str) -> Result<Vec<(String, Vec<String>)>> {
    let mut directives: Vec<(String, Vec<String>)> = Vec::new();
    for (index, raw_line) in py_splitlines(text).into_iter().enumerate() {
        let line_number = index + 1;
        let stripped = raw_line.trim();
        if stripped.is_empty() || stripped.starts_with('#') || stripped.starts_with(';') {
            continue;
        }
        let tokens = shlex_split_posix(stripped)
            .map_err(|exc| anyhow!("OpenVPN line {line_number}: {exc}"))?;
        if tokens.is_empty() {
            continue;
        }
        let key = tokens[0]
            .trim_start_matches('-')
            .trim()
            .to_lowercase();
        directives.push((key, tokens[1..].to_vec()));
    }
    Ok(directives)
}

type DirectiveMap = HashMap<String, Vec<Vec<String>>>;

fn last_values<'a>(by_key: &'a DirectiveMap, key: &str) -> Option<&'a Vec<String>> {
    by_key.get(key).and_then(|values| values.last())
}

fn last_values_cloned(by_key: &DirectiveMap, key: &str) -> Vec<String> {
    last_values(by_key, key).cloned().unwrap_or_default()
}

fn last_arg(by_key: &DirectiveMap, key: &str) -> String {
    last_values(by_key, key)
        .and_then(|values| values.first())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

// ── Small validators ──

fn positive_integer(value: &str, directive: &str, allow_zero: bool) -> Result<i64> {
    let parsed: i64 = value
        .trim()
        .parse()
        .map_err(|_| anyhow!("invalid OpenVPN `{directive}` value: {value}"))?;
    if parsed < if allow_zero { 0 } else { 1 } {
        bail!("invalid OpenVPN `{directive}` value: {value}");
    }
    Ok(parsed)
}

fn positive_port(value: &str) -> Result<i64> {
    let parsed: i64 = value
        .trim()
        .parse()
        .map_err(|_| anyhow!("invalid OpenVPN remote port `{value}`"))?;
    if parsed <= 0 || parsed > 65535 {
        bail!("invalid OpenVPN remote port `{value}`");
    }
    Ok(parsed)
}

fn normalize_proto(value: &str) -> Result<String> {
    let proto = if value.is_empty() {
        "udp".to_string()
    } else {
        value.trim().to_lowercase()
    };
    if proto.starts_with("udp") {
        return Ok("udp".into());
    }
    if [
        "tcp",
        "tcp-client",
        "tcp4",
        "tcp4-client",
        "tcp6",
        "tcp6-client",
    ]
    .contains(&proto.as_str())
    {
        return Ok("tcp".into());
    }
    bail!("unsupported OpenVPN transport `{value}`");
}

fn key_direction(value: &str) -> Result<i64> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "0" => Ok(0),
        "1" => Ok(1),
        "bidirectional" | "bi" | "-1" => Ok(-1),
        _ => bail!("invalid OpenVPN key direction `{value}`"),
    }
}

fn duration_seconds(value: &str, directive: &str) -> Result<String> {
    let seconds: f64 = value
        .trim()
        .parse()
        .map_err(|_| anyhow!("invalid OpenVPN `{directive}` value: {value}"))?;
    let seconds = seconds as i64;
    if seconds < 0 {
        bail!("invalid OpenVPN `{directive}` value: {value}");
    }
    Ok(format!("{seconds}s"))
}

fn verify_name_mode(value: &str) -> Result<String> {
    let normalized = if value.is_empty() {
        "name".to_string()
    } else {
        value.trim().to_lowercase()
    };
    if ["name", "name-prefix", "name-suffix"].contains(&normalized.as_str()) {
        return Ok(normalized);
    }
    if normalized == "subject" {
        bail!("OpenVPN verify-x509-name mode `subject` is not supported by the bundled core");
    }
    bail!("invalid OpenVPN verify-x509-name mode `{value}`");
}

fn select_cipher(by_key: &DirectiveMap) -> String {
    let mut candidates: Vec<String> = Vec::new();
    for key in ["data-ciphers", "ncp-ciphers", "cipher", "data-ciphers-fallback"] {
        let value = last_arg(by_key, key);
        if !value.is_empty() {
            candidates.extend(value.split(':').map(String::from));
        }
    }
    for value in candidates {
        let normalized = normalize_data_cipher(&Value::String(value));
        if !normalized.is_empty() {
            return normalized;
        }
    }
    String::new()
}

// ── Profile-directory file resources ──

fn lexical_resolve(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn read_profile_resource(value: &str, source_path: Option<&Path>) -> Result<String> {
    let Some(source_path) = source_path else {
        bail!(
            "OpenVPN resource `{value}` can only be resolved when importing an .ovpn file"
        );
    };
    let base = source_path
        .canonicalize()
        .map_err(|e| anyhow!("cannot resolve profile path: {e}"))?;
    let base = base.parent().map(Path::to_path_buf).unwrap_or(base);

    let stripped = value.trim().trim_matches(|c| c == '"' || c == '\'');
    let mut candidate = PathBuf::from(stripped);
    if !candidate.is_absolute() {
        candidate = base.join(candidate);
    }
    let candidate = candidate
        .canonicalize()
        .unwrap_or_else(|_| lexical_resolve(&candidate));

    let base_s = base.to_string_lossy().to_lowercase();
    let cand_s = candidate.to_string_lossy().to_lowercase();
    let inside = cand_s == base_s
        || cand_s.starts_with(&format!("{base_s}{}", std::path::MAIN_SEPARATOR))
        || (base_s.ends_with(std::path::MAIN_SEPARATOR) && cand_s.starts_with(&base_s));
    if !inside {
        bail!("OpenVPN resource must be inside the profile directory: {value}");
    }
    let name = candidate
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !candidate.is_file() {
        bail!("OpenVPN resource was not found: {name}");
    }
    let size = fs::metadata(&candidate)
        .map_err(|e| anyhow!("cannot stat OpenVPN resource {}: {e}", candidate.display()))?
        .len();
    if size > 1024 * 1024 {
        bail!("OpenVPN resource is too large: {name}");
    }
    let bytes = fs::read(&candidate)
        .map_err(|e| anyhow!("cannot read OpenVPN resource {}: {e}", candidate.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

// ── Comment-token helpers ──

fn openvpn_comment_tokens(text: &str, prefix: &str) -> Result<Vec<String>> {
    let marker = format!("# {prefix} ");
    let mut value = String::new();
    for line in py_splitlines(text) {
        let stripped = line.trim();
        if stripped.starts_with(&marker) {
            value = stripped[marker.len()..].trim().to_string();
        }
    }
    if value.is_empty() {
        return Ok(Vec::new());
    }
    shlex_split_posix(&value)
        .map_err(|exc| anyhow!("invalid OpenVPN `{prefix}` comment: {exc}"))
}

fn openvpn_key_password(text: &str, inline: &BTreeMap<String, String>) -> Result<String> {
    let inline_password = inline
        .get("askpass")
        .map(|body| {
            py_splitlines(body)
                .into_iter()
                .map(|line| line.trim().to_string())
                .find(|line| !line.is_empty())
        })
        .flatten()
        .unwrap_or_default();
    if !inline_password.is_empty() {
        return Ok(inline_password);
    }
    let values = openvpn_comment_tokens(text, "lumen-key-password")?;
    Ok(values.into_iter().next().unwrap_or_default())
}

// ── Proxy directives ──

fn parse_openvpn_proxy(
    text: &str,
    by_key: &DirectiveMap,
    source_path: Option<&Path>,
) -> Result<Option<Value>> {
    let obfs = openvpn_comment_tokens(text, "lumen-proxy")?;
    let mut auth = openvpn_comment_tokens(text, "lumen-proxy-auth")?;
    let http = last_values_cloned(by_key, "http-proxy");
    let socks = last_values_cloned(by_key, "socks-proxy");
    if let Some(first) = obfs.first() {
        if ["obfs2", "obfs2-legacy", "obfs3"].contains(&first.to_lowercase().as_str()) {
            bail!(
                "OpenVPN proxy `{first}` requires the Android obfs relay and is not supported on Windows"
            );
        }
    }
    let (proxy_type, endpoint) = if !http.is_empty() {
        ("http", http)
    } else if !socks.is_empty() {
        ("socks", socks)
    } else {
        return Ok(None);
    };
    if endpoint[0].trim().is_empty() {
        bail!("OpenVPN proxy `{proxy_type}` is missing a server address");
    }
    let port = positive_port(endpoint.get(1).map(String::as_str).unwrap_or(""))?;
    if endpoint.len() > 2 {
        let credentials = read_profile_resource(&endpoint[2], source_path)?;
        let lines: Vec<String> = py_splitlines(&credentials)
            .into_iter()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();
        if !lines.is_empty() {
            auth = lines.into_iter().take(2).collect();
        }
    }
    let mut result = Map::new();
    result.insert("type".into(), json!(proxy_type));
    result.insert("server".into(), json!(endpoint[0].trim()));
    result.insert("server_port".into(), json!(port));
    if let Some(username) = auth.first() {
        result.insert("username".into(), json!(username.clone()));
    }
    if auth.len() > 1 {
        result.insert("password".into(), json!(auth[1].clone()));
    }
    Ok(Some(Value::Object(result)))
}

// ── Public API ──

fn looks_regexes() -> &'static (Regex, Regex, Regex) {
    static REGEXES: OnceLock<(Regex, Regex, Regex)> = OnceLock::new();
    REGEXES.get_or_init(|| {
        (
            Regex::new(r"(?m)^\s*(?:--)?remote\s+\S+").unwrap(),
            Regex::new(r"(?m)^\s*(?:--)?client(?:\s|$)").unwrap(),
            Regex::new(r"(?m)^\s*(?:--)?tls-client(?:\s|$)").unwrap(),
        )
    })
}

pub fn looks_like_openvpn_config(text: &str) -> bool {
    let lowered = text.to_lowercase();
    let (remote_re, client_re, tls_client_re) = looks_regexes();
    remote_re.is_match(&lowered)
        && (client_re.is_match(&lowered) || tls_client_re.is_match(&lowered) || lowered.contains("<ca>"))
}

pub fn parse_openvpn_config(
    text: &str,
    source_path: Option<&std::path::Path>,
) -> Result<(Value, Vec<String>, String)> {
    if !looks_like_openvpn_config(text) {
        bail!("not an OpenVPN client profile");
    }

    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let (directives_text, inline_blocks) = extract_tagged_blocks(&text, &INLINE_TAGS, true);
    let mut inline: BTreeMap<String, String> = BTreeMap::new();
    for (tag, body) in inline_blocks {
        inline.insert(tag, body);
    }
    let (directives_text, connection_blocks) =
        extract_tagged_blocks(&directives_text, &["connection"], false);

    let directives = parse_directives(&directives_text)?;
    let mut by_key: DirectiveMap = HashMap::new();
    for (key, values) in directives {
        by_key.entry(key).or_default().push(values);
    }

    let mut connection_maps: Vec<DirectiveMap> = Vec::new();
    for (_, block) in &connection_blocks {
        let mut block_map: DirectiveMap = HashMap::new();
        for (key, values) in parse_directives(block)? {
            block_map.entry(key).or_default().push(values);
        }
        connection_maps.push(block_map);
    }

    for key in UNSAFE_UNSUPPORTED_DIRECTIVES {
        if by_key.contains_key(key) || connection_maps.iter().any(|map| map.contains_key(key)) {
            bail!("OpenVPN directive `{key}` is not supported by sing-box extended");
        }
    }

    let dev = last_arg(&by_key, "dev").to_lowercase();
    if dev.starts_with("tap") {
        bail!("OpenVPN TAP profiles are not supported; a TUN profile is required");
    }
    for key in ["compress", "comp-lzo"] {
        let value = last_arg(&by_key, key).to_lowercase();
        if !value.is_empty() && !["no", "disable", "stub", "stub-v2"].contains(&value.as_str()) {
            bail!("OpenVPN compression `{value}` is not supported");
        }
    }

    let proto_raw = last_arg(&by_key, "proto");
    let global_proto = normalize_proto(if proto_raw.is_empty() { "udp" } else { &proto_raw })?;
    let mut remotes: Vec<(String, Value)> = Vec::new();

    fn append_remotes(
        source: &DirectiveMap,
        default_proto: &str,
        remotes: &mut Vec<(String, Value)>,
    ) -> Result<()> {
        let local_raw = last_arg(source, "proto");
        let local_proto = normalize_proto(if local_raw.is_empty() {
            default_proto
        } else {
            &local_raw
        })?;
        if let Some(remotes_list) = source.get("remote") {
            for values in remotes_list {
                if values.is_empty() {
                    continue;
                }
                let server = values[0].trim().to_string();
                if server.is_empty() {
                    continue;
                }
                let port_text = values
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| "1194".to_string());
                let port = positive_port(&port_text)?;
                let remote_proto = if values.len() > 2 {
                    normalize_proto(&values[2])?
                } else {
                    local_proto.clone()
                };
                remotes.push((
                    remote_proto.clone(),
                    json!({
                        "server": server,
                        "server_port": port,
                        "network": remote_proto,
                    }),
                ));
            }
        }
        Ok(())
    }

    append_remotes(&by_key, &global_proto, &mut remotes)?;
    for connection in &connection_maps {
        append_remotes(connection, &global_proto, &mut remotes)?;
    }
    if remotes.is_empty() {
        bail!("OpenVPN profile does not contain a usable `remote` server");
    }
    let proto = remotes[0].0.clone();
    let servers: Vec<Value> = remotes.into_iter().map(|(_, server)| server).collect();

    let mut native = Map::new();
    native.insert("type".into(), json!("openvpn"));
    native.insert("tag".into(), json!("proxy"));
    native.insert("system".into(), json!(false));
    native.insert("name".into(), json!("openvpn0"));
    native.insert("servers".into(), Value::Array(servers.clone()));
    native.insert("proto".into(), json!(proto));
    if by_key.contains_key("remote-random") {
        native.insert("remote_random".into(), json!(true));
    }

    // Continue with the common profile fields below.
    finish_openvpn_profile(
        Value::Object(native),
        &inline,
        &by_key,
        &text,
        source_path,
        &servers,
    )
}

fn finish_openvpn_profile(
    mut native: Value,
    inline: &BTreeMap<String, String>,
    by_key: &DirectiveMap,
    text: &str,
    source_path: Option<&Path>,
    servers: &[Value],
) -> Result<(Value, Vec<String>, String)> {
    let native_obj = native.as_object_mut().unwrap();

    if by_key.contains_key("auth-user-pass") || inline.contains_key("auth-user-pass") {
        native_obj.insert("lumen_requires_user_auth".into(), json!(true));
    }

    if let Some(proxy) = parse_openvpn_proxy(text, by_key, source_path)? {
        native_obj.insert("lumen_proxy".into(), proxy);
    }

    let cipher = select_cipher(by_key);
    if !cipher.is_empty() {
        native_obj.insert("cipher".into(), json!(cipher));
    }
    let auth = last_arg(by_key, "auth");
    if !auth.is_empty() && !auth.eq_ignore_ascii_case("none") {
        let normalized_auth = normalize_auth_digest(&Value::String(auth.clone()));
        if !normalized_auth.is_empty() {
            native_obj.insert("auth".into(), json!(normalized_auth));
        }
    }

    for (directive, native_key) in [("mssfix", "mss_fix"), ("fragment", "fragment")] {
        let value = last_arg(by_key, directive);
        if !value.is_empty() {
            let parsed = positive_integer(&value, directive, directive == "mssfix")?;
            if directive == "mssfix" && parsed == 0 {
                native_obj.insert("mss_fix_disabled".into(), json!(true));
            } else {
                native_obj.insert(native_key.into(), json!(parsed));
            }
        }
    }
    if by_key.contains_key("explicit-exit-notify") {
        let mut value = last_arg(by_key, "explicit-exit-notify");
        if value.is_empty() {
            value = "1".to_string();
        }
        let parsed = positive_integer(&value, "explicit-exit-notify", true)?;
        native_obj.insert("explicit_exit_notify".into(), json!(parsed));
    }

    let mut credentials = inline.get("auth-user-pass").cloned().unwrap_or_default();
    let auth_user_pass = last_values_cloned(by_key, "auth-user-pass");
    if credentials.is_empty() && !auth_user_pass.is_empty() {
        credentials = read_profile_resource(&auth_user_pass[0], source_path)?;
    }
    if !credentials.is_empty() {
        let lines: Vec<String> = py_splitlines(&credentials)
            .into_iter()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();
        if !lines.is_empty() {
            native_obj.insert("username".into(), json!(lines[0].clone()));
        }
        if lines.len() > 1 {
            native_obj.insert("password".into(), json!(lines[1].clone()));
        }
    }

    let mut key_password = openvpn_key_password(text, inline)?;
    let askpass_values = last_values_cloned(by_key, "askpass");
    if key_password.is_empty() && !askpass_values.is_empty() {
        key_password = read_profile_resource(&askpass_values[0], source_path)?.trim().to_string();
    }
    if !key_password.is_empty() {
        let first = py_splitlines(&key_password)
            .into_iter()
            .next()
            .map(|line| line.trim().to_string())
            .unwrap_or_default();
        native_obj.insert("key_password".into(), json!(first));
    }

    for (directive, native_key) in
        [("tls-auth", "tls_auth"), ("tls-crypt", "tls_crypt"), ("tls-crypt-v2", "tls_crypt")]
    {
        let mut content = inline.get(directive).cloned().unwrap_or_default();
        let values = last_values_cloned(by_key, directive);
        if content.is_empty() && !values.is_empty() {
            content = read_profile_resource(&values[0], source_path)?;
        }
        if !content.is_empty() {
            native_obj.insert(native_key.into(), json!(content.clone()));
        }
        if directive == "tls-auth" && values.len() > 1 {
            let direction = key_direction(&values[1])?;
            native_obj.insert("key_direction".into(), json!(direction));
        }
        if directive == "tls-auth" && !content.is_empty() && values.len() <= 1 {
            // Inline tls-auth without an explicit direction defaults to -1.
            native_obj
                .entry("key_direction".to_string())
                .or_insert_with(|| json!(-1));
        }
        if directive == "tls-crypt-v2" && (!content.is_empty() || !values.is_empty()) {
            native_obj.insert("tls_crypt_v2".into(), json!(true));
        }
    }

    let direction = last_arg(by_key, "key-direction");
    if !direction.is_empty() {
        let parsed = key_direction(&direction)?;
        native_obj.insert("key_direction".into(), json!(parsed));
    }

    for (directive, native_key) in
        [("connect-retry", "reconnect_delay"), ("ping", "ping_interval"), ("ping-restart", "ping_restart")]
    {
        let value = last_arg(by_key, directive);
        if !value.is_empty() {
            let seconds = duration_seconds(&value, directive)?;
            native_obj.insert(native_key.into(), json!(seconds));
        }
    }

    let keepalive = last_values_cloned(by_key, "keepalive");
    if !keepalive.is_empty() {
        let seconds = duration_seconds(&keepalive[0], "keepalive")?;
        native_obj
            .entry("ping_interval".to_string())
            .or_insert_with(|| json!(seconds));
    }
    if keepalive.len() > 1 {
        let seconds = duration_seconds(&keepalive[1], "keepalive")?;
        native_obj
            .entry("ping_restart".to_string())
            .or_insert_with(|| json!(seconds));
    }

    let mut tls = Map::new();
    for (directive, native_key) in [("cert", "certificate"), ("key", "key"), ("ca", "ca")] {
        let mut content = inline.get(directive).cloned().unwrap_or_default();
        let values = last_values_cloned(by_key, directive);
        if content.is_empty() && !values.is_empty() {
            content = read_profile_resource(&values[0], source_path)?;
        }
        if !content.is_empty() {
            tls.insert(native_key.into(), json!(content));
        }
    }
    // The bundled core maps this field to Go TLS <=1.2 cipher suites;
    // tls-ciphersuites is a TLS 1.3 OpenVPN option and must not be copied here.
    let tls_ciphers =
        normalize_tls_cipher_suites(&Value::String(last_arg(by_key, "tls-cipher")));
    if !tls_ciphers.is_empty() {
        tls.insert("cipher_suites".into(), json!(tls_ciphers));
    }
    let verify_values = last_values_cloned(by_key, "verify-x509-name");
    if !verify_values.is_empty() {
        tls.insert("verify_x509_name".into(), json!(verify_values[0].clone()));
        if verify_values.len() > 1 {
            let mode = verify_name_mode(&verify_values[1])?;
            tls.insert("verify_x509_name_mode".into(), json!(mode));
        }
    }
    let remote_cert_tls = last_arg(by_key, "remote-cert-tls").to_lowercase();
    if remote_cert_tls == "server" || remote_cert_tls == "client" {
        tls.insert("remote_certificate_tls".into(), json!(remote_cert_tls));
    }
    let has_ca = tls
        .get("ca")
        .and_then(Value::as_str)
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    if !has_ca {
        bail!("OpenVPN profile does not contain a CA certificate supported by this core");
    }
    native_obj.insert("tls".into(), Value::Object(tls));

    let mut dns_servers: Vec<String> = Vec::new();
    if let Some(entries) = by_key.get("dhcp-option") {
        for values in entries {
            if values.len() >= 2 && (values[0].to_uppercase() == "DNS" || values[0].to_uppercase() == "DNS6")
            {
                let address = values[1].trim().to_string();
                if !address.is_empty() && !dns_servers.contains(&address) {
                    dns_servers.push(address);
                }
            }
        }
    }

    let profile_name = match source_path {
        Some(path) => path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default(),
        None => format!(
            "OpenVPN {}",
            servers[0]
                .get("server")
                .and_then(Value::as_str)
                .unwrap_or("")
        ),
    };
    Ok((native, dns_servers, profile_name))
}

pub fn openvpn_requires_user_auth(text: &str) -> bool {
    let (directives_text, blocks) = extract_tagged_blocks(text, &INLINE_TAGS, true);
    if blocks.iter().any(|(tag, _)| tag == "auth-user-pass") {
        return true;
    }
    for raw_line in py_splitlines(&directives_text) {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Ok(tokens) = shlex_split_posix(line) else {
            continue;
        };
        if tokens
            .first()
            .map(|first| {
                first
                    .trim_start_matches('-')
                    .trim()
                    .to_lowercase()
                    == "auth-user-pass"
            })
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const INLINE_CA_PROFILE: &str = "client\ndev tun\nproto udp\nremote 1.2.3.4 1194\n<ca>\n-----BEGIN CERTIFICATE-----\nMIIBkTCB+wIJAKl\n-----END CERTIFICATE-----\n</ca>\ncipher AES-256-GCM\nauth SHA256\nmssfix 0\nkeepalive 10 60\nremote-random\n";

    #[test]
    fn test_shlex_tokenizer_matches_python() {
        let cases: Vec<(&str, Vec<&str>)> = vec![
            ("remote 1.2.3.4 1194 udp", vec!["remote", "1.2.3.4", "1194", "udp"]),
            ("http-proxy \"my host\" 8080 auth.txt", vec!["http-proxy", "my host", "8080", "auth.txt"]),
            ("http-proxy 'my host' 8080", vec!["http-proxy", "my host", "8080"]),
            ("a\\ b c", vec!["a b", "c"]),
            ("key \"va\\\"lue\"", vec!["key", "va\"lue"]),
            ("\"a\\bc\" d", vec!["a\"bc", "d"]),
            ("plain'quoted'mix", vec!["plainquotedmix"]),
            ("a \\\\ b", vec!["a", "\\", "b"]),
            ("empty \"\" token", vec!["empty", "", "token"]),
            ("# lumen-proxy obfs2 --param \"x y\"", vec!["#", "lumen-proxy", "obfs2", "--param", "x y"]),
            ("<ca>", vec!["<ca>"]),
            ("remote\twith\ttabs", vec!["remote", "with", "tabs"]),
            ("  spaced   out  ", vec!["spaced", "out"]),
            ("a#b", vec!["a#b"]),
            ("x \"a b\" \"c\"d\"e\" f", vec!["x", "a b", "cde", "f"]),
            ("---remote", vec!["---remote"]),
            ("auth-user-pass \"\"", vec!["auth-user-pass", ""]),
            ("a\"b\"c", vec!["abc"]),
            ("\\x41", vec!["x41"]),
            ("val\\--flag", vec!["val--flag"]),
        ];
        for (input, expected) in cases {
            assert_eq!(shlex_split_posix(input).unwrap(), expected, "input: {input:?}");
        }
        for bad in ["unterminated \"quote", "\"unclosed", "'unclosed", "trailing\\", "'a\\'b'"] {
            assert!(shlex_split_posix(bad).is_err(), "input: {bad:?}");
        }
    }

    #[test]
    fn test_looks_like_openvpn_config() {
        assert!(looks_like_openvpn_config(INLINE_CA_PROFILE));
        assert!(looks_like_openvpn_config("--remote host 1194\n--tls-client\n"));
        assert!(looks_like_openvpn_config("remote host 1194\n<ca>\nX\n</ca>\n"));
        assert!(!looks_like_openvpn_config("remote host 1194\n"));
        assert!(!looks_like_openvpn_config("just some text"));
    }

    #[test]
    fn test_parse_inline_ca_profile() {
        let (native, dns_servers, profile_name) =
            parse_openvpn_config(INLINE_CA_PROFILE, None).unwrap();
        assert_eq!(native["type"], "openvpn");
        assert_eq!(native["tag"], "proxy");
        assert_eq!(native["system"], false);
        assert_eq!(native["name"], "openvpn0");
        assert_eq!(native["proto"], "udp");
        assert_eq!(native["remote_random"], true);
        assert_eq!(
            native["servers"],
            json!([{"server": "1.2.3.4", "server_port": 1194, "network": "udp"}])
        );
        assert_eq!(native["cipher"], "AES-256-GCM");
        assert_eq!(native["auth"], "SHA256");
        assert_eq!(native["mss_fix_disabled"], true);
        assert_eq!(native["ping_interval"], "10s");
        assert_eq!(native["ping_restart"], "60s");
        assert!(native["tls"]["ca"].as_str().unwrap().contains("BEGIN CERTIFICATE"));
        assert!(dns_servers.is_empty());
        assert_eq!(profile_name, "OpenVPN 1.2.3.4");
    }

    #[test]
    fn test_parse_rejects_unsupported_profiles() {
        assert!(parse_openvpn_config("remote x 1194\n", None).is_err());
        assert!(parse_openvpn_config("remote x 1194\nclient\ndev tap\n<ca>\nC\n</ca>\n", None).is_err());
        assert!(parse_openvpn_config("remote x 1194\nclient\ncomp-lzo yes\n<ca>\nC\n</ca>\n", None).is_err());
        assert!(parse_openvpn_config("remote x 1194\nclient\npkcs12 f.p12\n<ca>\nC\n</ca>\n", None).is_err());
        assert!(parse_openvpn_config(
            "remote x 1194\nclient\nverify-x509-name host subject\n<ca>\nC\n</ca>\n",
            None
        )
        .is_err());
        assert!(parse_openvpn_config("remote x 1194\nclient\n<cert>\nC\n</cert>\n", None).is_err());
    }

    #[test]
    fn test_requires_user_auth() {
        assert!(openvpn_requires_user_auth(
            "remote x 1194\nclient\n<auth-user-pass>\nu\np\n</auth-user-pass>\n"
        ));
        assert!(openvpn_requires_user_auth("remote x 1194\nclient\nauth-user-pass cred.txt\n"));
        assert!(openvpn_requires_user_auth("remote x 1194\nclient\n--auth-user-pass\n"));
        // An auth-user-pass block swallowed by an unterminated <ca> block must
        // not be detected (single combined pass mirrors the Python regex).
        assert!(!openvpn_requires_user_auth(
            "<ca>\n<auth-user-pass>\nu\n</auth-user-pass>\n</ca>\n"
        ));
        assert!(!openvpn_requires_user_auth("remote x 1194\nclient\n"));
        // Malformed quoting lines are skipped, not fatal.
        assert!(!openvpn_requires_user_auth(
            "remote x 1194\nclient\n\"unterminated auth-user-pass\n"
        ));
    }

    #[test]
    fn test_parse_connection_blocks_and_options() {
        let text = "client\ntls-client\n<connection>\nremote a.example.com 1195 tcp\n</connection>\n<connection>\nremote 2.3.4.5 1196\n</connection>\n<ca>\nCA\n</ca>\nfragment 1400\nexplicit-exit-notify\nkey-direction 1\n<tls-auth>\nTA\n</tls-auth>\ncompose-x\n";
        let (native, dns, name) = parse_openvpn_config(text, None).unwrap();
        assert_eq!(native["proto"], "tcp");
        assert_eq!(
            native["servers"],
            json!([
                {"server": "a.example.com", "server_port": 1195, "network": "tcp"},
                // A connection block without its own `proto` inherits the
                // global default (udp here).
                {"server": "2.3.4.5", "server_port": 1196, "network": "udp"}
            ])
        );
        assert_eq!(native["fragment"], 1400);
        assert_eq!(native["explicit_exit_notify"], 1);
        assert_eq!(native["key_direction"], 1);
        assert_eq!(native["tls_auth"], "TA\n");
        assert!(dns.is_empty());
        assert_eq!(name, "OpenVPN a.example.com");
    }

    #[test]
    fn test_parse_inline_auth_and_askpass_and_dhcp_dns() {
        let text = "client\nremote h 1194\n<ca>\nCA\n</ca>\n<auth-user-pass>\n  myuser  \n  mypass  \n</auth-user-pass>\n<askpass>\nsecret phrase\n</askpass>\ndhcp-option DNS 1.1.1.1\ndhcp-option DNS6 2606:4700:4700::1111\ndhcp-option DNS 1.1.1.1\ndhcp-option DOMAIN example.com\n";
        let (native, dns_servers, _) = parse_openvpn_config(text, None).unwrap();
        assert_eq!(native["lumen_requires_user_auth"], true);
        assert_eq!(native["username"], "myuser");
        assert_eq!(native["password"], "mypass");
        assert_eq!(native["key_password"], "secret phrase");
        assert_eq!(dns_servers, vec!["1.1.1.1", "2606:4700:4700::1111"]);
    }

    #[test]
    fn test_profile_resource_resolution() {
        let dir = std::env::temp_dir().join(format!("lumen_ovpn_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let ca_path = dir.join("ca.crt");
        fs::write(&ca_path, "CA FILE").unwrap();
        let profile = dir.join("client.ovpn");
        fs::write(&profile, "client\nremote h 1194\nca ca.crt\n").unwrap();
        let text = "client\nremote h 1194\nca ca.crt\n";
        let (native, _, name) = parse_openvpn_config(text, Some(&profile)).unwrap();
        assert_eq!(native["tls"]["ca"], "CA FILE");
        assert_eq!(name, "client");

        // Outside the profile directory is rejected. Forward slashes keep the
        // path intact (shlex consumes backslashes as escapes, like Python).
        let outside = std::env::temp_dir().join("outside_ca.crt");
        fs::write(&outside, "X").unwrap();
        let outside_ref = outside.display().to_string().replace('\\', "/");
        let err = parse_openvpn_config(
            &format!("client\nremote h 1194\nca {}\n", outside_ref),
            Some(&profile),
        )
        .unwrap_err()
        .to_string();
        assert_eq!(
            err,
            format!("OpenVPN resource must be inside the profile directory: {outside_ref}")
        );
        // Missing file.
        let err = parse_openvpn_config(
            "client\nremote h 1194\nca missing.crt\n",
            Some(&profile),
        )
        .unwrap_err()
        .to_string();
        assert_eq!(err, "OpenVPN resource was not found: missing.crt");
        // Inline text without a source path cannot resolve file resources.
        let err = parse_openvpn_config("client\nremote h 1194\nca ca.crt\n", None)
            .unwrap_err()
            .to_string();
        assert_eq!(
            err,
            "OpenVPN resource `ca.crt` can only be resolved when importing an .ovpn file"
        );
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_file(&outside);
        let _ = std::io::stdout().flush();
    }

    #[test]
    fn test_proxy_directives() {
        let text = "client\nremote h 1194\nhttp-proxy 5.6.7.8 8080\n<ca>\nCA\n</ca>\n";
        let (native, _, _) = parse_openvpn_config(text, None).unwrap();
        assert_eq!(
            native["lumen_proxy"],
            json!({"type": "http", "server": "5.6.7.8", "server_port": 8080})
        );

        // socks-proxy via lumen-proxy-auth comment tokens.
        let text = "client\nremote h 1194\nsocks-proxy 5.6.7.8 1080\n# lumen-proxy-auth user pass\n<ca>\nCA\n</ca>\n";
        let (native, _, _) = parse_openvpn_config(text, None).unwrap();
        assert_eq!(
            native["lumen_proxy"],
            json!({"type": "socks", "server": "5.6.7.8", "server_port": 1080, "username": "user", "password": "pass"})
        );

        // obfs proxy is rejected.
        let err = parse_openvpn_config(
            "client\nremote h 1194\n# lumen-proxy obfs3\nhttp-proxy 1.2.3.4 80\n<ca>\nCA\n</ca>\n",
            None,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(
            err,
            "OpenVPN proxy `obfs3` requires the Android obfs relay and is not supported on Windows"
        );

        // Missing port.
        let err = parse_openvpn_config("client\nremote h 1194\nhttp-proxy 1.2.3.4\n<ca>\nCA\n</ca>\n", None)
            .unwrap_err()
            .to_string();
        assert_eq!(err, "invalid OpenVPN remote port ``");
    }

    #[test]
    fn test_cipher_selection_priority() {
        // Candidates are collected in directive-key order (data-ciphers,
        // ncp-ciphers, cipher, data-ciphers-fallback), not file order.
        let text = "client\nremote h 1194\n<ca>\nCA\n</ca>\ncipher AES-256-CBC\nncp-ciphers CHACHA20-POLY1305:AES-128-GCM\n";
        let (native, _, _) = parse_openvpn_config(text, None).unwrap();
        assert_eq!(native["cipher"], "CHACHA20-POLY1305");

        let text = "client\nremote h 1194\n<ca>\nCA\n</ca>\ndata-ciphers AES-128-GCM:BOGUS\n";
        let (native, _, _) = parse_openvpn_config(text, None).unwrap();
        assert_eq!(native["cipher"], "AES-128-GCM");
    }

    #[test]
    fn test_tls_ciphers_go_suite_mapping() {
        let text = "client\nremote h 1194\n<ca>\nCA\n</ca>\ntls-cipher ECDHE-RSA-AES128-GCM-SHA256:ECDHE-RSA-AES256-SHA\n";
        let (native, _, _) = parse_openvpn_config(text, None).unwrap();
        assert_eq!(
            native["tls"]["cipher_suites"],
            json!([
                "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
                "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA"
            ])
        );
    }

    #[test]
    fn test_mssfix_and_durations() {
        let text = "client\nremote h 1194\n<ca>\nCA\n</ca>\nmssfix 1350\nconnect-retry 5\nping 15\nping-restart 120\nexplicit-exit-notify 2\n";
        let (native, _, _) = parse_openvpn_config(text, None).unwrap();
        assert_eq!(native["mss_fix"], 1350);
        assert_eq!(native["reconnect_delay"], "5s");
        assert_eq!(native["ping_interval"], "15s");
        assert_eq!(native["ping_restart"], "120s");
        assert_eq!(native["explicit_exit_notify"], 2);

        // keepalive fills the unset fields only.
        let text = "client\nremote h 1194\n<ca>\nCA\n</ca>\nping 9\nkeepalive 10 60\n";
        let (native, _, _) = parse_openvpn_config(text, None).unwrap();
        assert_eq!(native["ping_interval"], "9s");
        assert_eq!(native["ping_restart"], "60s");
    }

    #[test]
    fn test_verify_name_and_tls_extras() {
        let text = "client\nremote h 1194\n<ca>\nCA\n</ca>\nverify-x509-name my-server name\nremote-cert-tls server\n";
        let (native, _, _) = parse_openvpn_config(text, None).unwrap();
        assert_eq!(native["tls"]["verify_x509_name"], "my-server");
        assert_eq!(native["tls"]["verify_x509_name_mode"], "name");
        assert_eq!(native["tls"]["remote_certificate_tls"], "server");

        // Missing verify mode defaults to `name`.
        let text = "client\nremote h 1194\n<ca>\nCA\n</ca>\nverify-x509-name only-name\n";
        let (native, _, _) = parse_openvpn_config(text, None).unwrap();
        assert_eq!(native["tls"]["verify_x509_name"], "only-name");
        assert!(native["tls"].get("verify_x509_name_mode").is_none());
    }
}
