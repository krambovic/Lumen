from __future__ import annotations

import ctypes
import json
import logging
import os
import re
import tempfile

from pathlib import Path
import sys
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
INTERNET_PER_CONN_AUTOCONFIG_URL = 4
_PROXY_FIELDS = ("ProxyEnable", "ProxyServer", "ProxyOverride", "AutoConfigURL")
_logger = logging.getLogger(__name__)
PROXY_TYPE_DIRECT = 0x00000001
PROXY_TYPE_PROXY = 0x00000002
RAS_MAX_ENTRY_NAME = 256
MAX_PATH = 260
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


class _RasEntryName(ctypes.Structure):
    _fields_ = [
        ("dwSize", wintypes.DWORD),
        ("szEntryName", wintypes.WCHAR * (RAS_MAX_ENTRY_NAME + 1)),
        ("dwFlags", wintypes.DWORD),
        ("szPhonebookPath", wintypes.WCHAR * (MAX_PATH + 1)),
    ]


def _process_creation_time(pid: int) -> int | None:
    # Query one known process, never enumerate/kill by path. No new dependency.
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel.CloseHandle.restype = wintypes.BOOL
    kernel.GetProcessTimes.argtypes = [wintypes.HANDLE, *([ctypes.POINTER(wintypes.FILETIME)] * 4)]
    kernel.GetProcessTimes.restype = wintypes.BOOL
    handle = kernel.OpenProcess(0x1000, False, pid)
    if not handle:
        error = ctypes.get_last_error()
        if error == 87:  # ERROR_INVALID_PARAMETER: PID no longer exists.
            return None
        raise OSError(error, "Cannot verify proxy owner")
    try:
        times = [wintypes.FILETIME() for _ in range(4)]
        if not kernel.GetProcessTimes(handle, *(ctypes.byref(value) for value in times)):
            raise OSError(ctypes.get_last_error(), "Cannot verify process generation")
        return (times[0].dwHighDateTime << 32) | times[0].dwLowDateTime
    finally:
        kernel.CloseHandle(handle)


class ProxyManager:
    def __init__(self) -> None:
        self._backup: dict[str, str | int] | None = None
        self._applied: dict[str, str | int] | None = None
        self._owner: tuple[int, int] | None = None
        self._last_wininet_error: tuple[int, int] = (0, 0)
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

    @staticmethod
    def _is_lumen_proxy(value: object) -> bool:
        for entry in str(value or "").replace(",", ";").split(";"):
            endpoint = entry.strip()
            if not endpoint:
                continue
            _scheme, _separator, address = endpoint.rpartition("=")
            host = address.rsplit(":", 1)[0].strip().strip("[]")
            if host == PROXY_HOST:
                return True
        return False

    def _snapshot_settings(self) -> dict[str, str | int]:
        # Localhost may belong to another client. Never infer ownership from it.
        values = self._read_settings()
        flags = self._query_connection_flags()
        if flags is None:
            raise RuntimeError("Не удалось сохранить исходные параметры WinINET")
        values["WinInetFlags"] = flags
        return values

    def _query_connection_flags(self) -> int | None:
        if not self.is_supported:
            return None
        options = (_InternetPerConnOption * 1)()
        options[0].m_Option = INTERNET_PER_CONN_FLAGS
        payload = _InternetPerConnOptionList(ctypes.sizeof(_InternetPerConnOptionList), None, 1, 0, options)
        size = wintypes.DWORD(ctypes.sizeof(payload))
        try:
            query = ctypes.windll.Wininet.InternetQueryOptionW
            query.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPVOID, ctypes.POINTER(wintypes.DWORD)]
            query.restype = wintypes.BOOL
            if query(0, INTERNET_OPTION_PER_CONNECTION_OPTION, ctypes.byref(payload), ctypes.byref(size)):
                return int(options[0].m_Value.m_Int)
        except Exception:
            pass
        return None

    def _matches_settings(self, expected: dict[str, str | int] | None) -> bool:
        if expected is None:
            return False
        current = self._read_settings()
        return all(current.get(key) == expected.get(key) for key in _PROXY_FIELDS) and self._query_connection_flags() == expected.get("WinInetFlags")

    def _matches_registry_settings(self, expected: dict[str, str | int] | None) -> bool:
        if expected is None:
            return False
        current = self._read_settings()
        return all(current.get(key) == expected.get(key) for key in _PROXY_FIELDS)

    def _owner_is_live_elsewhere(self) -> bool:
        if self._owner is None:
            return False
        pid, created = self._owner
        try:
            return pid != os.getpid() and _process_creation_time(pid) == created
        except (OSError, AttributeError):
            return True

    def reconcile_stale_state(self) -> bool:
        if not self.is_supported or self._load_persisted_backup() is None:
            return False
        return self.disable(restore_previous=True)

    def _load_persisted_backup(self) -> dict[str, str | int] | None:
        if self._backup is None:
            self._applied = self._owner = None
        if not self._backup_file.exists():
            return None
        try:
            payload = json.loads(self._backup_file.read_text(encoding="utf-8"))
            if not isinstance(payload, dict) or payload.get("version") != 2:
                raise ValueError("unproven legacy ownership")
            original, applied, owner = payload["original"], payload["applied"], payload["owner"]
            for snapshot in (original, applied):
                if not isinstance(snapshot, dict) or any(key not in snapshot for key in (*_PROXY_FIELDS, "WinInetFlags")):
                    raise ValueError("invalid proxy snapshot")
                if type(snapshot["ProxyEnable"]) is not int or snapshot["ProxyEnable"] not in (0, 1):
                    raise ValueError("invalid proxy flag")
                if type(snapshot["WinInetFlags"]) is not int or not 0 <= snapshot["WinInetFlags"] <= 15:
                    raise ValueError("invalid WinINET flags")
                if any(not isinstance(snapshot[key], str) for key in _PROXY_FIELDS[1:]):
                    raise ValueError("invalid proxy strings")
            if not isinstance(owner, list) or len(owner) != 2 or type(owner[0]) is not int or not 0 < owner[0] <= 0xFFFFFFFF:
                raise ValueError("invalid proxy owner")
            if type(owner[1]) is not int or owner[1] <= 0:
                raise ValueError("invalid owner generation")
            self._applied, self._owner = dict(applied), (owner[0], owner[1])
            if self._owner_is_live_elsewhere():
                return None
            return dict(original)
        except (OSError, ValueError, KeyError, TypeError):
            _logger.warning("[proxy] Unproven or legacy backup retained; manual recovery required")
            return None

    def _persist_backup(self, values: dict[str, str | int] | None) -> None:
        if values is None:
            self._backup_file.unlink(missing_ok=True)
            return
        # Recovery must be durable BEFORE any system mutation. Never swallow failure.
        payload = {"version": 2, "original": values, "applied": self._applied, "owner": self._owner}
        self._backup_file.parent.mkdir(parents=True, exist_ok=True)
        staged = None
        try:
            with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=self._backup_file.parent, prefix=".proxy-", suffix=".tmp", delete=False) as stream:
                staged = Path(stream.name)
                json.dump(payload, stream, ensure_ascii=True)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(staged, self._backup_file)
        finally:
            if staged is not None:
                staged.unlink(missing_ok=True)

    def _refresh_system_proxy(self) -> None:
        if not self.is_supported:
            return

        try:
            wininet = ctypes.windll.Wininet
            wininet.InternetSetOptionW(0, INTERNET_OPTION_SETTINGS_CHANGED, 0, 0)
            wininet.InternetSetOptionW(0, INTERNET_OPTION_REFRESH, 0, 0)
        except Exception:
            pass

    def _set_wininet_connection_proxy(self, proxy_server: str, override: str, enabled: bool) -> bool:
        if not self.is_supported:
            return False
        result = self._set_connection_proxy(None, proxy_server, override, enabled)
        # Do not mutate RAS connections without per-connection snapshots.
        if result:
            self._refresh_system_proxy()
        return result

    def _set_connection_proxy(self, connection_name: str | None, proxy_server: str, override: str,
                              enabled: bool, *, flags: int | None = None, auto_config_url: str = "") -> bool:
        effective_flags = (
            int(flags)
            if flags is not None
            else PROXY_TYPE_DIRECT | (PROXY_TYPE_PROXY if enabled else 0)
        )
        values: list[tuple[int, int | str]] = [(INTERNET_PER_CONN_FLAGS, effective_flags)]

        # WinINET rejects an INTERNET_PER_CONN_OPTION_LIST on a number of
        # Windows 10/11 builds when AUTOCONFIG_URL is supplied as an empty
        # fourth option.  Send only options which are active.  The registry
        # write performed by the caller still clears stale fields before this
        # native notification, while a real PAC URL is restored explicitly.
        if enabled or effective_flags & PROXY_TYPE_PROXY:
            values.append((INTERNET_PER_CONN_PROXY_SERVER, str(proxy_server)))
            if override:
                values.append((INTERNET_PER_CONN_PROXY_BYPASS, str(override)))
        if auto_config_url:
            values.append((INTERNET_PER_CONN_AUTOCONFIG_URL, str(auto_config_url)))

        options = (_InternetPerConnOption * len(values))()
        string_buffers: list[ctypes.Array] = []
        for index, (option, value) in enumerate(values):
            options[index].m_Option = option
            if isinstance(value, int):
                options[index].m_Value.m_Int = value
            else:
                # Keep native buffers alive through InternetSetOptionW.  A raw
                # temporary Python string pointer is not a safe ownership
                # boundary for the WinINET call.
                buffer = ctypes.create_unicode_buffer(value)
                string_buffers.append(buffer)
                options[index].m_Value.m_StringPtr = ctypes.cast(buffer, wintypes.LPWSTR)
        payload = _InternetPerConnOptionList(
            ctypes.sizeof(_InternetPerConnOptionList),
            connection_name,
            len(values),
            0,
            options,
        )
        setter = ctypes.windll.Wininet.InternetSetOptionW
        setter.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPVOID, wintypes.DWORD]
        setter.restype = wintypes.BOOL
        ok = bool(setter(0, INTERNET_OPTION_PER_CONNECTION_OPTION, ctypes.byref(payload), ctypes.sizeof(payload)))
        if ok:
            self._last_wininet_error = (0, 0)
            return True
        try:
            error_code = int(ctypes.windll.kernel32.GetLastError())
        except Exception:
            error_code = 0
        self._last_wininet_error = (error_code, int(payload.dwOptionError))
        _logger.warning(
            "[proxy] InternetSetOptionW failed: error=%d option_error=%d option_count=%d",
            error_code,
            int(payload.dwOptionError),
            len(values),
        )
        return False

    def _restore_settings(self, values: dict[str, str | int]) -> None:
        self._write_settings(values)
        if not self._set_connection_proxy(None, str(values["ProxyServer"]), str(values["ProxyOverride"]),
                                          bool(values["ProxyEnable"]), flags=int(values["WinInetFlags"]),
                                          auto_config_url=str(values["AutoConfigURL"])):
            self._refresh_system_proxy()
            raise RuntimeError("Не удалось полностью восстановить параметры WinINET; резервная копия сохранена")
        self._refresh_system_proxy()

    def _enumerate_ras_entries(self) -> list[str]:
        try:
            rasapi = ctypes.windll.Rasapi32
            rasapi.RasEnumEntriesW.argtypes = [
                wintypes.LPCWSTR,
                wintypes.LPCWSTR,
                ctypes.POINTER(_RasEntryName),
                ctypes.POINTER(wintypes.DWORD),
                ctypes.POINTER(wintypes.DWORD),
            ]
            rasapi.RasEnumEntriesW.restype = wintypes.DWORD

            count = wintypes.DWORD(1)
            buffer_size = wintypes.DWORD(ctypes.sizeof(_RasEntryName))
            entries_type = _RasEntryName * 1
            entries = entries_type()
            entries[0].dwSize = ctypes.sizeof(_RasEntryName)
            error_buffer_too_small = 603
            result = rasapi.RasEnumEntriesW(None, None, entries, ctypes.byref(buffer_size), ctypes.byref(count))
            if result == error_buffer_too_small and buffer_size.value > 0:
                entry_count = max(1, buffer_size.value // ctypes.sizeof(_RasEntryName))
                entries_type = _RasEntryName * entry_count
                entries = entries_type()
                for entry in entries:
                    entry.dwSize = ctypes.sizeof(_RasEntryName)
                result = rasapi.RasEnumEntriesW(None, None, entries, ctypes.byref(buffer_size), ctypes.byref(count))
            if result != 0:
                return []
            return [
                str(entries[index].szEntryName)
                for index in range(min(count.value, len(entries)))
                if str(entries[index].szEntryName).strip()
            ]
        except Exception:
            return []

    def enable(
        self,
        http_port: int,
        socks_port: int,
        bypass_lan: bool = True,
        configure_firefox: bool = False,
    ) -> None:
        if not self.is_supported:
            return
        previous = (self._backup, self._applied, self._owner)
        if self._backup is None:
            saved = self._load_persisted_backup()
            if saved is None and self._backup_file.exists():
                raise RuntimeError("Сохранённые настройки прокси требуют проверки: другой экземпляр или неподтверждённая резервная копия")
            self._backup = saved if saved is not None else self._snapshot_settings()
        if self._owner_is_live_elsewhere() or (self._applied is not None and not self._matches_settings(self._applied)):
            raise RuntimeError("Системный прокси изменён вне Lumen; автоматическая перезапись отменена")

        # v2rayN-style system proxy: WinINET points at the local mixed inbound.
        # Xray `mixed` accepts both HTTP and SOCKS on this port, which is more
        # reliable for Necko/Firefox than split protocol-specific ports.
        proxy_server = f"{PROXY_HOST}:{int(socks_port)}"

        override = "<local>;localhost;127.*"
        if bypass_lan:
            override = DEFAULT_PROXY_BYPASS

        self._applied = {"ProxyEnable": 1, "ProxyServer": proxy_server, "ProxyOverride": override,
                         "AutoConfigURL": "", "WinInetFlags": PROXY_TYPE_DIRECT | PROXY_TYPE_PROXY}
        try:
            created = _process_creation_time(os.getpid())
            if created is None:
                raise RuntimeError("Не удалось определить владельца системного прокси")
            self._owner = (os.getpid(), created)
            self._persist_backup(self._backup)
        except Exception:
            self._backup, self._applied, self._owner = previous
            raise
        try:
            self._write_settings(self._applied)
            if not self._set_wininet_connection_proxy(proxy_server, override, True):
                # Some WinINET builds return FALSE even though the documented
                # registry-backed LAN settings were accepted.  Refresh and
                # verify both the complete registry snapshot and the effective
                # PROXY flag before accepting that result.  Never continue on
                # an unverified partial write.
                self._refresh_system_proxy()
                effective_flags = self._query_connection_flags()
                if (
                    not self._matches_registry_settings(self._applied)
                    or effective_flags is None
                    or not effective_flags & PROXY_TYPE_PROXY
                ):
                    error_code, option_error = self._last_wininet_error
                    detail = f" (Win32={error_code}, option={option_error})" if error_code or option_error else ""
                    raise RuntimeError(f"Не удалось применить параметры WinINET{detail}")
                self._applied["WinInetFlags"] = int(effective_flags)
                self._persist_backup(self._backup)
                _logger.warning(
                    "[proxy] WinINET returned failure, but verified registry settings and effective flags were applied"
                )
        except Exception:
            # Roll back only the values just written by this operation.
            if all(self._read_settings().get(key) == self._applied.get(key) for key in _PROXY_FIELDS):
                self._restore_settings(self._backup)
                self._persist_backup(None)
                self._backup = self._applied = self._owner = None
            raise
        # Firefox profile integration is optional.  WinINET is already fully
        # configured above, so a locked/read-only browser profile must never
        # turn a successful system-proxy operation into a connection failure.
        try:
            if configure_firefox:
                self._firefox_proxy.enable(
                    http_port=int(http_port),
                    socks_port=int(socks_port),
                    bypass_lan=bypass_lan,
                )
            else:
                # Remove overrides left by older Lumen versions where this
                # integration was always enabled.
                self._firefox_proxy.disable()
        except Exception:
            pass

    def disable(self, restore_previous: bool = True) -> bool:
        if not self.is_supported:
            return False
        backup = self._backup if self._backup is not None else self._load_persisted_backup()
        if backup is None or self._owner_is_live_elsewhere():
            return False
        if not self._matches_settings(self._applied):
            _logger.warning("[proxy] External changes detected; proxy and recovery snapshot left untouched")
            return False
        desired = backup if restore_previous else {"ProxyEnable": 0, "ProxyServer": "", "ProxyOverride": "",
                                                   "AutoConfigURL": "", "WinInetFlags": PROXY_TYPE_DIRECT}
        self._restore_settings(desired)
        self._persist_backup(None)
        self._backup = self._applied = self._owner = None
        try:
            self._firefox_proxy.disable()
        except Exception:
            _logger.warning("[proxy] Firefox recovery remains pending", exc_info=True)
        return True

    def disable_necko_overrides(self) -> None:
        """Restore only Firefox-family browser proxy prefs managed by Lumen."""
        self._firefox_proxy.disable()

    def is_enabled(self) -> bool:
        if not self.is_supported:
            return False
        values = self._read_settings()
        return int(values.get("ProxyEnable", 0)) == 1


class FirefoxProxyManager:
    """Make Firefox-family browsers use Lumen's mixed proxy directly.

    Windows itself follows v2rayN's mixed-port system proxy model. Some Necko
    profiles still read `network.proxy.type = 5` unreliably, so profile prefs
    use the same mixed endpoint explicitly.
    """

    _MARKER_BEGIN = "// Lumen system proxy begin"
    _MARKER_END = "// Lumen system proxy end"

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
            try:
                key = str(profile)
                if key not in backup:
                    user_js = profile / "user.js"
                    prefs_js = profile / "prefs.js"
                    backup[key] = {
                        "user.js": user_js.read_text(encoding="utf-8", errors="replace") if user_js.exists() else None,
                        "prefs.js": prefs_js.read_text(encoding="utf-8", errors="replace") if prefs_js.exists() else None,
                    }
                    # Persist originals before attempting to modify either file.
                    self._save_backup(backup)
                backup[key]["__applied"] = self._build_proxy_block(mixed_port=int(socks_port), bypass_lan=bypass_lan)
                self._save_backup(backup)
                self._write_profile_prefs(profile, mixed_port=int(socks_port), bypass_lan=bypass_lan)
                changed = True
            except Exception:
                # One protected Firefox profile must not prevent other profiles
                # (or the Windows system proxy) from being configured.
                continue
        if changed:
            self._save_backup(backup)

    def disable(self) -> None:
        backup = self._load_backup()
        remaining = {}
        for profile_text, original in backup.items():
            profile = Path(profile_text)
            files = {"user.js": original} if not isinstance(original, dict) else original
            failed = {}
            applied = files.get("__applied", "")
            for file_name, content in files.items():
                if file_name not in {"user.js", "prefs.js"}:
                    continue
                target = profile / file_name
                try:
                    if not target.exists():
                        continue  # Respect an external deletion.
                    current = target.read_text(encoding="utf-8", errors="replace")
                    expected = applied
                    if not expected:
                        start, end = current.find(self._MARKER_BEGIN), current.find(self._MARKER_END)
                        if start < 0 or end < start:
                            failed[file_name] = content
                            continue  # Legacy recovery without ownership is manual.
                        expected = current[start:end + len(self._MARKER_END)]
                    restored = self._restore_profile_text(current, str(content or ""), expected)
                    if restored != current:
                        if content is None and not restored.strip():
                            target.unlink()
                        else:
                            target.write_text(restored, encoding="utf-8")
                except Exception:
                    failed[file_name] = content
            if failed:
                if applied:
                    failed["__applied"] = applied
                remaining[profile_text] = failed
        self._save_backup(remaining)

    @staticmethod
    def _preference_lines(text: str) -> dict[str, tuple[object, str]]:
        result = {}
        for line in text.splitlines():
            match = re.fullmatch(r"\s*user_pref\((.*)\);\s*", line)
            if match is None:
                continue
            try:
                pair = json.loads("[" + match.group(1) + "]")
                if len(pair) == 2 and isinstance(pair[0], str):
                    result[pair[0]] = (pair[1], line)
            except (ValueError, TypeError):
                continue
        return result

    def _restore_profile_text(self, current: str, original: str, applied: str) -> str:
        expected = self._preference_lines(applied)
        live = self._preference_lines(current)
        # Treat the configuration as one lease. A changed proxy key can indicate
        # a different client; unrelated browser preference changes are preserved.
        if not expected or any(key not in live or live[key][0] != value[0] for key, value in expected.items()):
            return current
        before = self._preference_lines(original)
        lines = []
        for line in current.splitlines(keepends=True):
            if line.strip() in {self._MARKER_BEGIN, self._MARKER_END}:
                continue
            if any(key in expected for key in self._preference_lines(line)):
                continue
            lines.append(line)
        restored = "".join(lines).rstrip()
        originals = [before[key][1] for key in expected if key in before]
        return "\n".join(part for part in (restored, *originals) if part) + ("\n" if restored or originals else "")

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

    def _write_profile_prefs(self, profile: Path, *, mixed_port: int, bypass_lan: bool = True) -> None:
        block = self._build_proxy_block(mixed_port=mixed_port, bypass_lan=bypass_lan)
        for file_name in ("user.js", "prefs.js"):
            target = profile / file_name
            existing = target.read_text(encoding="utf-8", errors="replace") if target.exists() else ""
            existing = self._strip_managed_block(existing).rstrip()
            text = f"{existing}\n\n{block}\n" if existing else f"{block}\n"
            target.write_text(text, encoding="utf-8")

    def _build_proxy_block(self, *, mixed_port: int, bypass_lan: bool = True) -> str:
        no_proxies = "localhost, 127.0.0.1"
        if bypass_lan:
            no_proxies += ", 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, *.local, *.lan"
        return "\n".join(
            [
                self._MARKER_BEGIN,
                'user_pref("network.proxy.type", 1);',
                f'user_pref("network.proxy.http", "{PROXY_HOST}");',
                f'user_pref("network.proxy.http_port", {int(mixed_port)});',
                f'user_pref("network.proxy.ssl", "{PROXY_HOST}");',
                f'user_pref("network.proxy.ssl_port", {int(mixed_port)});',
                f'user_pref("network.proxy.ftp", "{PROXY_HOST}");',
                f'user_pref("network.proxy.ftp_port", {int(mixed_port)});',
                f'user_pref("network.proxy.socks", "{PROXY_HOST}");',
                f'user_pref("network.proxy.socks_port", {int(mixed_port)});',
                'user_pref("network.proxy.socks_version", 5);',
                'user_pref("network.proxy.socks_remote_dns", true);',
                'user_pref("network.proxy.share_proxy_settings", true);',
                f'user_pref("network.proxy.no_proxies_on", "{no_proxies}");',
                'user_pref("network.trr.mode", 5);',
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
                for file_name in ("user.js", "prefs.js", "__applied"):
                    if file_name not in value:
                        continue
                    content = value.get(file_name)
                    if content is None or isinstance(content, str):
                        clean[file_name] = content
                result[key] = clean
        return result

    def _save_backup(self, backup: dict[str, object]) -> None:
        if not backup:
            self._backup_file.unlink(missing_ok=True)
            return
        self._backup_file.parent.mkdir(parents=True, exist_ok=True)
        staged = None
        try:
            with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=self._backup_file.parent, prefix=".firefox-", suffix=".tmp", delete=False) as stream:
                staged = Path(stream.name)
                json.dump(backup, stream, ensure_ascii=True)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(staged, self._backup_file)
        finally:
            if staged is not None:
                staged.unlink(missing_ok=True)
