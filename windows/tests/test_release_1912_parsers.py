import json
from pathlib import Path
import pytest
from xray_fluent.link_parser import parse_links_text

CORPUS = Path(__file__).resolve().parents[2] / "android/core/config/src/test/resources/subscription_regressions_1912.json"
CASES = json.loads(CORPUS.read_text(encoding="utf-8"))["cases"]

@pytest.mark.parametrize("fixture", CASES, ids=[case["name"] for case in CASES])
def test_shared_subscription_corpus(fixture):
    nodes, errors = parse_links_text(fixture["text"])
    assert len(nodes) == fixture["count"], errors
    assert bool(errors) == fixture["error"], errors
    if "names" in fixture:
        assert [node.name for node in nodes] == fixture["names"]
    assert all(node.scheme != "unknown" and '\"network\":' not in node.name for node in nodes)


def test_metadata_extraction_does_not_stringify_objects_or_drop_other_arrays():
    from xray_fluent.application.node_service import _extract_userinfo_from_body
    first, second = CASES[0]["text"].splitlines()
    body = json.dumps({"user": {"upload": 7}, "links": [{"url": first}], "nodes": [second]})
    normalized, info = _extract_userinfo_from_body(body)
    nodes, errors = parse_links_text(normalized)
    assert len(nodes) == 2
    assert not errors
    assert info["upload"] == 7
