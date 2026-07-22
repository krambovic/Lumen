from __future__ import annotations

from xray_fluent import discord_proxy_manager
from xray_fluent.discord_proxy_manager import DiscordInstall


def test_legacy_droute_marker_is_renamed_without_reinstall(monkeypatch, tmp_path) -> None:
    root = tmp_path / "Discord"
    app_dir = root / "app-1.0.0"
    app_dir.mkdir(parents=True)
    install = DiscordInstall("stable", root, app_dir, app_dir / "Discord.exe", "Discord.exe")
    legacy = root / discord_proxy_manager.LEGACY_DROUTE_INSTALL_VERSION_FILE
    current = root / discord_proxy_manager.DROUTE_INSTALL_VERSION_FILE
    legacy.write_text("2.0.0\n", encoding="utf-8")
    monkeypatch.setattr(discord_proxy_manager, "find_installed_discords", lambda: [install])

    discord_proxy_manager.migrate_legacy_droute_markers()

    assert current.read_text(encoding="utf-8") == "2.0.0\n"
    assert not legacy.exists()


def test_droute_payload_version_marker_controls_reinstall(monkeypatch, tmp_path) -> None:
    bundle_dir = tmp_path / "bundle"
    bundle_dir.mkdir()
    bundle_exe = bundle_dir / "droute.exe"
    bundle_version = bundle_dir / "version.txt"
    bundle_exe.write_bytes(b"bundle")
    monkeypatch.setattr(discord_proxy_manager, "DROUTE_EXE", bundle_exe)
    monkeypatch.setattr(discord_proxy_manager, "DROUTE_VERSION_FILE", bundle_version)

    root = tmp_path / "Discord"
    app_dir = root / "app-1.0.0"
    app_dir.mkdir(parents=True)
    for path in (
        app_dir / "version.dll",
        app_dir / "droute.dll",
        root / "Droute.UpdaterHook.dll",
        root / "Update.exe.config",
    ):
        path.write_bytes(b"payload")
    install = DiscordInstall("stable", root, app_dir, app_dir / "Discord.exe", "Discord.exe")

    assert discord_proxy_manager._droute_payload_installed(install) is True

    bundle_version.write_text("1.2.0\n", encoding="utf-8")
    assert discord_proxy_manager._droute_payload_installed(install) is False

    marker = root / discord_proxy_manager.DROUTE_INSTALL_VERSION_FILE
    marker.write_text("1.1.2\n", encoding="utf-8")
    assert discord_proxy_manager._droute_payload_installed(install) is False

    marker.write_text("1.2.0\n", encoding="utf-8")
    assert discord_proxy_manager._droute_payload_installed(install) is True
