//! Replays `fixtures/parser_parity.json` (generated from the Python parser by
//! `rust/tools/parity/gen_parser_corpus.py`) through the Rust link parser.
//!
//! The corpus holds every text the Python test suite parses plus generated
//! links and documents for each supported scheme, transport, security mode
//! and container format. Regenerate it after changing the Python parser.

use lumen_core::link_parser::{
    Node, normalize_node_outbound, parse_links_text, parse_single, validate_node_outbound,
};
use serde_json::{Value, json};

fn node_payload(node: &Node) -> Value {
    let mut data = node.to_dict();
    let object = data.as_object_mut().expect("node dict");
    object.shift_remove("id");
    object.shift_remove("created_at");
    object.insert(
        "_validate".into(),
        validate_node_outbound(node).map_or(Value::Null, Value::String),
    );
    let mut normalized = node.clone();
    let changed = normalize_node_outbound(&mut normalized);
    object.insert(
        "_normalized".into(),
        json!({ "changed": changed, "outbound": normalized.outbound }),
    );
    data
}

fn replay(function: &str, text: &str) -> Value {
    if function == "parse_single" {
        return match parse_single(text) {
            Ok(node) => json!({ "node": node_payload(&node) }),
            Err(error) => json!({ "error": error.message }),
        };
    }
    match parse_links_text(text, false) {
        Ok((nodes, errors)) => json!({
            "nodes": nodes.iter().map(node_payload).collect::<Vec<_>>(),
            "errors": errors,
        }),
        Err(error) => json!({ "nodes": [], "errors": [error.message] }),
    }
}

/// Python crashed with a non-`LinkParseError` exception; Rust must still
/// reject the input, but its message is its own.
fn crashed_in_python(expected: &Value) -> bool {
    expected.get("python_exception").is_some()
}

#[test]
fn rust_parser_matches_python_on_the_whole_corpus() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/parser_parity.json")).expect("fixture");
    let cases = corpus["cases"].as_array().expect("cases");
    assert!(cases.len() > 2000, "corpus too small: {}", cases.len());
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let function = case["fn"].as_str().unwrap();
        let input = case["input"].as_str().unwrap();
        let expected = &case["expected"];
        let actual = replay(function, input);
        let matches = if crashed_in_python(expected) {
            actual.get("error").is_some()
        } else {
            &actual == expected
        };
        if !matches {
            failures.push((index, case["origin"].as_str().unwrap_or(""), function, input, expected, actual));
        }
    }
    if !failures.is_empty() {
        if let Ok(path) = std::env::var("LUMEN_PARITY_DUMP") {
            let dump: Vec<Value> = failures
                .iter()
                .map(|(index, origin, function, input, expected, actual)| {
                    json!({"index": index, "origin": origin, "fn": function, "input": input,
                           "python": expected, "rust": actual})
                })
                .collect();
            std::fs::write(path, serde_json::to_string(&dump).unwrap()).unwrap();
        }
        let shown: Vec<String> = failures
            .iter()
            .take(15)
            .map(|(index, origin, function, input, expected, actual)| {
                let input: String = input.chars().take(200).collect();
                format!(
                    "#{index} [{origin}] {function}({input:?})\n  python: {}\n  rust:   {}",
                    short(expected),
                    short(actual)
                )
            })
            .collect();
        panic!(
            "{} of {} corpus cases differ from Python:\n{}",
            failures.len(),
            cases.len(),
            shown.join("\n")
        );
    }
}

fn short(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() > 1500 {
        format!("{}...", text.chars().take(1500).collect::<String>())
    } else {
        text
    }
}
