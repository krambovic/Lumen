from __future__ import annotations

import ctypes
import json
import os
from pathlib import Path
import sys
import threading
from ctypes import wintypes

if sys.platform == "win32":
    import winreg

from .constants import PROXY_HOST, RUNTIME_DIR


INTERNET_OPTION_REFRESH = 37
INTERNET_OPTION_SETTINGS_CHANGED = 39
INTERNET_OPTION_PER_CONNECTION_OPTION = 75
INTERNET_SETTINGS_KEY = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings"
INTERNET_PER_CONN_FLAGS = 1
INTERNET_PER_CONN_PROXY_SERVER = 2
INTERNET_PER_CONN_PROXY_BYPASS = 3
PROXY_TYPE_DIRECT = 0x00000001
PROXY_TYPE_PROXY = 0x00000002
DEFAULT_PROXY_BYPASS = (
    "localhost;127.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;"
    "172.20.*;172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;"
    "172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;192.168.*;"
    "*.lan;*.local;::1"
)


class _InternetPerConnOptionValue(ctypes.Union):
    _fields_ = [
        ("m_Int", wintypes.DWORD),
        ("m_StringPtr", wintypes.LPWSTR),
        ("m_FileTime", wintypes.FILETIME),
    ]


class _InternetPerConnOption(ctypes.Structure):
    _fields_ = [
        ("m_Option", wintypes.DWORD),
        ("m_Value", _InternetPerConnOptionValue),
    ]


class _InternetPerConnOptionList(ctypes.Structure):
    _fields_ = [
        ("dwSize", wintypes.DWORD),
        ("szConnection", wintypes.LPWSTR),
        ("dwOptionCount", wintypes.DWORD),
        ("dwOptionError", wintypes.DWORD),
        ("pOptions", ctypes.POINTER(_InternetPerConnOption)),
    ]


class ProxyManager:
    def __init__(self) -> None:
        self._backup: dict[str, str | int] | None = None
        self._backup_file = RUNTIME_DIR / "system_proxy_backup.json"
        self._firefox_proxy = FirefoxProxyManager()

    @property
    def is_supported(self) -> bool:
        return sys.platform == "win32"

    def _read_settings(self) -> dict[str, str | int]:
        if not self.is_supported:
            return {}
        values: dict[str, str | int] = {}
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, INTERNET_SETTINGS_KEY, 0, winreg.KEY_READ) as key:
            for name, default in (
                ("ProxyEnable", 0),
                ("ProxyServer", ""),
                ("ProxyOverride", ""),
                ("AutoConfigURL", ""),
            ):
                try:
                    values[name], _ = winreg.QueryValueEx(key, name)
                except FileNotFoundError:
                    values[name] = default
        return values

    def _write_settings(self, values: dict[str, str | int]) -> None:
        if not self.is_supported:
            return
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, INTERNET_SETTINGS_KEY, 0, winreg.KEY_SET_VALUE) as key:
            if "ProxyEnable" in values:
                winreg.SetValueEx(key, "ProxyEnable", 0, winreg.REG_DWORD, int(values["ProxyEnable"]))
            if "ProxyServer" in values:
                winreg.SetValueEx(key, "ProxyServer", 0, winreg.REG_SZ, str(values["ProxyServer"]))
            if "ProxyOverride" in values:
                winreg.SetValueEx(key, "ProxyOverride", 0, winreg.REG_SZ, str(values["ProxyOverride"]))
            if "AutoConfigURL" in values:
                winreg.SetValueEx(key, "AutoConfigURL", 0, winreg.REG_SZ, str(values["AutoConfigURL"]))

    def _load_persisted_backup(self) -> dict[str, str | int] | None:
        if not self._backup_file.exists():
            return None
        try:
            payload = json.loads(self._backup_file.read_text(encoding="utf-8"))
        except Exception:
            return None
        if not isinstance(payload, dict):
            return None
        result: dict[str, str | int] = {}
        for key in ("ProxyEnable", "ProxyServer", "ProxyOverride", "AutoConfigURL"):
            if key in payload:
                result[key] = payload[key]
        return result or None

    def _persist_backup(self, values: dict[str, str | int] | None) -> None:
        try:
            if values:
                self._backup_file.parent.mkdir(parents=True, exist_ok=True)
                self._backup_file.write_text(json.dumps(values, ensure_ascii=True, indent=2), encoding="utf-8")
            elif self._backup_file.exists():
                self._backup_file.unlink()
        except Exception:
            pass

    def _refresh_system_proxy(self) -> None:
        if not self.is_supported:
            return

        def _notify() -> None:
            try:
                wininet = ctypes.windll.Wininet
                wininet.InternetSetOptionW(0, INTERNET_OPTION_SETTINGS_CHANGED, 0, 0)
                wininet.InternetSetOptionW(0, INTERNET_OPTION_REFRESH, 0, 0)
            except Exception:
                pass

        threading.Thread(target=_notify, name="proxy-refresh", daemon=True).start()

    def _set_wininet_connection_proxy(self, proxy_server: str, override: str, enabled: bool) -> bool:
        if not self.is_supported:
            return False
        options_count = 3 if enabled and override else 2 if enabled else 1
        options_array_type = _InternetPerConnOption * options_count
        options = options_array_type()
        options[0].m_Option = INTERNET_PER_CONN_FLAGS
        options[0].m_Value.m_Int = PROXY_TYPE_DIRECT | (PROXY_TYPE_PROXY if enabled else 0)
        if enabled:
            options[1].m_Option = INTERNET_PER_CONN_PROXY_SERVER
            options[1].m_Value.m_StringPtr = proxy_server
            if override:
                options[2].m_Option = INTERNET_PER_CONN_PROXY_BYPASS
                options[2].m_Value.m_StringPtr = override
        payload = _InternetPerConnOptionList(
            ctypes.sizeof(_InternetPerConnOptionList),
            None,
            options_count,
            0,
            options,
        )
        wininet = ctypes.windll.Wininet
        wininet.InternetSetOptionW.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPVOID, wintypes.DWORD]
        wininet.InternetSetOptionW.restype = wintypes.BOOL
        ok = bool(
            wininet.InternetSetOptionW(
                0,
                INTERNET_OPTION_PER_CONNECTION_OPTION,
                ctypes.byref(payload),
                ctypes.sizeof(payload),
            )
        )
        if ok:
            self._refresh_system_proxy()
        return ok

    def enable(self, http_port: int, socks_port: int, bypass_lan: bool = True) -> None:
        if not self.is_supported:
            return
        if self._backup is None:
            self._backup = self._read_settings()
            self._persist_backup(self._backup)

        # v2rayN-style system proxy: WinINET points at the local mixed inbound.
        # Xray `mixed` accepts both HTTP and SOCKS on this port, which is more
        # reliable for Necko/Firefox than split protocol-specific ports.
        proxy_server = f"{PROXY_HOST}:{int(socks_port)}"

        override = "<local>;localhost;127.*"
        if bypass_lan:
            override = DEFAULT_PROXY_BYPASS

        self._write_settings(
            {
                "ProxyEnable": 1,
                "ProxyServer": proxy_server,
                "ProxyOverride": override,
                "AutoConfigURL": "",
            }
        )
        if not self._set_wininet_connection_proxy(proxy_server, override, True):
            self._refresh_system_proxy()
        self._firefox_proxy.enable(http_port=int(http_port), socks_port=int(socks_port), bypass_lan=bypass_lan)

    def disable(self, restore_previous: bool = True) -> None:
        if not self.is_supported:
            return
        backup = self._backup or self._load_persisted_backup()
        if restore_previous and backup:
            self._write_settings(dict(backup))
        else:
            self._write_settings({"ProxyEnable": 0, "ProxyServer": "", "ProxyOverride": "", "AutoConfigURL": ""})
        self._backup = None
        self._persist_backup(None)
        self._firefox_proxy.disable()
        if not self._set_wininet_connection_proxy("", "", False):
            self._refresh_system_proxy()

    def is_enabled(self) -> bool:
        if not self.is_supported:
            return False
        values = self._read_settings()
        return int(values.get("ProxyEnable", 0)) == 1


class FirefoxProxyManager:
    """Make Firefox-family browsers follow the Windows system proxy.

    This mirrors v2rayN's model: Windows gets a local mixed proxy endpoint,
    while Necko profiles are nudged into `network.proxy.type = 5`.
    """

    _MARKER_BEGIN = "// Lumen KVN system proxy begin"
    _MARKER_END = "// Lumen KVN system proxy end"

    def __init__(self) -> None:
        self._backup_file = RUNTIME_DIR / "firefox_proxy_backup.json"

    def enable(self, *, http_port: int, socks_port: int, bypass_lan: bool = True) -> None:
        if sys.platform != "win32":
            return
        profiles = self._find_profiles()
        if not profiles:
            return
        backup = self._load_backup()
        changed = False
        for profile in profiles:
            key = str(profile)
            if key not in backup:
                user_js = profile / "user.js"
                prefs_js = profile / "prefs.js"
                backup[key] = {
                    "user.js": user_js.read_text(encoding="utf-8", errors="replace") if user_js.exists() else None,
                    "prefs.js": prefs_js.read_text(encoding="utf-8", errors="replace") if prefs_js.exists() else None,
                }
            self._write_profile_prefs(profile)
            changed = True
        if changed:
            self._save_backup(backup)

    def disable(self) -> None:
        backup = self._load_backup()
        if not backup:
            return
        for profile_text, original in backup.items():
            profile = Path(profile_text)
            files = {"user.js": original} if not isinstance(original, dict) else original
            for file_name, content in files.items():
                if file_name not in {"user.js", "prefs.js"}:
                    continue
                target = profile / file_name
                try:
                    if content is None:
                        if target.exists():
                            remaining = self._strip_managed_block(
                                target.read_text(encoding="utf-8", errors="replace")
                            ).strip()
                            if remaining:
                                target.write_text(remaining + "\n", encoding="utf-8")
                            else:
                                target.unlink()
                    else:
                        target.write_text(str(content), encoding="utf-8")
                except Exception:
                    continue
        self._save_backup({})

    def _find_profiles(self) -> list[Path]:
        roots: list[Path] = []
        appdata = os.environ.get("APPDATA")
        localappdata = os.environ.get("LOCALAPPDATA")
        for base in (appdata, localappdata):
            if not base:
                continue
            root = Path(base)
            roots.extend(
                [
                    root / "Mozilla" / "Firefox" / "Profiles",
                    root / "librewolf" / "Profiles",
                    root / "Waterfox" / "Profiles",
                    root / "Floorp" / "Profiles",
                ]
            )
        profiles: list[Path] = []
        for root in roots:
            if not root.is_dir():
                continue
            for child in root.iterdir():
                if not child.is_dir():
                    continue
                if (child / "prefs.js").exists() or child.suffix.lower() in {".default", ".default-release"}:
                    profiles.append(child)
        return sorted(set(profiles))

    def _write_profile_prefs(self, profile: Path) -> None:
        block = self._build_proxy_block()
        for file_name in ("user.js", "prefs.js"):
            target = profile / file_name
            existing = target.read_text(encoding="utf-8", errors="replace") if target.exists() else ""
            existing = self._strip_managed_block(existing).rstrip()
            text = f"{existing}\n\n{block}\n" if existing else f"{block}\n"
            target.write_text(text, encoding="utf-8")

    def _build_proxy_block(self) -> str:
        return "\n".join(
            [
                self._MARKER_BEGIN,
                'user_pref("network.proxy.type", 5);',
                self._MARKER_END,
            ]
        )

    def _strip_managed_block(self, text: str) -> str:
        start = text.find(self._MARKER_BEGIN)
        end = text.find(self._MARKER_END)
        if start == -1 or end == -1 or end < start:
            return text
        end += len(self._MARKER_END)
        if end < len(text) and text[end:end + 1] == "\n":
            end += 1
        return text[:start].rstrip() + ("\n" if text[:start].strip() and text[end:].strip() else "") + text[end:].lstrip()

    def _load_backup(self) -> dict[str, object]:
        try:
            payload = json.loads(self._backup_file.read_text(encoding="utf-8"))
        except Exception:
            return {}
        if not isinstance(payload, dict):
            return {}
        result: dict[str, object] = {}
        for key, value in payload.items():
            if isinstance(key, str) and (value is None or isinstance(value, str)):
                result[key] = value
            elif isinstance(key, str) and isinstance(value, dict):
                clean: dict[str, str | None] = {}
                for file_name in ("user.js", "prefs.js"):
                    content = value.get(file_name)
                    if content is None or isinstance(content, str):
                        clean[file_name] = content
                result[key] = clean
        return result

    def _save_backup(self, backup: dict[str, object]) -> None:
        try:
            if backup:
                self._backup_file.parent.mkdir(parents=True, exist_ok=True)
                self._backup_file.write_text(json.dumps(backup, ensure_ascii=True, indent=2), encoding="utf-8")
            elif self._backup_file.exists():
                self._backup_file.unlink()
        except Exception:
            pass
