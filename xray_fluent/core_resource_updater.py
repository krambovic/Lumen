from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import json
import re
import shutil
import subprocess
import tempfile
import threading
import time
import uuid
from urllib.request import Request
import zipfile

from PyQt6.QtCore import QThread, pyqtSignal

from .constants import APP_VERSION, SINGBOX_PATH_DEFAULT, XRAY_PATH_DEFAULT
from .component_compatibility import ensure_component_compatible, required_geodata_codes
from .discord_proxy_manager import (
    DROUTE_BUNDLED_DIR,
    DROUTE_DIR,
    DROUTE_EXE,
    DROUTE_LEGACY_VERSION,
    DROUTE_VERSION_FILE,
    get_bundled_droute_version,
    get_droute_bundle_version,
    install_bundled_droute,
)
from .engines.singbox import get_singbox_version
from .geodata_resources import (
    REGIONAL_SINGBOX_RULE_SETS,
    SINGBOX_BINARY_RULE_SETS,
    SINGBOX_RULE_SET_DIR,
)
from .engines.xray.core_updater import (
    UpdateCancelled,
    _download_file,
    _extract_digest,
    _extract_version,
    _fetch_dgst_hash,
    _is_newer,
    _raise_if_cancelled,
    _request_json,
    _sha256_file,
)
from .http_utils import abort_http_response, urlopen_proxy_first
from .path_utils import resolve_configured_path
from .subprocess_utils import CREATE_NO_WINDOW, result_output_text
from .zip_utils import safe_extract_zip

SINGBOX_EXTENDED_LATEST_API = "https://api.github.com/repos/shtorm-7/sing-box-extended/releases/latest"
RUNETFREEDOM_LATEST_API = "https://api.github.com/repos/runetfreedom/russia-v2ray-rules-dat/releases/latest"
LOYALSOLDIER_LATEST_API = "https://api.github.com/repos/Loyalsoldier/v2ray-rules-dat/releases/latest"
IRAN_RULES_LATEST_API = "https://api.github.com/repos/Chocolate4U/Iran-v2ray-rules/releases/latest"
GEODATA_METADATA_NAME = "lumen-geodata.json"
_MAX_RESOURCE_DOWNLOAD_BYTES = 512 * 1024 * 1024
_MAX_RULE_SET_MEMBER_BYTES = 64 * 1024 * 1024
_MAX_RULE_SET_TOTAL_BYTES = 256 * 1024 * 1024


@dataclass(slots=True)
class ResourceUpdateResult:
    kind: str
    status: str  # available | up_to_date | updated | error
    message: str
    current_version: str = ""
    latest_version: str = ""


def _core_dir() -> Path:
    return XRAY_PATH_DEFAULT.parent


def _download_direct(
    url: str,
    destination: Path,
    on_progress=None,
    *,
    proxy_url: str | None = None,
    cancelled=None,
    response_opened=None,
    response_closed=None,
) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    attempts = (proxy_url, None) if proxy_url else (None,)
    for index, active_proxy in enumerate(attempts):
        _raise_if_cancelled(cancelled)
        request = Request(url, headers={"User-Agent": f"LumenKVN/{APP_VERSION}"})
        try:
            with urlopen_proxy_first(request, timeout=120, proxy_url=active_proxy) as response:
                if response_opened is not None:
                    response_opened(response)
                try:
                    total = int(response.headers.get("Content-Length", 0))
                    if total > _MAX_RESOURCE_DOWNLOAD_BYTES:
                        raise RuntimeError(f"ресурс слишком большой: {total} байт")
                    downloaded = 0
                    with open(destination, "wb") as file:
                        while True:
                            _raise_if_cancelled(cancelled)
                            chunk = response.read(1024 * 1024)
                            if not chunk:
                                break
                            file.write(chunk)
                            downloaded += len(chunk)
                            if downloaded > _MAX_RESOURCE_DOWNLOAD_BYTES:
                                raise RuntimeError(
                                    f"ресурс превышает лимит {_MAX_RESOURCE_DOWNLOAD_BYTES} байт"
                                )
                            if on_progress and total > 0:
                                on_progress(downloaded, total)
                finally:
                    if response_closed is not None:
                        response_closed(response)
            _raise_if_cancelled(cancelled)
            return
        except UpdateCancelled:
            raise
        except Exception:
            if index == len(attempts) - 1:
                raise
            destination.unlink(missing_ok=True)


def _extract_singbox_version(text: str) -> str:
    value = _extract_version(text or "")
    match = re.search(r"(\d+\.\d+\.\d+(?:-extended-\d+\.\d+\.\d+)?)", text or "")
    return match.group(1) if match else value


def _is_lumen_singbox_build(text: str) -> bool:
    return bool(re.search(r"-lumen(?:\.|\b)", text or "", flags=re.IGNORECASE))


def _release_asset_digest(
    release: dict,
    asset_name: str,
    *,
    proxy_url: str | None = None,
    cancelled=None,
) -> str:
    assets = [asset for asset in (release.get("assets") or []) if isinstance(asset, dict)]
    selected = next(
        (asset for asset in assets if str(asset.get("name") or "").lower() == asset_name.lower()),
        None,
    )
    digest = _extract_digest(str((selected or {}).get("digest") or ""))
    if digest:
        return digest
    for suffix in (".sha256", ".sha256sum", ".dgst"):
        sidecar = next(
            (
                asset
                for asset in assets
                if str(asset.get("name") or "").lower() == f"{asset_name}{suffix}".lower()
            ),
            None,
        )
        if sidecar:
            return _fetch_dgst_hash(
                str(sidecar.get("browser_download_url") or ""),
                proxy_url=proxy_url,
                cancelled=cancelled,
            )
    return ""


def _resolve_geodata_release(
    *,
    region: str = "russia",
    proxy_url: str | None = None,
    cancelled=None,
) -> tuple[str, dict[str, tuple[str, str]]]:
    region = str(region or "russia").strip().lower()
    source_name, release_api, required_assets = {
        "russia": ("RuNetFreedom", RUNETFREEDOM_LATEST_API, ("geoip.dat", "geosite.dat", "sing-box.zip")),
        "china": ("Loyalsoldier", LOYALSOLDIER_LATEST_API, ("geoip.dat", "geosite.dat")),
        "iran": ("Chocolate4U", IRAN_RULES_LATEST_API, ("geoip.dat", "geosite.dat")),
    }.get(region, ("RuNetFreedom", RUNETFREEDOM_LATEST_API, ("geoip.dat", "geosite.dat", "sing-box.zip")))
    release = _request_json(
        release_api,
        proxy_url=proxy_url,
        cancelled=cancelled,
    )
    if not isinstance(release, dict):
        raise RuntimeError(f"invalid {source_name} release response")
    version = str(release.get("tag_name") or release.get("name") or "").strip()
    assets: dict[str, tuple[str, str]] = {}
    for name in required_assets:
        asset = next(
            (
                item
                for item in (release.get("assets") or [])
                if isinstance(item, dict)
                and str(item.get("name") or "").lower() == name.lower()
            ),
            None,
        )
        if asset is None:
            raise RuntimeError(f"{source_name} release does not contain {name}")
        url = str(asset.get("browser_download_url") or "").strip()
        digest = _release_asset_digest(
            release,
            name,
            proxy_url=proxy_url,
            cancelled=cancelled,
        )
        if not url or not digest:
            raise RuntimeError(f"{source_name} release does not provide a published SHA-256 for {name}")
        assets[name] = (url, digest)
    return version, assets


def _read_geodata_metadata(target_dir: Path) -> dict:
    path = target_dir / GEODATA_METADATA_NAME
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError, TypeError):
        return {}
    return payload if isinstance(payload, dict) else {}


def regional_geodata_installed(
    region: str,
    *,
    target_dir: Path | None = None,
    rule_set_dir: Path | None = None,
) -> bool:
    """Return whether the locally active geodata fully supports *region*.

    Xray uses one active ``geoip.dat``/``geosite.dat`` pair, so merely having
    another region's sing-box ``.srs`` files is not enough.  The metadata must
    also identify the requested region.  This check intentionally avoids
    network access and hashing large files on the GUI thread.
    """
    normalized = str(region or "russia").strip().lower()
    if normalized not in REGIONAL_SINGBOX_RULE_SETS:
        normalized = "russia"
    target_dir = Path(target_dir) if target_dir is not None else _core_dir()
    rule_set_dir = Path(rule_set_dir) if rule_set_dir is not None else SINGBOX_RULE_SET_DIR
    metadata = _read_geodata_metadata(target_dir)
    installed_region = str(metadata.get("region") or "russia").strip().lower()
    if installed_region != normalized:
        return False
    for name in ("geoip.dat", "geosite.dat"):
        path = target_dir / name
        try:
            if not path.is_file() or path.stat().st_size < 1024:
                return False
        except OSError:
            return False
    for key, source in REGIONAL_SINGBOX_RULE_SETS[normalized].items():
        filename = (
            Path(source.removeprefix("archive:")).name
            if source.startswith("archive:")
            else f"{key.replace(':', '-')}.srs"
        )
        path = rule_set_dir / filename
        try:
            if not path.is_file() or path.stat().st_size < 64:
                return False
        except OSError:
            return False
    return True


def _validate_geodata_with_xray(geoip: Path, geosite: Path, *, region: str = "russia") -> None:
    exe = resolve_configured_path(
        "",
        default_path=XRAY_PATH_DEFAULT,
        use_default_if_empty=True,
        migrate_default_location=True,
    )
    if exe is None or not exe.is_file():
        return
    token = uuid.uuid4().hex
    staged_geoip = exe.parent / f"lumen-geoip-check-{token}.dat"
    staged_geosite = exe.parent / f"lumen-geosite-check-{token}.dat"
    config_path = exe.parent / f"lumen-geodata-check-{token}.json"
    try:
        if region == "china":
            geosite_codes, geoip_codes = ["cn", "gfw", "greatfire", "google"], ["cn", "google", "telegram"]
        elif region == "iran":
            geosite_codes, geoip_codes = ["ir"], ["ir"]
        else:
            geosite_codes, geoip_codes = ["ru-available-only-inside", "ru-blocked"], ["ru", "ru-blocked", "ru-blocked-community"]
        shutil.copy2(geoip, staged_geoip)
        shutil.copy2(geosite, staged_geosite)
        config = {
            "log": {"loglevel": "none"},
            "outbounds": [{"protocol": "freedom", "tag": "direct"}],
            "routing": {
                "rules": [
                    {
                        "type": "field",
                        "domain": [f"ext:{staged_geosite.name}:{code}" for code in geosite_codes],
                        "outboundTag": "direct",
                    },
                    {
                        "type": "field",
                        "ip": [f"ext:{staged_geoip.name}:{code}" for code in geoip_codes],
                        "outboundTag": "direct",
                    },
                ]
            },
        }
        config_path.write_text(json.dumps(config, ensure_ascii=True), encoding="utf-8")
        result = subprocess.run(
            [str(exe), "run", "-test", "-c", str(config_path)],
            timeout=20,
            cwd=str(exe.parent),
            creationflags=CREATE_NO_WINDOW,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        if result.returncode != 0:
            detail = result_output_text(result).strip()
            raise RuntimeError(f"Xray rejected downloaded geodata: {detail}")
    finally:
        staged_geoip.unlink(missing_ok=True)
        staged_geosite.unlink(missing_ok=True)
        config_path.unlink(missing_ok=True)


def _atomic_replace_files(
    replacements: list[tuple[Path, Path]],
    *,
    backup_targets: set[Path] | None = None,
    validator=None,
) -> None:
    """Stage and replace a group of files, rolling every target back on failure."""
    backup_targets = set(backup_targets or ())
    staged: list[tuple[Path, Path]] = []
    rollback: dict[Path, Path | None] = {}
    replaced: list[Path] = []

    try:
        for source, destination in replacements:
            destination.parent.mkdir(parents=True, exist_ok=True)
            with tempfile.NamedTemporaryFile(
                prefix=f".{destination.name}.",
                suffix=".new",
                dir=destination.parent,
                delete=False,
            ) as temp_file:
                staged_path = Path(temp_file.name)
            shutil.copy2(source, staged_path)
            staged.append((staged_path, destination))

            rollback_path: Path | None = None
            if destination.exists():
                with tempfile.NamedTemporaryFile(
                    prefix=f".{destination.name}.",
                    suffix=".rollback",
                    dir=destination.parent,
                    delete=False,
                ) as rollback_file:
                    rollback_path = Path(rollback_file.name)
                rollback[destination] = rollback_path
                shutil.copy2(destination, rollback_path)
            else:
                rollback[destination] = None

        for staged_path, destination in staged:
            staged_path.replace(destination)
            replaced.append(destination)

        if validator is not None:
            validator()

        for destination in backup_targets:
            rollback_path = rollback.get(destination)
            if rollback_path is not None:
                shutil.copy2(
                    rollback_path,
                    destination.with_suffix(destination.suffix + ".bak"),
                )
    except Exception:
        for destination in reversed(replaced):
            rollback_path = rollback.get(destination)
            restore_path: Path | None = None
            try:
                if rollback_path is None:
                    destination.unlink(missing_ok=True)
                    continue
                with tempfile.NamedTemporaryFile(
                    prefix=f".{destination.name}.",
                    suffix=".restore",
                    dir=destination.parent,
                    delete=False,
                ) as restore_file:
                    restore_path = Path(restore_file.name)
                shutil.copy2(rollback_path, restore_path)
                restore_path.replace(destination)
            except Exception:
                pass
            finally:
                if restore_path is not None:
                    restore_path.unlink(missing_ok=True)
        raise
    finally:
        for staged_path, _destination in staged:
            staged_path.unlink(missing_ok=True)
        for rollback_path in rollback.values():
            if rollback_path is not None:
                rollback_path.unlink(missing_ok=True)


def _extract_singbox_rule_sets(
    archive: Path,
    staging_dir: Path,
    target_dir: Path,
) -> list[tuple[Path, Path]]:
    staging_dir.mkdir(parents=True, exist_ok=True)
    replacements: list[tuple[Path, Path]] = []
    with zipfile.ZipFile(archive, "r") as zip_file:
        names = set(zip_file.namelist())
        missing = sorted(set(SINGBOX_BINARY_RULE_SETS.values()) - names)
        if missing:
            raise RuntimeError(f"в архиве sing-box отсутствуют rule-set: {', '.join(missing)}")
        total_size = 0
        for member in SINGBOX_BINARY_RULE_SETS.values():
            info = zip_file.getinfo(member)
            if info.file_size > _MAX_RULE_SET_MEMBER_BYTES:
                raise RuntimeError(f"rule-set {member} слишком большой")
            total_size += info.file_size
            if total_size > _MAX_RULE_SET_TOTAL_BYTES:
                raise RuntimeError("архив rule-set превышает допустимый размер")
            payload = zip_file.read(member)
            if len(payload) < 64:
                raise RuntimeError(f"rule-set {member} выглядит повреждённым")
            source = staging_dir / Path(member).name
            source.write_bytes(payload)
            replacements.append((source, target_dir / source.name))
    return replacements


def _install_singbox_rule_sets(archive: Path, target_dir: Path = SINGBOX_RULE_SET_DIR) -> None:
    with tempfile.TemporaryDirectory(prefix="singbox_rules_stage_") as temp_dir:
        replacements = _extract_singbox_rule_sets(archive, Path(temp_dir), target_dir)
        _atomic_replace_files(replacements)


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


def _singbox_asset_digest(
    release: dict,
    asset_name: str,
    *,
    proxy_url: str | None = None,
    cancelled=None,
) -> str:
    assets = [asset for asset in (release.get("assets") or []) if isinstance(asset, dict)]
    selected = next(
        (asset for asset in assets if str(asset.get("name") or "").lower() == asset_name.lower()),
        None,
    )
    digest = _extract_digest(str((selected or {}).get("digest") or ""))
    if digest:
        return digest
    for suffix in (".sha256", ".dgst"):
        sidecar_name = f"{asset_name}{suffix}".lower()
        sidecar = next(
            (asset for asset in assets if str(asset.get("name") or "").lower() == sidecar_name),
            None,
        )
        if sidecar:
            return _fetch_dgst_hash(
                str(sidecar.get("browser_download_url") or ""),
                proxy_url=proxy_url,
                cancelled=cancelled,
            )
    return ""


def _ensure_zip_file(path: Path, label: str) -> None:
    if not zipfile.is_zipfile(path):
        size = path.stat().st_size if path.exists() else 0
        raise RuntimeError(f"{label}: скачанный файл не является zip-архивом ({size} байт)")


def check_or_update_droute(
    apply_update: bool,
    *,
    on_progress=None,
    proxy_url: str | None = None,
    cancelled=None,
    response_opened=None,
    response_closed=None,
    bundle_dir: Path | None = None,
    bundled_dir: Path | None = None,
) -> ResourceUpdateResult:
    del proxy_url, response_opened, response_closed
    _raise_if_cancelled(cancelled)
    target_dir = bundle_dir or DROUTE_DIR
    source_dir = bundled_dir or DROUTE_BUNDLED_DIR
    target_exe = target_dir / DROUTE_EXE.name
    version_file = target_dir / DROUTE_VERSION_FILE.name
    if bundle_dir is None:
        current = get_droute_bundle_version()
    elif target_exe.is_file():
        try:
            current = version_file.read_text(encoding="utf-8").strip().lstrip("v")
        except OSError:
            current = DROUTE_LEGACY_VERSION
    else:
        current = ""

    latest = get_bundled_droute_version(source_dir)
    if not latest:
        return ResourceUpdateResult(
            "droute",
            "error",
            "Не удалось проверить droute: встроенный пакет не найден. Переустановите Lumen KVN.",
            current,
            "",
        )

    if current and not _is_newer(latest, current):
        return ResourceUpdateResult("droute", "up_to_date", f"droute актуален ({current}, встроенный)", current, latest)
    if not apply_update:
        return ResourceUpdateResult("droute", "available", f"Доступен droute {latest} (встроенный)", current, latest)

    try:
        _raise_if_cancelled(cancelled)
        install_bundled_droute(target_dir, bundle_dir=source_dir)
        if on_progress:
            on_progress(1, 1)
    except UpdateCancelled:
        raise
    except Exception as exc:
        return ResourceUpdateResult("droute", "error", f"Не удалось обновить droute: {exc}", current, latest)
    return ResourceUpdateResult("droute", "updated", f"droute обновлен до {latest} (из встроенного пакета)", current, latest)


def check_or_update_singbox(
    singbox_path: str,
    apply_update: bool,
    on_progress=None,
    *,
    proxy_url: str | None = None,
    on_install_start=None,
    cancelled=None,
    response_opened=None,
    response_closed=None,
) -> ResourceUpdateResult:
    _raise_if_cancelled(cancelled)
    exe = resolve_configured_path(
        singbox_path,
        default_path=SINGBOX_PATH_DEFAULT,
        use_default_if_empty=True,
        migrate_default_location=True,
    ) or SINGBOX_PATH_DEFAULT
    current_text = get_singbox_version(str(exe)) or ""
    current = _extract_singbox_version(current_text)
    try:
        release = _request_json(
            SINGBOX_EXTENDED_LATEST_API,
            proxy_url=proxy_url,
            cancelled=cancelled,
        )
        if not isinstance(release, dict):
            raise RuntimeError("GitHub вернул неожиданный ответ")
        asset_name, url = _pick_singbox_asset(release)
        latest = _extract_singbox_version(str(release.get("tag_name") or release.get("name") or ""))
        if not url:
            raise RuntimeError("не найден windows-amd64 архив sing-box extended")
    except UpdateCancelled:
        raise
    except Exception as exc:
        return ResourceUpdateResult("singbox", "error", f"Не удалось проверить sing-box: {exc}", current, "")

    if current and latest and not _is_newer(latest, current):
        return ResourceUpdateResult("singbox", "up_to_date", f"sing-box актуален ({current})", current, latest)
    if _is_lumen_singbox_build(current_text):
        return ResourceUpdateResult(
            "singbox",
            "available",
            (
                f"Доступен sing-box extended {latest}. Совместимое ядро "
                "устанавливается вместе с обновлением Lumen"
            ),
            current,
            latest,
        )
    if not apply_update:
        return ResourceUpdateResult("singbox", "available", f"Доступен sing-box extended {latest}", current, latest)

    try:
        expected_hash = _singbox_asset_digest(
            release,
            asset_name,
            proxy_url=proxy_url,
            cancelled=cancelled,
        )
        if not expected_hash:
            raise RuntimeError("для архива sing-box отсутствует SHA-256")
    except UpdateCancelled:
        raise
    except Exception as exc:
        return ResourceUpdateResult(
            "singbox",
            "error",
            f"Не удалось проверить архив sing-box: {exc}",
            current,
            latest,
        )

    try:
        with tempfile.TemporaryDirectory(prefix="singbox_update_") as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            archive = temp_dir / asset_name
            _download_file(
                url,
                archive,
                on_progress=on_progress,
                proxy_url=proxy_url,
                cancelled=cancelled,
                response_opened=response_opened,
                response_closed=response_closed,
            )
            _ensure_zip_file(archive, "sing-box")
            if _sha256_file(archive).lower() != expected_hash.lower():
                raise RuntimeError("контрольная сумма архива sing-box не совпадает")
            extract_dir = temp_dir / "extract"
            extract_dir.mkdir()
            with zipfile.ZipFile(archive, "r") as zip_file:
                safe_extract_zip(zip_file, extract_dir)
            new_exe = next((p for p in extract_dir.rglob("sing-box.exe") if p.is_file()), None)
            if new_exe is None:
                raise RuntimeError("sing-box.exe не найден в архиве")
            candidate_version = _extract_singbox_version(get_singbox_version(str(new_exe)) or "")
            ensure_component_compatible("singbox", candidate_version)
            latest_core = re.search(r"\d+\.\d+\.\d+", latest or "")
            candidate_core = re.search(r"\d+\.\d+\.\d+", candidate_version)
            if latest_core and candidate_core and latest_core.group(0) != candidate_core.group(0):
                raise RuntimeError(
                    f"архив содержит sing-box {candidate_version}, ожидалась версия {latest}"
                )
            exe.parent.mkdir(parents=True, exist_ok=True)
            if on_install_start:
                try:
                    on_install_start()
                except UpdateCancelled:
                    raise
                except Exception as exc:
                    return ResourceUpdateResult("singbox", "error", f"Не удалось остановить службы перед обновлением: {exc}", current, latest)
            refreshed_holder: dict[str, str] = {}

            def _validate_installed() -> None:
                refreshed = _extract_singbox_version(get_singbox_version(str(exe)) or "")
                if not refreshed:
                    raise RuntimeError("установленный sing-box не запускается")
                ensure_component_compatible("singbox", refreshed)
                latest_core = re.search(r"\d+\.\d+\.\d+", latest or "")
                refreshed_core = re.search(r"\d+\.\d+\.\d+", refreshed)
                if latest_core and refreshed_core and latest_core.group(0) != refreshed_core.group(0):
                    raise RuntimeError(
                        f"установлен sing-box {refreshed}, ожидалась версия {latest}"
                    )
                refreshed_holder["version"] = refreshed

            _atomic_replace_files(
                [(new_exe, exe)],
                backup_targets={exe},
                validator=_validate_installed,
            )
            refreshed = refreshed_holder["version"]
    except UpdateCancelled:
        raise
    except Exception as exc:
        return ResourceUpdateResult("singbox", "error", f"Не удалось обновить sing-box: {exc}", current, latest)

    return ResourceUpdateResult("singbox", "updated", f"sing-box обновлен до {refreshed}", current, refreshed)


def update_geodata(
    on_progress=None,
    *,
    region: str = "russia",
    proxy_url: str | None = None,
    on_install_start=None,
    cancelled=None,
    response_opened=None,
    response_closed=None,
) -> ResourceUpdateResult:
    _raise_if_cancelled(cancelled)
    region = str(region or "russia").strip().lower()
    if region not in REGIONAL_SINGBOX_RULE_SETS:
        region = "russia"
    target_dir = _core_dir()
    target_dir.mkdir(parents=True, exist_ok=True)
    current_version = str(_read_geodata_metadata(target_dir).get("version") or "")
    latest_version = ""
    def _scaled_progress(offset: int, span: int):
        def _emit(done: int, total: int) -> None:
            if on_progress and total > 0:
                percent = offset + int(done * span / total)
                on_progress(max(0, min(percent, 100)), 100)
        return _emit

    try:
        latest_version, release_assets = _resolve_geodata_release(
            region=region,
            proxy_url=proxy_url,
            cancelled=cancelled,
        )
        if _geodata_install_matches(target_dir, latest_version, release_assets, region=region):
            return ResourceUpdateResult(
                "geodata",
                "up_to_date",
                "geoip/geosite актуальны",
                current_version,
                latest_version,
            )
        with tempfile.TemporaryDirectory(prefix="geodata_update_") as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            geoip = temp_dir / "geoip.dat"
            geosite = temp_dir / "geosite.dat"
            _download_direct(
                release_assets["geoip.dat"][0],
                geoip,
                on_progress=_scaled_progress(0, 25),
                proxy_url=proxy_url,
                cancelled=cancelled,
                response_opened=response_opened,
                response_closed=response_closed,
            )
            _download_direct(
                release_assets["geosite.dat"][0],
                geosite,
                on_progress=_scaled_progress(25, 25),
                proxy_url=proxy_url,
                cancelled=cancelled,
                response_opened=response_opened,
                response_closed=response_closed,
            )
            for name, path in (("geoip.dat", geoip), ("geosite.dat", geosite)):
                if _sha256_file(path).lower() != release_assets[name][1].lower():
                    raise RuntimeError(f"SHA-256 mismatch for {name}")
            if geoip.stat().st_size < 1024 or geosite.stat().st_size < 1024:
                raise RuntimeError("скачанные geodata файлы выглядят поврежденными")
            rule_replacements: list[tuple[Path, Path]] = []
            if region == "russia":
                singbox_rules = temp_dir / "sing-box.zip"
                _download_direct(
                    release_assets["sing-box.zip"][0], singbox_rules,
                    on_progress=_scaled_progress(50, 50), proxy_url=proxy_url,
                    cancelled=cancelled, response_opened=response_opened,
                    response_closed=response_closed,
                )
                if _sha256_file(singbox_rules).lower() != release_assets["sing-box.zip"][1].lower():
                    raise RuntimeError("SHA-256 mismatch for sing-box.zip")
                _ensure_zip_file(singbox_rules, "sing-box rules")
                rule_replacements = _extract_singbox_rule_sets(
                    singbox_rules, temp_dir / "rule-sets", SINGBOX_RULE_SET_DIR,
                )
            else:
                sources = list(REGIONAL_SINGBOX_RULE_SETS[region].items())
                staging = temp_dir / "rule-sets"
                staging.mkdir(parents=True, exist_ok=True)
                for index, (key, url) in enumerate(sources):
                    filename = f"{key.replace(':', '-')}.srs"
                    source = staging / filename
                    offset = 50 + int(index * 50 / max(1, len(sources)))
                    span = max(1, int(50 / max(1, len(sources))))
                    _download_direct(
                        url, source, on_progress=_scaled_progress(offset, span),
                        proxy_url=proxy_url, cancelled=cancelled,
                        response_opened=response_opened, response_closed=response_closed,
                    )
                    if source.stat().st_size < 64:
                        raise RuntimeError(f"rule-set {key} выглядит повреждённым")
                    rule_replacements.append((source, SINGBOX_RULE_SET_DIR / filename))
            replacements = [
                (geoip, target_dir / "geoip.dat"),
                (geosite, target_dir / "geosite.dat"),
                *rule_replacements,
            ]
            _validate_geodata_with_xray(geoip, geosite, region=region)
            metadata_path = temp_dir / GEODATA_METADATA_NAME
            metadata_path.write_text(
                json.dumps(
                    {
                        "schema": 2,
                        "region": region,
                        "version": latest_version,
                        "rule_sets": REGIONAL_SINGBOX_RULE_SETS[region],
                        "sha256": {
                            name: digest
                            for name, (_url, digest) in release_assets.items()
                        },
                    },
                    ensure_ascii=True,
                    indent=2,
                ),
                encoding="utf-8",
            )
            replacements.append((metadata_path, target_dir / GEODATA_METADATA_NAME))
            if on_install_start:
                try:
                    on_install_start()
                except UpdateCancelled:
                    raise
                except Exception as exc:
                    raise RuntimeError(f"Не удалось остановить службы перед обновлением geoip/geosite: {exc}")
            _raise_if_cancelled(cancelled)
            _atomic_replace_files(
                replacements,
                backup_targets={target_dir / "geoip.dat", target_dir / "geosite.dat"},
            )
    except UpdateCancelled:
        raise
    except Exception as exc:
        return ResourceUpdateResult(
            "geodata",
            "error",
            f"Не удалось обновить geoip/geosite: {exc}",
            current_version,
            latest_version,
        )
    return ResourceUpdateResult(
        "geodata",
        "updated",
        f"geoip.dat и geosite.dat обновлены до {latest_version}",
        current_version,
        latest_version,
    )


def check_geodata_update(
    *,
    region: str = "russia",
    proxy_url: str | None = None,
    cancelled=None,
    response_opened=None,
    response_closed=None,
) -> ResourceUpdateResult:
    """Check the verified release identity without downloading payloads."""
    target_dir = _core_dir()
    return _check_geodata_release_identity(
        target_dir,
        region=region,
        proxy_url=proxy_url,
        cancelled=cancelled,
    )


def _check_geodata_release_identity(
    target_dir: Path,
    *,
    region: str = "russia",
    proxy_url: str | None = None,
    cancelled=None,
) -> ResourceUpdateResult:
    try:
        latest_version, release_assets = _resolve_geodata_release(
            region=region,
            proxy_url=proxy_url,
            cancelled=cancelled,
        )
        metadata = _read_geodata_metadata(target_dir)
        current_version = str(metadata.get("version") or "")
        changed = not _geodata_install_matches(
            target_dir,
            latest_version,
            release_assets,
            region=region,
        )
    except UpdateCancelled:
        raise
    except Exception as exc:
        return ResourceUpdateResult(
            "geodata",
            "error",
            f"Не удалось проверить geoip/geosite: {exc}",
        )
    if changed:
        return ResourceUpdateResult(
            "geodata",
            "available",
            "Доступно обновление geoip/geosite",
            current_version,
            latest_version,
        )
    return ResourceUpdateResult(
        "geodata",
        "up_to_date",
        "geoip/geosite актуальны",
        current_version,
        latest_version,
    )


def _geodata_install_matches(
    target_dir: Path,
    latest_version: str,
    release_assets: dict[str, tuple[str, str]],
    *,
    region: str = "russia",
) -> bool:
    metadata = _read_geodata_metadata(target_dir)
    installed_region = str(metadata.get("region") or "russia").strip().lower()
    if installed_region != str(region or "russia").strip().lower():
        return False
    if str(metadata.get("version") or "") != str(latest_version or ""):
        return False
    recorded_hashes = metadata.get("sha256")
    if not isinstance(recorded_hashes, dict):
        return False
    for name, (_url, expected_hash) in release_assets.items():
        if str(recorded_hashes.get(name) or "").lower() != expected_hash.lower():
            return False
        if name in {"geoip.dat", "geosite.dat"}:
            path = target_dir / name
            if not path.is_file() or _sha256_file(path).lower() != expected_hash.lower():
                return False
    expected_rules = REGIONAL_SINGBOX_RULE_SETS.get(region, REGIONAL_SINGBOX_RULE_SETS["russia"])
    recorded_rules = metadata.get("rule_sets")
    if metadata.get("schema") == 2 and recorded_rules != expected_rules:
        return False
    return all(
        (SINGBOX_RULE_SET_DIR / (
            Path(source.removeprefix("archive:")).name
            if source.startswith("archive:")
            else f"{key.replace(':', '-')}.srs"
        )).is_file()
        for key, source in expected_rules.items()
    )


class StartupResourceCheckWorker(QThread):
    """Фоновая проверка обновлений ядра sing-box и geoip/geosite при запуске."""

    done = pyqtSignal(object)  # list[ResourceUpdateResult]

    def __init__(self, *, singbox_path: str = "", proxy_url: str | None = None, region: str = "russia") -> None:
        super().__init__()
        self._singbox_path = singbox_path
        self._proxy_url = proxy_url
        self._region = region
        self._cancelled = threading.Event()
        self._responses: list[object] = []
        self._response_lock = threading.Lock()
        self.setObjectName("lumen-startup-resource-check")

    def cancel(self) -> None:
        self._cancelled.set()
        with self._response_lock:
            responses = list(self._responses)
        for response in responses:
            abort_http_response(response)

    def _register_response(self, response: object) -> None:
        with self._response_lock:
            self._responses.append(response)

    def _unregister_response(self, response: object) -> None:
        with self._response_lock:
            self._responses = [item for item in self._responses if item is not response]

    def run(self) -> None:
        results: list[ResourceUpdateResult] = []
        try:
            results.append(
                check_or_update_singbox(
                    self._singbox_path,
                    apply_update=False,
                    proxy_url=self._proxy_url,
                    cancelled=self._cancelled.is_set,
                    response_opened=self._register_response,
                    response_closed=self._unregister_response,
                )
            )
        except UpdateCancelled:
            return
        except Exception as exc:
            results.append(ResourceUpdateResult("singbox", "error", str(exc)))
        try:
            results.append(
                check_geodata_update(
                    region=self._region,
                    proxy_url=self._proxy_url,
                    cancelled=self._cancelled.is_set,
                    response_opened=self._register_response,
                    response_closed=self._unregister_response,
                )
            )
        except UpdateCancelled:
            return
        except Exception as exc:
            results.append(ResourceUpdateResult("geodata", "error", str(exc)))
        if not self._cancelled.is_set():
            self.done.emit(results)


class ResourceUpdateWorker(QThread):
    done = pyqtSignal(object)
    progress = pyqtSignal(int)
    request_disconnect = pyqtSignal()

    def __init__(self, kind: str, *, singbox_path: str = "", apply_update: bool = True, proxy_url: str | None = None, region: str = "russia") -> None:
        super().__init__()
        self._kind = kind
        self._singbox_path = singbox_path
        self._apply_update = apply_update
        self._proxy_url = proxy_url
        self._region = region
        self._cancelled = threading.Event()
        self._disconnect_ack = threading.Event()
        self._disconnect_success = False
        self._responses: list[object] = []
        self._response_lock = threading.Lock()
        self.setObjectName(f"lumen-resource-updater-{kind}")

    def cancel(self) -> None:
        self._cancelled.set()
        self._disconnect_ack.set()
        with self._response_lock:
            responses = list(self._responses)
        for response in responses:
            abort_http_response(response)

    def _register_response(self, response: object) -> None:
        with self._response_lock:
            self._responses.append(response)

    def _unregister_response(self, response: object) -> None:
        with self._response_lock:
            self._responses = [item for item in self._responses if item is not response]

    def confirm_disconnect(self, success: bool) -> None:
        self._disconnect_success = bool(success)
        self._disconnect_ack.set()

    def run(self) -> None:
        try:
            if self._kind == "singbox":
                result = check_or_update_singbox(
                    self._singbox_path,
                    self._apply_update,
                    on_progress=lambda done, total: self.progress.emit(int(done * 100 / total)),
                    proxy_url=self._proxy_url,
                    on_install_start=self._trigger_disconnect_request,
                    cancelled=self._cancelled.is_set,
                    response_opened=self._register_response,
                    response_closed=self._unregister_response,
                )
            elif self._kind == "geodata":
                if self._apply_update:
                    result = update_geodata(
                        region=self._region,
                        on_progress=lambda done, total: self.progress.emit(int(done * 100 / total)),
                        proxy_url=self._proxy_url,
                        on_install_start=self._trigger_disconnect_request,
                        cancelled=self._cancelled.is_set,
                        response_opened=self._register_response,
                        response_closed=self._unregister_response,
                    )
                else:
                    result = check_geodata_update(
                        region=self._region,
                        proxy_url=self._proxy_url,
                        cancelled=self._cancelled.is_set,
                        response_opened=self._register_response,
                        response_closed=self._unregister_response,
                    )
            elif self._kind == "droute":
                result = check_or_update_droute(
                    self._apply_update,
                    on_progress=lambda done, total: self.progress.emit(int(done * 100 / total)),
                    proxy_url=self._proxy_url,
                    cancelled=self._cancelled.is_set,
                    response_opened=self._register_response,
                    response_closed=self._unregister_response,
                )
            else:
                result = ResourceUpdateResult(self._kind, "error", f"Неизвестный тип обновления: {self._kind}")
        except UpdateCancelled:
            return
        if not self._cancelled.is_set():
            self.done.emit(result)

    def _trigger_disconnect_request(self) -> None:
        self._disconnect_success = False
        self._disconnect_ack.clear()
        self.request_disconnect.emit()
        deadline = time.monotonic() + 60.0
        while not self._disconnect_ack.wait(0.1):
            _raise_if_cancelled(self._cancelled.is_set)
            if time.monotonic() >= deadline:
                raise RuntimeError("timed out waiting for the active core to stop")
        _raise_if_cancelled(self._cancelled.is_set)
        if not self._disconnect_success:
            raise RuntimeError("failed to stop the active core before update")
