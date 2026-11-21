"""Build the helper-function parity fixture for the Rust test suite.

Replays, through the Python implementation:

* every call the Python test suite makes to the pure helpers listed in
  ``record_parser_inputs.RECORDED_FUNCTIONS`` (log cleaning and levels, the
  secret scrubber, flags and countries, transport labels, routing rule
  import, Happ/Incy decryption), recorded with ``--recorded``;
* synthetic calls: every parser-corpus input through the scrubber, every
  corpus server through the transport/flag/country helpers, and a set of
  generated core log lines through the log helpers.

Results go to ``rust/crates/lumen-core/tests/fixtures/helper_parity.json``.

    python rust/tools/parity/gen_helper_corpus.py --recorded inputs.jsonl
"""

from __future__ import annotations

import argparse
import dataclasses
import importlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "windows"))
sys.path.insert(0, str(Path(__file__).resolve().parent))

from record_parser_inputs import RECORDED_FUNCTIONS  # noqa: E402
from xray_fluent.models import Node  # noqa: E402

PARSER_FIXTURE = ROOT / "rust" / "crates" / "lumen-core" / "tests" / "fixtures" / "parser_parity.json"
FIXTURE = ROOT / "rust" / "crates" / "lumen-core" / "tests" / "fixtures" / "helper_parity.json"

LOG_LINES = [
    "[xray] 2026/06/22 12:00:00 [Info] app/proxyman/inbound: connection opened",
    "[xray] 2026/06/22 12:00:00 [Warning] core: deprecated option",
    "[xray] 2026/06/22 12:00:00 [Error] app/dns: failed to lookup example.com",
    "[singbox] \x1b[31m+0300 2026-06-22 12:00:00 ERROR\x1b[0m [2503566513] 7ms dns: exchange failed",
    "[singbox] +0300 2026-06-22 12:00:00 INFO [123 0ms] inbound/tun[tun-in]: inbound connection to 1.1.1.1:443",
    "[singbox] +0300 2026-06-22 12:00:00 WARN router: deprecated rule",
    "[singbox] FATAL[0000] start service: start inbound/tun[tun-in]: configure tun interface: Access is denied.",
    "[singbox] ERROR [4263401339 226ms] connection: open connection to 142.251.9.188:5228 using outbound/trojan[proxy]: i/o timeout",
    "[singbox] ERROR router: process DNS packet: endpoint not initialized",
    "[singbox-error] sing-box exited during startup: FATAL start inbound/tun: configure tun interface: Element not found.",
    "[xray] process stopped with code 0",
    "[singbox] process stopped with code 1",
    "[singbox] process stopped with code 1 (expected)",
    "[zapret] ERROR: windivert: access denied",
    "[zapret] WARN: deprecated option",
    "[app] connection is ready",
    "[transition] background task error: Regional rule set geoip:ir was not found.",
    "[core] 1 repeated connection failures hidden in 30 s (outbound/direct[direct], i/o timeout)",
    "abc\x00\x07 def",
    "  plain text without a source  ",
    "",
    "PANIC: runtime error",
    "[tun] WARN deprecated option",
    "[xray] connected",
    "ERROR: four",
    "WARNING: three",
    "INFO: one",
    "Normal application warning",
    "unexpected HTTP response status: 502",
]


def plain(value):
    if isinstance(value, dict) and "__node__" in value:
        return Node.from_dict(value["__node__"])
    return value


def output(value):
    if dataclasses.is_dataclass(value):
        data = dataclasses.asdict(value)
        data.pop("timestamp", None)
        return data
    if isinstance(value, tuple):
        return [output(item) for item in value]
    if isinstance(value, list):
        return [output(item) for item in value]
    return value


def call(name: str, args: list, kwargs: dict) -> dict:
    module_name, function = name.rsplit(".", 1)
    function = getattr(importlib.import_module(module_name), function)
    try:
        return {"value": output(function(*[plain(arg) for arg in args], **{k: plain(v) for k, v in kwargs.items()}))}
    except Exception as exc:  # noqa: BLE001 - recorded as data
        return {"error": str(exc)}


def corpus_inputs_and_nodes() -> tuple[list[str], list[dict]]:
    cases = json.loads(PARSER_FIXTURE.read_text(encoding="utf-8"))["cases"]
    texts, nodes, seen = [], [], set()
    for case in cases:
        texts.append(case["input"])
        expected = case["expected"]
        for payload in expected.get("nodes") or ([expected["node"]] if "node" in expected else []):
            data = {key: value for key, value in payload.items() if not key.startswith("_")}
            key = json.dumps(data, sort_keys=True)
            if key not in seen:
                seen.add(key)
                data["id"] = f"node-{len(nodes):04d}"
                data["created_at"] = "2026-01-01T00:00:00+00:00"
                nodes.append(data)
    return texts, nodes


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--recorded", type=Path)
    args = parser.parse_args()
    calls: list[tuple[str, list, dict]] = []
    if args.recorded:
        for line in args.recorded.read_text(encoding="utf-8").splitlines():
            entry = json.loads(line)
            if entry.get("fn") == "call":
                calls.append((entry["name"], entry["args"], entry.get("kwargs") or {}))
    texts, nodes = corpus_inputs_and_nodes()
    for text in texts:
        calls.append(("xray_fluent.secret_scrubber.scrub_text", [text], {}))
        calls.append(("xray_fluent.happ_crypt.is_happ_link", [text], {}))
        calls.append(("xray_fluent.incy_crypt.is_incy_link", [text], {}))
    for data in nodes:
        node = {"__node__": data}
        calls.append(("xray_fluent.node_transport.node_transport", [node], {}))
        calls.append(("xray_fluent.zapret_server_scope.node_transports", [node], {}))
        calls.append(("xray_fluent.country_flags.detect_country", [data["name"], data["server"]], {}))
        calls.append(("xray_fluent.secret_scrubber.scrub_text", [data["link"]], {}))
    for code in ["", "us", "DE", "gb", "uk", "xx", "eu", "un", "ru", "1", "usa", "ZZ"]:
        calls.append(("xray_fluent.country_flags.get_flag_emoji", [code], {}))
    for line in LOG_LINES:
        calls.append(("xray_fluent.log_utils.clean_log_text", [line], {}))
        calls.append(("xray_fluent.log_utils.classify_log_level", [line], {}))
        calls.append(("xray_fluent.log_utils.parse_log_line", [line], {}))
    seen: set[str] = set()
    cases = []
    for name, call_args, kwargs in calls:
        key = json.dumps([name, call_args, kwargs], sort_keys=True, ensure_ascii=False)
        if key in seen:
            continue
        seen.add(key)
        cases.append({"name": name.removeprefix("xray_fluent."), "args": call_args, "kwargs": kwargs,
                      "expected": call(name, call_args, kwargs)})
    FIXTURE.write_text(json.dumps({"cases": cases}, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"{len(cases)} cases -> {FIXTURE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
