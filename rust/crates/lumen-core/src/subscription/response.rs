//! Provider metadata and subscription text from a single HTTP response.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value, json};

use super::fetcher::SubscriptionHttpPayload;
use super::presentation::{EXTRA_PROVIDER_PARAMETERS, link_value, text_value};
use super::text::{py_str, repair_utf8_header};
use crate::link_parser::{decode_subscription_blob, py_int_or};

const HAPP_PARAMETERS: &[&str] = &[
    "new-url",
    "new-domain",
    "subscription-always-hwid-enable",
    "notification-subs-expire",
    "hide-settings",
    "server-address-resolve-enable",
    "server-address-resolve-dns-domain",
    "server-address-resolve-dns-ip",
    "subscription-autoconnect",
    "subscription-autoconnect-type",
    "subscription-ping-onopen-enabled",
    "subscription-auto-update-enable",
    "fragmentation-enable",
    "fragmentation-packets",
    "fragmentation-length",
    "fragmentation-interval",
    "ping-type",
    "check-url-via-proxy",
    "change-user-agent",
    "app-auto-start",
    "subscription-auto-update-open-enable",
    "per-app-proxy-mode",
    "per-app-proxy-list",
    "sniffing-enable",
    "subscriptions-collapse",
    "ping-result",
    "mux-enable",
    "mux-tcp-connections",
    "mux-xudp-connections",
    "mux-quic",
    "exclude-routes",
];

const METADATA_FIELDS: &[(&str, &[&str])] = &[
    ("profileTitle", &["profile-title", "subscription-name"]),
    ("profileDescription", &["profile-description"]),
    ("supportUrl", &["support-url", "support-link", "support"]),
    (
        "profileUrl",
        &[
            "profile-web-page-url",
            "profile-url",
            "panel-url",
            "sub-web-page-url",
            "subscription-url",
            "homepage",
        ],
    ),
    (
        "telegramUrl",
        &["telegram-url", "telegram-link", "telegram"],
    ),
    ("announcement", &["announce", "announcement"]),
    ("announcementUrl", &["announce-url", "announcement-url"]),
    ("providerId", &["providerid", "provider-id"]),
    ("profileUpdateInterval", &["profile-update-interval"]),
    ("supportEmail", &["support-email"]),
    ("premiumUrl", &["premium-url"]),
    ("bannerText", &["banner-text"]),
    ("bannerButtonText", &["banner-button-text"]),
    ("bannerButtonUrl", &["banner-button-url"]),
    ("bannerBgColor", &["banner-bg-color"]),
    ("bannerButtonColor", &["banner-button-color"]),
    ("hideUrl", &["hide-url"]),
    ("sortOrder", &["sort-order"]),
];

static DIRECTIVE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*#\s*([A-Za-z0-9_-]+)\s*:?[ \t]*(.*?)\s*$").unwrap());
static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$").unwrap());
static FILE_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)filename\*?=(?:UTF-8''|")?([^";]+)"#).unwrap());

#[derive(Default)]
pub struct SubscriptionResponse {
    pub text: String,
    pub userinfo: Map<String, Value>,
    pub headers: BTreeMap<String, String>,
    pub status: u16,
    pub transport: String,
    pub not_modified: bool,
}

impl SubscriptionResponse {
    pub fn from_http(payload: SubscriptionHttpPayload, profile: &str) -> Self {
        let headers: BTreeMap<_, _> = payload
            .headers
            .into_iter()
            .map(|(key, value)| (key.to_ascii_lowercase(), value))
            .collect();
        let mut userinfo = headers
            .get("subscription-userinfo")
            .map_or_else(Map::new, |value| parse_userinfo(value));
        let not_modified = payload.status == 304;
        let text = if not_modified {
            String::new()
        } else {
            let raw = String::from_utf8_lossy(&payload.body);
            let (text, directives) = body_directives(raw.trim());
            merge_subscription_info(&mut userinfo, directives);
            merge_subscription_info(&mut userinfo, json_metadata(&text));
            decode_subscription_blob(&text)
                .filter(|decoded| decoded.contains("://"))
                .unwrap_or(text)
        };
        merge_subscription_info(&mut userinfo, header_metadata(&headers, profile));
        Self {
            text,
            userinfo,
            headers,
            status: payload.status,
            transport: payload.transport,
            not_modified,
        }
    }
}

fn parse_userinfo(text: &str) -> Map<String, Value> {
    text.split([';', ','])
        .filter_map(|part| {
            let (key, raw) = part.split_once('=')?;
            let key = key.trim().to_lowercase();
            let raw = raw.trim();
            if raw.is_empty() {
                return None;
            }
            let value = Value::String(raw.into());
            let number = py_int_or(Some(&value), 0).ok()?;
            if key.is_empty() || (number < 0 && key != "expire") {
                return None;
            }
            let number = if key == "expire" {
                normalize_expire(number)
            } else {
                number
            };
            Some((key, json!(number)))
        })
        .collect()
}

fn normalize_expire(mut value: i64) -> i64 {
    while value > 253_402_300_799 {
        value /= 1000;
    }
    value.max(0)
}

fn header_metadata(headers: &BTreeMap<String, String>, profile: &str) -> Map<String, Value> {
    let mut info = Map::from_iter([("clientProfile".into(), json!(profile))]);
    for (field, aliases) in METADATA_FIELDS {
        let present: Vec<_> = aliases
            .iter()
            .flat_map(|key| [headers.get(*key), headers.get(&key.replace('-', "_"))])
            .flatten()
            .collect();
        let Some(raw) = present
            .iter()
            .find(|value| !value.trim().is_empty())
            .copied()
            .or_else(|| present.first().copied())
        else {
            continue;
        };
        let raw = repair_utf8_header(raw);
        if *field == "hideUrl" {
            info.insert(
                (*field).into(),
                json!(matches!(
                    raw.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes"
                )),
            );
        } else if let Some(value) = normalize_field(field, &raw) {
            info.insert((*field).into(), value);
        } else if matches!(
            *field,
            "bannerText"
                | "bannerButtonText"
                | "bannerButtonUrl"
                | "bannerBgColor"
                | "bannerButtonColor"
                | "premiumUrl"
        ) {
            info.insert((*field).into(), json!(""));
        }
    }
    if !info.contains_key("profileTitle")
        && let Some(value) = headers.get("content-disposition")
        && let Some(capture) = FILE_NAME
            .captures(value)
            .and_then(|captures| captures.get(1))
    {
        let name = percent_encoding::percent_decode_str(capture.as_str()).decode_utf8_lossy();
        let name = name.trim();
        let stem = name
            .rsplit_once('.')
            .filter(|(_, suffix)| {
                matches!(
                    suffix.to_ascii_lowercase().as_str(),
                    "yaml" | "yml" | "json" | "txt" | "conf"
                )
            })
            .map_or(name, |(stem, _)| stem);
        if !stem.is_empty()
            && !matches!(
                stem.to_ascii_lowercase().as_str(),
                "config" | "subscription" | "download"
            )
            && !stem.chars().all(char::is_numeric)
        {
            info.insert(
                "profileTitle".into(),
                json!(stem.chars().take(160).collect::<String>()),
            );
        }
    }
    let premium: Map<_, _> = premium_parameters()
        .filter_map(|key| {
            let direct = headers.get(key);
            let alias = headers.get(&key.replace('-', "_"));
            let raw = direct
                .filter(|value| !value.is_empty())
                .or(alias)
                .or(direct)?;
            Some((key.into(), json!(repair_utf8_header(raw).trim())))
        })
        .collect();
    if !premium.is_empty() {
        info.insert("premiumFeatures".into(), Value::Object(premium));
    }
    info
}

fn body_directives(text: &str) -> (String, Map<String, Value>) {
    if !text.contains('#') {
        return (text.into(), Map::new());
    }
    let mut retained = Vec::new();
    let mut info = Map::new();
    let mut premium = Map::new();
    for line in text.lines() {
        let Some(parts) = DIRECTIVE.captures(line) else {
            retained.push(line);
            continue;
        };
        let key = parts[1].to_ascii_lowercase().replace('_', "-");
        let raw = &parts[2];
        if key == "subscription-userinfo" {
            info.extend(parse_userinfo(raw));
        } else if premium_parameters().any(|parameter| parameter == key) {
            premium.insert(key, json!(raw));
        } else if let Some((field, _)) = METADATA_FIELDS
            .iter()
            .find(|(_, aliases)| aliases.contains(&key.as_str()))
            && !matches!(
                key.as_str(),
                "support-link"
                    | "profile-url"
                    | "panel-url"
                    | "sub-web-page-url"
                    | "subscription-url"
                    | "telegram-link"
            )
        {
            if let Some(value) = normalize_field(field, raw) {
                info.insert((*field).into(), value);
            } else if field.ends_with("Url") && *field != "hideUrl"
                || matches!(
                    *field,
                    "profileTitle"
                        | "profileDescription"
                        | "announcement"
                        | "providerId"
                        | "bannerText"
                        | "bannerButtonText"
                        | "profileUpdateInterval"
                )
            {
                info.insert((*field).into(), json!(""));
            }
        } else {
            retained.push(line);
        }
    }
    if !premium.is_empty() {
        info.insert("premiumFeatures".into(), Value::Object(premium));
    }
    (retained.join("\n"), info)
}

fn json_metadata(text: &str) -> Map<String, Value> {
    let text = text.trim().trim_start_matches('\u{feff}').trim();
    if !text.starts_with('{') {
        return Map::new();
    }
    let Ok(Value::Object(body)) = serde_json::from_str(text) else {
        return Map::new();
    };
    let mut info = body
        .get("user")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(|| {
            if body.get("userStatus").is_some_and(Value::is_string) || body.contains_key("username")
            {
                body.iter()
                    .filter(|(key, _)| *key != "links")
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect()
            } else {
                Map::new()
            }
        });
    if let Some(expire) = info.get_mut("expire") {
        *expire = json!(normalize_expire(
            py_int_or(Some(expire), 0).unwrap_or_default()
        ));
    }
    if let Some(name) = body
        .get("subscriptionName")
        .filter(|value| !value.is_null())
    {
        info.insert("subscriptionName".into(), name.clone());
    }
    for (field, _) in METADATA_FIELDS {
        if let Some(value) = body.get(*field).filter(|value| !value.is_null()) {
            info.insert((*field).into(), value.clone());
        }
        if (field.ends_with("Url") && *field != "hideUrl") || field.ends_with("Color") {
            if let Some(raw) = info.shift_remove(*field)
                && let Some(value) = normalize_field(field, &py_str(&raw))
            {
                info.insert((*field).into(), value);
            }
        }
    }
    let mut premium: Map<_, _> = body
        .get("premiumFeatures")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(key, value)| (key.clone(), json!(py_str(value))))
        .collect();
    for key in premium_parameters() {
        if let Some(value) = body.get(key).filter(|value| !value.is_null()) {
            premium.insert(key.into(), json!(py_str(value)));
        }
    }
    if !premium.is_empty() {
        info.insert("premiumFeatures".into(), Value::Object(premium));
    }
    let config = body
        .get("premiumConfig")
        .map(Value::as_object)
        .unwrap_or(Some(&body));
    if let Some(config) = config {
        for key in ["isPremium", "deviceLimitExceeded", "hide_url", "logoUrl"] {
            if let Some(value) = config.get(key) {
                info.insert(key.into(), value.clone());
            }
        }
        for (key, alias) in [("providerSettings", "settings"), ("providerTheme", "theme")] {
            let direct = config.get(key).filter(|value| value.is_object());
            let value = if config.contains_key("isPremium") {
                config
                    .get(alias)
                    .filter(|value| value.is_object())
                    .or(direct)
            } else {
                direct
            };
            if let Some(value) = value {
                info.insert(key.into(), value.clone());
            }
        }
    }
    info
}

fn normalize_field(field: &str, raw: &str) -> Option<Value> {
    let raw = raw.trim();
    let value = match field {
        "profileTitle" | "profileDescription" | "announcement" | "bannerText"
        | "bannerButtonText" => {
            let value = text_value(&json!(raw), 512);
            if value.is_empty() && !raw.is_empty() {
                repair_utf8_header(raw).trim().chars().take(512).collect()
            } else {
                value
            }
        }
        "hideUrl" => {
            return match raw.to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" | "enabled" => Some(json!(true)),
                "0" | "false" | "no" | "off" | "disabled" => Some(json!(false)),
                _ => None,
            };
        }
        "sortOrder" => {
            return match raw.to_ascii_lowercase().as_str() {
                value @ ("ping" | "name" | "none") => Some(json!(value)),
                _ => None,
            };
        }
        "supportEmail" => {
            let address = if raw
                .get(..7)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("mailto:"))
            {
                raw[7..].trim()
            } else {
                raw
            };
            return (address.len() <= 254 && EMAIL.is_match(address)).then(|| json!(address));
        }
        field if field.ends_with("Color") => {
            let hex = raw.strip_prefix('#').unwrap_or(raw);
            return (hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .then(|| json!(format!("#{}", hex.to_ascii_uppercase())));
        }
        field if field.ends_with("Url") => {
            if raw.is_empty() || raw.chars().any(char::is_whitespace) || raw.chars().count() > 2048
            {
                return None;
            }
            let url = if field == "telegramUrl"
                && let Some(handle) = raw.strip_prefix('@').filter(|handle| !handle.is_empty())
            {
                format!("https://t.me/{handle}")
            } else if !raw.contains(':')
                && raw.split('/').next().is_some_and(|host| {
                    host.contains('.') && !host.starts_with('.') && !host.ends_with('.')
                })
            {
                format!("https://{raw}")
            } else {
                raw.into()
            };
            if field != "telegramUrl" && url.to_ascii_lowercase().starts_with("tg:") {
                return None;
            }
            link_value(&json!(url))
        }
        _ => raw.into(),
    };
    (!value.is_empty()).then(|| json!(value))
}

pub fn merge_subscription_info(target: &mut Map<String, Value>, mut extra: Map<String, Value>) {
    if let Some(Value::Object(premium)) = extra.shift_remove("premiumFeatures") {
        let mut combined = match target.shift_remove("premiumFeatures") {
            Some(Value::Object(existing)) => existing,
            _ => Map::new(),
        };
        combined.extend(premium);
        target.insert("premiumFeatures".into(), Value::Object(combined));
    }
    target.extend(extra);
}

fn premium_parameters() -> impl Iterator<Item = &'static str> {
    HAPP_PARAMETERS
        .iter()
        .copied()
        .chain(EXTRA_PROVIDER_PARAMETERS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};

    #[test]
    fn response_metadata_merges_quota_directives_and_explicit_headers() {
        let body = "#profile-title Body title\n#banner-text Body banner\n#app-auto-start 1\n#fragmentation-length 100\n#subscription-userinfo download=25\nhttps://example.test/node#node";
        let response = SubscriptionResponse::from_http(
            SubscriptionHttpPayload {
                body: body.as_bytes().into(),
                headers: BTreeMap::from([
                    ("Profile-Title".into(), "base64:SGVhZGVy".into()),
                    ("Banner-Text".into(), String::new()),
                    ("Fragmentation-Length".into(), "200".into()),
                    (
                        "Subscription-Userinfo".into(),
                        "upload=10; download=20; expire=1700000000000; total= ; invalid=-1".into(),
                    ),
                ]),
                status: 200,
                transport: "direct-route".into(),
            },
            "Happ",
        );

        assert_eq!(response.text, "https://example.test/node#node");
        assert_eq!(response.userinfo["profileTitle"], "Header");
        assert_eq!(response.userinfo["clientProfile"], "Happ");
        assert_eq!(response.userinfo["bannerText"], "");
        assert_eq!(response.userinfo["upload"], 10);
        assert_eq!(response.userinfo["download"], 25);
        assert_eq!(response.userinfo["expire"], 1_700_000_000);
        assert!(!response.userinfo.contains_key("total"));
        assert!(!response.userinfo.contains_key("invalid"));
        assert_eq!(
            response.userinfo["premiumFeatures"],
            json!({
                "app-auto-start": "1", "fragmentation-length": "200"
            })
        );
    }

    #[test]
    fn metadata_accepts_common_button_headers() {
        let response = SubscriptionResponse::from_http(
            SubscriptionHttpPayload {
                body: b"vless://00000000-0000-0000-0000-000000000001@one.example:443#one".to_vec(),
                headers: BTreeMap::from([
                    ("profile-title".into(), "VPN".into()),
                    ("support-link".into(), "https://support.example".into()),
                    ("panel-url".into(), "https://panel.example".into()),
                    ("telegram-link".into(), "https://t.me/example".into()),
                ]),
                status: 200,
                ..SubscriptionHttpPayload::default()
            },
            "Happ Windows",
        );
        assert_eq!(response.userinfo["profileTitle"], "VPN");
        assert_eq!(response.userinfo["supportUrl"], "https://support.example");
        assert_eq!(response.userinfo["profileUrl"], "https://panel.example");
        assert_eq!(response.userinfo["telegramUrl"], "https://t.me/example");
    }

    #[test]
    fn happ_premium_headers_and_body_directives_are_preserved() {
        let body = "#subscription-autoconnect: 1\n#ping-type proxy\nvless://00000000-0000-0000-0000-000000000001@one.example:443?encryption=none&type=tcp&security=none#one";
        let response = SubscriptionResponse::from_http(
            SubscriptionHttpPayload {
                body: body.as_bytes().into(),
                headers: BTreeMap::from([
                    ("profile-title".into(), "Lumen Premium".into()),
                    ("providerid".into(), "ABCD1234".into()),
                    ("support-url".into(), "https://support.example".into()),
                    ("fragmentation-enable".into(), "1".into()),
                    ("mux-enable".into(), "true".into()),
                ]),
                status: 200,
                ..SubscriptionHttpPayload::default()
            },
            "Happ Windows",
        );
        assert_eq!(response.userinfo["profileTitle"], "Lumen Premium");
        assert_eq!(response.userinfo["providerId"], "ABCD1234");
        assert_eq!(
            response.userinfo["premiumFeatures"],
            json!({
                "fragmentation-enable": "1",
                "mux-enable": "true",
                "subscription-autoconnect": "1",
                "ping-type": "proxy",
            })
        );
        assert!(response.text.starts_with("vless://"));
    }

    #[test]
    fn json_metadata_preserves_structured_nodes_and_sibling_containers() {
        let body = r#"{"user":{"expire":"1700000000000000"},"links":["ss://example"],"outbounds":[{"type":"socks"}],"subscriptionName":"Panel","telegramUrl":"@panel","premiumConfig":{"isPremium":true,"theme":{"color":"blue"}}}"#;
        let response = SubscriptionResponse::from_http(
            SubscriptionHttpPayload {
                body: body.as_bytes().into(),
                status: 200,
                ..SubscriptionHttpPayload::default()
            },
            "Lumen",
        );

        assert_eq!(response.text, body);
        assert_eq!(response.userinfo["expire"], 1_700_000_000);
        assert_eq!(response.userinfo["subscriptionName"], "Panel");
        assert_eq!(response.userinfo["telegramUrl"], "https://t.me/panel");
        assert_eq!(response.userinfo["providerTheme"], json!({"color": "blue"}));
    }

    #[test]
    fn decodes_base64_links_and_keeps_metadata_on_not_modified() {
        let link = "https://example.test/node";
        let response = SubscriptionResponse::from_http(
            SubscriptionHttpPayload {
                body: STANDARD.encode(link).into_bytes(),
                status: 200,
                ..SubscriptionHttpPayload::default()
            },
            "Lumen",
        );
        assert_eq!(response.text, link);

        let cached = SubscriptionResponse::from_http(
            SubscriptionHttpPayload {
                body: b"ignored".to_vec(),
                headers: BTreeMap::from([
                    ("ETag".into(), "cache-tag".into()),
                    ("Subscription-Userinfo".into(), "upload=0".into()),
                ]),
                status: 304,
                ..SubscriptionHttpPayload::default()
            },
            "Lumen",
        );
        assert!(cached.not_modified);
        assert!(cached.text.is_empty());
        assert_eq!(cached.headers["etag"], "cache-tag");
        assert_eq!(cached.userinfo["upload"], 0);
    }
}
