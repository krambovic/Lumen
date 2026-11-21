//! Replays `fixtures/helper_parity.json` (generated from Python by
//! `rust/tools/parity/gen_helper_corpus.py`) through the Rust helpers: log
//! cleaning and levels, the secret scrubber, flags and countries, transport
//! labels, routing rule import and Happ/Incy decryption. The inputs are every
//! call the Python test suite makes plus corpus-wide synthetic calls.

use lumen_core::country_flags::{detect_country, get_flag_emoji};
use lumen_core::crypt::{decrypt_happ_link, decrypt_incy_link, is_happ_crypt_link, is_happ_link, is_incy_crypt_link, is_incy_link};
use lumen_core::log_utils::{classify_log_level, clean_log_text, parse_log_line};
use lumen_core::models::Node;
use lumen_core::node_transport::node_transport;
use lumen_core::routing_rule_import::{normalize_routing_address, parse_routing_rules, split_routing_addresses};
use lumen_core::secret_scrubber::scrub_text;
use lumen_core::zapret_server_scope::node_transports;
use serde_json::{Value, json};

fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}

fn node(value: &Value) -> Node {
    Node::from_dict(&value["__node__"])
}

fn run(name: &str, args: &[Value], kwargs: &Value) -> Result<Value, String> {
    let arg = |index: usize| text(&args[index]);
    Ok(match name {
        "log_utils.clean_log_text" => json!(clean_log_text(&arg(0))),
        "log_utils.classify_log_level" => json!(classify_log_level(&arg(0))),
        "log_utils.parse_log_line" => {
            let entry = parse_log_line(&arg(0), None);
            json!({
                "level": entry.level, "source": entry.source, "message": entry.message,
                "details": entry.details, "action_id": entry.action_id,
                "action_label": entry.action_label, "search_text": entry.search_text,
            })
        }
        "secret_scrubber.scrub_text" => json!(scrub_text(&args[0])),
        "country_flags.get_flag_emoji" => json!(get_flag_emoji(&arg(0))),
        "country_flags.detect_country" => {
            let server = if args.len() > 1 { arg(1) } else { text(&kwargs["server"]) };
            json!(detect_country(&arg(0), &server))
        }
        "node_transport.node_transport" => json!(node_transport(&node(&args[0]))),
        "zapret_server_scope.node_transports" => {
            let (first, second) = node_transports(&node(&args[0]));
            json!([first, second])
        }
        "routing_rule_import.parse_routing_rules" => {
            let suffix = kwargs.get("suffix").map(text).unwrap_or_default();
            let rules = parse_routing_rules(&arg(0), &suffix).map_err(|error| error.to_string())?;
            json!(rules.into_iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>())
        }
        "routing_rule_import.normalize_routing_address" => json!(normalize_routing_address(&arg(0))),
        "routing_rule_import.split_routing_addresses" => json!(split_routing_addresses(&arg(0))),
        "happ_crypt.is_happ_link" => json!(is_happ_link(&arg(0))),
        "happ_crypt.is_happ_crypt_link" => json!(is_happ_crypt_link(&arg(0))),
        "happ_crypt.decrypt_happ_link" => json!(decrypt_happ_link(&arg(0)).map_err(|error| error.message)?),
        "incy_crypt.is_incy_link" => json!(is_incy_link(&arg(0))),
        "incy_crypt.is_incy_crypt_link" => json!(is_incy_crypt_link(&arg(0))),
        "incy_crypt.decrypt_incy_link" => json!(decrypt_incy_link(&arg(0)).map_err(|error| error.message)?),
        other => panic!("no Rust mapping for {other}"),
    })
}

#[test]
fn rust_helpers_match_python_on_the_whole_corpus() {
    let corpus: Value = serde_json::from_str(include_str!("fixtures/helper_parity.json")).expect("fixture");
    let cases = corpus["cases"].as_array().unwrap();
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let args = case["args"].as_array().unwrap();
        let expected = &case["expected"];
        let outcome = std::panic::catch_unwind(|| run(name, args, &case["kwargs"]));
        let actual = match outcome {
            Ok(Ok(value)) => json!({ "value": value }),
            Ok(Err(error)) => json!({ "error": error }),
            Err(panic) => json!({ "panic": panic.downcast_ref::<String>().cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string())) }),
        };
        if &actual != expected {
            failures.push(json!({"index": index, "name": name, "args": args, "python": expected, "rust": actual}));
        }
    }
    if failures.is_empty() {
        return;
    }
    if let Ok(path) = std::env::var("LUMEN_PARITY_DUMP") {
        std::fs::write(path, serde_json::to_string(&failures).unwrap()).unwrap();
    }
    let mut kinds = std::collections::BTreeMap::<String, usize>::new();
    for failure in &failures {
        *kinds.entry(failure["name"].as_str().unwrap().to_string()).or_default() += 1;
    }
    panic!("{} of {} helper cases differ from Python: {kinds:?}", failures.len(), cases.len());
}
