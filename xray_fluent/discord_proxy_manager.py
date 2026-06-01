from __future__ import annotations

import ctypes
import json
import os
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from urllib.request import Request

from .constants import APP_VERSION, DATA_DIR
from .http_utils import urlopen
from .subprocess_utils import CREATE_NO_WINDOW, result_output_text, run_text_pumped


FORCE_PROXY_RELEASE_URL = "https://github.com/runetfreedom/force-proxy/releases/download/v0.2.0/force-proxy.dll"
FORCE_PROXY_SOURCE_URL = "https://github.com/runetfreedom/force-proxy"
FORCE_PROXY_DIR = DATA_DIR / "external" / "force-proxy"
FORCE_PROXY_DLL = FORCE_PROXY_DIR / "force-proxy.dll"
FORCE_PROXY_NOTICE = FORCE_PROXY_DIR / "README.force-proxy.txt"

_DISCORD_BRANCHES = {
    "stable": ("Discord", "Discord.exe"),
    "canary": ("DiscordCanary", "DiscordCanary.exe"),
    "ptb": ("DiscordPTB", "DiscordPTB.exe"),
}


@dataclass(slots=True)
class DiscordInstall:
    branch_id: str
    root: Path
    app_dir: Path
    exe_path: Path
    process_name: str


@dataclass(slots=True)
class DiscordProxyResult:
    ok: bool
    message: str
    affected: int = 0


def _version_key(path: Path) -> tuple[int, ...]:
    raw = path.name.removeprefix("app-")
    parts: list[int] = []
    for chunk in raw.split("."):
        try:
            parts.append(int(chunk))
        except ValueError:
            parts.append(0)
    return tuple(parts)


def _local_appdata() -> Path:
    value = os.environ.get("LOCALAPPDATA")
    if value:
        return Path(value)
    return Path.home() / "AppData" / "Local"


def _find_install(branch_id: str) -> DiscordInstall | None:
    folder, exe_name = _DISCORD_BRANCHES[branch_id]
    root = _local_appdata() / folder
    if not root.is_dir():
        return None
    app_dirs = [p for p in root.glob("app-*") if p.is_dir()]
    if not app_dirs:
        return None
    app_dir = sorted(app_dirs, key=_version_key, reverse=True)[0]
    exe_path = app_dir / exe_name
    if not exe_path.is_file():
        return None
    return DiscordInstall(branch_id, root, app_dir, exe_path, exe_name)


def find_installed_discords() -> list[DiscordInstall]:
    installs: list[DiscordInstall] = []
    for branch_id in _DISCORD_BRANCHES:
        install = _find_install(branch_id)
        if install is not None:
            installs.append(install)
    return installs


def _write_force_proxy_notice() -> None:
    FORCE_PROXY_NOTICE.write_text(
        "force-proxy.dll is an external GPL-3.0 component downloaded from:\n"
        f"{FORCE_PROXY_SOURCE_URL}\n\n"
        "Bebra VPN does not embed force-proxy source code. The DLL is used as a separate helper\n"
        "to route Discord TCP/UDP traffic to the local SOCKS5 proxy.\n",
        encoding="utf-8",
    )


def ensure_force_proxy_dll() -> Path:
    FORCE_PROXY_DIR.mkdir(parents=True, exist_ok=True)
    if FORCE_PROXY_DLL.is_file() and FORCE_PROXY_DLL.stat().st_size > 0:
        _write_force_proxy_notice()
        return FORCE_PROXY_DLL

    request = Request(FORCE_PROXY_RELEASE_URL, headers={"User-Agent": f"BebraVPN/{APP_VERSION}"})
    with urlopen(request, timeout=30) as response:
        payload = response.read()
    if len(payload) < 1024:
        raise RuntimeError("force-proxy.dll download is damaged")
    FORCE_PROXY_DLL.write_bytes(payload)
    _write_force_proxy_notice()
    return FORCE_PROXY_DLL


def _discord_processes() -> list[dict[str, object]]:
    names = ",".join(f"'{exe}'" for _, exe in _DISCORD_BRANCHES.values())
    script = (
        f"$names = @({names}); "
        "$items = @(Get-CimInstance Win32_Process | Where-Object { $names -contains $_.Name } | "
        "Select-Object ProcessId,Name,ExecutablePath); "
        "if ($items.Count -eq 0) { '[]' } else { $items | ConvertTo-Json -Compress }"
    )
    result = run_text_pumped(
        ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
        timeout=6,
        creationflags=CREATE_NO_WINDOW,
    )
    if result.returncode != 0:
        return []
    text = result_output_text(result).strip()
    if not text:
        return []
    try:
        data = json.loads(text)
    except json.JSONDecodeError:
        return []
    if isinstance(data, dict):
        return [data]
    if isinstance(data, list):
        return [item for item in data if isinstance(item, dict)]
    return []


def _terminate_discord(install: DiscordInstall) -> None:
    try:
        run_text_pumped(
            ["taskkill", "/F", "/T", "/IM", install.process_name],
            timeout=8,
            creationflags=CREATE_NO_WINDOW,
        )
    except Exception:
        pass


def _launch_discord(install: DiscordInstall, *, socks_port: int | None = None) -> None:
    env = os.environ.copy()
    if socks_port is not None:
        env["SOCKS5_PROXY_ADDRESS"] = "127.0.0.1"
        env["SOCKS5_PROXY_PORT"] = str(int(socks_port))
        env.setdefault("SOCKS5_PROXY_TIMEOUT", "5000")
    subprocess.Popen(
        [str(install.exe_path)],
        cwd=str(install.app_dir),
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        close_fds=True,
    )


def _wait_for_discord_pids(install: DiscordInstall, timeout_sec: float = 10.0) -> list[int]:
    started = time.monotonic()
    deadline = time.monotonic() + timeout_sec
    last: list[int] = []
    last_change = started
    while time.monotonic() < deadline:
        current = _process_pids_for_install(install)
        if current:
            if current != last:
                last = current
                last_change = time.monotonic()
            if time.monotonic() - started >= 3.0 and time.monotonic() - last_change >= 1.0:
                return current
        time.sleep(0.25)
    return last


def _process_pids_for_install(install: DiscordInstall) -> list[int]:
    pids: list[int] = []
    install_root = str(install.root).lower()
    for item in _discord_processes():
        if str(item.get("Name") or "").lower() != install.process_name.lower():
            continue
        exe_path = str(item.get("ExecutablePath") or "").lower()
        if install_root not in exe_path:
            continue
        try:
            pid = int(item.get("ProcessId") or 0)
        except (TypeError, ValueError):
            continue
        if pid > 0:
            pids.append(pid)
    return sorted(set(pids))


def _inject_dll(pid: int, dll_path: Path) -> None:
    if os.name != "nt":
        raise RuntimeError("Discord proxy is only available on Windows")

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.OpenProcess.argtypes = [ctypes.c_uint32, ctypes.c_bool, ctypes.c_uint32]
    kernel32.OpenProcess.restype = ctypes.c_void_p
    kernel32.VirtualAllocEx.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_uint32, ctypes.c_uint32]
    kernel32.VirtualAllocEx.restype = ctypes.c_void_p
    kernel32.WriteProcessMemory.argtypes = [
        ctypes.c_void_p,
        ctypes.c_void_p,
        ctypes.c_void_p,
        ctypes.c_size_t,
        ctypes.POINTER(ctypes.c_size_t),
    ]
    kernel32.WriteProcessMemory.restype = ctypes.c_bool
    kernel32.GetModuleHandleW.argtypes = [ctypes.c_wchar_p]
    kernel32.GetModuleHandleW.restype = ctypes.c_void_p
    kernel32.GetProcAddress.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    kernel32.GetProcAddress.restype = ctypes.c_void_p
    kernel32.CreateRemoteThread.argtypes = [
        ctypes.c_void_p,
        ctypes.c_void_p,
        ctypes.c_size_t,
        ctypes.c_void_p,
        ctypes.c_void_p,
        ctypes.c_uint32,
        ctypes.POINTER(ctypes.c_uint32),
    ]
    kernel32.CreateRemoteThread.restype = ctypes.c_void_p
    kernel32.WaitForSingleObject.argtypes = [ctypes.c_void_p, ctypes.c_uint32]
    kernel32.WaitForSingleObject.restype = ctypes.c_uint32
    kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
    kernel32.CloseHandle.restype = ctypes.c_bool

    process_rights = 0x0002 | 0x0400 | 0x0008 | 0x0020 | 0x0010
    mem_commit = 0x1000
    mem_reserve = 0x2000
    page_readwrite = 0x04

    handle = kernel32.OpenProcess(process_rights, False, int(pid))
    if not handle:
        raise OSError(ctypes.get_last_error(), f"OpenProcess failed for PID {pid}")

    thread = None
    try:
        dll_text = str(dll_path.resolve())
        data = ctypes.create_unicode_buffer(dll_text)
        size = ctypes.sizeof(data)
        remote_mem = kernel32.VirtualAllocEx(handle, None, size, mem_commit | mem_reserve, page_readwrite)
        if not remote_mem:
            raise OSError(ctypes.get_last_error(), f"VirtualAllocEx failed for PID {pid}")
        written = ctypes.c_size_t(0)
        if not kernel32.WriteProcessMemory(handle, remote_mem, data, size, ctypes.byref(written)):
            raise OSError(ctypes.get_last_error(), f"WriteProcessMemory failed for PID {pid}")
        kernel = kernel32.GetModuleHandleW("kernel32.dll")
        load_library = kernel32.GetProcAddress(kernel, b"LoadLibraryW") if kernel else None
        if not load_library:
            raise OSError(ctypes.get_last_error(), "LoadLibraryW not found")
        thread_id = ctypes.c_uint32(0)
        thread = kernel32.CreateRemoteThread(handle, None, 0, load_library, remote_mem, 0, ctypes.byref(thread_id))
        if not thread:
            raise OSError(ctypes.get_last_error(), f"CreateRemoteThread failed for PID {pid}")
        kernel32.WaitForSingleObject(thread, 8000)
    finally:
        if thread:
            kernel32.CloseHandle(thread)
        kernel32.CloseHandle(handle)


class DiscordProxyManager:
    def __init__(self) -> None:
        self._injected_pids: set[int] = set()
        self._active_socks_port = 0

    def _inject_pids(self, pids: list[int], dll_path: Path) -> tuple[int, list[str]]:
        affected = 0
        errors: list[str] = []
        for pid in pids:
            if pid in self._injected_pids:
                continue
            try:
                _inject_dll(pid, dll_path)
                self._injected_pids.add(pid)
                affected += 1
            except Exception as exc:
                errors.append(f"PID {pid}: {exc}")
        return affected, errors

    def _restart_install_with_proxy(self, install: DiscordInstall, socks_port: int, dll_path: Path) -> tuple[int, list[str]]:
        _terminate_discord(install)
        self._injected_pids.difference_update(_process_pids_for_install(install))
        _launch_discord(install, socks_port=int(socks_port))
        pids = _wait_for_discord_pids(install)
        if not pids:
            return 0, [f"{install.process_name}: process did not start"]
        affected, errors = self._inject_pids(pids, dll_path)
        return affected, [f"{install.process_name}: {error}" for error in errors]

    def enable(self, socks_port: int) -> DiscordProxyResult:
        if os.name != "nt":
            return DiscordProxyResult(False, "Discord proxy is only available on Windows")
        installs = find_installed_discords()
        if not installs:
            return DiscordProxyResult(False, "Discord not found")
        if int(socks_port) <= 0:
            return DiscordProxyResult(False, "Bebra VPN SOCKS5 port not found")

        try:
            dll_path = ensure_force_proxy_dll()
        except Exception as exc:
            return DiscordProxyResult(False, f"Failed to prepare force-proxy.dll: {exc}")

        self._active_socks_port = int(socks_port)
        self._injected_pids.clear()
        affected = 0
        errors: list[str] = []
        for install in installs:
            try:
                count, branch_errors = self._restart_install_with_proxy(install, int(socks_port), dll_path)
                affected += count
                errors.extend(branch_errors)
            except Exception as exc:
                errors.append(f"{install.process_name}: {exc}")

        if affected:
            message = f"Discord started through SOCKS5 127.0.0.1:{int(socks_port)}"
            if errors:
                message += f"; partial errors: {'; '.join(errors[:2])}"
            return DiscordProxyResult(True, message, affected)
        return DiscordProxyResult(False, "; ".join(errors) or "Failed to enable Discord proxy")

    def ensure_active(self, socks_port: int) -> DiscordProxyResult:
        if os.name != "nt":
            return DiscordProxyResult(False, "Discord proxy is only available on Windows")
        if int(socks_port) <= 0:
            return DiscordProxyResult(False, "Bebra VPN SOCKS5 port not found")
        if self._active_socks_port and int(socks_port) != self._active_socks_port:
            return self.enable(int(socks_port))

        installs = find_installed_discords()
        if not installs:
            return DiscordProxyResult(True, "Discord not found", 0)

        try:
            dll_path = ensure_force_proxy_dll()
        except Exception as exc:
            return DiscordProxyResult(False, f"Failed to prepare force-proxy.dll: {exc}")

        current_pids = {int(item.get("ProcessId") or 0) for item in _discord_processes()}
        self._injected_pids.intersection_update(current_pids)
        self._active_socks_port = int(socks_port)

        affected = 0
        errors: list[str] = []
        for install in installs:
            pids = _process_pids_for_install(install)
            if not pids:
                continue

            injected_here = [pid for pid in pids if pid in self._injected_pids]
            if not injected_here:
                try:
                    count, branch_errors = self._restart_install_with_proxy(install, int(socks_port), dll_path)
                    affected += count
                    errors.extend(branch_errors)
                except Exception as exc:
                    errors.append(f"{install.process_name}: {exc}")
                continue

            count, inject_errors = self._inject_pids(pids, dll_path)
            affected += count
            errors.extend(f"{install.process_name}: {error}" for error in inject_errors)

        if errors and not affected:
            return DiscordProxyResult(False, "; ".join(errors[:3]))
        if affected:
            return DiscordProxyResult(True, f"Discord proxy refreshed for processes: {affected}", affected)
        return DiscordProxyResult(True, "Discord proxy is active", 0)

    def disable(self) -> DiscordProxyResult:
        installs = find_installed_discords()
        if not installs:
            return DiscordProxyResult(False, "Discord not found")
        affected = 0
        errors: list[str] = []
        for install in installs:
            try:
                _terminate_discord(install)
                _launch_discord(install, socks_port=None)
                affected += 1
            except Exception as exc:
                errors.append(f"{install.process_name}: {exc}")
        self._injected_pids.clear()
        self._active_socks_port = 0
        if affected:
            return DiscordProxyResult(True, "Discord restarted without force-proxy", affected)
        return DiscordProxyResult(False, "; ".join(errors) or "Failed to disable Discord proxy")
