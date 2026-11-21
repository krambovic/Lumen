"""pytest plugin: record every text the Python test suite feeds the link parser.

Run from ``windows/``::

    set LUMEN_PARITY_RECORD=path\\to\\inputs.jsonl
    python -m pytest -p record_parser_inputs   (with this directory on PYTHONPATH)

Each line is ``{"fn": "parse_links_text" | "parse_single", "text": ...}``.
``gen_parser_corpus.py`` replays them through Python to build the fixture
the Rust parity test checks.
"""

from __future__ import annotations

import functools
import json
import os
import threading

_LOCK = threading.Lock()
_SEEN: set[tuple[str, str]] = set()


def _record(fn: str, text: object) -> None:
    path = os.environ.get("LUMEN_PARITY_RECORD")
    if not path or not isinstance(text, str):
        return
    key = (fn, text)
    with _LOCK:
        if key in _SEEN:
            return
        _SEEN.add(key)
        with open(path, "a", encoding="utf-8") as handle:
            handle.write(json.dumps({"fn": fn, "text": text}, ensure_ascii=False) + "\n")


def pytest_configure(config) -> None:  # noqa: ARG001 - pytest hook signature
    from xray_fluent import link_parser
    from xray_fluent.application import node_service

    original_text = link_parser.parse_links_text
    original_single = link_parser.parse_single

    @functools.wraps(original_text)
    def parse_links_text(text, *args, **kwargs):
        if not kwargs.get("allow_file_reference") and not args:
            _record("parse_links_text", text)
        return original_text(text, *args, **kwargs)

    @functools.wraps(original_single)
    def parse_single(raw):
        _record("parse_single", raw)
        return original_single(raw)

    link_parser.parse_links_text = parse_links_text
    link_parser.parse_single = parse_single
    node_service.parse_links_text = parse_links_text
