"""Explain config-parity mismatches dumped by the Rust test.

    set LUMEN_PARITY_DUMP=cdump.json & cargo test -p lumen-engines --test config_parity
    python rust/tools/parity/diff_config_dump.py cdump.json [--limit N] [--buckets]

Python's output is recomputed for each dumped case and compared with the
Rust canonical JSON path by path. ``--buckets`` groups the differing paths
(array indexes stripped) across all mismatches to find systematic causes.
"""

from __future__ import annotations

import argparse
import collections
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import gen_config_corpus as gen  # noqa: E402  (sets the data-dir env first)
from diff_dump import diffs, short  # noqa: E402


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser()
    parser.add_argument("dump")
    parser.add_argument("--limit", type=int, default=5)
    parser.add_argument("--max-cases", type=int, default=20)
    parser.add_argument("--buckets", action="store_true")
    parser.add_argument("--kind")
    args = parser.parse_args()
    failures = json.load(open(args.dump, encoding="utf-8"))
    cases, by_id = gen.build_cases()
    buckets: collections.Counter = collections.Counter()
    examples: dict[str, int] = {}
    shown = 0
    for failure in failures:
        case = cases[failure["index"]]
        if args.kind and case["kind"] != args.kind:
            continue
        python = gen.replay(case, by_id)
        if "error" not in python:
            python = json.loads(gen.canonical(python))
        rust = failure["rust"]
        rust = json.loads(rust["canonical"]) if "canonical" in rust else rust
        found = list(diffs(python, rust))
        if args.buckets:
            for path, _, _ in found:
                key = case["kind"] + " " + re.sub(r"\[\d+\]", "[]", path)
                buckets[key] += 1
                examples.setdefault(key, failure["index"])
            continue
        if shown >= args.max_cases:
            break
        shown += 1
        print(f"#{failure['index']} {case['label']} {failure['node']} ({len(found)} diffs)")
        for path, a, b in found[: args.limit]:
            print(f"   {path}\n      py: {short(a, 300)}\n      rs: {short(b, 300)}")
    if args.buckets:
        for key, count in buckets.most_common(60):
            print(f"{count:6} {key}  (e.g. #{examples[key]})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
