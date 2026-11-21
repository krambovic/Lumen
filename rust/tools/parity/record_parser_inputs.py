"""pytest plugin: record every text the Python test suite feeds the link parser.

Run from ``windows/``::

    set LUMEN_PARITY_RECORD=path\\to\\inputs.jsonl
    python -m pytest -p record_parser_inputs   (with this directory on PYTHONPATH)

Each line is ``{"fn": "parse_links_text" | "parse_single", "text": ...}`` or,
for the server editor, ``{"fn": "build_node_updates", "node": {...},
"fields": {...}}`` / ``{"fn": "load_node_edit_fields", "node": {...}}``.
``gen_parser_corpus.py`` replays them through Python to build the fixture
the Rust parity test checks.
"""

from __future__ import annotations

import functools
import json
import os
import threading

_LOCK = threading.Lock()
_SEEN: set[str] = set()


def _record(fn: str, text: object) -> None:
    if isinstance(text, str):
        _write({"fn": fn, "text": text})


def _write(entry: dict) -> None:
    path = os.environ.get("LUMEN_PARITY_RECORD")
    if not path:
        return
    try:
        line = json.dumps(entry, ensure_ascii=False, sort_keys=True)
    except (TypeError, ValueError):
        return  # non-JSON test doubles
    with _LOCK:
        if line in _SEEN:
            return
        _SEEN.add(line)
        with open(path, "a", encoding="utf-8") as handle:
            handle.write(line + "\n")


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

    from xray_fluent.qml_app.bridge import node_edit_helpers

    original_load = node_edit_helpers.load_node_edit_fields
    original_build = node_edit_helpers.build_node_updates

    def node_dict(node):
        to_dict = getattr(node, "to_dict", None)
        return to_dict() if callable(to_dict) else None

    @functools.wraps(original_load)
    def load_node_edit_fields(node):
        data = node_dict(node)
        if data is not None:
            _write({"fn": "load_node_edit_fields", "node": data})
        return original_load(node)

    @functools.wraps(original_build)
    def build_node_updates(node, fields):
        data = node_dict(node)
        if data is not None and isinstance(fields, dict):
            _write({"fn": "build_node_updates", "node": data, "fields": fields})
        return original_build(node, fields)

    node_edit_helpers.load_node_edit_fields = load_node_edit_fields
    node_edit_helpers.build_node_updates = build_node_updates


# ── pure helper functions (``{"fn": "call", "name": ..., "args": [...]}``) ──

RECORDED_FUNCTIONS = {
    "xray_fluent.log_utils": ("clean_log_text", "classify_log_level", "parse_log_line"),
    "xray_fluent.secret_scrubber": ("scrub_text",),
    "xray_fluent.country_flags": ("get_flag_emoji", "detect_country"),
    "xray_fluent.node_transport": ("node_transport",),
    "xray_fluent.routing_rule_import": ("parse_routing_rules", "normalize_routing_address", "split_routing_addresses"),
    "xray_fluent.happ_crypt": ("is_happ_link", "is_happ_crypt_link", "decrypt_happ_link"),
    "xray_fluent.incy_crypt": ("is_incy_link", "is_incy_crypt_link", "decrypt_incy_link"),
    "xray_fluent.zapret_server_scope": ("node_transports",),
}


def _plain(value):
    to_dict = getattr(value, "to_dict", None)
    if callable(to_dict):
        return {"__node__": to_dict()}
    return value


def _wrap(module_name: str, name: str, original):
    @functools.wraps(original)
    def wrapper(*args, **kwargs):
        _write({"fn": "call", "name": f"{module_name}.{name}",
                "args": [_plain(arg) for arg in args], "kwargs": {k: _plain(v) for k, v in kwargs.items()}})
        return original(*args, **kwargs)

    wrapper.__lumen_original__ = original
    return wrapper


def pytest_sessionstart(session) -> None:  # noqa: ARG001 - pytest hook signature
    import importlib
    import sys

    for module_name, names in RECORDED_FUNCTIONS.items():
        module = importlib.import_module(module_name)
        for name in names:
            original = getattr(module, name)
            if hasattr(original, "__lumen_original__"):
                continue
            wrapped = _wrap(module_name, name, original)
            setattr(module, name, wrapped)
            # Rebind copies imported with `from module import name`.
            for other in list(sys.modules.values()):
                if getattr(other, "__name__", "").startswith("xray_fluent") and getattr(other, name, None) is original:
                    setattr(other, name, wrapped)
