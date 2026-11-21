//! Replays `fixtures/editor_parity.json` (generated from the Python server
//! editor by `rust/tools/parity/gen_editor_corpus.py`) through the Rust
//! `node_edit_helpers`: the editor schema, share links and saved updates for
//! every server in the parser corpus, plus every edit the Python tests make.
//!
//! Set `LUMEN_PARITY_DUMP=<file>` to write the Rust output of mismatches;
//! `gen_editor_corpus.py --case N` prints Python's side.

use lumen_core::models::Node;
use lumen_core::pyjson::dumps_compact_sorted_ascii;
use lumen_ui::view_models::node_edit_helpers::{
    build_node_updates, load_node_edit_fields, new_node_edit_fields, normalized_node_export_link,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
}

fn run(corpus: &Value, case: &Value) -> Result<Value, String> {
    let node = case["node"].as_u64().map(|index| Node::from_dict(&corpus["nodes"][index as usize]));
    match case["fn"].as_str().unwrap() {
        "load_node_edit_fields" => Ok(load_node_edit_fields(node.as_ref().unwrap())),
        "normalized_node_export_link" => Ok(json!(normalized_node_export_link(node.as_ref()))),
        "build_node_updates" => {
            let fields = case["fields"].as_object().cloned().unwrap_or_default();
            build_node_updates(node.as_ref().unwrap(), &fields)
        }
        "new_node_edit_fields" => Ok(new_node_edit_fields(case["protocol"].as_str().unwrap(), "Default")),
        other => panic!("unknown case kind {other}"),
    }
}

#[test]
fn rust_server_editor_matches_python_on_the_whole_corpus() {
    let corpus: Value = serde_json::from_str(include_str!("fixtures/editor_parity.json")).expect("fixture");
    let cases = corpus["cases"].as_array().unwrap();
    assert!(cases.len() > 5_000, "corpus too small: {}", cases.len());
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let expected = &case["expected"];
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&corpus, case)));
        let (matches, actual) = match outcome {
            Ok(Ok(value)) => {
                let canonical = dumps_compact_sorted_ascii(value.clone());
                (expected["sha256"].as_str() == Some(sha256(&canonical).as_str()), json!({ "value": value }))
            }
            Ok(Err(message)) => (expected["error"].as_str() == Some(message.as_str()), json!({ "error": message })),
            Err(panic) => {
                let message = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
                    .unwrap_or_default();
                (false, json!({ "panic": message }))
            }
        };
        if !matches {
            failures.push(json!({ "index": index, "fn": case["fn"], "expected": expected, "rust": actual }));
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
        *kinds.entry(failure["fn"].as_str().unwrap_or("").to_string()).or_default() += 1;
    }
    panic!("{} of {} editor cases differ from Python: {kinds:?}", failures.len(), cases.len());
}
