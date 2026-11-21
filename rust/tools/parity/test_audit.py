"""Heuristic map of Python tests to Rust test coverage.

For each Python test function the distinctive string literals in its body
(length >= 6, not common words) are looked up in the Rust test code (files
under ``tests/`` or named ``tests.rs``, and everything after
``#[cfg(test)]``). The share of literals found suggests whether the test was
ported; low shares are listed for manual review.

    python rust/tools/parity/test_audit.py [--file test_x.py] [--threshold 0.5]
"""

from __future__ import annotations

import argparse
import ast
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
PY_TESTS = ROOT / "windows" / "tests"
RUST = ROOT / "rust" / "crates"
COMMON = {"proxy", "direct", "example", "default", "enabled", "protocol", "server", "settings", "outbound",
          "routing", "singbox", "config", "server_port", "password", "username", "streamSettings", "network",
          "security", "address", "outbounds", "inbounds", "example.com", "127.0.0.1", "localhost", "utf-8",
          "vless", "vmess", "trojan", "shadowsocks", "wireguard", "hysteria2", "socks", "transport", "Default"}


def rust_test_text() -> str:
    parts = []
    for path in RUST.rglob("*.rs"):
        text = path.read_text(encoding="utf-8", errors="ignore")
        if "tests" in path.parts or path.name == "tests.rs":
            parts.append(text)
            continue
        index = text.find("#[cfg(test)]")
        if index >= 0:
            parts.append(text[index:])
    return "\n".join(parts)


def literals(node: ast.AST) -> set[str]:
    found = set()
    for child in ast.walk(node):
        if isinstance(child, ast.Constant) and isinstance(child.value, str):
            value = child.value.strip()
            if len(value) >= 6 and value not in COMMON and "\n" not in value and len(value) < 200:
                found.add(value)
    return found


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser()
    parser.add_argument("--file")
    parser.add_argument("--threshold", type=float, default=0.34)
    parser.add_argument("--summary", action="store_true")
    args = parser.parse_args()
    corpus = rust_test_text()
    rows = []
    for path in sorted(PY_TESTS.glob("test_*.py")):
        if args.file and path.name != args.file:
            continue
        tree = ast.parse(path.read_text(encoding="utf-8"))
        for node in ast.walk(tree):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name.startswith("test_"):
                values = literals(node)
                hits = [value for value in values if value in corpus]
                share = len(hits) / len(values) if values else None
                rows.append((path.name, node.name, share, len(values), sorted(set(values) - set(hits))[:4]))
    if args.summary:
        per_file: dict[str, list] = {}
        for name, _, share, _, _ in rows:
            per_file.setdefault(name, []).append(share)
        for name, shares in sorted(per_file.items(), key=lambda item: -len(item[1])):
            low = sum(1 for share in shares if share is not None and share < args.threshold)
            none = sum(1 for share in shares if share is None)
            print(f"{len(shares):3} tests  {low:3} low  {none:3} no-literals  {name}")
        return 0
    for name, test, share, count, missing in rows:
        mark = "?" if share is None else ("LOW" if share < args.threshold else "ok")
        shown = "-" if share is None else f"{share:.2f}"
        print(f"{mark:3} {shown:>5} ({count:2}) {name}::{test}  missing={missing}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
