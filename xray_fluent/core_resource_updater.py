from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import shutil
import tempfile
from urllib.request import Request
import zipfile

from PyQt6.QtCore import QThread, pyqtSignal

from .constants import APP_VERSION, SINGBOX_PATH_DEFAULT, XRAY_PATH_DEFAULT
from .engines.singbox import get_singbox_version
from .engines.xray.core_updater import _download_file, _extract_version, _is_newer, _request_json
from .http_utils import urlopen
from .path_utils import resolve_configured_path
from .zip_utils import safe_extract_zip

SINGBOX_EXTENDED_LATEST_API = "https://api.github.com/repos/shtorm-7/sing-box-extended/releases/latest"
RUNETFREEDOM_RULES_BASE_URL = "https://github.com/runetfreedom/russia-v2ray-rules-dat/releases/latest/download"
GEOIP_DAT_URL = f"{RUNETFREEDOM_RULES_BASE_URL}/geoip.dat"
GEOSITE_DAT_URL = f"{RUNETFREEDOM_RULES_BASE_URL}/geosite.dat"


@dataclass(slots=True)
class ResourceUpdateResult:
    kind: str
    status: str  # available | up_to_date | updated | error
    message: str
    current_version: str = ""
    latest_version: str = ""


def _core_dir() -> Path:
    return XRAY_PATH_DEFAULT.parent


def _download_direct(url: str, destination: Path, on_progress=None) -> None:
    request = Request(url, headers={"User-Agent": f"LumenKVN/{APP_VERSION}"})
    destination.parent.mkdir(parents=True, exist_ok=True)
    with urlopen(request, timeout=120) as response:
        total = int(response.headers.get("Content-Length", 0))
        downloaded = 0
        with open(destination, "wb") as file:
            while True:
                chunk = response.read(1024 * 1024)
                if not chunk:
                    break
                file.write(chunk)
                downloaded += len(chunk)
                if on_progress and total > 0:
                    on_progress(downloaded, total)


def _extract_singbox_version(text: str) -> str:
    value = _extract_version(text or "")
    match = re.search(r"(\d+\.\d+\.\d+(?:-extended-\d+\.\d+\.\d+)?)", text or "")
    return match.group(1) if match else value


def _pick_singbox_asset(release: dict) -> tuple[str, str]:
    assets = release.get("assets") or []
    for asset in assets:
        name = str(asset.get("name") or "")
        lower = name.lower()
        if lower.endswith(".zip") and "windows-amd64.zip" in lower and "purego" not in lower:
            return name, str(asset.get("browser_download_url") or "")
    for asset in assets:
        name = str(asset.get("name") or "")
        lower = name.lower()
        if lower.endswith(".zip") and "windows" in lower and ("amd64" in lower or "x64" in lower):
            return name, str(asset.get("browser_download_url") or "")
    return "", ""


def check_or_update_singbox(singbox_path: str, apply_update: bool, on_progress=None) -> ResourceUpdateResult:
    exe = resolve_configured_path(
        singbox_path,
        default_path=SINGBOX_PATH_DEFAULT,
        use_default_if_empty=True,
        migrate_default_location=True,
    ) or SINGBOX_PATH_DEFAULT
    current_text = get_singbox_version(str(exe)) or ""
    current = _extract_singbox_version(current_text)
    try:
        release = _request_json(SINGBOX_EXTENDED_LATEST_API)
        if not isinstance(release, dict):
            raise RuntimeError("GitHub вернул неожиданный ответ")
        asset_name, url = _pick_singbox_asset(release)
        latest = _extract_singbox_version(str(release.get("tag_name") or release.get("name") or ""))
        if not url:
            raise RuntimeError("не найден windows-amd64 архив sing-box extended")
    except Exception as exc:
        return ResourceUpdateResult("singbox", "error", f"Не удалось проверить sing-box: {exc}", current, "")

    if current and latest and not _is_newer(latest, current):
        return ResourceUpdateResult("singbox", "up_to_date", f"sing-box актуален ({current})", current, latest)
    if not apply_update:
        return ResourceUpdateResult("singbox", "available", f"Доступен sing-box extended {latest}", current, latest)

    try:
        with tempfile.TemporaryDirectory(prefix="singbox_update_") as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            archive = temp_dir / asset_name
            _download_file(url, archive, on_progress=on_progress)
            extract_dir = temp_dir / "extract"
            extract_dir.mkdir()
            with zipfile.ZipFile(archive, "r") as zip_file:
                safe_extract_zip(zip_file, extract_dir)
            new_exe = next((p for p in extract_dir.rglob("sing-box.exe") if p.is_file()), None)
            if new_exe is None:
                raise RuntimeError("sing-box.exe не найден в архиве")
            exe.parent.mkdir(parents=True, exist_ok=True)
            backup = exe.with_suffix(".exe.bak")
            if exe.exists():
                shutil.copy2(exe, backup)
            with tempfile.NamedTemporaryFile(prefix=".sing-box.", suffix=".exe", dir=exe.parent, delete=False) as tmp:
                staged = Path(tmp.name)
            try:
                shutil.copy2(new_exe, staged)
                staged.replace(exe)
            finally:
                if staged.exists():
                    staged.unlink(missing_ok=True)
    except Exception as exc:
        return ResourceUpdateResult("singbox", "error", f"Не удалось обновить sing-box: {exc}", current, latest)

    refreshed = _extract_singbox_version(get_singbox_version(str(exe)) or latest)
    return ResourceUpdateResult("singbox", "updated", f"sing-box обновлен до {refreshed}", current, refreshed)


def update_geodata(on_progress=None) -> ResourceUpdateResult:
    target_dir = _core_dir()
    target_dir.mkdir(parents=True, exist_ok=True)
    def _scaled_progress(offset: int, span: int):
        def _emit(done: int, total: int) -> None:
            if on_progress and total > 0:
                percent = offset + int(done * span / total)
                on_progress(max(0, min(percent, 100)), 100)
        return _emit

    try:
        with tempfile.TemporaryDirectory(prefix="geodata_update_") as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            geoip = temp_dir / "geoip.dat"
            geosite = temp_dir / "geosite.dat"
            _download_direct(GEOIP_DAT_URL, geoip, on_progress=_scaled_progress(0, 50))
            _download_direct(GEOSITE_DAT_URL, geosite, on_progress=_scaled_progress(50, 50))
            if geoip.stat().st_size < 1024 or geosite.stat().st_size < 1024:
                raise RuntimeError("скачанные geodata файлы выглядят поврежденными")
            for src, name in ((geoip, "geoip.dat"), (geosite, "geosite.dat")):
                dest = target_dir / name
                if dest.exists():
                    shutil.copy2(dest, dest.with_suffix(dest.suffix + ".bak"))
                src.replace(dest)
    except Exception as exc:
        return ResourceUpdateResult("geodata", "error", f"Не удалось обновить geoip/geosite: {exc}")
    return ResourceUpdateResult("geodata", "updated", "geoip.dat и geosite.dat обновлены")


def check_geodata_update() -> ResourceUpdateResult:
    """Лёгкая проверка обновления geoip/geosite по размеру файлов (без скачивания)."""
    target_dir = _core_dir()
    try:
        changed = False
        for url, name in ((GEOIP_DAT_URL, "geoip.dat"), (GEOSITE_DAT_URL, "geosite.dat")):
            request = Request(url, method="HEAD", headers={"User-Agent": f"LumenKVN/{APP_VERSION}"})
            with urlopen(request, timeout=20) as response:
                remote_size = int(response.headers.get("Content-Length", 0) or 0)
            dest = target_dir / name
            local_size = dest.stat().st_size if dest.exists() else -1
            if local_size < 0 or (remote_size > 0 and remote_size != local_size):
                changed = True
    except Exception as exc:
        return ResourceUpdateResult("geodata", "error", f"Не удалось проверить geoip/geosite: {exc}")
    if changed:
        return ResourceUpdateResult("geodata", "available", "Доступно обновление geoip/geosite")
    return ResourceUpdateResult("geodata", "up_to_date", "geoip/geosite актуальны")


class StartupResourceCheckWorker(QThread):
    """Фоновая проверка обновлений ядра sing-box и geoip/geosite при запуске."""

    done = pyqtSignal(object)  # list[ResourceUpdateResult]

    def __init__(self, *, singbox_path: str = "") -> None:
        super().__init__()
        self._singbox_path = singbox_path

    def run(self) -> None:
        results: list[ResourceUpdateResult] = []
        try:
            results.append(check_or_update_singbox(self._singbox_path, apply_update=False))
        except Exception as exc:
            results.append(ResourceUpdateResult("singbox", "error", str(exc)))
        try:
            results.append(check_geodata_update())
        except Exception as exc:
            results.append(ResourceUpdateResult("geodata", "error", str(exc)))
        self.done.emit(results)


class ResourceUpdateWorker(QThread):
    done = pyqtSignal(object)
    progress = pyqtSignal(int)

    def __init__(self, kind: str, *, singbox_path: str = "", apply_update: bool = True) -> None:
        super().__init__()
        self._kind = kind
        self._singbox_path = singbox_path
        self._apply_update = apply_update

    def run(self) -> None:
        if self._kind == "singbox":
            result = check_or_update_singbox(
                self._singbox_path,
                self._apply_update,
                on_progress=lambda done, total: self.progress.emit(int(done * 100 / total)),
            )
        elif self._kind == "geodata":
            result = update_geodata(
                on_progress=lambda done, total: self.progress.emit(int(done * 100 / total)),
            )
        else:
            result = ResourceUpdateResult(self._kind, "error", f"Неизвестный тип обновления: {self._kind}")
        self.done.emit(result)
