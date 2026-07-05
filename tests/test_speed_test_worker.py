from __future__ import annotations

from xray_fluent.speed_test_worker import _resolve_speed_test_concurrency


def test_configured_speed_concurrency_is_not_capped_at_six() -> None:
    assert _resolve_speed_test_concurrency(node_count=30, configured=10) == 10


def test_auto_speed_concurrency_uses_default_ten() -> None:
    assert _resolve_speed_test_concurrency(node_count=30, configured=0) == 10


def test_speed_concurrency_never_exceeds_node_count() -> None:
    assert _resolve_speed_test_concurrency(node_count=3, configured=10) == 3
