//! `_build_stream_settings` — transport/TLS/REALITY outbound stream builder
//! (link_parser.py lines ~621-885).

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use super::helpers::{get_param, to_bool_str};

fn insert_str(map: &mut Map<String, Value>, key: &str, value: String) {
    map.insert(key.to_string(), Value::String(value));
}

/// Python `_build_stream_settings(params, default_network, default_security)`.
pub(super) fn build_stream_settings(
    params: &BTreeMap<String, String>,
    default_network: &str,
    default_security: &str,
) -> Value {
    let mut network = get_param(params, &["type", "net"], if default_network.is_empty() { "tcp" } else { default_network });
    if network.is_empty() {
        network = "tcp".to_string();
    }
    let network = network.to_lowercase();
    let network = match network.as_str() {
        "gun" => "grpc",
        "http-upgrade" => "httpupgrade",
        "splithttp" => "xhttp",
        "split-http" => "xhttp",
        other => other,
    }
    .to_string();

    let mut security = get_param(
        params,
        &["security", "tls"],
        if default_security.is_empty() { "none" } else { default_security },
    );
    if security.is_empty() {
        security = "none".to_string();
    }
    let mut security = security.to_lowercase();
    if security == "none" && get_param(params, &["tls"], "") == "tls" {
        security = "tls".to_string();
    }

    let mut stream = Map::new();
    insert_str(&mut stream, "network", network.clone());
    insert_str(&mut stream, "security", security.clone());

    let host = get_param(params, &["host"], "");
    let path = get_param(params, &["path"], "");
    let host_nonempty = !host.is_empty();
    let path_nonempty = !path.is_empty();

    if network == "ws" {
        let mut ws_settings = Map::new();
        if path_nonempty {
            insert_str(&mut ws_settings, "path", path.clone());
        }
        if host_nonempty {
            ws_settings.insert("headers".to_string(), json!({ "Host": host }));
        }
        let early_data = get_param(params, &["ed", "maxEarlyData", "max_early_data"], "");
        if !early_data.is_empty() {
            match early_data.parse::<i64>() {
                Ok(value) => {
                    ws_settings.insert("maxEarlyData".to_string(), json!(value));
                }
                Err(_) => {
                    insert_str(&mut ws_settings, "maxEarlyData", early_data.clone());
                }
            }
        }
        let early_data_header = get_param(
            params,
            &["eh", "earlyDataHeaderName", "early_data_header_name"],
            "",
        );
        if !early_data_header.is_empty() {
            insert_str(&mut ws_settings, "earlyDataHeaderName", early_data_header);
        }
        stream.insert("wsSettings".to_string(), Value::Object(ws_settings));
    } else if network == "http" || network == "h2" {
        let mut http_settings = Map::new();
        if host_nonempty {
            let hosts: Vec<Value> = host
                .split(',')
                .map(|h| h.trim().to_string())
                .filter(|h| !h.is_empty())
                .map(Value::String)
                .collect();
            http_settings.insert("host".to_string(), Value::Array(hosts));
        }
        if path_nonempty {
            insert_str(&mut http_settings, "path", path.clone());
        }
        for (source_key, target_key) in [
            ("method", "method"),
            ("idleTimeout", "idleTimeout"),
            ("idle_timeout", "idleTimeout"),
            ("pingTimeout", "pingTimeout"),
            ("ping_timeout", "pingTimeout"),
        ] {
            let value = get_param(params, &[source_key], "");
            if !value.is_empty() {
                insert_str(&mut http_settings, target_key, value);
            }
        }
        stream.insert("httpSettings".to_string(), Value::Object(http_settings));
    } else if network == "grpc" {
        let mut grpc_settings = Map::new();
        let service_name = get_param(params, &["serviceName", "service_name"], "");
        if !service_name.is_empty() {
            insert_str(&mut grpc_settings, "serviceName", service_name);
        }
        let authority = get_param(params, &["authority"], "");
        if !authority.is_empty() {
            insert_str(&mut grpc_settings, "authority", authority);
        }
        let mode = get_param(params, &["mode"], "");
        if mode == "multi" {
            grpc_settings.insert("multiMode".to_string(), Value::Bool(true));
        }
        for (source_key, target_key) in [
            ("idleTimeout", "idleTimeout"),
            ("idle_timeout", "idleTimeout"),
            ("pingTimeout", "pingTimeout"),
            ("ping_timeout", "pingTimeout"),
        ] {
            let value = get_param(params, &[source_key], "");
            if !value.is_empty() {
                insert_str(&mut grpc_settings, target_key, value);
            }
        }
        let permit_without_stream = get_param(params, &["permitWithoutStream", "permit_without_stream"], "");
        if !permit_without_stream.is_empty() {
            grpc_settings.insert(
                "permitWithoutStream".to_string(),
                Value::Bool(to_bool_str(&permit_without_stream)),
            );
        }
        stream.insert("grpcSettings".to_string(), Value::Object(grpc_settings));
    } else if network == "xhttp" {
        let mut xhttp_settings = Map::new();
        if path_nonempty {
            insert_str(&mut xhttp_settings, "path", path.clone());
        }
        if host_nonempty {
            insert_str(&mut xhttp_settings, "host", host.clone());
        }
        let mode = get_param(params, &["mode"], "");
        if matches!(mode.as_str(), "auto" | "packet-up" | "stream-up" | "stream-one") {
            insert_str(&mut xhttp_settings, "mode", mode);
        }
        let extra = get_param(params, &["extra"], "");
        if !extra.is_empty() {
            if let Ok(decoded_extra) = serde_json::from_str::<Value>(&extra) {
                if decoded_extra.is_object() {
                    xhttp_settings.insert("extra".to_string(), decoded_extra);
                }
            }
        }
        for key in [
            "scMaxEachPostBytes",
            "scMaxBufferedPosts",
            "scMinPostsIntervalMs",
            "xPaddingBytes",
            "padding",
            "downloadSettings",
            "download_settings",
            "noGRPCHeader",
            "noSSEHeader",
            "scStreamUpServerSecs",
            "serverMaxHeaderBytes",
            "trustedXForwardedFor",
            "xmux",
            "xPaddingObfsMode",
            "xPaddingKey",
            "xPaddingHeader",
            "xPaddingPlacement",
            "xPaddingMethod",
            "uplinkHTTPMethod",
            "sessionPlacement",
            "sessionKey",
            "seqPlacement",
            "seqKey",
            "uplinkDataPlacement",
            "uplinkDataKey",
            "uplinkChunkSize",
            "sessionIDTable",
            "sessionIDLength",
            "congestionController",
            "cwnd",
        ] {
            let value = get_param(params, &[key], "");
            if value.is_empty() {
                continue;
            }
            if key == "downloadSettings" || key == "download_settings" || key == "xmux" {
                let decoded = serde_json::from_str::<Value>(&super::helpers::unquote(&value))
                    .unwrap_or(Value::String(value.clone()));
                xhttp_settings.insert(key.to_string(), decoded);
            } else {
                insert_str(&mut xhttp_settings, key, value);
            }
        }
        stream.insert("xhttpSettings".to_string(), Value::Object(xhttp_settings));
    } else if network == "httpupgrade" {
        let mut httpupgrade_settings = Map::new();
        if path_nonempty {
            insert_str(&mut httpupgrade_settings, "path", path.clone());
        }
        if host_nonempty {
            insert_str(&mut httpupgrade_settings, "host", host.clone());
        }
        let early_data = get_param(params, &["ed"], "");
        if !early_data.is_empty() {
            insert_str(&mut httpupgrade_settings, "ed", early_data);
        }
        stream.insert(
            "httpupgradeSettings".to_string(),
            Value::Object(httpupgrade_settings),
        );
    } else if network == "quic" {
        let quic_security = get_param(params, &["quicSecurity", "quic_security"], "");
        let quic_key = get_param(params, &["key"], "");
        let header_type = get_param(params, &["headerType", "header_type"], "");
        stream.insert(
            "quicSettings".to_string(),
            json!({
                "security": if quic_security.is_empty() { "none".to_string() } else { quic_security },
                "key": quic_key,
                "header": { "type": if header_type.is_empty() { "none".to_string() } else { header_type } },
            }),
        );
    } else if network == "kcp" || network == "mkcp" {
        let header_type = get_param(params, &["headerType", "header_type"], "");
        let mut kcp_settings = Map::new();
        kcp_settings.insert(
            "header".to_string(),
            json!({ "type": if header_type.is_empty() { "none".to_string() } else { header_type } }),
        );
        for key in [
            "mtu",
            "tti",
            "uplinkCapacity",
            "downlinkCapacity",
            "readBufferSize",
            "writeBufferSize",
            "seed",
            "congestion",
        ] {
            let snake = super::helpers::camel_to_snake(key);
            let value = get_param(params, &[key, snake.as_str()], "");
            if value.is_empty() {
                continue;
            }
            if matches!(
                key,
                "mtu" | "tti" | "uplinkCapacity" | "downlinkCapacity" | "readBufferSize" | "writeBufferSize"
            ) {
                match value.parse::<i64>() {
                    Ok(number) => {
                        kcp_settings.insert(key.to_string(), json!(number));
                    }
                    Err(_) => {
                        insert_str(&mut kcp_settings, key, value);
                    }
                }
            } else if key == "congestion" {
                kcp_settings.insert(key.to_string(), Value::Bool(to_bool_str(&value)));
            } else {
                insert_str(&mut kcp_settings, key, value);
            }
        }
        insert_str(&mut stream, "network", "kcp".to_string());
        stream.insert("kcpSettings".to_string(), Value::Object(kcp_settings));
    }

    if security == "tls" {
        let mut tls_settings = Map::new();
        let sni = get_param(params, &["sni", "serverName", "server_name"], "");
        if !sni.is_empty() {
            insert_str(&mut tls_settings, "serverName", sni);
        }
        let alpn = get_param(params, &["alpn"], "");
        if !alpn.is_empty() {
            let items: Vec<Value> = alpn
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .map(Value::String)
                .collect();
            tls_settings.insert("alpn".to_string(), Value::Array(items));
        }
        let fp = get_param(params, &["fp", "fingerprint"], "");
        if !fp.is_empty() {
            insert_str(&mut tls_settings, "fingerprint", fp);
        }
        let allow_insecure = get_param(params, &["allowInsecure", "allow_insecure", "insecure"], "");
        if !allow_insecure.is_empty() {
            tls_settings.insert(
                "allowInsecure".to_string(),
                Value::Bool(to_bool_str(&allow_insecure)),
            );
        }
        let cert_sha = get_param(params, &["pcs", "pinnedPeerCertSha256", "certSha"], "");
        if !cert_sha.is_empty() {
            insert_str(&mut tls_settings, "pinnedPeerCertSha256", cert_sha);
        }
        let ech = get_param(params, &["ech", "echConfigList"], "");
        if !ech.is_empty() {
            insert_str(&mut tls_settings, "echConfigList", ech);
            insert_str(&mut tls_settings, "echForceQuery", "full".to_string());
        }
        let verify_names = get_param(params, &["vcn", "verifyPeerCertByName"], "");
        if !verify_names.is_empty() {
            let joined: Vec<&str> = verify_names
                .split(',')
                .map(|item| item.trim())
                .filter(|item| !item.is_empty())
                .collect();
            insert_str(&mut tls_settings, "verifyPeerCertByName", joined.join(","));
        }
        for (aliases, target_key) in [
            (["disableSNI", "disable_sni"], "disableSNI"),
            (["minVersion", "min_version"], "minVersion"),
            (["maxVersion", "max_version"], "maxVersion"),
            (["cipherSuites", "cipher_suites"], "cipherSuites"),
            (["curvePreferences", "curve_preferences"], "curvePreferences"),
            (["handshakeTimeout", "handshake_timeout"], "handshakeTimeout"),
        ] {
            let value = get_param(params, &aliases, "");
            if value.is_empty() {
                continue;
            }
            if target_key == "cipherSuites" || target_key == "curvePreferences" {
                let items: Vec<Value> = value
                    .split(',')
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .map(Value::String)
                    .collect();
                tls_settings.insert(target_key.to_string(), Value::Array(items));
            } else if target_key == "disableSNI" {
                tls_settings.insert(target_key.to_string(), Value::Bool(to_bool_str(&value)));
            } else {
                insert_str(&mut tls_settings, target_key, value);
            }
        }
        stream.insert("tlsSettings".to_string(), Value::Object(tls_settings));
    } else if security == "reality" {
        let mut reality_settings = Map::new();
        let sni = get_param(params, &["sni", "serverName", "server_name"], "");
        if !sni.is_empty() {
            insert_str(&mut reality_settings, "serverName", sni);
        }
        let fp = get_param(params, &["fp", "fingerprint"], "");
        if !fp.is_empty() {
            insert_str(&mut reality_settings, "fingerprint", fp);
        }
        let pbk = get_param(params, &["pbk", "publicKey", "public_key", "password"], "");
        if !pbk.is_empty() {
            insert_str(&mut reality_settings, "publicKey", pbk);
        }
        let sid = get_param(params, &["sid", "shortId", "short_id"], "");
        if !sid.is_empty() {
            insert_str(&mut reality_settings, "shortId", sid);
        }
        let spx = get_param(params, &["spx", "spiderX", "spider_x"], "");
        if !spx.is_empty() {
            insert_str(&mut reality_settings, "spiderX", spx);
        }
        let pqv = get_param(params, &["pqv", "mldsa65Verify", "mldsa65_verify"], "");
        if !pqv.is_empty() {
            insert_str(&mut reality_settings, "mldsa65Verify", pqv);
        }
        reality_settings.insert("show".to_string(), Value::Bool(false));
        stream.insert("realitySettings".to_string(), Value::Object(reality_settings));
    }

    let finalmask = get_param(params, &["fm", "finalmask"], "");
    if !finalmask.is_empty() {
        let decoded = serde_json::from_str::<Value>(&finalmask)
            .unwrap_or(Value::String(finalmask.clone()));
        stream.insert("finalmask".to_string(), decoded);
    }

    Value::Object(stream)
}
