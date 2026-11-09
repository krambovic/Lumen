//! Port of `windows/xray_fluent/routing_rule_import.py`.
//!
//! Parses user-supplied routing rule lists (Clash rule lines, JSON/YAML
//! structured payloads, `address|action` lines) into `(address, action)`
//! pairs, and normalizes hand-typed addresses into bare domains / masks /
//! IPs / CIDRs.

use anyhow::{bail, Result};
use serde_json::Value;

const ACTIONS: [(&str, &str); 8] = [
    ("direct", "direct"),
    ("freedom", "direct"),
    ("proxy", "proxy"),
    ("vpn", "proxy"),
    ("reject", "block"),
    ("block", "block"),
    ("blocked", "block"),
    ("blackhole", "block"),
];

/// Python truthiness (`value or "proxy"`).
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `str(x)` for scalars, `""` for falsy values (`x or ""`).
fn value_str_or_empty(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => {
            if n.as_f64().map(|f| f == 0.0).unwrap_or(false) {
                String::new()
            } else {
                n.to_string()
            }
        }
        Value::Bool(true) => "True".to_string(),
        _ => String::new(),
    }
}

fn rule_action(value: &Value) -> String {
    let raw = value_str_or_empty(value);
    let normalized = if raw.is_empty() {
        "proxy".to_string()
    } else {
        raw.trim().to_lowercase()
    };
    for (name, action) in ACTIONS {
        if name == normalized {
            return action.to_string();
        }
    }
    "proxy".to_string()
}

fn append_rule(result: &mut Vec<(String, String)>, address: &Value, action: &Value) {
    let value = value_str_or_empty(address).trim().to_string();
    if !value.is_empty() {
        result.push((value, rule_action(action)));
    }
}

/// `_parse_clash_rule`: `KIND,target[,action]` lines.
fn parse_clash_rule(value: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = value.split(',').map(|part| part.trim()).collect();
    if parts.len() < 2 {
        return None;
    }
    let kind = parts[0].to_uppercase();
    let action_value = if parts.len() >= 3 {
        Value::String(parts[2].to_string())
    } else {
        Value::String("proxy".to_string())
    };
    let target = parts[1];
    let prefix = match kind.as_str() {
        "DOMAIN" => "full:",
        "DOMAIN-SUFFIX" => "domain:",
        "DOMAIN-KEYWORD" => "keyword:",
        "DOMAIN-REGEX" => "regexp:",
        "GEOSITE" => "geosite:",
        "GEOIP" => "geoip:",
        _ => "",
    };
    if !prefix.is_empty() {
        return Some((format!("{prefix}{target}"), rule_action(&action_value)));
    }
    if kind == "IP-CIDR" || kind == "IP-CIDR6" {
        return Some((target.to_string(), rule_action(&action_value)));
    }
    None
}

/// `_parse_rule_object`: rule dicts with `address`/`domain`/`ip` lists.
fn parse_rule_object(value: &Value, result: &mut Vec<(String, String)>) {
    let mut action = Value::Null;
    for key in ["action", "outboundTag", "outbound"] {
        if let Some(found) = value.get(key) {
            if is_truthy(found) {
                action = found.clone();
                break;
            }
        }
    }
    for key in ["address", "domain", "domains", "ip", "ips"] {
        match value.get(key) {
            Some(Value::String(s)) => {
                append_rule(result, &Value::String(s.clone()), &action);
            }
            Some(Value::Array(addresses)) => {
                for address in addresses {
                    append_rule(result, address, &action);
                }
            }
            _ => {}
        }
    }
}

/// `_parse_structured`.
fn parse_structured(payload: &Value) -> Vec<(String, String)> {
    let mut result: Vec<(String, String)> = Vec::new();
    if let Some(items) = payload.as_array() {
        for item in items {
            match item {
                Value::String(s) => match parse_clash_rule(s) {
                    Some(parsed) => result.push(parsed),
                    None => {
                        append_rule(&mut result, item, &Value::String("proxy".to_string()));
                    }
                },
                Value::Object(_) => parse_rule_object(item, &mut result),
                _ => {}
            }
        }
        return result;
    }
    let Some(obj) = payload.as_object() else {
        return result;
    };

    for (key, action) in [("direct", "direct"), ("proxy", "proxy"), ("block", "block")] {
        if let Some(values) = obj.get(key).and_then(Value::as_array) {
            for value in values {
                append_rule(&mut result, value, &Value::String(action.to_string()));
            }
        }
    }

    let mut rules: Option<&Value> = match obj.get("rules") {
        Some(Value::Null) | None => None,
        found => found,
    };
    if rules.is_none() {
        if let Some(routing) = obj.get("routing") {
            if routing.is_object() {
                rules = match routing.get("rules") {
                    Some(Value::Null) | None => None,
                    found => found,
                };
            }
        }
    }
    if let Some(rules) = rules {
        if rules.is_array() {
            result.extend(parse_structured(rules));
        }
    }
    result
}

/// `parse_routing_rules(text, suffix)`: parse a rule list payload.
///
/// `suffix` is the source file extension (`.json`/`.yaml`/`.yml`) or empty
/// for plain text. Errors carry the Python module's Russian messages.
pub fn parse_routing_rules(text: &str, suffix: &str) -> Result<Vec<(String, String)>> {
    let source = text.trim();
    if source.is_empty() {
        return Ok(Vec::new());
    }

    let extension = suffix.trim().to_lowercase();
    let mut structured: Option<Value> = None;
    if extension == ".json" || source.starts_with('{') || source.starts_with('[') {
        match serde_json::from_str::<Value>(source) {
            Ok(value) => structured = Some(value),
            Err(_) => {
                if extension == ".json" {
                    bail!("Некорректный JSON-файл правил");
                }
            }
        }
    } else if extension == ".yaml" || extension == ".yml" {
        match serde_yaml::from_str::<Value>(source) {
            Ok(value) => structured = Some(value),
            Err(exc) => bail!("Некорректный YAML-файл правил: {exc}"),
        }
    }

    if let Some(structured) = structured {
        return Ok(parse_structured(&structured));
    }

    let mut result: Vec<(String, String)> = Vec::new();
    for raw_line in source.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.contains('|') {
            let (address, action) = line.split_once('|').unwrap_or((line, ""));
            append_rule(
                &mut result,
                &Value::String(address.to_string()),
                &Value::String(action.to_string()),
            );
            continue;
        }
        match parse_clash_rule(line) {
            Some(clash) => result.push(clash),
            None => append_rule(
                &mut result,
                &Value::String(line.to_string()),
                &Value::String("proxy".to_string()),
            ),
        }
    }
    Ok(result)
}

// ── Normalisation of hand-typed addresses ───────────────────────────────
// Users paste whole URLs ("https://www.example.com:8443/path?q"), add a port
// or a trailing dot; routing needs the bare host, IP or CIDR.

const PASSTHROUGH_PREFIXES: [&str; 7] =
    ["keyword:", "regexp:", "geosite:", "geoip:", "ext:", "ext-domain:", "ext-ip:"];
const HOST_PREFIXES: [&str; 2] = ["domain:", "full:"];

/// Python `_LABEL_RE`: `^(?!-)[a-z0-9_-]{1,63}(?<!-)$`.
fn is_valid_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// `_normalize_ip`: bare address or `addr/prefix`, canonicalized like
/// `ipaddress.ip_network(strict=False)` (host bits are masked).
fn normalize_ip(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    crate::routing_runtime::ip_network_strict_false(value)
}

/// `_extract_host`: netloc of a URL-ish string, minus scheme, path, query,
/// fragment, credentials, brackets and a single-part port.
fn extract_host(value: &str) -> String {
    let text = value.trim();
    let mut netloc: String;
    if text.contains("://") || text.starts_with("//") {
        // urlsplit().netloc: everything between the scheme marker and the
        // first '/', '?' or '#'.
        let rest = if let Some(idx) = text.find("://") {
            &text[idx + 3..]
        } else {
            &text[2..]
        };
        let end = rest.find(|c| c == '/' || c == '?' || c == '#').unwrap_or(rest.len());
        netloc = rest[..end].to_string();
    } else {
        let end = text.find(|c| c == '/' || c == '?' || c == '#').unwrap_or(text.len());
        netloc = text[..end].to_string();
    }
    netloc = netloc
        .rsplit_once('@')
        .map(|(_, host)| host.trim().to_string())
        .unwrap_or(netloc);
    if netloc.starts_with('[') {
        let end = netloc.find(']');
        return match end {
            Some(e) if e > 0 => netloc[1..e].to_string(),
            _ => String::new(),
        };
    }
    if netloc.matches(':').count() == 1 {
        netloc = netloc.split_once(':').map(|(host, _)| host.to_string()).unwrap_or(netloc);
    }
    netloc.trim().to_string()
}

/// `_normalize_host`.
fn normalize_host(value: &str, allow_mask: bool, strip_www: bool) -> Option<String> {
    let mut host = value.trim().to_lowercase();
    while host.ends_with('.') {
        host.pop();
    }
    let mut mask = String::new();
    if allow_mask && host.starts_with("*.") {
        mask = "*.".to_string();
        host = host[2..].to_string();
    } else if allow_mask && host.starts_with('.') {
        mask = "*.".to_string();
        host = host[1..].to_string();
    }
    if host.is_empty() {
        return None;
    }
    host = idna::domain_to_ascii(&host).ok()?.to_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 || !labels.iter().all(|label| is_valid_label(label)) {
        return None;
    }
    if labels
        .last()
        .unwrap()
        .chars()
        .all(|c| c.is_ascii_digit())
    {
        // Looks like a broken IPv4 address (e.g. 10.0.0.300).
        return None;
    }
    if strip_www && mask.is_empty() && labels[0] == "www" && labels.len() > 2 {
        host = labels[1..].join(".");
    }
    Some(format!("{mask}{host}"))
}

/// `normalize_routing_address`: bare domain / mask / IP / CIDR for a
/// hand-typed rule, `None` if invalid.
///
/// URLs lose scheme, credentials, port, path, query and fragment; hosts are
/// lower-cased, IDN is converted to punycode, a leading `www.` and a
/// trailing dot are dropped. `domain:`/`full:` keep their prefix with a
/// normalised host; `keyword:`/`regexp:`/`geosite:`/`geoip:`/`ext:` rules are
/// kept as typed.
pub fn normalize_routing_address(value: &str) -> Option<String> {
    let text = value
        .trim()
        .trim_matches(|c| c == '\'' || c == '"' || c == '<' || c == '>')
        .to_string();
    if text.is_empty() {
        return None;
    }
    let lower = text.to_lowercase();
    for prefix in PASSTHROUGH_PREFIXES {
        if lower.starts_with(prefix) {
            let rest = text[prefix.len()..].trim();
            if rest.is_empty() {
                return None;
            }
            return Some(format!("{prefix}{rest}"));
        }
    }
    for prefix in HOST_PREFIXES {
        if lower.starts_with(prefix) {
            let host = extract_host(&text[prefix.len()..]);
            let normalized = if host.is_empty() {
                None
            } else {
                normalize_host(&host, false, false)
            };
            return normalized.map(|host| format!("{prefix}{host}"));
        }
    }
    if let Some(ip) = normalize_ip(&text) {
        return Some(ip);
    }
    let host = extract_host(&text);
    if host.is_empty() {
        return None;
    }
    normalize_ip(&host).or_else(|| normalize_host(&host, true, true))
}

/// `split_routing_addresses`: several addresses typed in one field (commas,
/// spaces or new lines).
pub fn split_routing_addresses(text: &str) -> Vec<String> {
    let source = text.trim();
    if source.is_empty() {
        return Vec::new();
    }
    if source.to_lowercase().starts_with("regexp:") {
        return vec![source.to_string()];
    }
    source
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clash_and_action_lines() {
        let text = "DOMAIN,example.com,proxy\nDOMAIN-SUFFIX,ru,DIRECT\nIP-CIDR,10.0.0.0/8,REJECT\n\
                    IP-CIDR6,2001:db8::/32,freedom\n# comment\n; note\nbad|direct\nplain.example\n";
        let parsed = parse_routing_rules(text, "").unwrap();
        assert_eq!(
            parsed,
            vec![
                ("full:example.com".to_string(), "proxy".to_string()),
                ("domain:ru".to_string(), "direct".to_string()),
                ("10.0.0.0/8".to_string(), "block".to_string()),
                ("2001:db8::/32".to_string(), "direct".to_string()),
                ("bad".to_string(), "direct".to_string()),
                ("plain.example".to_string(), "proxy".to_string()),
            ]
        );
    }

    #[test]
    fn parses_json_payloads_and_fails_bad_json() {
        let parsed = parse_routing_rules(
            r#"{"routing": {"rules": [{"domain": ["a.com"], "outboundTag": "direct"}]}}"#,
            ".json",
        )
        .unwrap();
        assert_eq!(parsed, vec![("a.com".to_string(), "direct".to_string())]);

        let parsed = parse_routing_rules(
            r#"{"block": ["blocked.example"], "rules": ["DOMAIN,x.com,block"]}"#,
            "",
        )
        .unwrap();
        assert_eq!(
            parsed,
            vec![
                ("blocked.example".to_string(), "block".to_string()),
                ("full:x.com".to_string(), "block".to_string()),
            ]
        );

        let parsed =
            parse_routing_rules(r#"[{"ip": ["5.6.7.8"], "outbound": "proxy"}]"#, "").unwrap();
        assert_eq!(parsed, vec![("5.6.7.8".to_string(), "proxy".to_string())]);

        assert!(parse_routing_rules("nope", ".json").is_err());
        // Same malformed text without the .json suffix falls back to lines.
        let parsed = parse_routing_rules("nope", "").unwrap();
        assert_eq!(parsed, vec![("nope".to_string(), "proxy".to_string())]);
    }

    #[test]
    fn parses_yaml_payloads() {
        let parsed = parse_routing_rules("proxy:\n  - one.example\n  - two.example\n", ".yaml").unwrap();
        assert_eq!(
            parsed,
            vec![
                ("one.example".to_string(), "proxy".to_string()),
                ("two.example".to_string(), "proxy".to_string()),
            ]
        );
    }

    #[test]
    fn normalizes_hand_typed_addresses() {
        assert_eq!(
            normalize_routing_address("https://www.example.com:8443/path?q=1#frag").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            normalize_routing_address("https://user:pass@Пример.рф/").as_deref(),
            Some("xn--e1afmkfd.xn--p1ai")
        );
        assert_eq!(normalize_routing_address("1.2.3.4/24").as_deref(), Some("1.2.3.0/24"));
        assert_eq!(normalize_routing_address("2001:DB8::1").as_deref(), Some("2001:db8::1"));
        assert_eq!(
            normalize_routing_address("*.Example.COM.").as_deref(),
            Some("*.example.com")
        );
        assert_eq!(
            normalize_routing_address(".subdomain.example").as_deref(),
            Some("*.subdomain.example")
        );
        assert_eq!(
            normalize_routing_address("www.sub.example.").as_deref(),
            Some("sub.example")
        );
        assert_eq!(normalize_routing_address("keyword:MyKey").as_deref(), Some("keyword:MyKey"));
        assert_eq!(
            normalize_routing_address("domain://WWW.Site.com:99").as_deref(),
            Some("domain:www.site.com")
        );
        assert_eq!(normalize_routing_address("full://site.com/page").as_deref(), Some("full:site.com"));
        assert_eq!(normalize_routing_address("10.0.0.300"), None);
        assert_eq!(normalize_routing_address("localhost"), None);
        assert_eq!(normalize_routing_address("'\"< >\"'"), None);
        assert_eq!(normalize_routing_address(""), None);
    }

    #[test]
    fn splits_multi_address_fields() {
        assert_eq!(
            split_routing_addresses("a.com, b.com;c.com\nd.com"),
            vec!["a.com", "b.com", "c.com", "d.com"]
        );
        assert_eq!(split_routing_addresses("regexp:foo,bar"), vec!["regexp:foo,bar"]);
        assert_eq!(split_routing_addresses("  "), Vec::<String>::new());
    }
}
