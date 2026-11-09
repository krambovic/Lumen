//! Read-only view of `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`
//! and Python `urllib.request.getproxies()` semantics.

use std::collections::BTreeMap;

pub const INTERNET_SETTINGS_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InternetSettings {
    pub proxy_enable: Option<u32>,
    pub proxy_server: Option<String>,
    pub proxy_override: Option<String>,
    pub auto_config_url: Option<String>,
}

/// `None` when the key itself cannot be opened.
#[cfg(windows)]
pub fn read_internet_settings() -> Option<InternetSettings> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey(INTERNET_SETTINGS_KEY).ok()?;
    Some(InternetSettings {
        proxy_enable: key.get_value::<u32, _>("ProxyEnable").ok(),
        proxy_server: key.get_value::<String, _>("ProxyServer").ok(),
        proxy_override: key.get_value::<String, _>("ProxyOverride").ok(),
        auto_config_url: key.get_value::<String, _>("AutoConfigURL").ok(),
    })
}

#[cfg(not(windows))]
pub fn read_internet_settings() -> Option<InternetSettings> {
    None
}

/// `urllib.request.getproxies_environment()`: `<scheme>_proxy` variables,
/// lowercase winning over uppercase.
pub fn environment_proxies() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut lower = BTreeMap::new();
    for (name, value) in std::env::vars() {
        if value.is_empty() || name.len() <= 6 {
            continue;
        }
        let low = name.to_lowercase();
        if let Some(scheme) = low.strip_suffix("_proxy") {
            if name.ends_with("_proxy") {
                lower.insert(scheme.to_string(), value);
            } else {
                out.insert(scheme.to_string(), value);
            }
        }
    }
    out.extend(lower);
    out
}

/// Parse a WinINet `ProxyServer` value like Python `getproxies_registry`.
pub fn parse_proxy_server(value: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let normalized = if !value.contains('=') && !value.contains(';') {
        format!("http={value};https={value};ftp={value}")
    } else {
        value.to_string()
    };
    for part in normalized.split(';') {
        let Some((protocol, address)) = part.split_once('=') else { continue };
        let protocol = protocol.trim();
        let address = address.trim();
        if protocol.is_empty() || address.is_empty() {
            continue;
        }
        let has_scheme = regex::Regex::new(r"^(?:[^/:]+)://").unwrap().is_match(address);
        let address = if has_scheme {
            address.to_string()
        } else if matches!(protocol, "http" | "https" | "ftp") {
            String::from("http://") + address
        } else if protocol == "socks" {
            format!("socks://{address}")
        } else {
            address.to_string()
        };
        out.insert(protocol.to_string(), address);
    }
    if let Some(socks) = out.remove("socks") {
        let socks = socks.replacen("socks://", "socks4://", 1);
        out.entry("http".into()).or_insert_with(|| socks.clone());
        out.entry("https".into()).or_insert(socks);
    }
    out
}

/// `urllib.request.getproxies()` on Windows: environment first, otherwise the
/// enabled WinINet proxy.
pub fn system_proxies() -> BTreeMap<String, String> {
    let env = environment_proxies();
    if !env.is_empty() {
        return env;
    }
    let Some(settings) = read_internet_settings() else { return BTreeMap::new() };
    if settings.proxy_enable.unwrap_or(0) == 0 {
        return BTreeMap::new();
    }
    settings.proxy_server.as_deref().map(parse_proxy_server).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_server_parsing() {
        let all = parse_proxy_server("127.0.0.1:8080");
        assert_eq!(all.get("https").unwrap(), "http://127.0.0.1:8080");
        let split = parse_proxy_server("http=a:1;https=b:2");
        assert_eq!(split.get("http").unwrap(), "http://a:1");
        assert_eq!(split.get("https").unwrap(), "http://b:2");
        let socks = parse_proxy_server("socks=c:3");
        assert_eq!(socks.get("https").unwrap(), "socks4://c:3");
    }
}
