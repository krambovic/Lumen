"""Summarise a parity mismatch dump: the first differing JSON paths per case.

    python rust/tools/parity/diff_dump.py dump.json [--limit N] [--case INDEX]
"""

from __future__ import annotations

import argparse
import collections
import json
import sys


def diffs(python, rust, path="$"):
    if type(python) is not type(rust):
        yield path, python, rust
        return
    if isinstance(python, dict):
        for key in sorted(set(python) | set(rust)):
            if key not in python or key not in rust:
                yield f"{path}.{key}", python.get(key, "<missing>"), rust.get(key, "<missing>")
            else:
                yield from diffs(python[key], rust[key], f"{path}.{key}")
    elif isinstance(python, list):
        if len(python) != len(rust):
            yield f"{path}[len]", len(python), len(rust)
        for index, (a, b) in enumerate(zip(python, rust)):
            yield from diffs(a, b, f"{path}[{index}]")
    elif python != rust:
        yield path, python, rust


def short(value, limit=160):
    text = json.dumps(value, ensure_ascii=False)
    return text if len(text) <= limit else text[:limit] + "..."


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser()
    parser.add_argument("dump")
    parser.add_argument("--limit", type=int, default=6)
    parser.add_argument("--case", type=int)
    args = parser.parse_args()
    cases = json.load(open(args.dump, encoding="utf-8"))
    buckets = collections.Counter()
    for case in cases:
        if args.case is not None and case["index"] != args.case:
            continue
        found = list(diffs(case["python"], case["rust"]))
        for path, _, _ in found:
            buckets[path.split("[")[0] if "outbound" not in path else path] += 1
        print(f"#{case['index']} {case['fn']} {short(case['input'], 110)}")
        for path, python, rust in found[: args.limit]:
            print(f"   {path}\n      py: {short(python)}\n      rs: {short(rust)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
