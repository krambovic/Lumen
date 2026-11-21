"""Explain editor-parity mismatches dumped by the Rust test.

    set LUMEN_PARITY_DUMP=edump.json & cargo test -p lumen-ui --test editor_parity
    python rust/tools/parity/diff_editor_dump.py edump.json [--fn NAME] [--limit N] [--buckets]
"""

from __future__ import annotations

import argparse
import collections
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import gen_editor_corpus as gen  # noqa: E402
from diff_dump import diffs, short  # noqa: E402
from xray_fluent.models import Node  # noqa: E402

FIXTURE = gen.FIXTURE


def python_result(corpus: dict, case: dict):
    node = Node.from_dict(corpus["nodes"][case["node"]]) if "node" in case else None
    kind = case["fn"]
    if kind == "load_node_edit_fields":
        return gen.call(gen.helpers.load_node_edit_fields, node)
    if kind == "normalized_node_export_link":
        return gen.call(gen.helpers.normalized_node_export_link, node)
    if kind == "build_node_updates":
        return gen.call(gen.helpers.build_node_updates, node, dict(case.get("fields") or {}))
    return gen.call(gen.helpers.new_node_edit_fields, case["protocol"], "Default")


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser()
    parser.add_argument("dump")
    parser.add_argument("--fn")
    parser.add_argument("--limit", type=int, default=4)
    parser.add_argument("--max-cases", type=int, default=12)
    parser.add_argument("--buckets", action="store_true")
    args = parser.parse_args()
    corpus = json.loads(FIXTURE.read_text(encoding="utf-8"))
    failures = json.load(open(args.dump, encoding="utf-8"))
    buckets: collections.Counter = collections.Counter()
    example: dict[str, int] = {}
    shown = 0
    for failure in failures:
        case = corpus["cases"][failure["index"]]
        if args.fn and case["fn"] != args.fn:
            continue
        python = python_result(corpus, case)
        rust = failure["rust"]
        found = list(diffs(json.loads(gen.canonical(python)), json.loads(gen.canonical(rust))))
        if args.buckets:
            for path, _, _ in found:
                key = case["fn"] + " " + re.sub(r"\[\d+\]", "[]", path)
                buckets[key] += 1
                example.setdefault(key, failure["index"])
            continue
        if shown >= args.max_cases:
            break
        shown += 1
        node = corpus["nodes"][case["node"]] if "node" in case else {}
        print(f"#{failure['index']} {case['fn']} {short(node.get('link', case.get('protocol', '')), 120)} fields={short(case.get('fields'), 80)}")
        for path, a, b in found[: args.limit]:
            print(f"   {path}\n      py: {short(a, 260)}\n      rs: {short(b, 260)}")
    for key, count in buckets.most_common(50):
        print(f"{count:6} {key}  (e.g. #{example[key]})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
