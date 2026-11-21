"""Build the server-editor parity fixture for the Rust test suite.

For every distinct server in ``parser_parity.json`` the Python editor helpers
(``qml_app/bridge/node_edit_helpers.py``) produce:

* ``load_node_edit_fields(node)`` - the editor schema and values;
* ``normalized_node_export_link(node)`` - the share link;
* ``build_node_updates(node, edits)`` for an unchanged save and for a
  rename + new address, and for every edit the Python test suite submits
  (recorded with ``record_parser_inputs.py``);

plus ``new_node_edit_fields(protocol)`` for each manual protocol. Results go
to ``rust/crates/lumen-ui/tests/fixtures/editor_parity.json``.

    python rust/tools/parity/gen_editor_corpus.py --recorded inputs.jsonl
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "windows"))

from xray_fluent.models import Node  # noqa: E402
from xray_fluent.qml_app.bridge import node_edit_helpers as helpers  # noqa: E402

PARSER_FIXTURE = ROOT / "rust" / "crates" / "lumen-core" / "tests" / "fixtures" / "parser_parity.json"
FIXTURE = ROOT / "rust" / "crates" / "lumen-ui" / "tests" / "fixtures" / "editor_parity.json"
EDITS = [
    {},
    {"name": "Edited", "server": "new.example", "port": "8443"},
]


def corpus_nodes() -> list[dict]:
    cases = json.loads(PARSER_FIXTURE.read_text(encoding="utf-8"))["cases"]
    seen: set[str] = set()
    nodes: list[dict] = []
    for case in cases:
        expected = case["expected"]
        payloads = expected.get("nodes") or ([expected["node"]] if "node" in expected else [])
        for payload in payloads:
            data = {key: value for key, value in payload.items() if not key.startswith("_")}
            key = json.dumps(data, sort_keys=True)
            if key in seen:
                continue
            seen.add(key)
            data["id"] = f"node-{len(nodes):04d}"
            data["created_at"] = "2026-01-01T00:00:00+00:00"
            nodes.append(data)
    return nodes


def canonical(value) -> str:
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"))


def digest(value) -> str:
    return hashlib.sha256(canonical(value).encode("ascii")).hexdigest()


def call(function, *args):
    try:
        return {"value": function(*args)}
    except Exception as exc:  # noqa: BLE001 - recorded as data
        return {"error": str(exc)}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--recorded", type=Path)
    parser.add_argument("--case", type=int, help="print one case's full Python output")
    args = parser.parse_args()
    cases = []
    for data in corpus_nodes():
        node = Node.from_dict(data)
        cases.append({"fn": "load_node_edit_fields", "node": data,
                      "expected": call(helpers.load_node_edit_fields, Node.from_dict(data))})
        cases.append({"fn": "normalized_node_export_link", "node": data,
                      "expected": call(helpers.normalized_node_export_link, Node.from_dict(data))})
        for edit in EDITS:
            cases.append({"fn": "build_node_updates", "node": data, "fields": edit,
                          "expected": call(helpers.build_node_updates, Node.from_dict(data), dict(edit))})
        del node
    if args.recorded:
        for line in args.recorded.read_text(encoding="utf-8").splitlines():
            entry = json.loads(line)
            if entry["fn"] == "build_node_updates":
                data = dict(entry["node"])
                cases.append({"fn": "build_node_updates", "node": data, "fields": entry["fields"],
                              "expected": call(helpers.build_node_updates, Node.from_dict(data), dict(entry["fields"]))})
            elif entry["fn"] == "load_node_edit_fields":
                data = dict(entry["node"])
                cases.append({"fn": "load_node_edit_fields", "node": data,
                              "expected": call(helpers.load_node_edit_fields, Node.from_dict(data))})
    for protocol in helpers.MANUAL_NODE_PROTOCOLS:
        cases.append({"fn": "new_node_edit_fields", "protocol": protocol,
                      "expected": call(helpers.new_node_edit_fields, protocol, "Default")})
    # Outputs are large (full editor schemas): keep a digest of the canonical
    # JSON and store each server once. ``--case N`` prints one in full.
    if args.case is not None:
        sys.stdout.reconfigure(encoding="utf-8")
        print(json.dumps(cases[args.case], ensure_ascii=False, indent=1))
        return 0
    nodes: dict[str, dict] = {}
    for case in cases:
        if "node" in case:
            key = canonical(case["node"])
            nodes.setdefault(key, case["node"])
            case["node"] = key
        expected = case["expected"]
        case["expected"] = {"error": expected["error"]} if "error" in expected else {"sha256": digest(expected["value"])}
    order = {key: index for index, key in enumerate(nodes)}
    for case in cases:
        if "node" in case:
            case["node"] = order[case["node"]]
    text = json.dumps({"nodes": list(nodes.values()), "cases": cases}, ensure_ascii=False)
    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    FIXTURE.write_text(text + "\n", encoding="utf-8")
    errors = sum(1 for case in cases if "error" in case["expected"])
    print(f"{len(cases)} cases ({errors} expected errors) -> {FIXTURE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
