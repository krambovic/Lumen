from __future__ import annotations

import json
from pathlib import Path

from xray_fluent import release_notes
from xray_fluent.constants import APP_VERSION

ROOT = Path(__file__).resolve().parents[1]
LOCALES = ROOT / "xray_fluent" / "locales"


def test_current_release_has_curated_notes() -> None:
    assert APP_VERSION == "2.1.3"
    assert len(release_notes.RELEASE_NOTES[APP_VERSION]) == 4
    assert APP_VERSION in release_notes.RELEASE_NOTES
    groups = release_notes.notes_between(release_notes.LEGACY_PROFILE_VERSION, APP_VERSION)
    assert groups[0]["version"] == APP_VERSION
    assert groups[0]["items"] == [dict(item) for item in release_notes.RELEASE_NOTES[APP_VERSION]]


def test_release_notes_are_translated() -> None:
    strings = release_notes.release_note_strings()
    for language in ("en", "fa", "zh"):
        catalog = json.loads((LOCALES / f"{language}.json").read_text(encoding="utf-8"))
        missing = sorted(text for text in strings if not catalog.get(text))
        assert missing == [], (language, missing)


def test_version_comparison_rules() -> None:
    assert release_notes.version_key("v2.0.0-pre.3") == (2, 0, 0)
    assert release_notes.version_key("bad") == ()
    assert release_notes.is_upgrade("", "2.0.0") is False  # fresh install
    assert release_notes.is_upgrade("0", "2.0.0") is True  # pre-2.0 profile
    assert release_notes.is_upgrade("1.9.18", "2.0.0") is True
    assert release_notes.is_upgrade("2.0.0", "2.0.0") is False
    assert release_notes.is_upgrade("2.1.0", "2.0.0") is False  # downgrade


def test_unknown_release_falls_back_to_generic_note() -> None:
    groups = release_notes.notes_between("2.0.0", "2.0.1")
    assert [group["version"] for group in groups] == ["2.0.1"]
    assert groups[0]["items"] == [release_notes.FALLBACK_NOTE]


def test_skipped_releases_are_combined_newest_first(monkeypatch) -> None:
    notes = dict(release_notes.RELEASE_NOTES)
    notes["2.1.0"] = ({"glyph": "", "title": "a", "text": "b"},)
    monkeypatch.setattr(release_notes, "RELEASE_NOTES", notes)
    groups = release_notes.notes_between("1.9.18", "2.1.0")
    assert [group["version"] for group in groups] == ["2.1.0", "2.0.0"]


def test_changelog_matches_app_version() -> None:
    changelog = (ROOT.parent / "CHANGELOG.md").read_text(encoding="utf-8")
    assert f"...v{APP_VERSION}" in changelog
