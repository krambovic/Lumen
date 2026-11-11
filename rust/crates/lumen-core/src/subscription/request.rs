//! Subscription URL resolution and provider client profiles.

use std::{borrow::Cow, collections::BTreeMap};

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use url::Url;

use crate::constants::APP_VERSION;
use crate::crypt::{decrypt_happ_link, decrypt_incy_link, is_happ_crypt_link, is_incy_crypt_link};
use crate::models::DEFAULT_SUBSCRIPTION_HWID;

const HAPP_USER_AGENT: &str = "Happ/2.18.3/Windows/2606241603601";
const YAML_ACCEPT: &str = "text/yaml,application/yaml,*/*";
const JSON_TEXT_ACCEPT: &str = "application/json,text/plain,*/*";
const URL_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

pub fn decode_percent(value: &str) -> Cow<'_, str> {
    percent_encoding::percent_decode_str(value).decode_utf8_lossy()
}

pub enum SubscriptionSource {
    Http(Url),
    HappBody(String),
}

pub fn resolve_source(input: &str, converter: &str) -> Result<SubscriptionSource, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Пустой URL подписки".into());
    }
    let source = if is_happ_crypt_link(input) {
        let decrypted = decrypt_happ_link(input).map_err(|error| format!("Happ: {error}"))?;
        let decrypted = decrypted.trim();
        let lower = decrypted.to_ascii_lowercase();
        if decrypted.is_empty() {
            return Err("Happ: пустой результат расшифровки".into());
        }
        if lower.starts_with("http://") {
            return Err("Happ: URL подписки внутри crypt-ссылки должен использовать HTTPS".into());
        }
        if !lower.starts_with("https://") {
            return Ok(SubscriptionSource::HappBody(decrypted.into()));
        }
        decrypted.to_owned()
    } else if is_incy_crypt_link(input) {
        let decrypted = decrypt_incy_link(input).map_err(|error| format!("Incy: {error}"))?;
        let decrypted = decrypted.trim();
        if !decrypted.to_ascii_lowercase().starts_with("https://") {
            return Err("Incy: URL подписки внутри crypt-ссылки должен использовать HTTPS".into());
        }
        decrypted.to_owned()
    } else {
        input.to_owned()
    };
    let converter = converter.trim();
    let target = if converter.is_empty() {
        source
    } else {
        if web_url(converter).is_none() {
            return Err("URL конвертера должен начинаться с http:// или https://".into());
        }
        if !converter.contains("{url}") && !converter.contains("{raw_url}") {
            return Err("URL конвертера должен содержать {url} или {raw_url}".into());
        }
        converter
            .replace(
                "{url}",
                &utf8_percent_encode(&source, URL_COMPONENT).to_string(),
            )
            .replace("{raw_url}", &source)
    };
    web_url(&target)
        .map(SubscriptionSource::Http)
        .ok_or_else(|| format!("unknown url type: {target}"))
}

fn web_url(value: &str) -> Option<Url> {
    let (scheme, rest) = value.split_once("://")?;
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") || rest.starts_with('/') {
        return None;
    }
    Url::parse(value).ok().filter(Url::has_host)
}

pub fn request_hwid(custom: &str, machine_hwid: Option<&str>) -> Result<String, String> {
    let hwid = machine_hwid.unwrap_or(if custom.is_empty() {
        DEFAULT_SUBSCRIPTION_HWID
    } else {
        custom.trim()
    });
    if hwid.contains(['\r', '\n']) {
        return Err("HWID не должен содержать переносы строк".into());
    }
    if hwid.chars().count() > 256 {
        return Err("HWID не должен быть длиннее 256 символов".into());
    }
    Ok(hwid.into())
}

pub struct ClientProfile {
    pub name: String,
    pub headers: BTreeMap<String, String>,
}

impl ClientProfile {
    fn new(name: &str, agent: &str, accept: &str, hwid: &str) -> Self {
        Self {
            name: name.into(),
            headers: [
                ("user-agent", agent),
                ("accept", accept),
                ("profile-update-interval", "24"),
                ("x-hwid", hwid),
            ]
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect(),
        }
    }
}

pub fn happ_windows_profile(hwid: &str) -> ClientProfile {
    let mut profile = ClientProfile::new("Happ Windows", HAPP_USER_AGENT, "*/*", hwid);
    profile.headers.extend(
        [
            ("accept-language", "ru-RU,en,*"),
            ("x-app-version", "2.18.3"),
            ("x-device-locale", "RU"),
            ("x-device-model", "Windows_x86_64"),
            ("x-device-os", "Windows"),
            ("x-ver-os", "11_10.0.26200"),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value.into())),
    );
    profile
}

pub fn client_profiles(custom_user_agent: &str, hwid: &str) -> Vec<ClientProfile> {
    let mut profiles = Vec::new();
    if !custom_user_agent.trim().is_empty() {
        profiles.push(ClientProfile::new(
            "Custom",
            custom_user_agent.trim(),
            "text/yaml,application/yaml,application/json,*/*",
            hwid,
        ));
    }
    profiles.extend([
        ClientProfile::new(
            "Lumen",
            &format!("Lumen-Subscription/Windows-{APP_VERSION}"),
            "text/yaml,application/yaml,application/json,text/plain,*/*",
            hwid,
        ),
        happ_windows_profile(hwid),
        ClientProfile::new("SFA", "SFA/1.11.0", "application/json,*/*", hwid),
        ClientProfile::new("v2rayNG", "v2rayNG/1.10.29", JSON_TEXT_ACCEPT, hwid),
        ClientProfile::new("Streisand", "Streisand/1.6.54", JSON_TEXT_ACCEPT, hwid),
        ClientProfile::new("Clash Verge", "ClashVerge/2.0.0", YAML_ACCEPT, hwid),
        ClientProfile::new("Clash Meta", "clash.meta", YAML_ACCEPT, hwid),
        ClientProfile::new("FlClashX", "FlClashX/1.0", YAML_ACCEPT, hwid),
        ClientProfile::new("Happ", HAPP_USER_AGENT, "*/*", hwid),
    ]);
    profiles
}

pub fn needs_magvpn_variants(url: &Url, custom_user_agent: &str) -> bool {
    custom_user_agent.trim().is_empty()
        && url.scheme() == "https"
        && matches!(url.host_str(), Some("magvpn.ru" | "www.magvpn.ru"))
        && url.path().starts_with("/tsub/")
}

pub fn provider_id(url: &Url) -> Option<String> {
    let mut pairs = BTreeMap::new();
    for component in [
        url.query(),
        url.fragment().map(|text| text.trim_start_matches('?')),
    ]
    .into_iter()
    .flatten()
    {
        let mut part = BTreeMap::new();
        for (key, value) in url::form_urlencoded::parse(component.as_bytes()) {
            part.entry(key.to_ascii_lowercase().replace('_', "-"))
                .or_insert(value);
        }
        pairs.extend(part);
    }
    ["providerid", "provider-id"].into_iter().find_map(|key| {
        pairs
            .get(key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converter_distinguishes_encoded_and_raw_urls() {
        let source = "https://sub.example/~user?id=1+2";
        let SubscriptionSource::Http(encoded) =
            resolve_source(source, "https://converter.example/sub?url={url}").unwrap()
        else {
            panic!("expected a converter URL");
        };
        assert_eq!(
            encoded.as_str(),
            "https://converter.example/sub?url=https%3A%2F%2Fsub.example%2F~user%3Fid%3D1%2B2",
        );
        let SubscriptionSource::Http(raw) =
            resolve_source(source, "https://converter.example/sub?url={raw_url}").unwrap()
        else {
            panic!("expected a converter URL");
        };
        assert_eq!(raw.query().unwrap(), format!("url={source}"));
        assert!(resolve_source(source, "ftp://converter.example/{url}").is_err());
        assert!(resolve_source(source, "https://converter.example/sub").is_err());
    }

    #[test]
    fn client_profiles_match_provider_order_and_share_the_request_hwid() {
        let profiles = client_profiles(" My agent ", "device");
        assert_eq!(
            profiles
                .iter()
                .map(|profile| profile.name.as_str())
                .collect::<Vec<_>>(),
            [
                "Custom",
                "Lumen",
                "Happ Windows",
                "SFA",
                "v2rayNG",
                "Streisand",
                "Clash Verge",
                "Clash Meta",
                "FlClashX",
                "Happ"
            ],
        );
        assert_eq!(profiles[0].headers["user-agent"], "My agent");
        assert_eq!(profiles[2].headers["x-device-os"], "Windows");
        assert!(
            profiles
                .iter()
                .all(|profile| profile.headers["x-hwid"] == "device")
        );
        assert_eq!(request_hwid("custom", Some("local")).unwrap(), "local");
        assert_eq!(request_hwid("", None).unwrap(), DEFAULT_SUBSCRIPTION_HWID);
        assert!(request_hwid("line\nbreak", None).is_err());
        assert!(request_hwid(&"a".repeat(257), None).is_err());
    }

    #[test]
    fn provider_rules_use_the_resolved_host_and_fragment() {
        let url = Url::parse("https://www.magvpn.ru/tsub/123?provider-id=first#?provider_id=last")
            .unwrap();
        assert!(needs_magvpn_variants(&url, ""));
        assert!(!needs_magvpn_variants(&url, "custom"));
        assert_eq!(provider_id(&url).as_deref(), Some("last"));
    }
}
