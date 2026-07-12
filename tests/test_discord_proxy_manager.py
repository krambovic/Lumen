from __future__ import annotations

from xray_fluent import discord_proxy_manager
from xray_fluent.discord_proxy_manager import DiscordInstall


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
