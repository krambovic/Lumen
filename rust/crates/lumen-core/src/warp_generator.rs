//! Free Cloudflare WARP profiles in one click (`warp_generator.py`).
//!
//! Mirrors the defaults of https://warp-generation.github.io/: a public helper
//! registers a fresh WARP device and returns its keys; Lumen turns them into
//! ready-to-use nodes (three AmneziaWG masks and MASQUE over HTTP/3 and
//! HTTP/2). Network access is injected so the builders stay testable and the
//! caller decides whether to go through the active proxy or directly.

use std::collections::BTreeMap;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use rand::{Rng, RngCore};
use serde_json::{json, Map, Value};
use thiserror::Error;

use crate::constants::APP_VERSION;
use crate::models::Node;
use crate::wireguard_normalization::normalize_ip_prefixes;

pub const WARP_GROUP: &str = "WARP";

// Public registration helpers used by warp-generation.github.io, tried in order.
pub const AWG_DATA_ENDPOINTS: [&str; 5] = [
    "https://www.warp-generator.workers.dev",
    "https://warp-gen.netlify.app/",
    "https://warp.sub-aggregator.workers.dev",
    "https://warp-vercel-chi.vercel.app/api/warp-data",
    "https://warp-vercel-murex.vercel.app/api/warp-data",
];
pub const MASQUE_DATA_ENDPOINTS: [&str; 2] = [
    "https://www.masque-generator.workers.dev/",
    "https://masque-generator.vercel.app/api/warp-data",
];

pub const WARP_ENDPOINT_PREFIXES: [&str; 15] = [
    "162.159.192.",
    "162.159.195.",
    "engage.cloudflareclient.com",
    "8.6.112.",
    "8.34.70.",
    "8.34.146.",
    "8.35.211.",
    "8.39.125.",
    "8.39.204.",
    "8.39.214.",
    "8.47.69.",
    "188.114.96.",
    "188.114.97.",
    "188.114.98.",
    "188.114.99.",
];
pub const WARP_ENDPOINT_PORTS: [u16; 54] = [
    500, 854, 859, 864, 878, 880, 890, 891, 894, 903, 908, 928, 934, 939, 942, 943, 945, 946, 955,
    968, 987, 988, 1002, 1010, 1014, 1018, 1070, 1074, 1180, 1387, 1701, 1843, 2371, 2408, 2506,
    3138, 3476, 3581, 3854, 4177, 4198, 4233, 4500, 5279, 5956, 7103, 7152, 7156, 7281, 7559, 8319,
    8742, 8854, 8886,
];
pub const MASQUE_SERVER: &str = "162.159.198.2";
pub const MASQUE_PORT: u16 = 443;
pub const MASQUE_SNI: &str = "4pda.to";
pub const WARP_DNS: [&str; 2] = ["1.1.1.1", "1.0.0.1"];
pub const MASQUE_DNS: [&str; 4] = [
    "1.1.1.1",
    "1.0.0.1",
    "2606:4700:4700::1111",
    "2606:4700:4700::1001",
];
pub const WARP_MTU: i64 = 1280;
pub const REQUEST_TIMEOUT: f64 = 8.0;
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024;

// Obfuscation packets (AmneziaWG I1/I2) used by the generator's three
// "standard" profiles: QUIC Initial, a short QUIC packet and a SIP exchange.
const QUIC_MASK_HEX: &str = "ce000000010897a297ecc34cd6dd000044d0ec2e2e1ea2991f467ace4222129b5a098823784694b4897b9986ae0b7280135fa85e196d9ad980b150122129ce2a9379531b0fd3e871ca5fdb883c369832f730e272d7b8b74f393f9f0fa43f11e510ecb2219a52984410c204cf875585340c62238e14ad04dff382f2c200e0ee22fe743b9c6b8b043121c5710ec289f471c91ee414fca8b8be8419ae8ce7ffc53837f6ade262891895f3f4cecd31bc93ac5599e18e4f01b472362b8056c3172b513051f8322d1062997ef4a383b01706598d08d48c221d30e74c7ce000cdad36b706b1bf9b0607c32ec4b3203a4ee21ab64df336212b9758280803fcab14933b0e7ee1e04a7becce3e2633f4852585c567894a5f9efe9706a151b615856647e8b7dba69ab357b3982f554549bef9256111b2d67afde0b496f16962d4957ff654232aa9e845b61463908309cfd9de0a6abf5f425f577d7e5f6440652aa8da5f73588e82e9470f3b21b27b28c649506ae1a7f5f15b876f56abc4615f49911549b9bb39dd804fde182bd2dcec0c33bad9b138ca07d4a4a1650a2c2686acea05727e2a78962a840ae428f55627516e73c83dd8893b02358e81b524b4d99fda6df52b3a8d7a5291326e7ac9d773c5b43b8444554ef5aea104a738ed650aa979674bbed38da58ac29d87c29d387d80b526065baeb073ce65f075ccb56e47533aef357dceaa8293a523c5f6f790be90e4731123d3c6152a70576e90b4ab5bc5ead01576c68ab633ff7d36dcde2a0b2c68897e1acfc4d6483aaaeb635dd63c96b2b6a7a2bfe042f6aed82e5363aa850aace12ee3b1a93f30d8ab9537df483152a5527faca21efc9981b304f11fc95336f5b9637b174c5a0659e2b22e159a9fed4b8e93047371175b1d6d9cc8ab745f3b2281537d1c75fb9451871864efa5d184c38c185fd203de206751b92620f7c369e031d2041e152040920ac2c5ab5340bfc9d0561176abf10a147287ea90758575ac6a9f5ac9f390d0d5b23ee12af583383d994e22c0cf42383834bcd3ada1b3825a0664d8f3fb678261d57601ddf94a8a68a7c273a18c08aa99c7ad8c6c42eab67718843597ec9930457359dfdfbce024afc2dcf9348579a57d8d3490b2fa99f278f1c37d87dad9b221acd575192ffae1784f8e60ec7cee4068b6b988f0433d96d6a1b1865f4e155e9fe020279f434f3bf1bd117b717b92f6cd1cc9bea7d45978bcc3f24bda631a36910110a6ec06da35f8966c9279d130347594f13e9e07514fa370754d1424c0a1545c5070ef9fb2acd14233e8a50bfc5978b5bdf8bc1714731f798d21e2004117c61f2989dd44f0cf027b27d4019e81ed4b5c31db347c4a3a4d85048d7093cf16753d7b0d15e078f5c7a5205dc2f87e330a1f716738dce1c6180e9d02869b5546f1c4d2748f8c90d9693cba4e0079297d22fd61402dea32ff0eb69ebd65a5d0b687d87e3a8b2c42b648aa723c7c7daf37abcc4bb85caea2ee8f55bec20e913b3324ab8f5c3304f820d42ad1b9f2ffc1a3af9927136b4419e1e579ab4c2ae3c776d293d397d575df181e6cae0a4ada5d67ecea171cca3288d57c7bbdaee3befe745fb7d634f70386d873b90c4d6c6596bb65af68f9e5121e67ebf0d89d3c909ceedfb32ce9575a7758ff080724e1ab5d5f43074ecb53a479af21ed03d7b6899c36631c0166f9d47e5e1d4528a5d3d3f744029c4b1c190cbfbad06f5f83f7ad0429fa9a2719c56ffe3783460e166de2d8";
const SHORT_MASK_HEX: &str = "c400000001015c000040570b2e25e1a2fb2e1d5cf2bfdaeb0ca79c3255f6384628e6e6c22adb43440db63fa1d26ad16120d9cbdbf0dc2f7a8eb3525561b193c6b6a0ef44e8d118c3b04a3ae880c081a9b9e97321315915787938abd8b925506b830d";
const SIP_INVITE_HEX: &str = "494e56495445207369703a626f624062696c6f78692e636f6d205349502f322e300d0a5669613a205349502f322e302f55445020706333332e61746c616e74612e636f6d3b6272616e63683d7a39684734624b3737366173646864730d0a4d61782d466f7277617264733a2037300d0a546f3a20426f62203c7369703a626f624062696c6f78692e636f6d3e0d0a46726f6d3a20416c696365203c7369703a616c6963654061746c616e74612e636f6d3e3b7461673d313932383330313737340d0a43616c6c2d49443a20613834623463373665363637313040706333332e61746c616e74612e636f6d0d0a435365713a2033313431353920494e564954450d0a436f6e746163743a203c7369703a616c69636540706333332e61746c616e74612e636f6d3e0d0a436f6e74656e742d547970653a206170706c69636174696f6e2f7364700d0a436f6e74656e742d4c656e6774683a20300d0a0d0a";
const SIP_TRYING_HEX: &str = "5349502f322e302031303020547279696e670d0a5669613a205349502f322e302f55445020706333332e61746c616e74612e636f6d3b6272616e63683d7a39684734624b3737366173646864730d0a546f3a20426f62203c7369703a626f624062696c6f78692e636f6d3e0d0a46726f6d3a20416c696365203c7369703a616c6963654061746c616e74612e636f6d3e3b7461673d313932383330313737340d0a43616c6c2d49443a20613834623463373665363637313040706333332e61746c616e74612e636f6d0d0a435365713a2033313431353920494e564954450d0a436f6e74656e742d4c656e6774683a20300d0a0d0a";

/// The generator's three "standard" AWG profiles: QUIC Initial, a short QUIC
/// packet and a SIP exchange (AmneziaWG I1/I2 obfuscation packets).
pub static AWG_PROFILES: &[(&str, &[(&str, &str)])] = &[
    ("WARP AWG · QUIC", &[("I1", QUIC_MASK_HEX)]),
    ("WARP AWG · QUIC lite", &[("I1", SHORT_MASK_HEX)]),
    ("WARP AWG · SIP", &[("I1", SIP_INVITE_HEX), ("I2", SIP_TRYING_HEX)]),
];

/// No helper returned a usable WARP registration.
#[derive(Debug, Clone, Error)]
#[error("{0}")]
pub struct WarpGenerationError(pub String);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WarpRegistration {
    pub private_key: String,
    pub public_key: String,
    pub ipv4: String,
    pub ipv6: String,
}

#[derive(Debug, Clone, Default)]
pub struct WarpResult {
    pub nodes: Vec<Node>,
    pub errors: Vec<String>,
}

// ── Python semantics helpers ──

fn py_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn py_str_or_empty(value: &Value) -> String {
    if truthy(value) {
        py_str(value)
    } else {
        String::new()
    }
}

fn pick_first_truthy(values: &[&Value]) -> Value {
    for value in values {
        if truthy(value) {
            return (*value).clone();
        }
    }
    Value::Null
}

/// Python `int(value)`: numbers truncate, integer strings parse.
fn int_from_value(value: &Value) -> std::result::Result<i64, WarpGenerationError> {
    match value {
        Value::Number(n) => Ok(n
            .as_i64()
            .unwrap_or_else(|| n.as_f64().map(|f| f as i64).unwrap_or(0))),
        Value::String(s) => s.trim().parse::<i64>().map_err(|_| {
            WarpGenerationError(format!("invalid integer value: {s}"))
        }),
        Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
        Value::Null => Err(WarpGenerationError("invalid integer value: None".into())),
        _ => Err(WarpGenerationError("invalid integer value".into())),
    }
}

fn to_bool_value(value: &Value) -> bool {
    let rendered = if truthy(value) {
        py_str(value)
    } else {
        "false".to_string()
    };
    ["1", "true", "yes", "on"].contains(&rendered.trim().to_lowercase().as_str())
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Clash `_clash_list`: arrays keep their string items, scalars split on ",".
fn clash_list(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items
            .iter()
            .map(py_str)
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
        other => split_csv(&py_str_or_empty(other)),
    }
}

fn clean_b64_key(value: &str) -> String {
    value.trim().replace(' ', "+")
}

/// Python `_clean`: rejects any value containing whitespace.
fn clean_registration_value(value: &Value) -> String {
    let text = py_str_or_empty(value).trim().to_string();
    if text.chars().any(|c| c.is_whitespace()) {
        String::new()
    } else {
        text
    }
}

/// Python `str.splitlines()` semantics (\r\n consumed as a single break).
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

// ── Registration parsing ──

/// Validate a helper response: ``{privKey, peer_pub, client_ipv4, client_ipv6}``.
pub fn parse_registration(payload: &Value) -> std::result::Result<WarpRegistration, WarpGenerationError> {
    if !payload.is_object() {
        return Err(WarpGenerationError("unexpected response format".into()));
    }
    let data = WarpRegistration {
        private_key: clean_registration_value(&pick_first_truthy(&[
            payload.get("privKey").unwrap_or(&Value::Null),
            payload.get("private_key").unwrap_or(&Value::Null),
        ])),
        public_key: clean_registration_value(&pick_first_truthy(&[
            payload.get("peer_pub").unwrap_or(&Value::Null),
            payload.get("public_key").unwrap_or(&Value::Null),
        ])),
        ipv4: clean_registration_value(&pick_first_truthy(&[
            payload.get("client_ipv4").unwrap_or(&Value::Null),
            payload.get("ipv4").unwrap_or(&Value::Null),
        ])),
        ipv6: clean_registration_value(&pick_first_truthy(&[
            payload.get("client_ipv6").unwrap_or(&Value::Null),
            payload.get("ipv6").unwrap_or(&Value::Null),
        ])),
    };
    if data.private_key.is_empty() || data.public_key.is_empty() || data.ipv4.is_empty() {
        return Err(WarpGenerationError("response has no keys or address".into()));
    }
    Ok(data)
}

/// Ask each helper in turn and return the first valid registration.
pub async fn fetch_registration(
    client: &reqwest::Client,
    endpoints: &[&str],
    headers: &[(&str, &str)],
    timeout: Duration,
    cancelled: Option<&(dyn Fn() -> bool + Send + Sync)>,
) -> std::result::Result<WarpRegistration, WarpGenerationError> {
    let mut failures: Vec<String> = Vec::new();
    for url in endpoints {
        if cancelled.map(|check| check()).unwrap_or(false) {
            return Err(WarpGenerationError("cancelled".into()));
        }
        let mut request = client
            .get(*url)
            .header("User-Agent", format!("Lumen/{APP_VERSION}"))
            .header("Accept", "application/json")
            .timeout(timeout);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let outcome: std::result::Result<WarpRegistration, String> = async {
            let response = request
                .send()
                .await
                .map_err(|exc| exc.to_string())?;
            let status = response.status();
            if !status.is_success() {
                // urllib-style failure so a bad helper is skipped like in Python.
                let reason = status.canonical_reason().unwrap_or("Unknown");
                return Err(format!("HTTP Error {}: {}", status.as_u16(), reason));
            }
            let mut response = response;
            let mut raw: Vec<u8> = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|exc| exc.to_string())? {
                raw.extend_from_slice(&chunk);
                if raw.len() > MAX_RESPONSE_BYTES {
                    return Err("response is too large".into());
                }
            }
            let text = std::str::from_utf8(&raw).map_err(|_| "invalid utf-8".to_string())?;
            let payload: Value = serde_json::from_str(text).map_err(|exc| exc.to_string())?;
            parse_registration(&payload).map_err(|exc| exc.0)
        }
        .await;
        match outcome {
            Ok(registration) => return Ok(registration),
            Err(message) => failures.push(format!("{}: {}", host_of(url), message)),
        }
    }
    Err(WarpGenerationError(if failures.is_empty() {
        "no endpoints".into()
    } else {
        failures.join("; ")
    }))
}

/// Python `url.split("//", 1)[-1].split("/", 1)[0]`.
fn host_of(url: &str) -> &str {
    let after_scheme = url.split_once("//").map(|(_, rest)| rest).unwrap_or(url);
    after_scheme.split('/').next().unwrap_or(after_scheme)
}

// ── Endpoint randomization ──

pub fn random_endpoint(rng: &mut dyn RngCore) -> String {
    let port = WARP_ENDPOINT_PORTS[rng.gen_range(0..WARP_ENDPOINT_PORTS.len())];
    let prefix = WARP_ENDPOINT_PREFIXES[rng.gen_range(0..WARP_ENDPOINT_PREFIXES.len())];
    if !prefix.ends_with('.') {
        return format!("{prefix}:{port}");
    }
    format!("{prefix}{}:{port}", rng.gen_range(1..=10))
}

// ── AWG config builder ──

/// AmneziaWG .conf exactly like the generator's default download.
pub fn build_awg_config(
    registration: &WarpRegistration,
    masks: &[(&str, &str)],
    endpoint: &str,
) -> String {
    let mut mask_lines = String::new();
    for (key, value) in masks {
        mask_lines.push_str(&format!("{key} = <b 0x{value}>\n"));
    }
    format!(
        "[Interface]\n\
         PrivateKey = {private}\n\
         Address = {ipv4}\n\
         DNS = {dns}\n\
         MTU = {mtu}\n\
         S1 = 0\nS2 = 0\nS3 = 0\nS4 = 0\n\
         Jc = 4\nJmin = 40\nJmax = 70\n\
         H1 = 1\nH2 = 2\nH3 = 3\nH4 = 4\n\
         {mask_lines}\n\
         [Peer]\n\
         PublicKey = {public}\n\
         AllowedIPs = 0.0.0.0/0, ::/0\n\
         Endpoint = {endpoint}\n",
        private = registration.private_key,
        ipv4 = registration.ipv4,
        dns = WARP_DNS.join(", "),
        mtu = WARP_MTU,
        mask_lines = mask_lines,
        public = registration.public_key,
        endpoint = endpoint,
    )
}

// ── MASQUE payload builder ──

fn masque_common_mapping(registration: &WarpRegistration) -> serde_yaml::Mapping {
    let mut common = serde_yaml::Mapping::new();
    let mut insert = |key: &str, value: serde_yaml::Value| {
        common.insert(serde_yaml::Value::String(key.to_string()), value);
    };
    insert("type", serde_yaml::Value::String("masque".into()));
    insert("server", serde_yaml::Value::String(MASQUE_SERVER.into()));
    insert("port", serde_yaml::Value::Number(serde_yaml::Number::from(MASQUE_PORT)));
    insert("sni", serde_yaml::Value::String(MASQUE_SNI.into()));
    insert(
        "private-key",
        serde_yaml::Value::String(registration.private_key.clone()),
    );
    insert(
        "public-key",
        serde_yaml::Value::String(registration.public_key.clone()),
    );
    insert("ip", serde_yaml::Value::String(registration.ipv4.clone()));
    insert(
        "mtu",
        serde_yaml::Value::Number(serde_yaml::Number::from(WARP_MTU)),
    );
    insert("udp", serde_yaml::Value::Bool(true));
    insert("remote-dns-resolve", serde_yaml::Value::Bool(true));
    insert(
        "dns",
        serde_yaml::Value::Sequence(
            MASQUE_DNS
                .iter()
                .map(|entry| serde_yaml::Value::String(entry.to_string()))
                .collect(),
        ),
    );
    if !registration.ipv6.is_empty() {
        insert("ipv6", serde_yaml::Value::String(registration.ipv6.clone()));
    }
    common
}

fn masque_payload_mapping(
    name: &str,
    common: &serde_yaml::Mapping,
    network: Option<&str>,
) -> serde_yaml::Mapping {
    let mut payload = serde_yaml::Mapping::new();
    payload.insert(
        serde_yaml::Value::String("name".into()),
        serde_yaml::Value::String(name.to_string()),
    );
    for (key, value) in common.iter() {
        payload.insert(key.clone(), value.clone());
    }
    if let Some(network) = network {
        payload.insert(
            serde_yaml::Value::String("network".into()),
            serde_yaml::Value::String(network.to_string()),
        );
    }
    payload
}

/// Compact JSON in insertion order, matching Python
/// `json.dumps(payload, ensure_ascii=False, separators=(",", ":"))`.
fn yaml_to_compact_json(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(text) => {
            serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
        }
        serde_yaml::Value::Number(number) => number.to_string(),
        serde_yaml::Value::Bool(flag) => flag.to_string(),
        serde_yaml::Value::Null => "null".into(),
        serde_yaml::Value::Sequence(items) => {
            let inner: Vec<String> = items.iter().map(yaml_to_compact_json).collect();
            format!("[{}]", inner.join(","))
        }
        serde_yaml::Value::Mapping(map) => {
            let inner: Vec<String> = map
                .iter()
                .map(|(key, value)| {
                    format!("{}:{}", yaml_to_compact_json(key), yaml_to_compact_json(value))
                })
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        serde_yaml::Value::Tagged(tagged) => yaml_to_compact_json(&tagged.value),
    }
}

/// Clash/mihomo MASQUE proxies (HTTP/3 and HTTP/2) for one registration.
pub fn build_masque_payloads(registration: &WarpRegistration) -> Vec<Value> {
    let common = masque_common_mapping(registration);
    let mut payloads = Vec::new();
    for (name, network) in [("WARP MASQUE", None), ("WARP MASQUE · HTTP/2", Some("h2"))] {
        let mapping = masque_payload_mapping(name, &common, network);
        // Emit the mihomo YAML text exactly like the generator, then parse it
        // back through serde_yaml (the Clash payload path).
        let yaml_text = serde_yaml::to_string(&mapping).unwrap_or_default();
        let payload: Value = serde_yaml::from_str(&yaml_text).unwrap_or_else(|_| json!({}));
        payloads.push(payload);
    }
    payloads
}

fn finish_node(mut node: Node, name: &str) -> Node {
    node.name = name.to_string();
    node.group = WARP_GROUP.to_string();
    if !node.tags.iter().any(|tag| tag == WARP_GROUP) {
        node.tags.push(WARP_GROUP.to_string());
    }
    node
}

// ── Private WireGuard INI parser (mirrors link_parser._parse_wireguard_config) ──

fn canonical_wireguard_key(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{000B}' | '\u{000C}' | '_' | '-'))
        .collect()
}

fn split_reserved_text(text: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_whitespace() || c == ',' || c == ';' {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Raises on invalid values, unlike the normalization helper which returns [].
fn parse_reserved_bytes(value: Option<&str>) -> std::result::Result<Vec<i64>, WarpGenerationError> {
    let Some(value) = value.filter(|text| !text.trim().is_empty()) else {
        return Ok(Vec::new());
    };
    let text = value.trim().trim_matches(|c| c == '[' || c == ']');
    let parts = split_reserved_text(text);
    if parts.len() == 1 {
        let encoded = parts[0].replace('-', "+").replace('_', "/");
        let padding = "=".repeat((4 - encoded.len() % 4) % 4);
        if let Ok(decoded) = BASE64.decode(format!("{encoded}{padding}").as_bytes()) {
            if decoded.len() == 3 {
                return Ok(decoded.iter().map(|b| *b as i64).collect());
            }
        }
    }
    let mut parsed: Vec<i64> = Vec::with_capacity(parts.len());
    for part in &parts {
        match part.trim().parse::<i64>() {
            Ok(number) => parsed.push(number),
            Err(_) => {
                return Err(WarpGenerationError(
                    "wireguard reserved must contain three bytes".into(),
                ))
            }
        }
    }
    if parsed.len() == 3 && parsed.iter().all(|item| (0..=255).contains(item)) {
        Ok(parsed)
    } else {
        Err(WarpGenerationError(
            "wireguard reserved must contain three bytes".into(),
        ))
    }
}

const AWG_UINT32_MAX: u128 = 0xFFFF_FFFF;

fn validate_awg3_range(
    value: &str,
    field: &str,
) -> std::result::Result<Value, WarpGenerationError> {
    let text = value.trim();
    if !text.is_empty() && text.chars().all(|c| c.is_ascii_digit()) {
        let number = text
            .parse::<u128>()
            .unwrap_or(AWG_UINT32_MAX + 1);
        if number > AWG_UINT32_MAX {
            return Err(WarpGenerationError(format!(
                "слишком большое значение в поле {field}: {value}"
            )));
        }
        return Ok(json!(number));
    }
    if let Some((low_text, high_text)) = text.split_once('-') {
        if !low_text.is_empty()
            && !high_text.is_empty()
            && low_text.chars().all(|c| c.is_ascii_digit())
            && high_text.chars().all(|c| c.is_ascii_digit())
        {
            let low = low_text.parse::<u128>().unwrap_or(AWG_UINT32_MAX + 1);
            let high = high_text.parse::<u128>().unwrap_or(AWG_UINT32_MAX + 1);
            if high > AWG_UINT32_MAX || low > high {
                return Err(WarpGenerationError(format!(
                    "некорректный диапазон в поле {field}: {value}"
                )));
            }
            return Ok(json!(format!("{low}-{high}")));
        }
    }
    Err(WarpGenerationError(format!(
        "некорректный диапазон в поле {field}: {value} (ожидается число или A-B)"
    )))
}

fn validate_awg3_header_key(
    value: &str,
) -> std::result::Result<String, WarpGenerationError> {
    let text = value.trim();
    match BASE64.decode(text.as_bytes()) {
        Ok(raw) => {
            if raw.len() != 32 {
                return Err(WarpGenerationError(format!(
                    "некорректный HeaderProtectionKey: {} байт вместо 32",
                    raw.len()
                )));
            }
            Ok(text.to_string())
        }
        Err(_) => Err(WarpGenerationError(
            "некорректный HeaderProtectionKey: ожидается base64 ключ длиной 32 байта".into(),
        )),
    }
}

const AWG3_RANGE_KEYS: [(&str, &str); 6] = [
    ("contentpaddingaddition", "content_padding_addition"),
    ("rekeyaftertime", "rekey_after_time"),
    ("rekeytimeout", "rekey_timeout"),
    ("rejectaftertime", "reject_after_time"),
    ("keepalivetimeout", "keepalive_timeout"),
    ("maxhandshakeattempts", "max_handshake_attempts"),
];

const AWG3_CONF_NAMES: [(&str, &str); 6] = [
    ("contentpaddingaddition", "ContentPaddingAddition"),
    ("rekeyaftertime", "RekeyAfterTime"),
    ("rekeytimeout", "RekeyTimeout"),
    ("rejectaftertime", "RejectAfterTime"),
    ("keepalivetimeout", "KeepaliveTimeout"),
    ("maxhandshakeattempts", "MaxHandshakeAttempts"),
];

fn awg3_conf_name(source: &str) -> String {
    AWG3_CONF_NAMES
        .iter()
        .find(|(key, _)| *key == source)
        .map(|(_, name)| (*name).to_string())
        .unwrap_or_else(|| source.to_string())
}

fn amnezia_from_params(
    params: &BTreeMap<String, String>,
) -> std::result::Result<Value, WarpGenerationError> {
    let mut result = Map::new();
    for key in [
        "jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "h1", "h2", "h3", "h4", "itime",
    ] {
        if let Some(value) = params.get(key).filter(|value| !value.is_empty()) {
            match value.trim().parse::<i64>() {
                Ok(number) => {
                    result.insert(key.to_string(), json!(number));
                }
                Err(_) => {
                    result.insert(key.to_string(), json!(value.clone()));
                }
            }
        }
    }
    for key in ["i1", "i2", "i3", "i4", "i5", "j1", "j2", "j3"] {
        if let Some(value) = params.get(key).filter(|value| !value.is_empty()) {
            result.insert(key.to_string(), json!(value.clone()));
        }
    }
    if let Some(header_key) = params
        .get("headerprotectionkey")
        .filter(|value| !value.is_empty())
    {
        let validated = validate_awg3_header_key(header_key)?;
        result.insert("header_protection_key".into(), json!(validated));
    }
    for (source, json_key) in AWG3_RANGE_KEYS {
        if let Some(value) = params.get(source).filter(|value| !value.is_empty()) {
            let validated = validate_awg3_range(value, &awg3_conf_name(source))?;
            result.insert(json_key.to_string(), validated);
        }
    }
    Ok(Value::Object(result))
}

fn build_wireguard_peer(
    server: &str,
    port: i64,
    public_key: &str,
    allowed_ips: &[String],
    pre_shared_key: Option<&str>,
    persistent_keepalive: Option<&str>,
) -> std::result::Result<Value, WarpGenerationError> {
    let allowed_value = if allowed_ips.is_empty() {
        normalize_ip_prefixes(&json!(["0.0.0.0/0", "::/0"]))
    } else {
        allowed_ips.to_vec()
    };
    let mut peer = Map::new();
    peer.insert("address".into(), json!(server));
    peer.insert("port".into(), json!(port));
    peer.insert("public_key".into(), json!(clean_b64_key(public_key)));
    peer.insert(
        "allowed_ips".into(),
        Value::Array(allowed_value.into_iter().map(Value::String).collect()),
    );
    if let Some(psk) = pre_shared_key.filter(|value| !value.is_empty()) {
        peer.insert("pre_shared_key".into(), json!(clean_b64_key(psk)));
    }
    if let Some(keepalive) = persistent_keepalive.filter(|value| !value.is_empty()) {
        let interval = int_from_value(&Value::String(keepalive.to_string()))?;
        peer.insert("persistent_keepalive_interval".into(), json!(interval));
    }
    Ok(Value::Object(peer))
}

fn parse_config_int(value: &str, what: &str) -> std::result::Result<i64, WarpGenerationError> {
    value
        .trim()
        .parse::<i64>()
        .map_err(|_| WarpGenerationError(format!("invalid {what} value: {value}")))
}

fn build_warp_profile_endpoint(
    private_key: &str,
    persistent_keepalive: Option<&str>,
    amnezia: &Value,
    listen_port: Option<&str>,
) -> std::result::Result<Value, WarpGenerationError> {
    let mut endpoint = Map::new();
    endpoint.insert("type".into(), json!("warp"));
    endpoint.insert("tag".into(), json!("proxy"));
    endpoint.insert("udp_timeout".into(), json!("5m0s"));
    endpoint.insert(
        "profile".into(),
        json!({
            "detour": "direct",
            "private_key": clean_b64_key(private_key),
        }),
    );
    if let Some(listen) = listen_port.filter(|value| !value.is_empty()) {
        let port = parse_config_int(listen, "listen port")?;
        endpoint.insert("listen_port".into(), json!(port));
    }
    if let Some(keepalive) = persistent_keepalive.filter(|value| !value.is_empty()) {
        let interval = int_from_value(&Value::String(keepalive.to_string()))?;
        endpoint.insert("persistent_keepalive_interval".into(), json!(interval));
    }
    if amnezia.as_object().map(|obj| !obj.is_empty()).unwrap_or(false) {
        endpoint.insert("amnezia".into(), amnezia.clone());
    }
    Ok(Value::Object(endpoint))
}

fn apply_wireguard_performance_options(
    endpoint: &mut Map<String, Value>,
    workers: Option<&str>,
    preallocated_buffers_per_pool: Option<&str>,
    disable_pauses: Option<&str>,
) -> std::result::Result<(), WarpGenerationError> {
    if let Some(workers) = workers.filter(|value| !value.is_empty()) {
        let count = parse_config_int(workers, "workers")?;
        endpoint.insert("workers".into(), json!(count));
    }
    if let Some(buffers) = preallocated_buffers_per_pool.filter(|value| !value.is_empty()) {
        let count = parse_config_int(buffers, "preallocated buffers per pool")?;
        endpoint.insert("preallocated_buffers_per_pool".into(), json!(count));
    }
    if let Some(pauses) = disable_pauses.filter(|value| !value.is_empty()) {
        endpoint.insert("disable_pauses".into(), json!(to_bool_value(&json!(pauses))));
    }
    Ok(())
}

fn is_warp_endpoint(server: &str) -> bool {
    let host = server.trim().to_lowercase();
    let host = host.trim_matches('.');
    if host == "engage.cloudflareclient.com" || host.ends_with(".cloudflareclient.com") {
        return true;
    }
    let Ok(address) = host.parse::<std::net::IpAddr>() else {
        return false;
    };
    match address {
        std::net::IpAddr::V4(ip) => {
            let v = u32::from_be_bytes(ip.octets());
            const MASK24: u32 = 0xFFFF_FF00;
            const NET_192: u32 = (162 << 24) | (159 << 16) | (192 << 8);
            const NET_193: u32 = (162 << 24) | (159 << 16) | (193 << 8);
            const NET_195: u32 = (162 << 24) | (159 << 16) | (195 << 8);
            const MASK20: u32 = 0xFFFF_F000;
            const NET_188_114_96_20: u32 =
                ((188 << 24) | (114 << 16) | (96 << 8)) & MASK20;
            (v & MASK24) == NET_192
                || (v & MASK24) == NET_193
                || (v & MASK24) == NET_195
                || (v & MASK20) == NET_188_114_96_20
        }
        std::net::IpAddr::V6(ip) => {
            let prefix = u128::from(ip) >> 80;
            prefix == 0x2606_4700_00d0 || prefix == 0x2606_4700_00d1
        }
    }
}

fn split_endpoint(value: &str) -> (String, i64) {
    let text = value.trim();
    if text.is_empty() {
        return (String::new(), 0);
    }
    if text.starts_with('[') {
        if let Some(closing) = text.find(']') {
            if closing > 0 {
                let host = text[1..closing].to_string();
                let remainder = &text[closing + 1..];
                let port = match remainder.strip_prefix(':') {
                    Some(rest) => {
                        let source = Value::String(rest.to_string());
                        match int_from_value(&source) {
                            Ok(port) if port > 0 => port,
                            _ => 0,
                        }
                    }
                    None => 0,
                };
                return (host, port);
            }
        }
    }
    if let Some((host, port_text)) = text.rsplit_once(':') {
        if !host.is_empty()
            && !port_text.is_empty()
            && port_text.chars().all(|c| c.is_ascii_digit())
        {
            let port = port_text.parse::<i64>().unwrap_or(0);
            return (host.to_string(), port);
        }
    }
    (text.to_string(), 0)
}

fn native_singbox_outbound(native: &Value) -> std::result::Result<Value, WarpGenerationError> {
    let mut native = native.clone();
    let mut protocol = py_str_or_empty(native.get("type").unwrap_or(&Value::Null)).to_lowercase();
    if protocol.is_empty() {
        protocol = "custom".into();
    }
    if ["wireguard", "awg", "warp"].contains(&protocol.as_str()) {
        native = crate::wireguard_normalization::normalize_wireguard_endpoint(&native)
            .map_err(|exc| WarpGenerationError(exc.to_string()))?;
        protocol = py_str_or_empty(native.get("type").unwrap_or(&Value::Null)).to_lowercase();
        if protocol.is_empty() {
            protocol = "custom".into();
        }
    }
    let protocol = if protocol == "wireguard"
        && native
            .get("amnezia")
            .and_then(Value::as_object)
            .map(|obj| !obj.is_empty())
            .unwrap_or(false)
    {
        "awg"
    } else {
        &protocol
    };
    Ok(json!({
        "protocol": protocol,
        "singbox": native,
    }))
}

/// WireGuard INI parser mirroring `link_parser._parse_wireguard_config`.
fn parse_wireguard_ini(text: &str) -> std::result::Result<Node, WarpGenerationError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut interface: BTreeMap<String, String> = BTreeMap::new();
    let mut peers: Vec<BTreeMap<String, String>> = Vec::new();
    enum Section {
        Interface,
        Peer(usize),
        None,
    }
    let mut current = Section::None;
    for raw_line in py_splitlines(text) {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = line
                .trim_matches(|c| c == '[' || c == ']')
                .trim()
                .to_lowercase();
            if section == "interface" {
                current = Section::Interface;
            } else if section.starts_with("peer") {
                peers.push(BTreeMap::new());
                current = Section::Peer(peers.len() - 1);
            } else {
                current = Section::None;
            }
            continue;
        }
        if !line.contains('=') || matches!(current, Section::None) {
            continue;
        }
        let (key_part, value_part) = line.split_once('=').unwrap();
        let key = canonical_wireguard_key(key_part);
        let value = value_part.trim().to_string();
        let map: &mut BTreeMap<String, String> = match &mut current {
            Section::Interface => &mut interface,
            Section::Peer(index) => &mut peers[*index],
            Section::None => unreachable!(),
        };
        if ["address", "allowedips", "dns"].contains(&key.as_str())
            && map.get(&key).map(|existing| !existing.is_empty()).unwrap_or(false)
        {
            let existing = map.get_mut(&key).unwrap();
            existing.push(',');
            existing.push_str(&value);
        } else {
            map.insert(key, value);
        }
    }

    let private_key = interface.get("privatekey").cloned().unwrap_or_default();
    if peers.is_empty() || private_key.is_empty() {
        return Err(WarpGenerationError(
            "wireguard config must contain [Interface] PrivateKey and [Peer] Endpoint/PublicKey"
                .into(),
        ));
    }

    let mut parsed_peers: Vec<Value> = Vec::new();
    let mut first_server = String::new();
    let mut first_port: i64 = 0;
    for peer in &peers {
        let (server, port) = split_endpoint(peer.get("endpoint").map(String::as_str).unwrap_or(""));
        let public_key = peer.get("publickey").cloned().unwrap_or_default();
        if server.is_empty() || public_key.is_empty() {
            return Err(WarpGenerationError(
                "each [Peer] must contain Endpoint and PublicKey".into(),
            ));
        }
        if first_server.is_empty() {
            first_server = server.clone();
            first_port = if port > 0 { port } else { 51820 };
        }
        let allowed_raw = peer
            .get("allowedips")
            .cloned()
            .unwrap_or_else(|| "0.0.0.0/0,::/0".to_string());
        let allowed = normalize_ip_prefixes(&Value::Array(
            split_csv(&allowed_raw).into_iter().map(Value::String).collect(),
        ));
        parsed_peers.push(build_wireguard_peer(
            &server,
            if port > 0 { port } else { 51820 },
            &public_key,
            &allowed,
            peer.get("presharedkey").map(String::as_str),
            peer.get("persistentkeepalive").map(String::as_str),
        )?);
    }

    let amnezia = amnezia_from_params(&interface)?;
    let reserved_source = interface
        .get("reserved")
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            peers
                .first()
                .and_then(|peer| peer.get("reserved"))
                .filter(|value| !value.trim().is_empty())
        })
        .map(String::as_str);
    let reserved = parse_reserved_bytes(reserved_source)?;

    let warp_profile = is_warp_endpoint(&first_server) && !reserved.is_empty();
    let endpoint = if warp_profile {
        build_warp_profile_endpoint(
            &private_key,
            peers[0].get("persistentkeepalive").map(String::as_str),
            &amnezia,
            interface.get("listenport").map(String::as_str),
        )?
    } else {
        let mut endpoint = Map::new();
        endpoint.insert("type".into(), json!("wireguard"));
        endpoint.insert("tag".into(), json!("proxy"));
        let mtu_raw = interface.get("mtu").cloned().unwrap_or_default();
        let mtu = if mtu_raw.is_empty() {
            1408
        } else {
            parse_config_int(&mtu_raw, "MTU")?
        };
        endpoint.insert("mtu".into(), json!(mtu));
        let address_raw = interface
            .get("address")
            .cloned()
            .unwrap_or_else(|| "10.0.0.2/32".to_string());
        endpoint.insert(
            "address".into(),
            Value::Array(
                normalize_ip_prefixes(&Value::Array(
                    split_csv(&address_raw).into_iter().map(Value::String).collect(),
                ))
                .into_iter()
                .map(Value::String)
                .collect(),
            ),
        );
        endpoint.insert("private_key".into(), json!(clean_b64_key(&private_key)));
        endpoint.insert("peers".into(), Value::Array(parsed_peers));
        let udp_timeout = interface.get("udptimeout").cloned().unwrap_or_default();
        let udp_value = if udp_timeout.is_empty() { "5m0s".to_string() } else { udp_timeout };
        endpoint.insert("udp_timeout".into(), json!(udp_value));
        if let Some(listen) = interface.get("listenport").filter(|value| !value.is_empty()) {
            let port = parse_config_int(listen, "listen port")?;
            endpoint.insert("listen_port".into(), json!(port));
        }
        if amnezia.as_object().map(|obj| !obj.is_empty()).unwrap_or(false) {
            endpoint.insert("amnezia".into(), amnezia.clone());
        }
        apply_wireguard_performance_options(
            &mut endpoint,
            interface.get("workers").map(String::as_str),
            interface
                .get("preallocatedbuffersperpool")
                .map(String::as_str),
            interface.get("disablepauses").map(String::as_str),
        )?;
        Value::Object(endpoint)
    };

    let scheme = if amnezia
        .as_object()
        .map(|obj| !obj.is_empty())
        .unwrap_or(false)
    {
        "awg"
    } else if endpoint.get("type").and_then(Value::as_str) == Some("warp") {
        "warp"
    } else {
        "wireguard"
    };
    let mut outbound = native_singbox_outbound(&endpoint)?;
    let dns_servers = split_csv(interface.get("dns").map(String::as_str).unwrap_or(""));
    if !dns_servers.is_empty() {
        outbound["_dns"] = json!(dns_servers);
    }
    let mut protocol_masking = Map::new();
    for key in ["id", "ip", "ib"] {
        if let Some(value) = interface
            .get(key)
            .filter(|value| !value.trim().is_empty())
        {
            protocol_masking.insert(key.to_string(), json!(value.clone()));
        }
    }
    if !protocol_masking.is_empty() {
        // sing-box-extended currently has no Id/Ip/Ib schema.  Preserve the
        // source values outside the native endpoint instead of silently
        // destroying them or passing unknown fields to the strict decoder.
        outbound["_protocol_masking"] = Value::Object(protocol_masking);
    }

    let mut node = Node::new();
    node.name = format!("{first_server}:{first_port}");
    node.scheme = scheme.to_string();
    node.server = first_server;
    node.port = first_port;
    node.link = text.to_string();
    node.outbound = outbound;
    node.tags = if is_warp_endpoint(&node.server) {
        vec![WARP_GROUP.to_string()]
    } else {
        Vec::new()
    };
    Ok(node)
}

// ── Node builders ──

pub fn build_awg_nodes(
    registration: &WarpRegistration,
    rng: Option<&mut dyn RngCore>,
) -> std::result::Result<Vec<Node>, WarpGenerationError> {
    let mut default_rng = rand::thread_rng();
    let rng: &mut dyn RngCore = match rng {
        Some(rng) => rng,
        None => &mut default_rng,
    };
    let mut nodes: Vec<Node> = Vec::new();
    for (name, masks) in AWG_PROFILES {
        let endpoint = random_endpoint(rng);
        let config = build_awg_config(registration, masks, &endpoint);
        let node = parse_wireguard_ini(&config)?;
        nodes.push(finish_node(node, name));
    }
    Ok(nodes)
}

pub fn build_masque_nodes(
    registration: &WarpRegistration,
) -> std::result::Result<Vec<Node>, WarpGenerationError> {
    let common = masque_common_mapping(registration);
    let mut nodes: Vec<Node> = Vec::new();
    for (name, network) in [("WARP MASQUE", None), ("WARP MASQUE · HTTP/2", Some("h2"))] {
        let mapping = masque_payload_mapping(name, &common, network);
        let yaml_text = serde_yaml::to_string(&mapping)
            .map_err(|exc| WarpGenerationError(exc.to_string()))?;
        let payload: Value = serde_yaml::from_str(&yaml_text)
            .map_err(|exc| WarpGenerationError(exc.to_string()))?;
        let link = yaml_to_compact_json(&serde_yaml::Value::Mapping(mapping));
        let node = parse_masque_payload(&payload, link)?;
        nodes.push(finish_node(node, name));
    }
    Ok(nodes)
}

/// Convert WARP-generator Clash MASQUE entries to sing-box-extended.
fn parse_masque_payload(
    payload: &Value,
    link: String,
) -> std::result::Result<Node, WarpGenerationError> {
    let server = py_str_or_empty(payload.get("server").unwrap_or(&Value::Null))
        .trim()
        .to_string();
    let server_ports = clash_list(&pick_first_truthy(&[
        payload.get("server-ports").unwrap_or(&Value::Null),
        payload.get("server_ports").unwrap_or(&Value::Null),
        payload.get("ports").unwrap_or(&Value::Null),
    ]));
    let port_source = pick_first_truthy(&[
        payload.get("port").unwrap_or(&Value::Null),
        payload.get("server-port").unwrap_or(&Value::Null),
    ]);
    let port_value = if truthy(&port_source) {
        port_source
    } else {
        match representative_clash_port(&server_ports) {
            Some(port) => json!(port),
            None => Value::Null,
        }
    };
    let port = int_from_value(&port_value)?;
    let name = {
        let source = pick_first_truthy(&[
            payload.get("name").unwrap_or(&Value::Null),
            payload.get("tag").unwrap_or(&Value::Null),
        ]);
        let fallback = format!("masque-{server}:{port}");
        if truthy(&source) {
            py_str(&source).trim().to_string()
        } else {
            fallback
        }
    };
    if server.is_empty() || port <= 0 {
        return Err(WarpGenerationError("proxy must contain server and port".into()));
    }

    let network = py_str_or_empty(payload.get("network").unwrap_or(&Value::Null))
        .trim()
        .to_lowercase();
    let mut profile = Map::new();
    profile.insert("detour".into(), json!("direct"));
    for (source, target) in [
        ("profile-id", "id"),
        ("profile_id", "id"),
        ("auth-token", "auth_token"),
        ("auth_token", "auth_token"),
        ("masque-private-key", "private_key"),
        ("masque_private_key", "private_key"),
    ] {
        let value = py_str_or_empty(payload.get(source).unwrap_or(&Value::Null))
            .trim()
            .to_string();
        if !value.is_empty() {
            profile.insert(target.to_string(), json!(value));
        }
    }
    let private_key = py_str_or_empty(&pick_first_truthy(&[
        payload.get("private-key").unwrap_or(&Value::Null),
        payload.get("private_key").unwrap_or(&Value::Null),
    ]))
    .trim()
    .to_string();
    let public_key = py_str_or_empty(&pick_first_truthy(&[
        payload.get("public-key").unwrap_or(&Value::Null),
        payload.get("public_key").unwrap_or(&Value::Null),
    ]))
    .trim()
    .to_string();
    let mut address = normalize_ip_prefixes(&Value::Array(
        clash_list(&pick_first_truthy(&[
            payload.get("ip").unwrap_or(&Value::Null),
            payload.get("address").unwrap_or(&Value::Null),
        ]))
        .into_iter()
        .map(Value::String)
        .collect(),
    ));
    for item in normalize_ip_prefixes(&Value::Array(
        clash_list(payload.get("ipv6").unwrap_or(&Value::Null))
            .into_iter()
            .map(Value::String)
            .collect(),
    )) {
        if !address.contains(&item) {
            address.push(item);
        }
    }
    let mut native = Map::new();
    native.insert("type".into(), json!("masque"));
    native.insert("tag".into(), json!("proxy"));
    native.insert("system".into(), json!(false));
    native.insert("name".into(), json!("masque0"));
    native.insert("use_http2".into(), json!(network == "h2" || network == "http2"));
    native.insert(
        "use_ipv6".into(),
        json!(to_bool_value(&pick_first_truthy(&[
            payload.get("use-ipv6").unwrap_or(&Value::Null),
            payload.get("use_ipv6").unwrap_or(&Value::Null),
        ]))),
    );
    native.insert("profile".into(), Value::Object(profile));
    native.insert("udp_timeout".into(), json!("5m0s"));
    native.insert("udp_keepalive_period".into(), json!("30s"));
    native.insert("reconnect_delay".into(), json!("5s"));
    native.insert("congestion_controller".into(), json!("bbr"));
    // Mihomo/usque MASQUE profiles contain a ready-to-use ECDSA client key,
    // endpoint public key and tunnel addresses.  Keep those values together;
    // treating this format as an empty Cloudflare registration profile leaves
    // the outbound permanently in "tunnel not initialized" state.
    if !private_key.is_empty() || !public_key.is_empty() {
        if private_key.is_empty() || public_key.is_empty() || server.is_empty() {
            return Err(WarpGenerationError(
                "direct MASQUE proxy must contain server, private-key and public-key".into(),
            ));
        }
        if address.is_empty() {
            return Err(WarpGenerationError(
                "direct MASQUE proxy must contain ip, ipv6 or address".into(),
            ));
        }
        native.insert("server".into(), json!(server.clone()));
        native.insert("server_port".into(), json!(port));
        native.insert("private_key".into(), json!(private_key));
        native.insert("public_key".into(), json!(public_key));
        native.insert("tunnel_address".into(), json!(address));
        let mtu_source = payload.get("mtu").cloned().unwrap_or(Value::Null);
        let mtu = if truthy(&mtu_source) {
            int_from_value(&mtu_source)?
        } else {
            1280
        };
        native.insert("mtu".into(), json!(mtu));
    }
    let allowed_ips = normalize_ip_prefixes(&Value::Array(
        clash_list(&pick_first_truthy(&[
            payload.get("allowed-ips").unwrap_or(&Value::Null),
            payload.get("allowed_ips").unwrap_or(&Value::Null),
        ]))
        .into_iter()
        .map(Value::String)
        .collect(),
    ));
    if !allowed_ips.is_empty() {
        native.insert("allowed_ips".into(), json!(allowed_ips));
    }
    let server_name = py_str_or_empty(&pick_first_truthy(&[
        payload.get("sni").unwrap_or(&Value::Null),
        payload.get("servername").unwrap_or(&Value::Null),
    ]))
    .trim()
    .to_string();
    if !server_name.is_empty() {
        native.insert("tls".into(), json!({"server_name": server_name}));
    }
    let mut outbound = json!({
        "protocol": "masque",
        "singbox": Value::Object(native),
    });
    let dns_servers = clash_list(payload.get("dns").unwrap_or(&Value::Null));
    if !dns_servers.is_empty() {
        outbound["_dns"] = json!(dns_servers);
    }
    let mut node = Node::new();
    node.name = name;
    node.scheme = "masque".into();
    node.server = server;
    node.port = port;
    node.link = link;
    node.outbound = outbound;
    node.tags = vec![WARP_GROUP.to_string()];
    Ok(node)
}

/// `_representative_clash_port`: the first `^\s*(\d{1,5})` match in 1..=65535.
fn representative_clash_port(ranges: &[String]) -> Option<i64> {
    for value in ranges {
        let trimmed = value.trim_start();
        let digits: String = trimmed
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .take(5)
            .collect();
        if let Ok(port) = digits.parse::<i64>() {
            if (1..=65535).contains(&port) {
                return Some(port);
            }
        }
    }
    None
}

// ── Generation ──

/// Fetch AWG and MASQUE registrations; succeed when at least one works.
pub async fn generate_warp_nodes(
    client: &reqwest::Client,
    rng: Option<&mut dyn RngCore>,
    cancelled: Option<&(dyn Fn() -> bool + Send + Sync)>,
) -> std::result::Result<WarpResult, WarpGenerationError> {
    let mut result = WarpResult::default();

    match fetch_registration(
        client,
        &AWG_DATA_ENDPOINTS,
        &[("X-Client", "WARP")],
        Duration::from_secs_f64(REQUEST_TIMEOUT),
        cancelled,
    )
    .await
    {
        Ok(registration) => match build_awg_nodes(&registration, rng) {
            Ok(nodes) => result.nodes.extend(nodes),
            Err(exc) => result.errors.push(format!("AmneziaWG: {exc}")),
        },
        Err(exc) => result.errors.push(format!("AmneziaWG: {exc}")),
    }
    match fetch_registration(
        client,
        &MASQUE_DATA_ENDPOINTS,
        &[],
        Duration::from_secs_f64(REQUEST_TIMEOUT),
        cancelled,
    )
    .await
    {
        Ok(registration) => match build_masque_nodes(&registration) {
            Ok(nodes) => result.nodes.extend(nodes),
            Err(exc) => result.errors.push(format!("MASQUE: {exc}")),
        },
        Err(exc) => result.errors.push(format!("MASQUE: {exc}")),
    }
    if result.nodes.is_empty() {
        return Err(WarpGenerationError(if result.errors.is_empty() {
            "no WARP profiles".into()
        } else {
            result.errors.join("; ")
        }));
    }
    Ok(result)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn sample_registration() -> WarpRegistration {
        WarpRegistration {
            private_key: "qJkASjBMXZLQsVRw5+Jm1GbdsmmTPGwpWUwyG4MyanE=".to_string(),
            public_key: "bmXOC+F1FxEMF9dyiK2H5/1SUtzH0JuVo51f2mJszW0=".to_string(),
            ipv4: "172.16.0.2".to_string(),
            ipv6: "2606:4700:110:89bc:b1cf:7a71:3abf:392b".to_string(),
        }
    }

    #[test]
    fn test_parse_registration() {
        let payload = json!({
            "privKey": " qJkASjBMXZLQsVRw5+Jm1GbdsmmTPGwpWUwyG4MyanE= ",
            "peer_pub": "bmXOC+F1FxEMF9dyiK2H5/1SUtzH0JuVo51f2mJszW0=",
            "client_ipv4": "172.16.0.2",
            "client_ipv6": "2606:4700:110:89bc:b1cf:7a71:3abf:392b",
        });
        let registration = parse_registration(&payload).unwrap();
        assert_eq!(registration, sample_registration());

        // Alias keys.
        let payload = json!({
            "private_key": "KEY", "public_key": "PUB", "ipv4": "10.0.0.2",
        });
        let registration = parse_registration(&payload).unwrap();
        assert_eq!(registration.ipv4, "10.0.0.2");
        assert!(registration.ipv6.is_empty());

        // Missing key or address fails.
        let err = parse_registration(&json!({"privKey": "K", "peer_pub": "P"}))
            .unwrap_err()
            .to_string();
        assert_eq!(err, "response has no keys or address");
        // Non-dict payload.
        let err = parse_registration(&json!("nope")).unwrap_err().to_string();
        assert_eq!(err, "unexpected response format");
        // Whitespace inside a value empties it.
        let err = parse_registration(&json!({
            "privKey": "has space", "peer_pub": "P", "client_ipv4": "1.2.3.4"
        }))
        .unwrap_err()
        .to_string();
        assert_eq!(err, "response has no keys or address");
    }

    #[test]
    fn test_build_awg_config() {
        let registration = WarpRegistration {
            private_key: "PRIVKEY=".to_string(),
            public_key: "PUBKEY=".to_string(),
            ipv4: "172.16.0.2".to_string(),
            ipv6: String::new(),
        };
        let config = build_awg_config(&registration, &[("I1", "abcd1234")], "162.159.192.5:500");
        let expected = "[Interface]\n\
            PrivateKey = PRIVKEY=\n\
            Address = 172.16.0.2\n\
            DNS = 1.1.1.1, 1.0.0.1\n\
            MTU = 1280\n\
            S1 = 0\nS2 = 0\nS3 = 0\nS4 = 0\n\
            Jc = 4\nJmin = 40\nJmax = 70\n\
            H1 = 1\nH2 = 2\nH3 = 3\nH4 = 4\n\
            I1 = <b 0xabcd1234>\n\
            \n\
            [Peer]\n\
            PublicKey = PUBKEY=\n\
            AllowedIPs = 0.0.0.0/0, ::/0\n\
            Endpoint = 162.159.192.5:500\n";
        assert_eq!(config, expected);
    }

    #[test]
    fn test_parse_wireguard_ini_awg_node() {
        let registration = sample_registration();
        let masks = AWG_PROFILES[2].1;
        let config = build_awg_config(&registration, masks, "engage.cloudflareclient.com:4500");
        let node = parse_wireguard_ini(&config).unwrap();
        assert_eq!(node.scheme, "awg");
        assert_eq!(node.server, "engage.cloudflareclient.com");
        assert_eq!(node.port, 4500);
        assert_eq!(node.link, config);
        assert_eq!(node.tags, vec![WARP_GROUP.to_string()]);
        let outbound = &node.outbound;
        assert_eq!(outbound["protocol"], "awg");
        let singbox = &outbound["singbox"];
        assert_eq!(singbox["type"], "wireguard");
        assert_eq!(singbox["system"], false);
        assert_eq!(singbox["mtu"], 1280);
        assert_eq!(singbox["address"], json!(["172.16.0.2/32"]));
        assert_eq!(singbox["private_key"], registration.private_key);
        assert_eq!(
            singbox["peers"][0]["address"],
            "engage.cloudflareclient.com"
        );
        assert_eq!(singbox["peers"][0]["port"], 4500);
        assert_eq!(singbox["peers"][0]["public_key"], registration.public_key);
        assert_eq!(
            singbox["peers"][0]["allowed_ips"],
            json!(["0.0.0.0/0", "::/0"])
        );
        let amnezia = &singbox["amnezia"];
        assert_eq!(amnezia["jc"], 4);
        assert_eq!(amnezia["jmin"], 40);
        assert_eq!(amnezia["jmax"], 70);
        assert_eq!(amnezia["s1"], 0);
        assert_eq!(amnezia["h4"], 4);
        assert_eq!(amnezia["i1"], format!("<b 0x{}>", masks[0].1));
        if masks.len() > 1 {
            assert_eq!(amnezia["i2"], format!("<b 0x{}>", masks[1].1));
        }
        assert_eq!(outbound["_dns"], json!(["1.1.1.1", "1.0.0.1"]));
    }

    #[test]
    fn test_parse_wireguard_ini_warp_profile_and_errors() {
        // Reserved bytes + Cloudflare peer produce a WARP profile endpoint.
        let config = "[Interface]\nPrivateKey = PRIV=\nAddress = 10.0.0.2/32\n[Peer]\nPublicKey = PUB=\nAllowedIPs = 0.0.0.0/0, ::/0\nEndpoint = 162.159.192.1:2408\nPersistentKeepalive = 5\nReserved = AQID\n";
        let node = parse_wireguard_ini(config).unwrap();
        assert_eq!(node.scheme, "warp");
        assert_eq!(node.outbound["singbox"]["type"], "warp");
        assert_eq!(node.outbound["singbox"]["profile"]["private_key"], "PRIV=");
        assert_eq!(node.outbound["singbox"]["persistent_keepalive_interval"], 5);

        // Missing keys fail with the Python error text.
        let err = parse_wireguard_ini("[Interface]\n[Peer]\nEndpoint = x:1\n")
            .unwrap_err()
            .to_string();
        assert_eq!(
            err,
            "wireguard config must contain [Interface] PrivateKey and [Peer] Endpoint/PublicKey"
        );
        let err = parse_wireguard_ini("[Interface]\nPrivateKey = K\n[Peer]\nPublicKey = P\n")
            .unwrap_err()
            .to_string();
        assert_eq!(err, "each [Peer] must contain Endpoint and PublicKey");
        // Invalid reserved bytes.
        let err = parse_wireguard_ini(
            "[Interface]\nPrivateKey = K\n[Peer]\nPublicKey = P\nEndpoint = x:1\nReserved = nothex!\n",
        )
        .unwrap_err()
        .to_string();
        assert_eq!(err, "wireguard reserved must contain three bytes");
    }

    #[test]
    fn test_build_masque_payloads_and_nodes() {
        let registration = sample_registration();
        let payloads = build_masque_payloads(&registration);
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0]["type"], "masque");
        assert_eq!(payloads[0]["server"], MASQUE_SERVER);
        assert_eq!(payloads[0]["port"], 443);
        assert_eq!(payloads[0]["sni"], MASQUE_SNI);
        assert_eq!(payloads[0]["private-key"], registration.private_key);
        assert_eq!(payloads[0]["ip"], registration.ipv4);
        assert_eq!(payloads[0]["mtu"], 1280);
        assert_eq!(payloads[0]["udp"], true);
        assert_eq!(payloads[0]["remote-dns-resolve"], true);
        assert_eq!(payloads[0]["ipv6"], registration.ipv6);
        assert!(payloads[1].get("network").is_none() || payloads[1]["network"] == "h2");
        assert_eq!(payloads[1]["network"], "h2");

        let nodes = build_masque_nodes(&registration).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].name, "WARP MASQUE");
        assert_eq!(nodes[0].scheme, "masque");
        assert_eq!(nodes[0].group, WARP_GROUP);
        assert_eq!(nodes[0].tags, vec![WARP_GROUP.to_string()]);
        assert_eq!(nodes[0].server, MASQUE_SERVER);
        assert_eq!(nodes[0].port, 443);
        let outbound = &nodes[0].outbound;
        assert_eq!(outbound["protocol"], "masque");
        let singbox = &outbound["singbox"];
        assert_eq!(singbox["type"], "masque");
        assert_eq!(singbox["system"], false);
        assert_eq!(singbox["name"], "masque0");
        assert_eq!(singbox["use_http2"], false);
        assert_eq!(singbox["profile"]["detour"], "direct");
        assert_eq!(singbox["server"], MASQUE_SERVER);
        assert_eq!(singbox["server_port"], 443);
        assert_eq!(singbox["private_key"], registration.private_key);
        assert_eq!(singbox["public_key"], registration.public_key);
        assert_eq!(
            singbox["tunnel_address"],
            json!(["172.16.0.2/32", "2606:4700:110:89bc:b1cf:7a71:3abf:392b/128"])
        );
        assert_eq!(singbox["mtu"], 1280);
        assert_eq!(singbox["tls"]["server_name"], MASQUE_SNI);
        assert_eq!(singbox["udp_timeout"], "5m0s");
        assert_eq!(singbox["udp_keepalive_period"], "30s");
        assert_eq!(singbox["reconnect_delay"], "5s");
        assert_eq!(singbox["congestion_controller"], "bbr");
        assert_eq!(
            outbound["_dns"],
            json!(["1.1.1.1", "1.0.0.1", "2606:4700:4700::1111", "2606:4700:4700::1001"])
        );

        // The HTTP/2 variant flips use_http2 and keeps the name.
        assert_eq!(nodes[1].name, "WARP MASQUE · HTTP/2");
        assert_eq!(nodes[1].outbound["singbox"]["use_http2"], true);
        // The link keeps Python's json.dumps(payload) shape (compact, named first).
        assert!(nodes[0].link.starts_with("{\"name\":\"WARP MASQUE\",\"type\":\"masque\""));
    }

    #[test]
    fn test_masque_payload_validation_errors() {
        // Missing tunnel address rejects a direct MASQUE proxy.
        let registration = WarpRegistration {
            private_key: "K".into(),
            public_key: "P".into(),
            ipv4: String::new(),
            ipv6: String::new(),
        };
        let err = build_masque_nodes(&registration).unwrap_err().to_string();
        assert_eq!(
            err,
            "direct MASQUE proxy must contain ip, ipv6 or address"
        );
    }

    #[test]
    fn test_random_endpoint() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for _ in 0..50 {
            let endpoint = random_endpoint(&mut rng);
            let (host, port) = endpoint.rsplit_once(':').unwrap();
            assert!(WARP_ENDPOINT_PORTS.contains(&port.parse::<u16>().unwrap()), "{endpoint}");
            if host.ends_with(|c: char| c.is_ascii_digit()) {
                // Dotted prefix: random last octet 1-10.
                let last = host.rsplit('.').next().unwrap().parse::<u8>().unwrap();
                assert!((1..=10).contains(&last), "{endpoint}");
            }
        }
        // Engage host keeps its suffix form.
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let mut saw_engage = false;
        for _ in 0..500 {
            let endpoint = random_endpoint(&mut rng);
            if endpoint.starts_with("engage.cloudflareclient.com:") {
                saw_engage = true;
                break;
            }
        }
        assert!(saw_engage);
    }

    #[test]
    fn test_yaml_payload_text_shape() {
        // The serde_yaml emission produces mihomo-style keys in order.
        let registration = sample_registration();
        let common = masque_common_mapping(&registration);
        let mapping = masque_payload_mapping("WARP MASQUE", &common, None);
        let yaml = serde_yaml::to_string(&mapping).unwrap();
        assert!(yaml.starts_with("name: WARP MASQUE\n"));
        assert!(yaml.contains("type: masque\n"));
        assert!(yaml.contains("server: 162.159.198.2\n"));
        assert!(yaml.contains("port: 443\n"));
        assert!(yaml.contains("sni: 4pda.to\n"));
        assert!(yaml.contains("mtu: 1280\n"));
        assert!(yaml.contains("udp: true\n"));
        assert!(yaml.contains("remote-dns-resolve: true\n"));
        assert!(yaml.contains("dns:\n"));
    }

    #[test]
    fn test_host_of_and_representative_port() {
        assert_eq!(host_of("https://www.warp-generator.workers.dev"), "www.warp-generator.workers.dev");
        assert_eq!(host_of("https://warp-gen.netlify.app/"), "warp-gen.netlify.app");
        assert_eq!(representative_clash_port(&["500-1000".into()]), Some(500));
        assert_eq!(representative_clash_port(&["abc".into(), "443".into()]), Some(443));
        assert_eq!(representative_clash_port(&["0".into()]), None);
    }
}
