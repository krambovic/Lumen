//! Replays `fixtures/config_parity.json` (generated from the Python builders
//! by `rust/tools/parity/gen_config_corpus.py`) through the Rust runtime
//! builders: `build_xray_config`, `build_singbox_outbound` and
//! `plan_singbox_runtime` on the bundled default sing-box template.
//!
//! Each case stores the SHA-256 of the canonical Python output (sorted keys,
//! compact, ASCII) after masking per-run secrets, or the Python error text.
//! Set `LUMEN_PARITY_DUMP=<file>` to write the full Rust output of every
//! mismatch; `gen_config_corpus.py --case N` prints Python's side.

use std::path::{Path, PathBuf};

use lumen_core::config::build_xray_config;
use lumen_core::models::{AppSettings, Node, RoutingSettings};
use lumen_core::pyjson::dumps_compact_sorted_ascii;
use lumen_engines::singbox::config_builder::build_singbox_outbound;
use lumen_engines::singbox::document::parse_singbox_document;
use lumen_engines::singbox::runtime_planner::{PlanRuntimeOptions, plan_singbox_runtime};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn template_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../windows/data/templates/sing-box/default.json")
}

/// `canonicalize` adds the `\\?\` verbatim prefix; Python paths have none.
fn plain_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text).to_string())
}

fn sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
}

fn mask(mut text: String, secrets: &[&str]) -> String {
    for (index, secret) in secrets.iter().enumerate() {
        if !secret.is_empty() {
            text = text.replace(secret, &format!("<masked-{index}>"));
        }
    }
    text
}

struct Fixture {
    corpus: Value,
    document: lumen_engines::singbox::document::ParsedSingboxDocument,
    escaped_root: String,
}

enum Outcome {
    Value(String),
    Error(String),
}

impl Fixture {
    fn node(&self, id: &Value) -> Option<Node> {
        id.as_str().map(|id| Node::from_dict(&self.corpus["nodes"][id]))
    }

    fn run(&self, case: &Value) -> Outcome {
        let routing = &self.corpus["routings"][case["routing"].as_u64().unwrap() as usize];
        let settings_value = &self.corpus["settings"][case["settings"].as_u64().unwrap() as usize];
        let settings = AppSettings::from_dict(settings_value);
        let node = self.node(&case["node"]);
        match case["kind"].as_str().unwrap() {
            "xray" => {
                let config = build_xray_config(
                    node.as_ref().unwrap(),
                    routing,
                    settings_value,
                    0,
                    settings.local_socks_port as u16,
                    settings.local_http_port as u16,
                );
                Outcome::Value(dumps_compact_sorted_ascii(json!({ "config": config })))
            }
            "outbound" => match build_singbox_outbound(
                node.as_ref().unwrap(),
                "proxy",
                settings.multiplex_enabled,
                settings.multiplex_concurrency,
            ) {
                Ok(outbound) => Outcome::Value(dumps_compact_sorted_ascii(json!({ "outbound": outbound }))),
                Err(error) => Outcome::Error(error.to_string()),
            },
            _ => {
                let hot_switch_nodes: Vec<Node> = case["hot_switch"]
                    .as_array()
                    .map(|ids| ids.iter().filter_map(|id| self.node(id)).collect())
                    .unwrap_or_default();
                let options = PlanRuntimeOptions {
                    routing: Some(RoutingSettings::from_dict(routing)),
                    enable_final_fragment: settings.enable_final_fragment,
                    fragment_packets: settings.fragment_packets.clone(),
                    fragment_length: settings.fragment_length.clone(),
                    fragment_delay: settings.fragment_delay.clone(),
                    tail_fragment_enabled: settings.tail_fragment_enabled,
                    multiplex_enabled: settings.multiplex_enabled,
                    multiplex_concurrency: settings.multiplex_concurrency,
                    discord_proxy_enabled: settings.discord_proxy_enabled,
                    tun_strict_route: settings.tun_strict_route,
                    tun_stack: settings.tun_stack.clone(),
                    tun_mtu: settings.tun_mtu,
                    tun_endpoint_independent_nat: settings.tun_endpoint_independent_nat,
                    tun_block_quic: settings.tun_block_quic,
                    local_socks_port: settings.local_socks_port,
                    local_http_port: settings.local_http_port,
                    proxy_auth_username: if settings.proxy_auth_enabled {
                        settings.proxy_auth_username.clone()
                    } else {
                        String::new()
                    },
                    proxy_auth_password: if settings.proxy_auth_enabled {
                        settings.proxy_auth_password.clone()
                    } else {
                        String::new()
                    },
                    preferred_relay_port: self.corpus["relay_port"].as_i64().unwrap(),
                    preferred_protect_port: self.corpus["protect_port"].as_i64().unwrap(),
                    preferred_protect_password: self.corpus["protect_password"].as_str().unwrap().to_string(),
                    system_dns_servers: vec!["192.168.1.1".to_string()],
                    tun_mode: case["tun_mode"].as_bool().unwrap_or(true),
                    enable_hot_switch: !hot_switch_nodes.is_empty(),
                    hot_switch_nodes,
                    proxy_chain: None,
                    core_preference: settings.core_preference.clone(),
                };
                match plan_singbox_runtime(&self.document, node.as_ref(), options) {
                    Ok(plan) => {
                        let sidecar = plan.xray_sidecar.as_ref();
                        let payload = json!({
                            "outcome": plan.outcome,
                            "singbox_config": plan.singbox_config,
                            "has_proxy_outbound": plan.has_proxy_outbound,
                            "used_selected_node": plan.used_selected_node,
                            "xray_sidecar": sidecar.map(|sidecar| json!({
                                "relay_port": sidecar.relay_port,
                                "protect_port": sidecar.protect_port,
                                "protect_password": sidecar.protect_password,
                                "config": sidecar.config,
                            })),
                            "clash_api_selector": plan.clash_api_selector,
                            "clash_api_node_signatures": plan.clash_api_node_signatures
                                .iter()
                                .map(|(a, b)| json!([a, b]))
                                .collect::<Vec<_>>(),
                            "health_proxy_url": plan.health_proxy_url,
                        });
                        let mut secrets = vec![plan.clash_api_secret.as_str()];
                        if let Some(sidecar) = sidecar {
                            secrets.push(&sidecar.relay_username);
                            secrets.push(&sidecar.relay_password);
                        }
                        let canonical = mask(dumps_compact_sorted_ascii(payload), &secrets);
                        Outcome::Value(canonical.replace(
                            &self.escaped_root,
                            &json_path_prefix(Path::new("<parity-root>")),
                        ))
                    }
                    Err(error) => Outcome::Error(error.to_string()),
                }
            }
        }
    }
}

fn check_case(fixture: &Fixture, index: usize, case: &Value) -> Option<Value> {
    let expected = &case["expected"];
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fixture.run(case)));
    let (matches, actual) = match outcome {
        Ok(Outcome::Value(canonical)) => {
            (expected["sha256"].as_str() == Some(sha256(&canonical).as_str()), json!({ "canonical": canonical }))
        }
        Ok(Outcome::Error(message)) => (expected["error"].as_str() == Some(message.as_str()), json!({ "error": message })),
        Err(panic) => {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
                .unwrap_or_default();
            (false, json!({ "panic": message }))
        }
    };
    (!matches).then(|| {
        json!({ "index": index, "label": case["label"], "node": case["node"], "expected": expected, "rust": actual })
    })
}

fn json_path_prefix(path: &Path) -> String {
    let prefix = format!("{}{}", path.display(), std::path::MAIN_SEPARATOR);
    let quoted = dumps_compact_sorted_ascii(json!(prefix));
    quoted[1..quoted.len() - 1].to_owned()
}

#[test]
fn rust_runtime_configs_match_python_on_the_whole_corpus() -> anyhow::Result<()> {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/config_parity.json")).expect("fixture");
    let temp = tempfile::tempdir()?;
    let root = plain_path(temp.path().canonicalize()?);
    let core = root.join("core");
    let rule_sets = core.join("rule-sets");
    std::fs::create_dir_all(&rule_sets)?;
    // The config builder checks for these paths but never opens the binaries.
    std::fs::File::create(core.join("xray.exe"))?;
    for name in corpus["binary_rule_sets"].as_array().expect("binary rule sets") {
        std::fs::File::create(rule_sets.join(name.as_str().expect("rule-set filename")))?;
    }
    let windows_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../windows");
    let zapret_lists = root.join("zapret/lists");
    std::fs::create_dir_all(&zapret_lists)?;
    for entry in std::fs::read_dir(windows_dir.join("zapret/lists"))? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), zapret_lists.join(entry.file_name()))?;
        }
    }

    // SAFETY: this integration-test binary runs a single test, so no other
    // thread reads the environment concurrently.
    unsafe {
        std::env::set_var("LOCALAPPDATA", root.join("Local"));
        std::env::set_var("APPDATA", root.join("Roaming"));
    }
    let escaped_root = json_path_prefix(&root);
    assert!(
        lumen_core::constants::set_base_dir(root),
        "base dir already fixed"
    );
    let template = template_path();
    let text = std::fs::read_to_string(&template).expect("default sing-box template");
    let document = parse_singbox_document(&template, &text).expect("template parses");
    let fixture = Fixture { corpus, document, escaped_root };
    let cases = fixture.corpus["cases"].as_array().unwrap();
    assert!(cases.len() > 10_000, "corpus too small: {}", cases.len());

    // Cases are independent; spread them over every core (a debug build
    // takes minutes on one thread).
    let threads = std::thread::available_parallelism().map_or(4, |count| count.get());
    let chunk = cases.len().div_ceil(threads);
    let fixture = &fixture;
    let mut failures: Vec<Value> = std::thread::scope(|scope| {
        let workers: Vec<_> = cases
            .chunks(chunk)
            .enumerate()
            .map(|(part, slice)| {
                scope.spawn(move || {
                    let mut found = Vec::new();
                    for (offset, case) in slice.iter().enumerate() {
                        if let Some(failure) = check_case(fixture, part * chunk + offset, case) {
                            found.push(failure);
                        }
                    }
                    found
                })
            })
            .collect();
        workers.into_iter().flat_map(|worker| worker.join().unwrap()).collect()
    });
    failures.sort_by_key(|failure| failure["index"].as_u64());
    if failures.is_empty() {
        return Ok(());
    }
    if let Ok(path) = std::env::var("LUMEN_PARITY_DUMP") {
        std::fs::write(path, serde_json::to_string(&failures).unwrap()).unwrap();
    }
    let shown: Vec<String> = failures
        .iter()
        .take(12)
        .map(|failure| {
            let rust = failure["rust"].to_string();
            let rust: String = rust.chars().take(400).collect();
            format!("#{} {} {}\n  python: {}\n  rust:   {rust}", failure["index"], failure["label"],
                    failure["node"], failure["expected"])
        })
        .collect();
    panic!("{} of {} config cases differ from Python:\n{}", failures.len(), cases.len(), shown.join("\n"));
}
