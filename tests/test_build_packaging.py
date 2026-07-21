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


def test_subscription_fetcher_is_copied_into_packaged_app(tmp_path: Path) -> None:
    main_executable = tmp_path / "Lumen.exe"
    main_executable.write_bytes(b"pyinstaller-launcher")

    helper = build_qml._install_subscription_fetcher(tmp_path)

    assert helper == tmp_path / "lumen-subscription-fetcher.exe"
    assert helper.read_bytes() == b"pyinstaller-launcher"
