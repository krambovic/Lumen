//! Ported Python test expectations for the link parser.

use base64::Engine;
use serde_json::{json, Value};

use super::{
    is_happ_auto_placeholder, normalize_outbound_for_runtime, parse_links_text, parse_single,
    validate_node_outbound, LinkParseError, MAX_IMPORT_BYTES, read_import_file_reference,
};

fn outbound_of(node: &super::Node) -> Value {
    node.outbound.clone()
}

#[test]
fn vless_full_reality_link() {
    let link = "vless://b831381d-6324-4d53-ad4f-8cda48b30811@example.com:443?encryption=none&flow=xtls-rprx-vision&security=reality&sni=www.microsoft.com&fp=chrome&pbk=somepublickey&sid=abcd&spx=%2F&type=tcp#My%20Node";
    let node = parse_single(link).expect("parse vless");
    assert_eq!(node.scheme, "vless");
    assert_eq!(node.server, "example.com");
    assert_eq!(node.port, 443);
    assert_eq!(node.name, "My Node");
    let outbound = outbound_of(&node);
    assert_eq!(outbound["protocol"], "vless");
    assert_eq!(outbound["settings"]["vnext"][0]["address"], "example.com");
    assert_eq!(outbound["settings"]["vnext"][0]["port"], 443);
    assert_eq!(
        outbound["settings"]["vnext"][0]["users"][0]["id"],
        "b831381d-6324-4d53-ad4f-8cda48b30811"
    );
    assert_eq!(outbound["settings"]["vnext"][0]["users"][0]["encryption"], "none");
    assert_eq!(
        outbound["settings"]["vnext"][0]["users"][0]["flow"],
        "xtls-rprx-vision"
    );
    let stream = &outbound["streamSettings"];
    assert_eq!(stream["network"], "tcp");
    assert_eq!(stream["security"], "reality");
    assert_eq!(stream["realitySettings"]["serverName"], "www.microsoft.com");
    assert_eq!(stream["realitySettings"]["fingerprint"], "chrome");
    assert_eq!(stream["realitySettings"]["publicKey"], "somepublickey");
    assert_eq!(stream["realitySettings"]["shortId"], "abcd");
    assert_eq!(stream["realitySettings"]["spiderX"], "/");
    assert_eq!(stream["realitySettings"]["show"], false);
    assert!(outbound.get("mux").is_none());

    // Missing pbk must fail validation with the Russian REALITY message.
    let mut broken = node.clone();
    if let Some(obj) = broken.outbound
        .get_mut("streamSettings")
        .and_then(|s| s.get_mut("realitySettings"))
        .and_then(Value::as_object_mut)
    {
        obj.shift_remove("publicKey");
    }
    let message = validate_node_outbound(&broken).expect("reality validation error");
    assert!(message.contains("publicKey"), "{message}");
    assert!(message.contains("REALITY"), "{message}");
}

#[test]
fn vless_xhttp_forces_mux_disabled() {
    let link = "vless://11111111-2222-3333-4444-555555555555@h:443?type=xhttp&path=%2Fxmux&mode=auto";
    let node = parse_single(link).expect("parse xhttp");
    assert_eq!(node.outbound["streamSettings"]["network"], "xhttp");
    assert_eq!(node.outbound["streamSettings"]["xhttpSettings"]["path"], "/xmux");
    assert_eq!(node.outbound["mux"]["enabled"], false);
    assert_eq!(node.outbound["mux"]["concurrency"], -1);
}

#[test]
fn vmess_base64_link() {
    // {"add":"vm.example.com","port":"8443","id":"uuid-1","aid":"0","scy":"aes-128-gcm","net":"ws","host":"cdn.example.com","path":"/ws","tls":"tls","sni":"sni.example.com","ps":"VM Node"}
    let payload = json!({
        "add": "vm.example.com", "port": "8443", "id": "uuid-1", "aid": "0",
        "scy": "aes-128-gcm", "net": "ws", "host": "cdn.example.com", "path": "/ws",
        "tls": "tls", "sni": "sni.example.com", "ps": "VM Node",
    });
    let encoded = base64::engine::general_purpose::STANDARD
        .encode(serde_json::to_vec(&payload).unwrap());
    let link = format!("vmess://{encoded}");
    let node = parse_single(&link).expect("parse vmess");
    assert_eq!(node.scheme, "vmess");
    assert_eq!(node.server, "vm.example.com");
    assert_eq!(node.port, 8443);
    assert_eq!(node.name, "VM Node");
    let outbound = outbound_of(&node);
    assert_eq!(outbound["settings"]["vnext"][0]["users"][0]["id"], "uuid-1");
    assert_eq!(outbound["settings"]["vnext"][0]["users"][0]["alterId"], 0);
    assert_eq!(
        outbound["settings"]["vnext"][0]["users"][0]["security"],
        "aes-128-gcm"
    );
    let stream = &outbound["streamSettings"];
    assert_eq!(stream["network"], "ws");
    assert_eq!(stream["security"], "tls");
    assert_eq!(stream["wsSettings"]["path"], "/ws");
    assert_eq!(stream["wsSettings"]["headers"]["Host"], "cdn.example.com");
    assert_eq!(stream["tlsSettings"]["serverName"], "sni.example.com");

    // null cipher sentinel maps to "auto"
    let payload2 = json!({
        "add": "h", "port": 443, "id": "i", "scy": Value::Null, "net": "tcp",
    });
    let encoded2 = base64::engine::general_purpose::STANDARD
        .encode(serde_json::to_vec(&payload2).unwrap());
    let node2 = parse_single(&format!("vmess://{encoded2}")).expect("parse vmess 2");
    assert_eq!(
        node2.outbound["settings"]["vnext"][0]["users"][0]["security"],
        "auto"
    );
}

#[test]
fn ss_sip002_with_plugin() {
    // method:password = aes-256-gcm:test1234
    let userinfo = base64::engine::general_purpose::STANDARD.encode("aes-256-gcm:test1234");
    let link = format!("ss://{userinfo}@1.2.3.4:8388#SS%20Node");
    let node = parse_single(&link).expect("parse ss");
    assert_eq!(node.scheme, "ss");
    assert_eq!(node.server, "1.2.3.4");
    assert_eq!(node.port, 8388);
    assert_eq!(node.name, "SS Node");
    assert_eq!(node.outbound["settings"]["servers"][0]["method"], "aes-256-gcm");
    assert_eq!(node.outbound["settings"]["servers"][0]["password"], "test1234");

    // plain SIP022-style userinfo with plugin
    let link2 = "ss://aes-256-gcm:secretpw%40x@[2001:db8::1]:9443?plugin=obfs-local%3Bobfs%3Dhttp%3Bobfs-host%3Dexample.com#P";
    let node2 = parse_single(link2).expect("parse ss2");
    assert_eq!(node2.server, "2001:db8::1");
    assert_eq!(node2.port, 9443);
    assert_eq!(node2.outbound["settings"]["servers"][0]["plugin"], "obfs-local");
    assert_eq!(
        node2.outbound["settings"]["servers"][0]["plugin_opts"],
        "obfs=http;obfs-host=example.com"
    );

    // default port 8388
    let userinfo3 = base64::engine::general_purpose::STANDARD.encode("none:pw");
    let node3 = parse_single(&format!("ss://{userinfo3}@h.example#x")).expect("parse ss3");
    assert_eq!(node3.port, 8388);

    // control characters in decoded credentials are rejected
    let bad = base64::engine::general_purpose::STANDARD.encode("aes-128-gcm:pw\n\x01x");
    let err = parse_single(&format!("ss://{bad}@h:1")).expect_err("reject control chars");
    assert_eq!(err.message, "invalid shadowsocks credentials");
}

#[test]
fn socks_link() {
    let node = parse_single("socks://user:pass@127.0.0.1:1080#Local").expect("parse socks");
    assert_eq!(node.scheme, "socks");
    assert_eq!(node.server, "127.0.0.1");
    assert_eq!(node.port, 1080);
    assert_eq!(node.name, "Local");
    assert_eq!(
        node.outbound["settings"]["servers"][0]["users"][0]["user"],
        "user"
    );
    assert_eq!(
        node.outbound["settings"]["servers"][0]["users"][0]["pass"],
        "pass"
    );

    // default port 1080 and no users without credentials
    let node2 = parse_single("socks5://h2").expect("parse socks5");
    assert_eq!(node2.port, 1080);
    assert!(node2.outbound["settings"]["servers"][0].get("users").is_none());
}

#[test]
fn trojan_and_hysteria2_links() {
    let node = parse_single("trojan://pw123@gw.example:443?sni=real.example&allowInsecure=true&type=ws&path=%2Ft#T").expect("trojan");
    assert_eq!(node.outbound["settings"]["servers"][0]["password"], "pw123");
    assert_eq!(node.outbound["streamSettings"]["security"], "tls");
    assert_eq!(node.outbound["streamSettings"]["wsSettings"]["path"], "/t");
    assert_eq!(
        node.outbound["streamSettings"]["tlsSettings"]["allowInsecure"],
        true
    );

    let h2 = parse_single("hysteria2://pw@h.example:8443?sni=s.example&insecure=1&obfs=salamander&obfs-password=ob#H").expect("hy2");
    assert_eq!(h2.scheme, "hysteria2");
    assert_eq!(h2.outbound["singbox"]["type"], "hysteria2");
    assert_eq!(h2.outbound["singbox"]["password"], "pw");
    assert_eq!(h2.outbound["singbox"]["tls"]["server_name"], "s.example");
    assert_eq!(h2.outbound["singbox"]["tls"]["insecure"], true);
    assert_eq!(h2.outbound["singbox"]["obfs"]["type"], "salamander");
    assert_eq!(h2.outbound["singbox"]["obfs"]["password"], "ob");
}

#[test]
fn clash_yaml_proxy_list() {
    let text = r#"port: 7890
proxies:
  - name: "vless-entry"
    type: vless
    server: 1.1.1.1
    port: 443
    uuid: 99999999-8888-7777-6666-555555555555
    tls: true
    servername: s.example
    network: ws
    ws-opts:
      path: /wsp
    reality-opts:
      public-key: pbk-from-clash
      short-id: 0123abcd
    client-fingerprint: chrome
  - name: "ss-entry"
    type: ss
    server: 2.2.2.2
    port: 8388
    cipher: aes-128-gcm
    password: "pw"
  - name: "hy2-entry"
    type: hysteria2
    server: 3.3.3.3
    port: 36712
    password: "hpw"
    sni: hs.example
rules:
  - MATCH,DIRECT
"#;
    let (nodes, errors) = parse_links_text(text, false).expect("clash yaml");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes[0].name, "vless-entry");
    assert_eq!(nodes[0].scheme, "vless");
    assert_eq!(nodes[0].outbound["settings"]["vnext"][0]["address"], "1.1.1.1");
    let stream = &nodes[0].outbound["streamSettings"];
    assert_eq!(stream["security"], "reality");
    assert_eq!(stream["realitySettings"]["publicKey"], "pbk-from-clash");
    assert_eq!(stream["realitySettings"]["shortId"], "0123abcd");
    assert_eq!(stream["realitySettings"]["serverName"], "s.example");
    assert_eq!(stream["wsSettings"]["path"], "/wsp");
    assert_eq!(stream["wsSettings"]["headers"], json!({}));
    assert_eq!(nodes[1].outbound["streamSettings"]["network"], "tcp");
    assert_eq!(nodes[1].outbound["streamSettings"]["security"], "none");
    assert_eq!(nodes[2].outbound["singbox"]["tag"], "hy2-entry");
    assert_eq!(nodes[1].outbound["protocol"], "shadowsocks");
    assert_eq!(nodes[1].outbound["settings"]["servers"][0]["method"], "aes-128-gcm");
    assert_eq!(nodes[2].outbound["singbox"]["type"], "hysteria2");
    assert_eq!(nodes[2].outbound["singbox"]["server"], "3.3.3.3");
}

#[test]
fn base64_subscription_blob_recurses() {
    let links = "vless://aaaa-bbbb@one.example:443#One\nss://YWVzLTI1Ni1nY206cHc=@two.example:8388#Two";
    let blob = base64::engine::general_purpose::STANDARD.encode(links.as_bytes());
    let (nodes, errors) = parse_links_text(&blob, false).expect("blob");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].name, "One");
    assert_eq!(nodes[1].name, "Two");
}

#[test]
fn plain_list_and_errors() {
    let text = "vless://aaaa-bbbb@a.example:443#A\nhttp://[::1]:8080#B\ngarbage-not-a-link\nssr://rejected";
    let (nodes, errors) = parse_links_text(text, false).expect("list");
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].scheme, "vless");
    assert_eq!(nodes[1].scheme, "http");
    assert_eq!(nodes[1].port, 8080);
    assert_eq!(errors.len(), 2);
    assert!(errors[0].starts_with("Line 3: "), "{errors:?}");
    assert!(errors[1].contains("unsupported scheme: ssr"), "{errors:?}");
}

#[test]
fn validation_messages_parity() {
    // missing server/port
    let mut node = super::Node::new();
    node.scheme = "vless".into();
    node.name = "Плохой".into();
    node.outbound = json!({"protocol": "vless", "settings": {}, "streamSettings": {"network": "tcp", "security": "none"}});
    let message = validate_node_outbound(&node).expect("validation");
    assert_eq!(message, "Сервер Плохой пропущен: нет адреса или порта.");

    // stub server 0.0.0.0:1
    let mut stub = super::Node::new();
    stub.scheme = "ss".into();
    stub.server = "0.0.0.0".into();
    stub.port = 1;
    stub.outbound = json!({"protocol": "shadowsocks", "settings": {}});
    let message = validate_node_outbound(&stub).expect("stub message");
    assert_eq!(message, "Сервер-заглушка 0.0.0.0:1 пропущен.");

    // WireGuard missing private key
    let mut wg = super::Node::new();
    wg.name = "WG".into();
    wg.outbound = json!({
        "protocol": "wireguard",
        "singbox": {"type": "wireguard", "address": ["10.0.0.2/32"], "peers": [
            {"address": "h", "port": 51820, "public_key": "k", "allowed_ips": ["0.0.0.0/0"]}
        ]},
    });
    let message = validate_node_outbound(&wg).expect("wg message");
    assert_eq!(message, "Сервер WG не содержит private_key для WireGuard/AWG.");
}

#[test]
fn happ_auto_placeholder_detection() {
    let mut node = super::Node::new();
    node.scheme = "ss".into();
    node.server = "MAX.RU".into();
    node.port = 1234;
    node.name = "Автовыбор сервера".into();
    assert!(is_happ_auto_placeholder(&node));
    node.name = "other".into();
    assert!(!is_happ_auto_placeholder(&node));
}

#[test]
fn normalize_outbound_runtime_fixes() {
    let mut outbound = json!({
        "protocol": "vless",
        "settings": {"vnext": []},
        "streamSettings": {"network": "xhttp", "security": "tls", "tlsSettings": {}},
    });
    assert!(normalize_outbound_for_runtime(&mut outbound, "srv.example"));
    assert_eq!(outbound["mux"]["enabled"], false);
    assert_eq!(outbound["mux"]["concurrency"], -1);
    assert_eq!(outbound["streamSettings"]["tlsSettings"]["serverName"], "srv.example");
    // idempotent second run
    assert!(!normalize_outbound_for_runtime(&mut outbound, "srv.example"));
}

#[test]
fn wireguard_ini_config() {
    let text = "[Interface]\nPrivateKey = ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=\nAddress = 10.0.0.2/32\nDNS = 1.1.1.1\n\n[Peer]\nPublicKey = ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ=\nEndpoint = peer.example:51820\nAllowedIPs = 0.0.0.0/0, ::/0\n";
    let (nodes, errors) = parse_links_text(text, false).expect("wg ini");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(nodes.len(), 1);
    let node = &nodes[0];
    assert_eq!(node.scheme, "wireguard");
    assert_eq!(node.server, "peer.example");
    assert_eq!(node.port, 51820);
    assert_eq!(node.outbound["singbox"]["type"], "wireguard");
    assert_eq!(node.outbound["singbox"]["address"][0], "10.0.0.2/32");
    assert_eq!(node.outbound["singbox"]["peers"][0]["address"], "peer.example");
    assert_eq!(node.outbound["singbox"]["system"], false);
    assert_eq!(node.outbound["singbox"]["udp_timeout"], "5m0s");
    assert_eq!(node.outbound["_dns"][0], "1.1.1.1");
}

#[test]
fn json_import_array_and_sip008() {
    let text = r#"[{"server": "5.6.7.8", "server_port": 8388, "method": "aes-256-gcm", "password": "pw"}]"#;
    let (nodes, errors) = parse_links_text(text, false).expect("sip008");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].scheme, "shadowsocks");
    assert_eq!(nodes[0].server, "5.6.7.8");
    assert_eq!(nodes[0].outbound["singbox"]["type"], "shadowsocks");

    let err = parse_single("[1,2,3").expect_err("bad json");
    assert!(!err.message.is_empty());
}

#[test]
fn parse_error_type_display() {
    let err = LinkParseError::new("boom");
    assert_eq!(err.to_string(), "boom");
}

#[test]
fn differential_single_links_match_python() {
    // Ground truth captured from the Python implementation.
    let cases: Vec<(&str, Value)> = vec![
        (
            "tuic://uuid-x:pass@t.example:8443?congestion_control=bbr&udp_relay_mode=native&alpn=h3#T",
            json!({
                "name": "T", "scheme": "tuic", "server": "t.example", "port": 8443,
                "outbound": {"protocol": "tuic", "singbox": {
                    "type": "tuic", "tag": "proxy", "server": "t.example", "server_port": 8443,
                    "uuid": "uuid-x", "password": "pass",
                    "congestion_control": "bbr", "udp_relay_mode": "native",
                    "tls": {"enabled": true, "server_name": "t.example", "alpn": ["h3"]},
                }},
            }),
        ),
        (
            "naive+https://user:pass@n.example:443?sni=n.example#N",
            json!({
                "name": "N", "scheme": "naive", "server": "n.example", "port": 443,
                "outbound": {"protocol": "naive", "singbox": {
                    "type": "naive", "tag": "proxy", "server": "n.example", "server_port": 443,
                    "username": "user", "password": "pass", "quic": false,
                    "tls": {"enabled": true, "server_name": "n.example"},
                }},
            }),
        ),
        (
            "anytls://pw@a.example:443?sni=a.example&insecure=1#A",
            json!({
                "name": "A", "scheme": "anytls", "server": "a.example", "port": 443,
                "outbound": {"protocol": "anytls", "singbox": {
                    "type": "anytls", "tag": "proxy", "server": "a.example", "server_port": 443,
                    "password": "pw",
                    "tls": {"enabled": true, "server_name": "a.example", "insecure": true},
                }},
            }),
        ),
        (
            "anytls://pw@a.example:443?idle-session-check-interval=30&idle-session-timeout=60&min-idle-session=2#A2",
            json!({
                "name": "A2", "scheme": "anytls", "server": "a.example", "port": 443,
                "outbound": {"protocol": "anytls", "singbox": {
                    "type": "anytls", "tag": "proxy", "server": "a.example", "server_port": 443,
                    "password": "pw",
                    "tls": {"enabled": true, "server_name": "a.example"},
                    "idle_session_check_interval": "30", "idle_session_timeout": "60",
                    "min_idle_session": 2,
                }},
            }),
        ),
        (
            "mieru://user:pw@m.example:1234?transport=TCP#M",
            json!({
                "name": "M", "scheme": "mieru", "server": "m.example", "port": 1234,
                "outbound": {"protocol": "mieru", "singbox": {
                    "type": "mieru", "tag": "proxy", "server": "m.example", "server_port": 1234,
                    "transport": "TCP", "username": "user", "password": "pw",
                }},
            }),
        ),
        (
            "masque://profileid:token@masque.example?udp_timeout=7m#MQ",
            json!({
                "name": "MQ", "scheme": "masque", "server": "masque.example", "port": 0,
                "outbound": {"protocol": "masque", "singbox": {
                    "type": "masque", "tag": "proxy", "system": false, "name": "masque0",
                    "use_http2": false, "use_ipv6": false,
                    "profile": {"detour": "direct", "id": "masque.example", "auth_token": "profileid"},
                    "udp_timeout": "7m", "udp_keepalive_period": "30s",
                    "reconnect_delay": "5s", "congestion_controller": "bbr",
                }},
            }),
        ),
        (
            "hy2://pw2@h2.example?sni=s2&insecure=1&obfs=salamander&obfs-password=x#HY",
            json!({
                "name": "HY", "scheme": "hysteria2", "server": "h2.example", "port": 443,
                "outbound": {"protocol": "hysteria2", "singbox": {
                    "type": "hysteria2", "tag": "proxy", "server": "h2.example", "server_port": 443,
                    "password": "pw2",
                    "tls": {"enabled": true, "server_name": "s2", "insecure": true},
                    "obfs": {"type": "salamander", "password": "x"},
                }},
            }),
        ),
    ];
    for (link, expected) in cases {
        let node = parse_single(link).unwrap_or_else(|e| panic!("{link}: {e}"));
        let actual = json!({
            "name": node.name, "scheme": node.scheme, "server": node.server,
            "port": node.port, "outbound": node.outbound,
        });
        assert_eq!(actual, expected, "mismatch for {link}");
    }

    // Plaintext SIP022 userinfo decodes percent escapes in the password.
    let node = parse_single("ss://2022-blake3-aes-128-gcm:pw%40x@two.example:8388")
        .expect("sip022 plaintext");
    assert_eq!(node.outbound["settings"]["servers"][0]["method"], "2022-blake3-aes-128-gcm");
    assert_eq!(node.outbound["settings"]["servers"][0]["password"], "pw@x");
    // A `%40`-encoded authority (no literal `@`) falls into the legacy
    // whole-body base64 path and fails like Python.
    let err = parse_single("ss://2022-blake3-aes-128-gcm:eyJub25jZSI6INQR}%40two.example:8388")
        .expect_err("legacy body rejected");
    assert_eq!(err.message, "invalid shadowsocks base64 credentials");
}

#[test]
fn openvpn_branch_dispatches() {
    let ovpn = "client\ndev tun\nproto udp\nremote vpn.example 1194\n<ca>\n-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n</ca>\n";
    let (nodes, errors) = parse_links_text(ovpn, false).expect("openvpn");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(nodes.len(), 1);
    let node = &nodes[0];
    assert_eq!(node.scheme, "openvpn");
    assert_eq!(node.server, "vpn.example");
    assert_eq!(node.port, 1194);
    assert_eq!(node.name, "OpenVPN vpn.example");
    assert_eq!(node.outbound["protocol"], "openvpn");
    assert_eq!(node.outbound["singbox"]["type"], "openvpn");
    assert_eq!(node.outbound["singbox"]["system"], false);
    assert_eq!(node.outbound["singbox"]["servers"][0]["server"], "vpn.example");
    assert_eq!(node.outbound["singbox"]["servers"][0]["server_port"], 1194);
}

#[test]
fn json_label_suffix_and_link_container() {
    let doc = r#"{"remarks": "Sub A", "outbounds": [{"protocol": "vless", "settings": {"vnext": [{"address": "a.example", "port": 1, "users": []}]}, "streamSettings": {"network": "tcp", "security": "none"}}, {"protocol": "trojan", "settings": {"servers": [{"address": "b.example", "port": 2, "password": "p"}]}}]}"#;
    let (nodes, errors) = parse_links_text(doc, false).expect("label json");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].name, "Sub A \u{b7} json-vless");
    assert_eq!(nodes[1].name, "Sub A \u{b7} json-trojan");

    let doc2 = r#"{"link": "vless://u@x.example:1?type=xhttp&path=/x#VX"}"#;
    let (nodes2, errors2) = parse_links_text(doc2, false).expect("link json");
    assert!(errors2.is_empty(), "{errors2:?}");
    assert_eq!(nodes2[0].name, "VX");
    assert_eq!(nodes2[0].outbound["streamSettings"]["network"], "xhttp");
    assert_eq!(nodes2[0].outbound["mux"]["enabled"], false);
    assert_eq!(nodes2[0].outbound["mux"]["concurrency"], -1);
}

/// Python `test_awg31_link_and_conf_preserve_boolean_options`.
#[test]
fn awg31_link_and_conf_preserve_boolean_options() {
    let key = base64::engine::general_purpose::STANDARD.encode((0u8..32).collect::<Vec<_>>());
    for enabled in [true, false] {
        let node = parse_single(&format!(
            "awg://{key}@192.0.2.1:51820?public_key={key}&address=10.0.0.2/32&random_trailers={enabled}&disable_cookies={enabled}"
        ))
        .expect("parse awg link");
        let amnezia = node.outbound["singbox"]["amnezia"].clone();
        assert_eq!(amnezia["random_trailers"], json!(enabled));
        assert_eq!(amnezia["disable_cookies"], json!(enabled));
        let conf = format!(
            "[Interface]\nPrivateKey = {key}\nAddress = 10.0.0.2/32\nRandomTrailers = {enabled}\nDisableCookies = {enabled}\n[Peer]\nPublicKey = {key}\nEndpoint = 192.0.2.1:51820\nAllowedIPs = 0.0.0.0/0\n"
        );
        let (nodes, errors) = parse_links_text(&conf, false).expect("parse conf");
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(nodes[0].outbound["singbox"]["amnezia"], amnezia);
    }
}

#[test]
fn awg31_rejects_non_boolean_flags() {
    let key = base64::engine::general_purpose::STANDARD.encode((0u8..32).collect::<Vec<_>>());
    let error = parse_single(&format!(
        "awg://{key}@192.0.2.1:51820?public_key={key}&address=10.0.0.2/32&random_trailers=maybe"
    ))
    .expect_err("non-boolean flag");
    assert!(error.to_string().contains("RandomTrailers"), "{error}");
}

#[test]
fn import_file_size_limit_is_checked_before_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.txt");
    let file = std::fs::File::create(&path).unwrap();
    // Sparse: the size check must not need the content.
    file.set_len(MAX_IMPORT_BYTES as u64 + 1).unwrap();
    let (nodes, errors) = parse_links_text(path.to_str().unwrap(), true).unwrap();
    assert!(nodes.is_empty());
    assert!(errors.first().is_some_and(|error| error.contains("exceeds")), "{errors:?}");
}

#[test]
fn file_reference_is_ignored_for_downloaded_subscription_bodies() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secret.txt");
    std::fs::write(&path, "vless://00000000-0000-0000-0000-000000000001@one.example:443#one").unwrap();
    let (nodes, errors) = parse_links_text(path.to_str().unwrap(), false).unwrap();
    assert!(nodes.is_empty());
    assert!(errors.first().is_some_and(|error| error.contains("unsupported scheme")), "{errors:?}");
}

#[test]
fn unc_file_reference_is_never_followed() {
    // Opening `\\host\share` makes Windows authenticate to `host` over SMB.
    for body in [
        "file://attacker.example.com/share/a.txt",
        r"\\attacker.example.com\share\a.txt",
        "//attacker.example.com/share/a.txt",
    ] {
        assert!(read_import_file_reference(body).unwrap().is_none(), "{body}");
        let (nodes, errors) = parse_links_text(body, true).unwrap();
        assert!(nodes.is_empty());
        assert!(!errors.is_empty(), "{body}");
    }
}

