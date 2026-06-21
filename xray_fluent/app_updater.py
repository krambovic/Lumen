"""Self-update: check GitHub releases, download setup, install, restart."""

from __future__ import annotations

import hashlib
import json
import logging
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path
from urllib.request import Request

from .http_utils import build_opener, urlopen
from PyQt6.QtCore import QThread, pyqtSignal

from .constants import APP_VERSION, BASE_DIR

GITHUB_REPO = "krambovic/lumen-kvn"
GITHUB_API = f"https://api.github.com/repos/{GITHUB_REPO}/releases/latest"
# Список релизов (новые → старые), включая pre-release, для выбора по каналу.
RELEASES_API = f"https://api.github.com/repos/{GITHUB_REPO}/releases?per_page=30"
USER_AGENT = f"LumenKVN/{APP_VERSION}"
APP_ID = "{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}_is1"


def _powershell_literal(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def _write_utf8_bom_text(path: Path, text: str) -> None:
    path.write_bytes(b"\xef\xbb\xbf" + text.encode("utf-8"))


def _program_files_app_dir() -> Path:
    app_name = "Lumen KVN"
    for env_name in ("ProgramW6432", "ProgramFiles"):
        root = os.environ.get(env_name)
        if root:
            return (Path(root) / app_name).resolve(strict=False)
    return Path(r"C:\Program Files") / app_name


def _is_root_program_dir(path: Path) -> bool:
    resolved = path.resolve(strict=False)
    parent = resolved.parent
    return resolved.name.lower() == "program" and parent == Path(resolved.anchor)


def _registered_install_dir() -> Path | None:
    if sys.platform != "win32":
        return None
    try:
        import winreg
    except Exception:
        return None

    subkeys = (
        rf"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{APP_ID}",
        rf"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\{APP_ID}",
    )
    roots = (winreg.HKEY_LOCAL_MACHINE, winreg.HKEY_CURRENT_USER)
    for root in roots:
        for subkey in subkeys:
            try:
                with winreg.OpenKey(root, subkey) as key:
                    value, _ = winreg.QueryValueEx(key, "InstallLocation")
            except OSError:
                continue
            text = str(value).strip().strip('"')
            if text:
                return Path(text).resolve(strict=False)
    return None


def _target_app_dir(current_app_dir: Path) -> Path:
    if _is_root_program_dir(current_app_dir):
        return _program_files_app_dir()
    registered = _registered_install_dir()
    if registered is not None:
        if _is_root_program_dir(registered):
            return _program_files_app_dir()
        return registered
    if sys.platform == "win32":
        return _program_files_app_dir()
    return current_app_dir.resolve(strict=False)


def _launch_update_script(script: Path, *, elevated: bool) -> bool:
    if sys.platform == "win32" and elevated:
        try:
            import ctypes

            args = subprocess.list2cmdline([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-File",
                str(script),
            ])
            result = ctypes.windll.shell32.ShellExecuteW(
                None,
                "runas",
                "powershell.exe",
                args,
                str(script.parent),
                0,
            )
            return int(result) > 32
        except Exception:
            return False

    subprocess.Popen(
        [
            "powershell",
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
            str(script),
        ],
        creationflags=0x08000000,
        close_fds=True,
    )
    return True


@dataclass(slots=True)
class AppUpdate:
    version: str
    tag: str
    download_url: str
    size: int
    notes: str
    digest_sha256: str = ""
    asset_name: str = ""
    channel: str = "stable"
    # True — целевая версия ниже установленной (откат при смене канала).
    is_downgrade: bool = False


def should_auto_install(
    update: AppUpdate | None,
    *,
    enabled: bool,
    allow_updates: bool,
) -> bool:
    """Automatic updates may upgrade the app, but must never downgrade it."""
    return bool(
        update is not None
        and enabled
        and allow_updates
        and not update.is_downgrade
    )


_SEMVER_RE = re.compile(r"(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?")


def _parse_semver(version: str) -> tuple[int, int, int, list[str]] | None:
    match = _SEMVER_RE.search(version.strip().lstrip("v"))
    if not match:
        return None
    major, minor, patch, suffix = match.groups()
    prerelease = suffix.split(".") if suffix else []
    return int(major), int(minor), int(patch), prerelease


def _compare_prerelease(left: list[str], right: list[str]) -> int:
    if not left and not right:
        return 0
    if not left:
        return 1
    if not right:
        return -1

    for left_part, right_part in zip(left, right):
        if left_part == right_part:
            continue
        left_is_num = left_part.isdigit()
        right_is_num = right_part.isdigit()
        if left_is_num and right_is_num:
            left_num = int(left_part)
            right_num = int(right_part)
            if left_num != right_num:
                return 1 if left_num > right_num else -1
            continue
        if left_is_num != right_is_num:
            return -1 if left_is_num else 1
        return 1 if left_part > right_part else -1

    if len(left) == len(right):
        return 0
    return 1 if len(left) > len(right) else -1


def _is_newer_version(latest: str, current: str) -> bool:
    latest_parts = _parse_semver(latest)
    current_parts = _parse_semver(current)
    if latest_parts is None or current_parts is None:
        return latest.strip().lstrip("v") != current.strip().lstrip("v")

    latest_core = latest_parts[:3]
    current_core = current_parts[:3]
    if latest_core != current_core:
        return latest_core > current_core
    return _compare_prerelease(latest_parts[3], current_parts[3]) > 0


def _extract_digest(value: str) -> str:
    text = value.strip().lower()
    if text.startswith("sha256:"):
        text = text.split(":", 1)[1].strip()
    parts = "".join(ch for ch in text if ch in "0123456789abcdef")
    return parts if len(parts) == 64 else ""


def _sha256_file(file_path: Path) -> str:
    digest = hashlib.sha256()
    with open(file_path, "rb") as file:
        while True:
            chunk = file.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def _fetch_text(url: str) -> str:
    request = Request(url, headers={"User-Agent": USER_AGENT})
    with urlopen(request, timeout=15) as response:
        return response.read().decode("utf-8", errors="replace")


def _is_nightly_asset(asset: dict) -> bool:
    name = str(asset.get("name") or "").lower()
    return "nightly" in name or "qml" in name


def _asset_score(asset: dict, prefer_qml: bool = False) -> tuple[int, str]:
    name = str(asset.get("name") or "").lower()
    if not name.endswith(".exe"):
        return (0, name)
    score = 1
    if "lumenkvn" in name:
        score += 2
    if "setup" in name:
        score += 4
    if "windows" in name:
        score += 2
    if "x64" in name or "win64" in name or "amd64" in name:
        score += 2
    if "portable" in name:
        score -= 4

    if _is_nightly_asset(asset):
        score -= 1
    return (score, name)


class UpdateChecker(QThread):
    """Check GitHub for a newer release."""

    result = pyqtSignal(object)  # AppUpdate | None
    error = pyqtSignal(str)

    def __init__(self, parent=None, channel: str = "stable", prefer_qml: bool = False):
        super().__init__(parent)
        self._channel = (channel or "stable").strip().lower()
        self._prefer_qml = bool(prefer_qml) or self._channel == "nightly"

    def run(self) -> None:
        try:
            target = self._pick_target()
            if target is None:
                self.result.emit(None)
                return

            tag = target.get("tag_name", "")
            # Предлагаем действие, когда установленная версия отличается от целевой:
            # вверх (обновление) или вниз (откат при смене канала).
            if tag.strip().lstrip("v") == APP_VERSION.strip().lstrip("v"):
                self.result.emit(None)
                return

            update = self._build_update(target)
            if update is not None:
                is_newer = _is_newer_version(tag, APP_VERSION)
                # На pre-release канале предлагаем только более новые версии; откат
                # имеет смысл лишь при переключении на Stable.
                if not is_newer and self._is_prerelease_channel():
                    self.result.emit(None)
                    return
                update.channel = self._channel
                update.is_downgrade = not is_newer
                self.result.emit(update)
        except Exception as exc:
            self.error.emit(str(exc))
            return

    def _is_prerelease_channel(self) -> bool:
        return self._channel in ("beta", "nightly", "prerelease", "pre-release", "pre")

    def _fetch_json(self, url: str):
        req = Request(url, headers={"User-Agent": USER_AGENT})
        with urlopen(req, timeout=15) as resp:
            return json.loads(resp.read())

    def _latest_stable(self) -> dict | None:
        # /releases/latest всегда отдаёт новейший НЕ-pre-release независимо от
        # количества промежуточных пререлизов (404 — если стабильных нет).
        try:
            data = self._fetch_json(GITHUB_API)
        except Exception:
            return None
        if isinstance(data, dict) and data.get("tag_name") and not data.get("draft"):
            return data
        return None

    def _latest_prerelease(self) -> dict | None:
        # GitHub may order tags such as pre.10 below pre.9 in /releases.
        # Pick the newest pre-release by semver instead of trusting API order.
        try:
            releases = self._fetch_json(RELEASES_API)
        except Exception:
            return None
        if not isinstance(releases, list):
            return None
        best: dict | None = None
        for release in releases:
            if not (isinstance(release, dict) and not release.get("draft") and release.get("prerelease")):
                continue
            if best is None or _is_newer_version(
                str(release.get("tag_name") or ""),
                str(best.get("tag_name") or ""),
            ):
                best = release
        return best

    def _pick_target(self) -> dict | None:
        """Выбрать релиз для текущего канала.

        stable      — /releases/latest (новейший стабильный);
        pre-release — новейший из последнего pre-release и последнего stable.
        """
        if not self._is_prerelease_channel():
            return self._latest_stable()
        stable = self._latest_stable()
        pre = self._latest_prerelease()
        if stable and pre:
            return stable if _is_newer_version(stable.get("tag_name", ""), pre.get("tag_name", "")) else pre
        return stable or pre

    def _build_update(self, data: dict):
        tag = data.get("tag_name", "")
        setup_assets = [
            a for a in data.get("assets", [])
            if str(a.get("name") or "").lower().endswith(".exe")
            and "setup" in str(a.get("name") or "").lower()
        ]
        asset = max(
            setup_assets,
            key=lambda a: _asset_score(a, self._prefer_qml),
            default=None,
        )
        if not asset:
            self.error.emit(
                f"Релиз {tag} найден, но отсутствует setup-установщик"
            )
            return None

        digest = _extract_digest(str(asset.get("digest") or ""))
        if not digest:
            asset_name = str(asset.get("name") or "")
            sidecar = None
            for suffix in (".sha256", ".dgst"):
                expected = f"{asset_name}{suffix}".lower()
                sidecar = next(
                    (
                        candidate for candidate in data.get("assets", [])
                        if str(candidate.get("name") or "").lower() == expected
                    ),
                    None,
                )
                if sidecar:
                    break
            if sidecar:
                digest = _extract_digest(
                    _fetch_text(str(sidecar.get("browser_download_url") or ""))
                )
        if not digest:
            self.error.emit(f"Релиз {tag} найден, но установщик не содержит SHA-256")
            return None

        return AppUpdate(
            version=tag.lstrip("v"),
            tag=tag,
            download_url=asset["browser_download_url"],
            size=asset.get("size", 0),
            notes=data.get("body", ""),
            digest_sha256=digest,
            asset_name=str(asset.get("name") or "LumenKVN-Setup-windows-x64.exe"),
        )


_log = logging.getLogger(__name__)

_DOWNLOAD_TIMEOUT = 30  # seconds — per socket operation (connect + each read)
_NUM_SEGMENTS = 4       # parallel download segments
_CHUNK_SIZE = 1024 * 1024  # 1 MB


class UpdateDownloader(QThread):
    """Download the setup installer, then launch the restart/install script."""

    progress = pyqtSignal(int)       # percent 0-100
    status = pyqtSignal(str)         # human-readable status message
    finished_ok = pyqtSignal()
    error = pyqtSignal(str)

    def __init__(
        self,
        update: AppUpdate,
        proxy_url: str | None = None,
        restart_in_tray: bool = False,
        parent=None,
    ):
        super().__init__(parent)
        self._update = update
        self._proxy_url = proxy_url
        self._restart_in_tray = restart_in_tray

    # ── download helpers ────────────────────────────────────────

    def _build_opener(self, proxy_url: str | None) -> urllib.request.OpenerDirector:
        if proxy_url:
            handler = urllib.request.ProxyHandler({"http": proxy_url, "https": proxy_url})
            return build_opener(handler)
        return build_opener()

    def _supports_range(self, url: str, opener: urllib.request.OpenerDirector) -> tuple[bool, int]:
        """HEAD request to check Range support and get Content-Length."""
        req = Request(url, method="HEAD", headers={"User-Agent": USER_AGENT})
        with opener.open(req, timeout=_DOWNLOAD_TIMEOUT) as resp:
            accepts = resp.headers.get("Accept-Ranges", "").lower()
            length = int(resp.headers.get("Content-Length", 0))
            return accepts == "bytes" and length > 0, length

    def _download_segment(
        self,
        url: str,
        proxy_url: str | None,
        start: int,
        end: int,
        seg_path: Path,
        seg_index: int,
        lock: threading.Lock,
        progress_arr: list[int],
        total: int,
    ) -> None:
        """Download one segment with Range header."""
        opener = self._build_opener(proxy_url)
        expected_length = end - start + 1
        req = Request(url, headers={
            "User-Agent": USER_AGENT,
            "Range": f"bytes={start}-{end}",
        })
        with opener.open(req, timeout=_DOWNLOAD_TIMEOUT) as resp:
            status_code = getattr(resp, "status", None)
            content_range = resp.headers.get("Content-Range", "")
            if status_code != 206 or not content_range.startswith(f"bytes {start}-{end}/"):
                raise RuntimeError("Сервер некорректно ответил на Range-запрос")

            downloaded = 0
            with open(seg_path, "wb") as f:
                while True:
                    chunk = resp.read(_CHUNK_SIZE)
                    if not chunk:
                        break
                    f.write(chunk)
                    downloaded += len(chunk)
                    with lock:
                        progress_arr[seg_index] += len(chunk)
                        done = sum(progress_arr)
                        self.progress.emit(int(done * 100 / total))
            if downloaded != expected_length:
                raise RuntimeError("Сервер вернул неполный фрагмент установщика")

    def _download_single(self, url: str, opener: urllib.request.OpenerDirector, target_path: Path) -> None:
        """Single-connection fallback download."""
        req = Request(url, headers={"User-Agent": USER_AGENT})
        with opener.open(req, timeout=_DOWNLOAD_TIMEOUT) as resp:
            total = int(resp.headers.get("Content-Length", 0))
            downloaded = 0
            with open(target_path, "wb") as f:
                while True:
                    chunk = resp.read(_CHUNK_SIZE)
                    if not chunk:
                        if downloaded == 0:
                            raise TimeoutError("Сервер не отдаёт данные")
                        break
                    f.write(chunk)
                    downloaded += len(chunk)
                    if total > 0:
                        self.progress.emit(int(downloaded * 100 / total))

    def _download(self, target_path: Path, proxy_url: str | None) -> None:
        """Download update installer with multi-segment acceleration.

        Tries parallel Range-based download first; falls back to single
        connection if the server doesn't support Range requests.
        """
        url = self._update.download_url
        opener = self._build_opener(proxy_url)

        # Check if server supports Range requests
        try:
            supports_range, total = self._supports_range(url, opener)
        except Exception:
            supports_range, total = False, 0

        if not supports_range or total == 0 or total < _NUM_SEGMENTS * _CHUNK_SIZE:
            _log.info("Server does not support Range or file too small — single download")
            self._download_single(url, opener, target_path)
            return

        # Split into segments
        seg_size = total // _NUM_SEGMENTS
        segments: list[tuple[int, int]] = []
        for i in range(_NUM_SEGMENTS):
            start = i * seg_size
            end = total - 1 if i == _NUM_SEGMENTS - 1 else (i + 1) * seg_size - 1
            segments.append((start, end))

        # Prepare temp segment files
        seg_dir = target_path.parent / "_segments"
        seg_dir.mkdir(exist_ok=True)
        seg_paths = [seg_dir / f"seg_{i}" for i in range(_NUM_SEGMENTS)]

        lock = threading.Lock()
        progress_arr = [0] * _NUM_SEGMENTS

        # Download segments in parallel
        try:
            with ThreadPoolExecutor(max_workers=_NUM_SEGMENTS) as pool:
                futures = []
                for i, (start, end) in enumerate(segments):
                    fut = pool.submit(
                        self._download_segment,
                        url, proxy_url, start, end,
                        seg_paths[i], i, lock, progress_arr, total,
                    )
                    futures.append(fut)

                # Re-raise any segment exception
                for fut in futures:
                    fut.result()

            # Concatenate segments into final file
            with open(target_path, "wb") as out:
                for sp in seg_paths:
                    with open(sp, "rb") as seg_f:
                        shutil.copyfileobj(seg_f, out)
        except Exception as exc:
            _log.warning("Segmented download failed, falling back to single download: %s", exc)
            if target_path.exists():
                target_path.unlink()
            self.progress.emit(0)
            self._download_single(url, opener, target_path)
        finally:
            # Clean up segment temp files
            shutil.rmtree(seg_dir, ignore_errors=True)

    # ── main thread entry ───────────────────────────────────────

    def run(self) -> None:
        tmp_dir: Path | None = None
        try:
            tmp_dir = Path(tempfile.mkdtemp(prefix="lumenkvn_update_"))
            setup_name = self._update.asset_name or "LumenKVN-Setup-windows-x64.exe"
            setup_path = tmp_dir / setup_name

            downloaded_ok = False

            # Attempt 1: through proxy (if available)
            if self._proxy_url:
                self.status.emit("Загрузка через прокси...")
                try:
                    self._download(setup_path, self._proxy_url)
                    downloaded_ok = True
                except Exception as exc:
                    _log.warning("Proxy download failed: %s", exc)
                    self.status.emit(
                        "Прокси-сервер недоступен, пробую напрямую..."
                    )
                    self.progress.emit(0)
                    # clean partial file
                    if setup_path.exists():
                        setup_path.unlink()

            # Attempt 2: direct (no proxy)
            if not downloaded_ok:
                self.status.emit("Загрузка напрямую...")
                try:
                    self._download(setup_path, None)
                    downloaded_ok = True
                except Exception as exc:
                    _log.warning("Direct download failed: %s", exc)

            if not downloaded_ok:
                msg = (
                    "Не удалось скачать обновление.\n"
                    "Переключитесь на рабочий сервер и попробуйте снова."
                )
                if self._proxy_url:
                    msg = (
                        "Не удалось скачать обновление ни через прокси, ни напрямую.\n"
                        "Переключитесь на рабочий сервер и попробуйте снова."
                    )
                self.error.emit(msg)
                # cleanup
                shutil.rmtree(tmp_dir, ignore_errors=True)
                return

            self.status.emit("Проверка установщика...")
            expected_hash = _extract_digest(self._update.digest_sha256)
            if not expected_hash:
                self.error.emit("У релизного установщика отсутствует SHA-256")
                shutil.rmtree(tmp_dir, ignore_errors=True)
                return

            real_hash = _sha256_file(setup_path)
            if real_hash.lower() != expected_hash.lower():
                self.error.emit("Контрольная сумма установщика не совпадает")
                shutil.rmtree(tmp_dir, ignore_errors=True)
                return

            self.progress.emit(100)
            self.status.emit("Подготовка установки...")

            exe_name = "LumenKVN.exe"

            # Write restart script
            current_pid = os.getpid()
            current_app_dir = BASE_DIR.resolve(strict=False)
            app_dir = _target_app_dir(current_app_dir)
            script = tmp_dir / "_update.ps1"
            script_text = "\r\n".join([
                "$ErrorActionPreference = 'Stop'",
                f"$pidToWait = {current_pid}",
                f"$setupPath = {_powershell_literal(str(setup_path))}",
                f"$currentAppDir = {_powershell_literal(str(current_app_dir))}",
                f"$appDir = {_powershell_literal(str(app_dir))}",
                f"$exePath = {_powershell_literal(str(app_dir / exe_name))}",
                "$fallbackExe = Join-Path $currentAppDir 'LumenKVN.exe'",
                f"$tempDir = {_powershell_literal(str(tmp_dir))}",
                f"$expectedVersion = {_powershell_literal(self._update.version)}",
                "$logDir = Join-Path (Join-Path $appDir 'data') 'logs'",
                "$runtimeDir = Join-Path (Join-Path $appDir 'data') 'runtime'",
                "$errorLog = Join-Path $logDir 'update_error.log'",
                "$setupLog = Join-Path $logDir 'setup_update.log'",
                "if ((Split-Path -Leaf $appDir) -ieq 'Program') { throw 'Install directory resolved to C:\\Program; aborting update.' }",
                "New-Item -ItemType Directory -Path $appDir -Force | Out-Null",
                "New-Item -ItemType Directory -Path $logDir -Force | Out-Null",
                "New-Item -ItemType Directory -Path $runtimeDir -Force | Out-Null",
                "$adminIdentity = [Security.Principal.WindowsIdentity]::GetCurrent()",
                "$adminPrincipal = New-Object Security.Principal.WindowsPrincipal($adminIdentity)",
                "if (-not $adminPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Lumen KVN updater must run as administrator to replace zapret drivers.' }",
                "for ($i = 0; $i -lt 120; $i++) {",
                "    if (-not (Get-Process -Id $pidToWait -ErrorAction SilentlyContinue)) { break }",
                "    Start-Sleep -Milliseconds 500",
                "}",
                "$proc = Get-Process -Id $pidToWait -ErrorAction SilentlyContinue",
                "if ($proc) { Stop-Process -Id $pidToWait -Force }",
                "Get-Process -Name 'LumenKVN' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue",
                "foreach ($coreName in @('xray','sing-box','singbox','winws','winws2','tun2socks','warp-svc')) { Get-Process -Name $coreName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue }",
                "foreach ($driverName in @('Monkey','WinDivert','WinDivert14','WinDivert64','WinDivert2')) {",
                "    & sc.exe stop $driverName *> $null",
                "    & sc.exe delete $driverName *> $null",
                "}",
                "$zapretExeDir = Join-Path (Join-Path $appDir 'zapret') 'exe'",
                "foreach ($driverFile in @('Monkey64.sys','WinDivert32.sys','WinDivert64.sys')) {",
                "    Remove-Item -LiteralPath (Join-Path $zapretExeDir $driverFile) -Force -ErrorAction SilentlyContinue",
                "}",
                "Start-Sleep -Milliseconds 1200",
                "try {",
                "    $installDirArg = '/DIR=\"' + $appDir + '\"'",
                "    $logArg = '/LOG=\"' + $setupLog + '\"'",
                "    $installerArgs = @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/CLOSEAPPLICATIONS','/FORCECLOSEAPPLICATIONS','/RESTARTEXITCODE=3010',$installDirArg,$logArg)",
                "    $install = $null",
                "    for ($attempt = 1; $attempt -le 3; $attempt++) {",
                "        $install = Start-Process -FilePath $setupPath -ArgumentList $installerArgs -Wait -PassThru -ErrorAction Stop",
                "        if ($install.ExitCode -eq 0 -or $install.ExitCode -eq 3010) { break }",
                "        Get-Process -Name 'LumenKVN' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue",
                "        foreach ($coreName in @('xray','sing-box','singbox','winws','winws2','tun2socks','warp-svc')) { Get-Process -Name $coreName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue }",
                "        foreach ($driverName in @('Monkey','WinDivert','WinDivert14','WinDivert64','WinDivert2')) {",
                "            & sc.exe stop $driverName *> $null",
                "            & sc.exe delete $driverName *> $null",
                "        }",
                "        foreach ($driverFile in @('Monkey64.sys','WinDivert32.sys','WinDivert64.sys')) {",
                "            Remove-Item -LiteralPath (Join-Path $zapretExeDir $driverFile) -Force -ErrorAction SilentlyContinue",
                "        }",
                "        Start-Sleep -Seconds 2",
                "    }",
                "    if ($install.ExitCode -ne 0 -and $install.ExitCode -ne 3010) {",
                "        $setupTail = ''",
                "        if (Test-Path -LiteralPath $setupLog) { $setupTail = (Get-Content -LiteralPath $setupLog -Tail 30 -ErrorAction SilentlyContinue) -join [Environment]::NewLine }",
                "        throw ('Installer exited with code ' + $install.ExitCode + [Environment]::NewLine + 'Setup log tail:' + [Environment]::NewLine + $setupTail) }",
                "    $uninstallKeys = @('HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}_is1','HKLM:\\SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}_is1','HKCU:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}_is1')",
                "    foreach ($uk in $uninstallKeys) { try { $loc = (Get-ItemProperty -LiteralPath $uk -ErrorAction Stop).InstallLocation; if ($loc -and (Test-Path -LiteralPath (Join-Path $loc 'LumenKVN.exe'))) { $appDir = $loc.TrimEnd('\\'); $exePath = Join-Path $appDir 'LumenKVN.exe'; break } } catch {} }",
                "    $currentDataDir = Join-Path $currentAppDir 'data'",
                "    if ($currentAppDir -ine $appDir -and (Test-Path -LiteralPath $currentDataDir)) {",
                "        Copy-Item -LiteralPath $currentDataDir -Destination $appDir -Recurse -Force",
                "    }",
                "    if (-not (Test-Path -LiteralPath $exePath)) { throw 'Updated LumenKVN.exe was not installed' }",
                "    $versionFile = Join-Path $runtimeDir 'update_version.txt'",
                "    Remove-Item -LiteralPath $versionFile -Force -ErrorAction SilentlyContinue",
                "    $versionProbe = Start-Process -FilePath $exePath -ArgumentList '--version-file',$versionFile -WorkingDirectory $appDir -PassThru -Wait -ErrorAction Stop",
                "    $versionLine = Get-Content -LiteralPath $versionFile -ErrorAction SilentlyContinue | Select-Object -First 1",
                "    if ($null -eq $versionLine -or [string]::IsNullOrWhiteSpace([string]$versionLine)) {",
                "        Write-Log 'Updated executable did not write version probe file; continuing because installer finished successfully'",
                "    } else {",
                "        $installedVersion = ([string]$versionLine).Trim().TrimStart('v')",
                "        if ($expectedVersion -and $installedVersion -and $installedVersion -ne $expectedVersion.TrimStart('v')) { throw ('Updated executable reports v' + $installedVersion + ', expected v' + $expectedVersion) }",
                "    }",
                "    $oldExePath = Join-Path $currentAppDir 'LumenKVN.exe'",
                "    $currentRoot = [System.IO.Path]::GetPathRoot($currentAppDir)",
                "    $currentParent = Split-Path -Parent $currentAppDir",
                "    $isRootProgramDir = ((Split-Path -Leaf $currentAppDir) -ieq 'Program' -and $currentParent.TrimEnd('\\') -ieq $currentRoot.TrimEnd('\\'))",
                "    if ($currentAppDir -ine $appDir -and $isRootProgramDir -and (Test-Path -LiteralPath $oldExePath)) {",
                "        Remove-Item -LiteralPath $currentAppDir -Recurse -Force -ErrorAction SilentlyContinue",
                "    }",
                "    $legacyRootProgram = 'C:\\Program'",
                "    $legacyRootExe = Join-Path $legacyRootProgram 'LumenKVN.exe'",
                "    if ($legacyRootProgram -ine $appDir -and (Test-Path -LiteralPath $legacyRootExe)) {",
                "        Remove-Item -LiteralPath $legacyRootProgram -Recurse -Force -ErrorAction SilentlyContinue",
                "    }",
                (
                    "    $started = Start-Process -FilePath $exePath -ArgumentList '--tray' -WorkingDirectory $appDir -PassThru -ErrorAction Stop"
                    if self._restart_in_tray
                    else "    $started = Start-Process -FilePath $exePath -WorkingDirectory $appDir -PassThru -ErrorAction Stop"
                ),
                "    Start-Sleep -Seconds 5",
                "    if ($started.HasExited) {",
                "        throw ('Updated application exited immediately with code ' + $started.ExitCode)",
                "    }",
                "}",
                "catch {",
                (
                    "    if (Test-Path -LiteralPath $exePath) { Start-Process -FilePath $exePath -ArgumentList '--tray','--relaunched' -WorkingDirectory $appDir -ErrorAction SilentlyContinue | Out-Null } elseif (Test-Path -LiteralPath $fallbackExe) { Start-Process -FilePath $fallbackExe -ArgumentList '--tray','--relaunched' -WorkingDirectory $currentAppDir -ErrorAction SilentlyContinue | Out-Null }"
                    if self._restart_in_tray
                    else "    if (Test-Path -LiteralPath $exePath) { Start-Process -FilePath $exePath -ArgumentList '--relaunched' -WorkingDirectory $appDir -ErrorAction SilentlyContinue | Out-Null } elseif (Test-Path -LiteralPath $fallbackExe) { Start-Process -FilePath $fallbackExe -ArgumentList '--relaunched' -WorkingDirectory $currentAppDir -ErrorAction SilentlyContinue | Out-Null }"
                ),
                "    New-Item -ItemType Directory -Path $logDir -Force | Out-Null",
                "    ($_ | Out-String) | Set-Content -LiteralPath $errorLog -Encoding UTF8",
                "    throw",
                "}",
                "Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue",
                "",
            ])
            _write_utf8_bom_text(script, script_text)

            # The installer requires elevation and owns Windows registration.
            if not _launch_update_script(script, elevated=True):
                self.error.emit("Не удалось запустить установку обновления с правами администратора")
                shutil.rmtree(tmp_dir, ignore_errors=True)
                return

            self.finished_ok.emit()

        except Exception as exc:
            if tmp_dir is not None:
                shutil.rmtree(tmp_dir, ignore_errors=True)
            self.error.emit(str(exc))
