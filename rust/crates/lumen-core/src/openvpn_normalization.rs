//! OpenVPN outbound normalization (`openvpn_normalization.py`).
//!
//! Migrates stored/legacy OpenVPN data to the sing-box 1.14 native
//! ``openvpn-client`` endpoint schema and normalizes cipher/digest names.

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

pub const DATA_CIPHERS: [&str; 7] = [
    "AES-128-GCM",
    "AES-192-GCM",
    "AES-256-GCM",
    "AES-128-CBC",
    "AES-192-CBC",
    "AES-256-CBC",
    "CHACHA20-POLY1305",
];

pub const AUTH_DIGESTS: [&str; 5] = ["MD5", "SHA1", "SHA256", "SHA384", "SHA512"];

const GO_TLS_CIPHER_SUITES: [&str; 10] = [
    "TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA",
    "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA",
    "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
    "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA",
    "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
    "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
    "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
    "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
    "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
    "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
];

const OPENSSL_TLS_ALIASES: [(&str, &str); 10] = [
    ("ECDHE-ECDSA-AES128-SHA", "TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA"),
    ("ECDHE-ECDSA-AES256-SHA", "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA"),
    ("ECDHE-RSA-AES128-SHA", "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA"),
    ("ECDHE-RSA-AES256-SHA", "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA"),
    (
        "ECDHE-ECDSA-AES128-GCM-SHA256",
        "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
    ),
    (
        "ECDHE-ECDSA-AES256-GCM-SHA384",
        "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
    ),
    (
        "ECDHE-RSA-AES128-GCM-SHA256",
        "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
    ),
    (
        "ECDHE-RSA-AES256-GCM-SHA384",
        "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
    ),
    (
        "ECDHE-RSA-CHACHA20-POLY1305",
        "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
    ),
    (
        "ECDHE-ECDSA-CHACHA20-POLY1305",
        "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
    ),
];

// ── Python semantics helpers ──

/// Python `str(value)` for the JSON shapes this module encounters.
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

/// Python `str(value or "")`: falsy values render as an empty string.
fn py_str_or_empty(value: &Value) -> String {
    if truthy(value) {
        py_str(value)
    } else {
        String::new()
    }
}

fn take_key(map: &mut Map<String, Value>, key: &str) -> Value {
    map.remove(key).unwrap_or(Value::Null)
}

// ── Cipher/digest normalization ──

pub fn normalize_data_cipher(value: &Value) -> String {
    let normalized = py_str_or_empty(value).trim().to_uppercase();
    if DATA_CIPHERS.contains(&normalized.as_str()) {
        normalized
    } else {
        String::new()
    }
}

pub fn normalize_auth_digest(value: &Value) -> String {
    let normalized = py_str_or_empty(value).trim().to_uppercase();
    if AUTH_DIGESTS.contains(&normalized.as_str()) {
        normalized
    } else {
        String::new()
    }
}

pub fn normalize_tls_cipher_suite(value: &Value) -> String {
    let normalized = py_str_or_empty(value).trim().to_uppercase();
    if normalized.is_empty() {
        return String::new();
    }
    if GO_TLS_CIPHER_SUITES.contains(&normalized.as_str()) {
        return normalized;
    }
    if let Some((_, mapped)) = OPENSSL_TLS_ALIASES
        .iter()
        .find(|(alias, _)| *alias == normalized)
    {
        return (*mapped).to_string();
    }
    let iana = normalized.replace('-', "_");
    if GO_TLS_CIPHER_SUITES.contains(&iana.as_str()) {
        iana
    } else {
        String::new()
    }
}

pub fn normalize_tls_cipher_suites(value: &Value) -> Vec<String> {
    let raw_values: Vec<String> = match value {
        Value::Array(items) => items.iter().map(py_str).collect(),
        Value::Null => Vec::new(),
        Value::String(s) if s.is_empty() => Vec::new(),
        other => vec![py_str(other)],
    };
    let mut result: Vec<String> = Vec::new();
    for raw in raw_values {
        for candidate in raw.split(':') {
            let normalized = normalize_tls_cipher_suite(&Value::String(candidate.to_string()));
            if !normalized.is_empty() && !result.contains(&normalized) {
                result.push(normalized);
            }
        }
    }
    result
}

// ── Legacy outbound → sing-box 1.14 native endpoint migration ──

/// Migrate stored/legacy OpenVPN data to the current endpoint schema.
///
/// sing-box 1.14 moved OpenVPN from the old extended ``openvpn`` outbound to
/// the native ``openvpn-client`` endpoint.  Lumen keeps the legacy shape in
/// its database so older profiles remain editable, and converts only the
/// disposable runtime copy here.
pub fn normalize_openvpn_outbound(outbound: &mut Value) -> Result<()> {
    if !outbound.is_object() {
        return Ok(());
    }
    let outbound_type = py_str_or_empty(outbound.get("type").unwrap_or(&Value::Null))
        .trim()
        .to_lowercase();
    if outbound_type != "openvpn" && outbound_type != "openvpn-client" {
        return Ok(());
    }
    let legacy = outbound_type == "openvpn";
    let obj = outbound.as_object_mut().unwrap();

    let popped_proto = take_key(obj, "proto");
    let network_source = if truthy(&popped_proto) {
        popped_proto
    } else {
        let stored = obj.get("network").cloned().unwrap_or(Value::Null);
        if truthy(&stored) {
            stored
        } else if legacy {
            Value::String("tcp".into())
        } else {
            Value::String("udp".into())
        }
    };
    let proto = normalize_openvpn_network(&network_source)?;

    obj.insert("type".into(), json!("openvpn-client"));
    obj.insert("network".into(), json!(proto.clone()));
    obj.insert("system".into(), json!(false));
    let name = py_str_or_empty(obj.get("name").unwrap_or(&Value::Null));
    let name_value = if name.is_empty() { "openvpn0".to_string() } else { name };
    obj.insert("name".into(), json!(name_value));

    if let Some(servers) = obj.get("servers").cloned() {
        if servers.is_array() {
            let mut normalized_servers: Vec<Value> = Vec::new();
            for raw_server in servers.as_array().unwrap() {
                if !raw_server.is_object() {
                    continue;
                }
                let mut server = raw_server.clone();
                let sobj = server.as_object_mut().unwrap();
                let popped = take_key(sobj, "proto");
                let remote_network = if truthy(&popped) {
                    popped
                } else {
                    let stored = sobj.get("network").cloned().unwrap_or(Value::Null);
                    if truthy(&stored) {
                        stored
                    } else {
                        Value::String(proto.clone())
                    }
                };
                sobj.insert("network".into(), json!(normalize_openvpn_network(&remote_network)?));
                normalized_servers.push(server);
            }
            obj.insert("servers".into(), Value::Array(normalized_servers));
        }
    }

    let mut raw_cipher = String::new();
    if legacy {
        raw_cipher = py_str_or_empty(&take_key(obj, "cipher")).trim().to_string();
    }
    if legacy && !raw_cipher.is_empty() {
        let cipher = normalize_data_cipher(&Value::String(raw_cipher.clone()));
        if !cipher.is_empty() {
            obj.insert("data_ciphers".into(), json!([cipher.clone()]));
            obj.insert("data_ciphers_fallback".into(), json!(cipher));
        } else {
            bail!("Unsupported OpenVPN data cipher `{}`", raw_cipher);
        }
    } else if obj.contains_key("data_ciphers") {
        let raw_ciphers = obj.get("data_ciphers").cloned().unwrap_or(Value::Null);
        let candidates: Vec<Value> = match &raw_ciphers {
            Value::Array(items) => items.clone(),
            other => py_str_or_empty(other)
                .split(':')
                .map(|part| Value::String(part.to_string()))
                .collect(),
        };
        let mut ciphers: Vec<String> = Vec::new();
        for candidate in &candidates {
            let normalized = normalize_data_cipher(candidate);
            if !normalized.is_empty() {
                ciphers.push(normalized);
            }
        }
        let raw_is_empty = match &raw_ciphers {
            Value::Null => true,
            Value::String(s) => s.is_empty(),
            Value::Array(a) => a.is_empty(),
            _ => false,
        };
        if ciphers.is_empty() && !raw_is_empty {
            bail!("Unsupported OpenVPN data cipher");
        }
        obj.insert("data_ciphers".into(), json!(ciphers));
    }

    let raw_auth = py_str_or_empty(obj.get("auth").unwrap_or(&Value::Null))
        .trim()
        .to_string();
    let auth = normalize_auth_digest(&Value::String(raw_auth.clone()));
    if !auth.is_empty() {
        obj.insert("auth".into(), json!(auth));
    } else if !raw_auth.is_empty() {
        bail!("Unsupported OpenVPN auth digest `{}`", raw_auth);
    }

    if let Some(tls) = obj.get("tls").cloned() {
        if tls.is_object() {
            let mut tls = tls;
            let tobj = tls.as_object_mut().unwrap();
            if legacy {
                let ca = take_key(tobj, "ca");
                let ca_path = take_key(tobj, "ca_path");
                let client_certificate = take_key(tobj, "certificate");
                let client_certificate_path = take_key(tobj, "certificate_path");
                let client_key = take_key(tobj, "key");
                let client_key_path = take_key(tobj, "key_path");
                if truthy(&ca) {
                    tobj.insert("certificate".into(), ca);
                }
                if truthy(&ca_path) {
                    tobj.insert("certificate_path".into(), ca_path);
                }
                if truthy(&client_certificate) {
                    tobj.insert("client_certificate".into(), client_certificate);
                }
                if truthy(&client_certificate_path) {
                    tobj.insert("client_certificate_path".into(), client_certificate_path);
                }
                if truthy(&client_key) {
                    tobj.insert("client_key".into(), client_key);
                }
                if truthy(&client_key_path) {
                    tobj.insert("client_key_path".into(), client_key_path);
                }
                let verify_name = take_key(tobj, "verify_x509_name");
                if truthy(&verify_name) {
                    tobj.insert("server_name".into(), verify_name);
                }
                let mut verify_mode = py_str_or_empty(&take_key(tobj, "verify_x509_name_mode"))
                    .trim()
                    .to_lowercase();
                if verify_mode == "exact" {
                    verify_mode = "name".into();
                }
                if !verify_mode.is_empty() {
                    if !["subject", "name", "name-prefix"].contains(&verify_mode.as_str()) {
                        bail!(
                            "OpenVPN verify-x509-name mode `{}` is not supported",
                            verify_mode
                        );
                    }
                    tobj.insert("server_name_type".into(), json!(verify_mode));
                }
                let raw_suites = take_key(tobj, "cipher_suites");
                let suites = normalize_tls_cipher_suites(&raw_suites);
                if !suites.is_empty() {
                    tobj.insert("cipher".into(), json!(suites.join(":")));
                } else {
                    let raw_is_empty = match &raw_suites {
                        Value::Null => true,
                        Value::String(s) => s.is_empty(),
                        Value::Array(a) => a.is_empty(),
                        _ => false,
                    };
                    if !raw_is_empty {
                        bail!("Unsupported OpenVPN TLS cipher suite");
                    }
                }

                let mut control_type = String::new();
                let mut control_key = String::new();
                let mut control_path = String::new();
                let auth_present = {
                    let picked = pick_first_truthy(&[
                        obj.get("tls_auth").unwrap_or(&Value::Null),
                        obj.get("tls_auth_path").unwrap_or(&Value::Null),
                    ]);
                    !py_str_or_empty(&picked).trim().is_empty()
                };
                if auth_present {
                    control_type = "tls_auth".into();
                    control_key = py_str_or_empty(&take_key(obj, "tls_auth"));
                    control_path = py_str_or_empty(&take_key(obj, "tls_auth_path"));
                } else {
                    let crypt_present = {
                        let picked = pick_first_truthy(&[
                            obj.get("tls_crypt").unwrap_or(&Value::Null),
                            obj.get("tls_crypt_path").unwrap_or(&Value::Null),
                        ]);
                        !py_str_or_empty(&picked).trim().is_empty()
                    };
                    if crypt_present {
                        let v2 = take_key(obj, "tls_crypt_v2");
                        control_type = if truthy(&v2) { "tls_crypt_v2" } else { "tls_crypt" }.into();
                        control_key = py_str_or_empty(&take_key(obj, "tls_crypt"));
                        control_path = py_str_or_empty(&take_key(obj, "tls_crypt_path"));
                    }
                }
                if !control_type.is_empty() {
                    let mut control_wrap = Map::new();
                    control_wrap.insert("type".into(), json!(control_type.clone()));
                    if !control_key.is_empty() {
                        control_wrap.insert("key".into(), json!(control_key));
                    } else if !control_path.is_empty() {
                        control_wrap.insert("key_path".into(), json!(control_path));
                    }
                    let direction = take_key(obj, "key_direction");
                    if control_type == "tls_auth" {
                        let rendered = py_str(&direction);
                        if rendered == "0" || rendered == "1" {
                            control_wrap.insert(
                                "direction".into(),
                                json!(if rendered == "0" { "server" } else { "client" }),
                            );
                        }
                    }
                    tobj.insert("control_wrap".into(), Value::Object(control_wrap));
                }
            }
            take_key(tobj, "kernel_tx");
            take_key(tobj, "kernel_rx");
            obj.insert("tls".into(), tls);
        }
    }

    // These belonged to the pre-1.14 extension and are rejected by the native endpoint.
    for key in [
        "tls_auth",
        "tls_auth_path",
        "tls_crypt",
        "tls_crypt_path",
        "tls_crypt_v2",
        "key_direction",
        "key_password",
        "reconnect_delay",
        "allowed_ips",
        "lumen_requires_user_auth",
    ] {
        take_key(obj, key);
    }
    Ok(())
}

fn pick_first_truthy(values: &[&Value]) -> Value {
    for value in values {
        if truthy(value) {
            return (*value).clone();
        }
    }
    Value::Null
}

fn normalize_openvpn_network(value: &Value) -> Result<String> {
    let raw = if truthy(value) {
        py_str(value)
    } else {
        "udp".to_string()
    };
    let network = raw.trim().to_lowercase();
    match network.as_str() {
        "udp" | "udp4" | "udp6" => Ok(network),
        "tcp" | "tcp-client" => Ok("tcp".into()),
        "tcp4" | "tcp4-client" => Ok("tcp4".into()),
        "tcp6" | "tcp6-client" => Ok("tcp6".into()),
        _ => bail!("Unsupported OpenVPN transport `{}`", network),
    }
}

pub fn openvpn_private_key_is_encrypted(outbound: &Value) -> bool {
    let empty = Map::new();
    let tls = outbound
        .get("tls")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let private_key = py_str_or_empty(&pick_first_truthy(&[
        tls.get("key").unwrap_or(&Value::Null),
        tls.get("client_key").unwrap_or(&Value::Null),
    ]))
    .to_uppercase();
    private_key.contains("ENCRYPTED PRIVATE KEY")
        || (private_key.contains("PROC-TYPE:") && private_key.contains("ENCRYPTED"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_normalize_tls_cipher_suite() {
        assert_eq!(
            normalize_tls_cipher_suite(&json!("ECDHE-RSA-AES128-GCM-SHA256")),
            "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256"
        );
        assert_eq!(
            normalize_tls_cipher_suite(&json!("tls_ecdhe_rsa_with_aes_128_gcm_sha256")),
            "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256"
        );
        assert_eq!(
            normalize_tls_cipher_suite(&json!("TLS_AES_128_GCM_SHA256")),
            ""
        );
        assert_eq!(normalize_tls_cipher_suite(&json!("")), "");
        assert_eq!(
            normalize_tls_cipher_suite(&json!("TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA")),
            "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA"
        );
    }

    #[test]
    fn test_normalize_tls_cipher_suites() {
        assert_eq!(
            normalize_tls_cipher_suites(&json!(
                "ECDHE-RSA-AES128-GCM-SHA256:ECDHE-RSA-AES256-GCM-SHA384:BOGUS:ECDHE-RSA-AES128-GCM-SHA256"
            )),
            vec![
                "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
                "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384"
            ]
        );
        assert_eq!(
            normalize_tls_cipher_suites(&json!(["ECDHE-ECDSA-AES128-SHA"])),
            vec!["TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA"]
        );
        assert!(normalize_tls_cipher_suites(&json!("")).is_empty());
        assert!(normalize_tls_cipher_suites(&Value::Null).is_empty());
    }

    #[test]
    fn test_normalize_data_cipher_and_auth() {
        assert_eq!(normalize_data_cipher(&json!("aes-256-gcm")), "AES-256-GCM");
        assert_eq!(normalize_data_cipher(&json!("AES-128-CBC")), "AES-128-CBC");
        assert_eq!(normalize_data_cipher(&json!("BOGUS")), "");
        assert_eq!(normalize_auth_digest(&json!("sha256")), "SHA256");
        assert_eq!(normalize_auth_digest(&json!("MD5")), "MD5");
        assert_eq!(normalize_auth_digest(&json!("SHAKE256")), "");
    }

    #[test]
    fn test_legacy_openvpn_outbound_migration() {
        let mut outbound = json!({
            "type": "openvpn",
            "proto": "udp",
            "name": "legacy0",
            "servers": [{"server": "1.2.3.4", "server_port": 1194, "proto": "udp"}],
            "cipher": "AES-256-GCM",
            "auth": "sha256",
            "tls_auth": "KEYDATA",
            "key_direction": 1,
            "key_password": "pw",
            "tls": {
                "ca": "CERT",
                "certificate": "CLIENTCERT",
                "key": "CLIENTKEY",
                "verify_x509_name": "vpn.example.com",
                "verify_x509_name_mode": "exact",
                "cipher_suites": ["ECDHE-RSA-AES128-GCM-SHA256"],
                "kernel_tx": 1,
                "kernel_rx": 2,
            },
            "reconnect_delay": "5s",
            "allowed_ips": ["0.0.0.0/0"],
            "lumen_requires_user_auth": true,
        });
        normalize_openvpn_outbound(&mut outbound).unwrap();
        assert_eq!(outbound["type"], "openvpn-client");
        assert_eq!(outbound["network"], "udp");
        assert_eq!(outbound["system"], false);
        assert_eq!(outbound["name"], "legacy0");
        assert_eq!(outbound["servers"][0]["network"], "udp");
        assert_eq!(outbound["data_ciphers"], json!(["AES-256-GCM"]));
        assert_eq!(outbound["data_ciphers_fallback"], "AES-256-GCM");
        assert_eq!(outbound["auth"], "SHA256");
        let tls = &outbound["tls"];
        assert_eq!(tls["certificate"], "CERT");
        assert_eq!(tls["client_certificate"], "CLIENTCERT");
        assert_eq!(tls["client_key"], "CLIENTKEY");
        assert_eq!(tls["server_name"], "vpn.example.com");
        assert_eq!(tls["server_name_type"], "name");
        assert_eq!(
            tls["cipher"],
            "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256"
        );
        assert_eq!(tls["control_wrap"]["type"], "tls_auth");
        assert_eq!(tls["control_wrap"]["key"], "KEYDATA");
        assert_eq!(tls["control_wrap"]["direction"], "client");
        assert!(tls.get("kernel_tx").is_none());
        for key in [
            "tls_auth",
            "key_direction",
            "key_password",
            "reconnect_delay",
            "allowed_ips",
            "lumen_requires_user_auth",
        ] {
            assert!(outbound.get(key).is_none(), "{key} must be removed");
        }
    }

    #[test]
    fn test_openvpn_client_cipher_list_normalization() {
        let mut outbound = json!({
            "type": "openvpn-client",
            "data_ciphers": ["aes-128-gcm", "BOGUS"],
        });
        normalize_openvpn_outbound(&mut outbound).unwrap();
        assert_eq!(outbound["data_ciphers"], json!(["AES-128-GCM"]));

        let mut outbound = json!({"type": "openvpn-client", "data_ciphers": ["BOGUS"]});
        assert!(normalize_openvpn_outbound(&mut outbound).is_err());

        let mut outbound = json!({"type": "openvpn-client", "auth": "SHAKE"});
        assert!(normalize_openvpn_outbound(&mut outbound).is_err());

        // Unsupported transport must fail with the Python message.
        let mut outbound = json!({"type": "openvpn-client", "network": "dccp"});
        let err = normalize_openvpn_outbound(&mut outbound).unwrap_err().to_string();
        assert_eq!(err, "Unsupported OpenVPN transport `dccp`");
    }

    #[test]
    fn test_private_key_encrypted_detection() {
        assert!(openvpn_private_key_is_encrypted(&json!({
            "tls": {"key": "-----BEGIN ENCRYPTED PRIVATE KEY-----\nabc"}
        })));
        assert!(openvpn_private_key_is_encrypted(&json!({
            "tls": {"client_key": "Proc-Type: 4,ENCRYPTED\nDEK-Info: AES-128-CBC"}
        })));
        assert!(!openvpn_private_key_is_encrypted(&json!({
            "tls": {"key": "-----BEGIN PRIVATE KEY-----"}
        })));
        assert!(!openvpn_private_key_is_encrypted(&json!({})));
    }
}
