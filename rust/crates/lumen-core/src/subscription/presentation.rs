//! Offline, bounded provider presentation. Never apply system settings.
//!
//! Port of `windows/xray_fluent/subscription_presentation.py`.

use std::sync::LazyLock;

use base64::engine::general_purpose::URL_SAFE;
use base64::Engine as _;
use chrono::{DateTime, NaiveDate, NaiveDateTime};
use regex::Regex;
use serde_json::{json, Map, Value};
use url::Url;

use super::text::{py_str, py_truthy, repair_utf8_header};

/// Provider parameters recognised by the clients (kebab-case, parity with the
/// Python module which exposes them for cross-module consumers).
pub const EXTRA_PROVIDER_PARAMETERS: [&str; 55] = [
    "fallback-url",
    "routing-enable",
    "custom-tunnel-config",
    "routing",
    "autorouting",
    "socks-auth-mode",
    "socks-auth-user",
    "socks-auth-password",
    "http-auth-mode",
    "http-auth-user",
    "http-auth-password",
    "sub-info-color",
    "sub-info-text",
    "sub-info-button-text",
    "sub-info-button-link",
    "sub-expire",
    "sub-expire-button-link",
    "no-limit-enabled",
    "no-limit-xhttp-enabled",
    "fragmentation-maxsplit",
    "noises-enable",
    "noises-type",
    "noises-packet-type",
    "noises-packet",
    "noises-delay",
    "noises-rand",
    "noises-rand-range",
    "per-app-proxy-enable",
    "per-app-proxy-list-invert",
    "per-app-proxy-list-set",
    "subscriptions-expand-now",
    "proxy-enable",
    "tun-enable",
    "tun-mode",
    "tun-type",
    "exclude-routes-set",
    "color-profile",
    "include-all-networks-enable",
    "exclude-local-networks-enable",
    "exclude-apns-enable",
    "dont-use-filter",
    "subscription-pin",
    "manual-block-user-agent",
    "subscriptions-sort-type",
    "dns-from-json-enable",
    "user-agent-geo-files",
    "proxy-ping-timeout",
    "hide-vpn-icon",
    "subscription-request-timeout",
    "inbound-http-enable",
    "xray-tun-enable",
    "xray-tun-mtu",
    "block-bind-to-tunnel-enable",
    "subscription-alternative-hwid-enabled",
    "proxy-ping-mode",
];

/// Parameters the ported client never applies (other-platform settings).
const OTHER_PLATFORM_PARAMS: [&str; 10] = [
    "include-all-networks-enable",
    "exclude-local-networks-enable",
    "exclude-apns-enable",
    "xray-tun-enable",
    "xray-tun-mtu",
    "no-limit-enabled",
    "no-limit-xhttp-enabled",
    "color-profile",
    "proxy-ping-timeout",
    "block-bind-to-tunnel-enable",
];

const NULL_VALUE: Value = Value::Null;

static HEX_COLOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#[0-9a-fA-F]{6}$").unwrap());
static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z0-9.!#$%&'*+/=_`{|}~-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$").unwrap()
});

/// Python `a or b` over two optional decoded values.
fn first_truthy(primary: Option<&Value>, secondary: Option<&Value>) -> Value {
    match primary.filter(|value| py_truthy(value)) {
        Some(value) => value.clone(),
        None => secondary.cloned().unwrap_or(Value::Null),
    }
}

/// Python `info.get(key, settings.get(fallback))`: the default applies only
/// when the key is absent, not when its value is null.
fn inherited(info: &Value, key: &str, settings: &Value, fallback: &str) -> Value {
    info.get(key)
        .cloned()
        .unwrap_or_else(|| settings.get(fallback).cloned().unwrap_or(Value::Null))
}

/// Truncate by code points the way Python slices strings.
fn truncate_chars(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// `text(value, limit)` from the Python module.
pub(crate) fn text_value(value: &Value, limit: usize) -> String {
    if value.is_null() {
        return String::new();
    }
    let text = repair_utf8_header(&py_str(value)).trim().to_string();
    if text.chars().count() <= 4096 && text.to_lowercase().starts_with("base64:") {
        // Python: base64.b64decode(raw + "=" * (-len(raw) % 4), altchars=b"-_",
        // validate=True).decode("utf-8").strip(); failures return "".
        let raw = text.split_once(':').map(|(_, rest)| rest.trim()).unwrap_or("");
        let pad = (4 - raw.chars().count() % 4) % 4;
        let padded = format!("{raw}{}", "=".repeat(pad));
        return match URL_SAFE.decode(padded.as_bytes())
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
        {
            Some(decoded) => truncate_chars(decoded.trim(), limit),
            None => String::new(),
        };
    }
    truncate_chars(&text, limit)
}

/// `enabled(value)` from the Python module.
pub(crate) fn enabled(value: &Value) -> bool {
    matches!(value, Value::Bool(true))
        || matches!(py_str(value).trim().to_lowercase().as_str(), "1" | "true")
}

/// '@' in the raw URL authority (`user[:pass]@host` or the empty-username
/// spelling `https://@host`). Only `scheme://...` forms carry an authority.
fn raw_authority_has_userinfo(raw: &str) -> bool {
    let Some(colon) = raw.find(':') else {
        return false;
    };
    let scheme = &raw[..colon];
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
    {
        return false;
    }
    let Some(rest) = raw[colon + 1..].strip_prefix("//") else {
        return false;
    };
    let end = rest
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(rest.len());
    rest[..end].contains('@')
}

/// `link(value)` from the Python module: reject control characters and
/// userinfo URLs, allow http/https with a hostname and tg://resolve|join.
pub(crate) fn link_value(value: &Value) -> String {
    let value = text_value(value, 2048);
    if value.chars().any(|c| (c as u32) < 32 || (c as u32) == 127) {
        return String::new();
    }
    let parsed = match Url::parse(&value) {
        Ok(parsed) => parsed,
        Err(_) => return String::new(),
    };
    // A '@' inside the authority is exactly userinfo; hosts cannot contain it.
    // The raw check also catches the empty-username spelling (`https://@host`)
    // that the url crate normalizes away while Python's urlsplit rejects.
    if parsed[url::Position::AfterScheme..url::Position::BeforePath].contains('@')
        || raw_authority_has_userinfo(&value)
    {
        return String::new();
    }
    let scheme = parsed.scheme();
    if matches!(scheme, "https" | "http")
        && parsed.host_str().map(|host| !host.is_empty()).unwrap_or(false)
    {
        return value;
    }
    if scheme == "tg" && matches!(parsed.host_str(), Some("resolve") | Some("join")) {
        return value;
    }
    String::new()
}

/// `color(value, fallback)` from the Python module.
pub(crate) fn color_value(value: &Value, fallback: &str) -> String {
    let value = text_value(value, 9);
    if HEX_COLOR_RE.is_match(&value) {
        value
    } else {
        fallback.to_string()
    }
}

/// `contrast(value)` from the Python module (luminance threshold 155). The
/// Python helper re-runs color() with a "#2563EB" fallback; the port applies it
/// to already-resolved colors, which behave identically.
fn contrast_of_hex(color: &str) -> String {
    let hex = color.strip_prefix('#').unwrap_or(color);
    let channel = |range: std::ops::Range<usize>| {
        u32::from_str_radix(&hex[range], 16).unwrap_or(0)
    };
    let (r, g, b) = (channel(0..2), channel(2..4), channel(4..6));
    if r as f64 * 0.299 + g as f64 * 0.587 + b as f64 * 0.114 > 155.0 {
        "#111827".into()
    } else {
        "#FFFFFF".into()
    }
}

/// `_expiry(info)` from the Python module.
fn expiry(info: &Value) -> Option<f64> {
    let chosen = match info.get("expire").filter(|value| py_truthy(value)) {
        Some(value) => value,
        // Python: value = info.get("expire") or info.get("expiresAt").
        None => info.get("expiresAt").unwrap_or(&NULL_VALUE),
    };
    let as_float = match chosen {
        Value::Number(number) => number.as_f64(),
        // Python float() also parses numeric strings ("1e3", "inf", ...).
        Value::String(text) => text.trim().parse::<f64>().ok(),
        Value::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        _ => None,
    };
    if let Some(mut number) = as_float {
        if number > 32_000_000_000.0 {
            number /= 1000.0;
        }
        return if number.is_finite() && number > 0.0 {
            Some(number)
        } else {
            None
        };
    }
    parse_iso_epoch(chosen)
}

/// ISO-8601 fallback: aware timestamps via RFC 3339, naive ones treated as UTC
/// (Python `replace(tzinfo=parsed.tzinfo or timezone.utc)`).
fn parse_iso_epoch(value: &Value) -> Option<f64> {
    let text = py_str(value).replace('Z', "+00:00");
    if let Ok(datetime) = DateTime::parse_from_rfc3339(&text) {
        return Some(
            datetime.timestamp() as f64 + datetime.timestamp_subsec_micros() as f64 / 1_000_000.0,
        );
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&text, format) {
            return Some(naive.and_utc().timestamp_micros() as f64 / 1_000_000.0);
        }
    }
    let date = NaiveDate::parse_from_str(&text, "%Y-%m-%d").ok()?;
    let midnight = date.and_hms_opt(0, 0, 0)?;
    Some(midnight.and_utc().timestamp_micros() as f64 / 1_000_000.0)
}

/// `_display_value(value, key, depth)` from the Python module.
fn display_value(value: &Value, key: &str, depth: u32) -> Value {
    let lowered = key.to_lowercase();
    if ["password", "auth-user", "token", "adminhwid"]
        .iter()
        .any(|part| lowered.contains(part))
        || matches!(lowered.as_str(), "hwid" | "uid")
    {
        return Value::String("••••".into());
    }
    match value {
        Value::Object(map) => {
            if depth >= 3 {
                return Value::String("…".into());
            }
            let mut out = Map::new();
            for (item_key, item) in map.iter().take(16) {
                out.insert(
                    text_value(&Value::String(item_key.clone()), 80),
                    display_value(item, item_key, depth + 1),
                );
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            if depth >= 3 {
                return Value::String("…".into());
            }
            Value::Array(
                items
                    .iter()
                    .take(16)
                    .map(|item| display_value(item, key, depth + 1))
                    .collect(),
            )
        }
        other => Value::String(text_value(other, if depth == 0 { 512 } else { 160 })),
    }
}

/// `present_subscription(subscription, *, now=None)` from the Python module.
/// `now` is the reference epoch in seconds; `None` uses the current time.
pub fn present_subscription(subscription: &Value, now: Option<f64>) -> Value {
    let null = &NULL_VALUE;
    let info = subscription
        .get("userinfo")
        .filter(|value| value.is_object())
        .unwrap_or(null);
    let premium = info
        .get("premiumFeatures")
        .filter(|value| value.is_object())
        .unwrap_or(null);
    let settings = info
        .get("providerSettings")
        .filter(|value| value.is_object())
        .unwrap_or(null);
    let theme = info
        .get("providerTheme")
        .filter(|value| value.is_object())
        .unwrap_or(null);

    let happ = !text_value(info.get("providerId").unwrap_or(null), 512).is_empty();
    let incy = enabled(info.get("isPremium").unwrap_or(null))
        && !enabled(info.get("deviceLimitExceeded").unwrap_or(null));
    let expires = expiry(info);
    let current = now.unwrap_or_else(|| chrono::Utc::now().timestamp_micros() as f64 / 1_000_000.0);
    let delta = expires.map(|value| value - current);
    let within_three_days = delta
        .map(|value| value <= 3.0 * 86_400.0)
        .unwrap_or(false);

    let mut banners: Vec<Value> = Vec::new();

    fn push_banner(
        banners: &mut Vec<Value>,
        message: &Value,
        button: &Value,
        url: &Value,
        background: &Value,
        button_background: &Value,
        kind: &str,
    ) {
        let message = text_value(message, 200);
        if message.is_empty() || (kind.starts_with("happ-") && message == "0") {
            return;
        }
        let background = color_value(background, "#1E40AF");
        let button_background = color_value(button_background, "#2563EB");
        banners.push(json!({
            "text": message,
            "button": text_value(button, 40),
            "url": link_value(url),
            "background": background,
            "foreground": contrast_of_hex(&background),
            "buttonBackground": button_background,
            "buttonForeground": contrast_of_hex(&button_background),
            "kind": kind,
        }));
    }

    // Happ: expiry takes precedence over its informational banner.
    if happ
        && enabled(premium.get("sub-expire").unwrap_or(null))
        && within_three_days
    {
        let message = match delta {
            Some(value) if value <= 0.0 => "Подписка истекла".to_string(),
            // Python: str(max(0, int(delta // 86400)))
            Some(value) => format!(
                "Подписка истекает: {} дн.",
                (value / 86_400.0).floor().max(0.0) as i64
            ),
            None => String::new(),
        };
        push_banner(
            &mut banners,
            &Value::String(message),
            &json!("Продлить"),
            premium.get("sub-expire-button-link").unwrap_or(null),
            &json!("#991B1B"),
            &json!("#2563EB"),
            "happ-expiry",
        );
    } else if happ {
        let palette = match text_value(premium.get("sub-info-color").unwrap_or(null), 512).as_str()
        {
            "red" => "#991B1B",
            "green" => "#166534",
            _ => "#1E40AF", // "blue" and every other spelling
        };
        push_banner(
            &mut banners,
            premium.get("sub-info-text").unwrap_or(null),
            premium.get("sub-info-button-text").unwrap_or(null),
            premium.get("sub-info-button-link").unwrap_or(null),
            &json!(palette),
            &json!("#2563EB"),
            "happ-info",
        );
    }

    // INCY: header maintenance overrides the panel, and maintenance beats expiry.
    if incy {
        let combined = inherited(info, "bannerText", settings, "bannerText");
        let maintenance = !text_value(&combined, 512).is_empty()
            && (info
                .get("bannerText")
                .map(|value| !text_value(value, 512).is_empty())
                .unwrap_or(false)
                || enabled(settings.get("bannerEnabled").unwrap_or(null)));
        if maintenance {
            let button_url = match info.get("premiumUrl").filter(|value| py_truthy(value)) {
                Some(value) => value.clone(),
                None => inherited(info, "bannerButtonUrl", settings, "bannerButtonUrl"),
            };
            push_banner(
                &mut banners,
                &combined,
                &inherited(info, "bannerButtonText", settings, "bannerButtonText"),
                &button_url,
                &inherited(info, "bannerBgColor", settings, "bannerBgColor"),
                &inherited(info, "bannerButtonColor", settings, "bannerButtonColor"),
                "incy-info",
            );
        } else if enabled(settings.get("expiryBannerEnabled").unwrap_or(null)) && within_three_days
        {
            push_banner(
                &mut banners,
                settings.get("expiryBannerText").unwrap_or(null),
                settings.get("expiryBannerButtonText").unwrap_or(null),
                settings.get("expiryBannerButtonUrl").unwrap_or(null),
                settings.get("expiryBannerBgColor").unwrap_or(null),
                settings.get("expiryBannerButtonColor").unwrap_or(null),
                "incy-expiry",
            );
        }
    }

    let mut links: Vec<Value> = Vec::new();
    let link_specs: [(&str, Value); 6] = [
        (
            "Поддержка",
            first_truthy(info.get("supportUrl"), settings.get("supportUrl")),
        ),
        (
            "Сайт",
            first_truthy(info.get("profileUrl"), settings.get("webPageUrl")),
        ),
        (
            "Telegram",
            first_truthy(info.get("telegramUrl"), settings.get("channelUrl")),
        ),
        (
            "Бот",
            settings.get("botUrl").cloned().unwrap_or(Value::Null),
        ),
        (
            "Premium",
            info.get("premiumUrl").cloned().unwrap_or(Value::Null),
        ),
        (
            "Объявление",
            first_truthy(info.get("announcementUrl"), settings.get("announceUrl")),
        ),
    ];
    for (label, raw) in link_specs {
        let safe = link_value(&raw);
        if safe.is_empty() {
            continue;
        }
        if links
            .iter()
            .any(|item| item.get("url").and_then(Value::as_str) == Some(safe.as_str()))
        {
            continue;
        }
        links.push(json!({"label": label, "url": safe}));
    }
    let email = text_value(info.get("supportEmail").unwrap_or(null), 254);
    if EMAIL_RE.is_match(&email) {
        links.push(json!({"label": "Email", "url": format!("mailto:{email}")}));
    }

    let mut fields: Vec<(String, Value)> = Vec::new();
    if let Some(map) = premium.as_object() {
        for (key, value) in map.iter() {
            fields.push((key.clone(), value.clone()));
        }
    }
    if let Some(map) = settings.as_object() {
        for (key, value) in map.iter() {
            fields.push((format!("INCY.{key}"), value.clone()));
        }
    }
    if let Some(map) = theme.as_object() {
        for (key, value) in map.iter() {
            fields.push((format!("INCY.theme.{key}"), value.clone()));
        }
    }

    let mut features: Vec<Value> = Vec::new();
    for (key, value) in fields.into_iter().take(256) {
        let rendered = display_value(&value, &key, 0);
        let rendered_string = match &rendered {
            Value::Object(_) | Value::Array(_) => {
                serde_json::to_string(&rendered).unwrap_or_default()
            }
            Value::String(text) => text.clone(),
            other => py_str(other),
        };
        let status = if key.starts_with("sub-info-") || key.starts_with("sub-expire") {
            if happ {
                "Отображается по условиям подписки"
            } else {
                "Требуется Provider ID"
            }
        } else if key == "subscription-pin" {
            if happ {
                "Порядок подписок"
            } else {
                "Требуется Provider ID"
            }
        } else if key == "subscriptions-collapse" || key == "subscriptions-expand-now" {
            "Список серверов всегда развёрнут"
        } else if key.starts_with("INCY.banner") || key.starts_with("INCY.expiryBanner") {
            if incy {
                "Отображается по условиям подписки"
            } else {
                "Нет активной Premium-конфигурации"
            }
        } else if key.starts_with("per-app-") || OTHER_PLATFORM_PARAMS.contains(&key.as_str()) {
            "Параметр другой платформы"
        } else {
            "Получено; не применяется автоматически"
        };
        features.push(json!({
            "key": key,
            "value": truncate_chars(&rendered_string, 512),
            "status": status,
        }));
    }

    let hide_url = ["hideUrl", "hide_url"].iter().any(|key| {
        matches!(
            py_str(info.get(key).unwrap_or(null)).trim().to_lowercase().as_str(),
            "true" | "1" | "yes"
        )
    });
    let has_content = !banners.is_empty()
        || !links.is_empty()
        || py_truthy(info.get("announcement").unwrap_or(null))
        || py_truthy(info.get("profileDescription").unwrap_or(null))
        || !features.is_empty()
        || info
            .get("total")
            .map(|value| !value.is_null())
            .unwrap_or(false)
        || expires.is_some()
        || py_truthy(info.get("bannerText").unwrap_or(null))
        || py_truthy(info.get("deviceLimitExceeded").unwrap_or(null));

    json!({
        "description": text_value(info.get("profileDescription").unwrap_or(null), 512),
        "announcement": text_value(info.get("announcement").unwrap_or(null), 200),
        "links": links,
        "banners": banners,
        "featureRows": features,
        "hideUrl": hide_url,
        "pinned": happ && enabled(premium.get("subscription-pin").unwrap_or(null)),
        "provider": if happ { "Happ" } else if incy { "INCY" } else { "" },
        "notice": if enabled(info.get("deviceLimitExceeded").unwrap_or(null)) {
            "Лимит Premium-устройств исчерпан; базовые серверы доступны"
        } else {
            ""
        },
        "hasContent": has_content,
        "themeAccent": if incy && enabled(theme.get("enabled").unwrap_or(null)) {
            color_value(theme.get("accent").unwrap_or(null), "")
        } else {
            "".to_string()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_repairs_and_decodes_base64() {
        let payload = "Привет, мир";
        let encoded = URL_SAFE.encode(payload.as_bytes());
        let value = Value::String(format!("base64:{encoded}"));
        assert_eq!(text_value(&value, 512), payload);
        // Invalid base64 payload fails closed to "".
        assert_eq!(text_value(&json!("base64:!!!"), 512), "");
        assert_eq!(text_value(&json!(42), 512), "42");
        assert_eq!(text_value(&json!("abcdef"), 3), "abc");
        assert_eq!(text_value(&Value::Null, 512), "");
    }

    #[test]
    fn link_validation_matches_python() {
        assert_eq!(
            link_value(&json!("https://panel.example.com/sub")),
            "https://panel.example.com/sub"
        );
        assert_eq!(
            link_value(&json!("tg://resolve?domain=lumen")),
            "tg://resolve?domain=lumen"
        );
        assert_eq!(
            link_value(&json!("tg://join?invite=abc")),
            "tg://join?invite=abc"
        );
        assert_eq!(link_value(&json!("ftp://panel.example.com")), "");
        assert_eq!(link_value(&json!("https://user@panel.example.com")), "");
        assert_eq!(link_value(&json!("tg://unknown?domain=lumen")), "");
        // Control characters are rejected before parsing.
        assert_eq!(link_value(&json!("https://a.example.com/a\u{0001}b")), "");
    }

    #[test]
    fn color_and_contrast_follow_the_luminance_rule() {
        assert_eq!(color_value(&json!("#AABBCC"), "#FALLBACK"), "#AABBCC");
        assert_eq!(color_value(&json!("nope"), "#123456"), "#123456");
        // #991B1B is dark -> white foreground; #FFFFFF is bright -> dark.
        assert_eq!(contrast_of_hex("#991B1B"), "#FFFFFF");
        assert_eq!(contrast_of_hex("#FFFFFF"), "#111827");
        assert_eq!(contrast_of_hex("#2563EB"), "#FFFFFF"); // the contrast fallback
    }

    #[test]
    fn expiry_treats_large_epochs_as_milliseconds() {
        let now = 1_700_000_000.0;
        let info = json!({"expire": (now + 86_400.0) * 1000.0});
        let epoch = expiry(&info).unwrap();
        assert!((epoch - (now + 86_400.0)).abs() < 1.0);
        // ISO-8601 fallback with a trailing Z.
        let info = json!({"expire": "2023-11-14T22:13:20Z"});
        let epoch = expiry(&info).unwrap();
        assert!((epoch - 1_700_000_000.0).abs() < 1.0);
        // Zero and negative values are rejected (Python truthiness on "expire").
        assert_eq!(expiry(&json!({"expire": 0, "expiresAt": -5})), None);
    }

    #[test]
    fn happ_expiry_banner_wins_over_info() {
        let now = 1_700_000_000.0;
        let subscription = json!({
            "userinfo": {
                "providerId": "happ",
                "premiumFeatures": {
                    "sub-expire": true,
                    "sub-expire-button-link": "https://pay.example.com",
                    "sub-info-text": "welcome"
                },
                "expire": (now + 2.0 * 86_400.0) * 1000.0
            }
        });
        let presented = present_subscription(&subscription, Some(now));
        assert_eq!(presented["provider"], "Happ");
        let banners = presented["banners"].as_array().unwrap();
        assert_eq!(banners.len(), 1);
        assert_eq!(banners[0]["kind"], "happ-expiry");
        assert_eq!(banners[0]["text"], "Подписка истекает: 2 дн.");
        assert_eq!(banners[0]["button"], "Продлить");
        assert_eq!(banners[0]["background"], "#991B1B");
        assert_eq!(banners[0]["foreground"], "#FFFFFF");
        assert_eq!(banners[0]["url"], "https://pay.example.com");
    }

    #[test]
    fn expired_happ_banner_says_istekla() {
        let now = 1_700_000_000.0;
        let subscription = json!({
            "userinfo": {"providerId": "happ", "expire": (now - 100.0) * 1000.0,
                         "premiumFeatures": {"sub-expire": true}}
        });
        let presented = present_subscription(&subscription, Some(now));
        assert_eq!(presented["banners"][0]["text"], "Подписка истекла");
    }

    #[test]
    fn incy_maintenance_outranks_expiry() {
        let now = 1_700_000_000.0;
        let subscription = json!({
            "userinfo": {
                "isPremium": true,
                "bannerText": "Сервис на обслуживании",
                "bannerButtonText": "Подробнее",
                "bannerButtonUrl": "tg://resolve?domain=lumen",
                "bannerBgColor": "#1E40AF",
                "bannerButtonColor": "#2563EB",
                "expire": (now + 86_400.0) * 1000.0,
                "providerSettings": {"expiryBannerEnabled": true, "expiryBannerText": "soon"}
            }
        });
        let presented = present_subscription(&subscription, Some(now));
        assert_eq!(presented["provider"], "INCY");
        let banners = presented["banners"].as_array().unwrap();
        assert_eq!(banners.len(), 1);
        assert_eq!(banners[0]["kind"], "incy-info");
        assert_eq!(banners[0]["url"], "tg://resolve?domain=lumen");
    }

    #[test]
    fn feature_rows_mask_secrets_and_classify_status() {
        let subscription = json!({
            "userinfo": {
                "providerId": "happ",
                "premiumFeatures": {"password": "hunter2", "sub-info-color": "red",
                                     "per-app-proxy-enable": true}
            }
        });
        let presented = present_subscription(&subscription, None);
        let rows = presented["featureRows"].as_array().unwrap();
        let by_key = |key: &str| {
            rows.iter()
                .find(|row| row["key"] == json!(key))
                .expect("row")
                .clone()
        };
        assert_eq!(by_key("password")["value"], "••••");
        assert_eq!(by_key("sub-info-color")["value"], "red");
        assert_eq!(by_key("sub-info-color")["status"], "Отображается по условиям подписки");
        assert_eq!(by_key("per-app-proxy-enable")["status"], "Параметр другой платформы");
    }

    #[test]
    fn links_labels_email_and_provider_detection() {
        let subscription = json!({
            "userinfo": {
                "isPremium": true,
                "supportUrl": "https://help.example.com",
                "supportEmail": "ops@example.com",
                "announcement": "hello",
                "hideUrl": true
            }
        });
        let presented = present_subscription(&subscription, None);
        assert_eq!(presented["provider"], "INCY");
        assert_eq!(presented["hideUrl"], true);
        assert_eq!(presented["hasContent"], true);
        let links = presented["links"].as_array().unwrap();
        assert_eq!(links[0]["label"], "Поддержка");
        assert_eq!(links[0]["url"], "https://help.example.com");
        assert_eq!(links[1]["label"], "Email");
        assert_eq!(links[1]["url"], "mailto:ops@example.com");
    }

    #[test]
    fn device_limit_notice_and_theme_accent() {
        let presented = present_subscription(
            &json!({
                "userinfo": {"isPremium": true, "deviceLimitExceeded": true}
            }),
            None,
        );
        assert_eq!(
            presented["notice"],
            "Лимит Premium-устройств исчерпан; базовые серверы доступны"
        );
        // A device limit exhaustion disables INCY mode entirely.
        assert_eq!(presented["provider"], "");

        let presented = present_subscription(
            &json!({
                "userinfo": {
                    "isPremium": true,
                    "providerTheme": {"enabled": true, "accent": "#112233"}
                }
            }),
            None,
        );
        assert_eq!(presented["provider"], "INCY");
        assert_eq!(presented["themeAccent"], "#112233");

        // A disabled theme yields an empty accent even in INCY mode.
        let presented = present_subscription(
            &json!({
                "userinfo": {"isPremium": true, "providerTheme": {"accent": "#112233"}}
            }),
            None,
        );
        assert_eq!(presented["themeAccent"], "");
    }

    #[test]
    fn empty_subscription_has_no_content() {
        let presented = present_subscription(&json!({}), None);
        assert_eq!(presented["hasContent"], false);
        assert_eq!(presented["provider"], "");
        assert_eq!(presented["themeAccent"], "");
        assert!(presented["links"].as_array().unwrap().is_empty());
        assert!(presented["banners"].as_array().unwrap().is_empty());
    }
}
