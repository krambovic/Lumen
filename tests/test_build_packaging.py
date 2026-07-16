from __future__ import annotations

from pathlib import Path

import pytest

import build_qml


def test_repository_droute_bundle_is_complete() -> None:
    assert build_qml._validate_droute_bundle() == "2.0.0"


def test_droute_bundle_validation_rejects_missing_payload(tmp_path: Path) -> None:
    (tmp_path / "version.txt").write_text("2.0.0\n", encoding="utf-8")

    with pytest.raises(RuntimeError, match="incomplete"):
        build_qml._validate_droute_bundle(tmp_path)
