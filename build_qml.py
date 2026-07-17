"""
Build the Qt Quick (QML) edition of Lumen KVN via PyInstaller.

Usage:  python build_qml.py            - full build (clean + compile + pack zip)
        python build_qml.py --no-zip   - skip zip creation
        python build_qml.py --no-installer  - skip Inno Setup installer
        python build_qml.py --clean    - only wipe previous QML build artefacts

Requires .venv created by setup.bat (or manually).
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import shutil
import subprocess
import sys
import threading
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
VENV_DIR = ROOT / ".venv"
VENV_PYTHON = VENV_DIR / "Scripts" / "python.exe"

APP_NAME = "LumenKVN"
SUBSCRIPTION_FETCHER_EXE_NAME = "lumen-subscription-fetcher.exe"
SPEC_OUTPUT_NAME = "LumenKVN"
SPEC_FILE = ROOT / "LumenKVN-qml.spec"

DIST_DIR = ROOT / "dist"
BUILD_DIR = ROOT / "build"
PYINSTALLER_VERSION_FILE = BUILD_DIR / "LumenKVN-version-info.txt"
APP_DIR = DIST_DIR / "LumenKVN"
PORTABLE_ZIP_PATH = DIST_DIR / f"{APP_NAME}-portable-windows-x64.zip"
INSTALLER_PATH = DIST_DIR / f"{APP_NAME}-Setup-windows-x64.exe"

CORE_DIR = ROOT / "core"
ZAPRET_DIR = ROOT / "zapret"
DATA_TEMPLATES_DIR = ROOT / "data" / "templates"
DROUTE_BUNDLE_DIR = ROOT / "data" / "external" / "droute"
INNO_SCRIPT = ROOT / "installer" / "LumenKVN.iss"
ASSETS_DIR = ROOT / "assets"
NOTICE_FILES = (ROOT / "LICENSE", ROOT / "NOTICE.md", ROOT / "README_QML.md", ROOT / "README.md")
LEGACY_CORE_FILES = ("tun2socks.exe",)
DROUTE_REQUIRED_FILES = (
    "droute.exe",
    "droute.exe.config",
    "LICENSE.txt",
    "SHA256SUMS.txt",
    "version.txt",
)


def _print(msg: str) -> None:
    print(f"[build-qml] {msg}", flush=True)


def _validate_droute_bundle(directory: Path = DROUTE_BUNDLE_DIR) -> str:
    missing = [name for name in DROUTE_REQUIRED_FILES if not (directory / name).is_file()]
    if missing:
        raise RuntimeError(f"Bundled droute is incomplete: {', '.join(missing)}")
    if (directory / "droute.exe").stat().st_size < 1024:
        raise RuntimeError("Bundled droute.exe is empty or damaged")
    version = (directory / "version.txt").read_text(encoding="utf-8").strip().lstrip("v")
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise RuntimeError(f"Bundled droute version is invalid: {version or 'empty'}")
    checksum_lines = (directory / "SHA256SUMS.txt").read_text(encoding="utf-8").splitlines()
    for line in checksum_lines:
        expected, separator, relative_name = line.strip().partition("  ")
        if not separator or not re.fullmatch(r"[0-9a-f]{64}", expected):
            raise RuntimeError("Bundled droute checksum manifest is invalid")
        source = (directory / relative_name).resolve()
        try:
            source.relative_to(directory.resolve())
        except ValueError as exc:
            raise RuntimeError("Bundled droute checksum path escapes its directory") from exc
        if not source.is_file():
            raise RuntimeError(f"Bundled droute checksum target is missing: {relative_name}")
        actual = hashlib.sha256(source.read_bytes()).hexdigest()
        if actual != expected:
            raise RuntimeError(f"Bundled droute checksum mismatch: {relative_name}")
    return version


def _windows_path(path: Path) -> str:
    """Convert a repo path to a Windows path when running via WSL interop."""
    resolved = path.resolve()
    if os.name == "nt":
        return str(resolved)
    result = subprocess.run(
        ["wslpath", "-w", str(resolved)],
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def _run(cmd: list[str], **kwargs) -> None:
    _print(f"> {' '.join(cmd)}")
    subprocess.run(cmd, check=True, **kwargs)


def _read_app_version() -> str:
    constants = ROOT / "xray_fluent" / "constants.py"
    match = re.search(r'APP_VERSION\s*=\s*"([^"]+)"', constants.read_text(encoding="utf-8"))
    if not match:
        raise RuntimeError("APP_VERSION not found")
    return match.group(1)


def _numeric_version(version: str) -> str:
    core, _, suffix = version.partition("-")
    parts = [p for p in core.split(".") if p.isdigit()][:3]
    while len(parts) < 3:
        parts.append("0")
    build = "".join(ch for ch in suffix if ch.isdigit()) or "0"
    return ".".join(parts) + "." + build


def _version_tuple(version_info: str) -> tuple[int, int, int, int]:
    parts = [int(part) for part in version_info.split(".")]
    while len(parts) < 4:
        parts.append(0)
    return tuple(parts[:4])


def _write_pyinstaller_version_file(version: str) -> None:
    version_info = _numeric_version(version)
    major, minor, patch, build = _version_tuple(version_info)
    PYINSTALLER_VERSION_FILE.parent.mkdir(parents=True, exist_ok=True)
    PYINSTALLER_VERSION_FILE.write_text(
        f"""# UTF-8
VSVersionInfo(
  ffi=FixedFileInfo(
    filevers=({major}, {minor}, {patch}, {build}),
    prodvers=({major}, {minor}, {patch}, {build}),
    mask=0x3f,
    flags=0x0,
    OS=0x40004,
    fileType=0x1,
    subtype=0x0,
    date=(0, 0)
  ),
  kids=[
    StringFileInfo([
      StringTable(
        '040904B0',
        [
          StringStruct('CompanyName', 'krambovic'),
          StringStruct('FileDescription', 'Lumen KVN'),
          StringStruct('FileVersion', '{version}'),
          StringStruct('InternalName', 'LumenKVN'),
          StringStruct('LegalCopyright', 'Copyright (c) krambovic/lumen-kvn contributors'),
          StringStruct('OriginalFilename', 'LumenKVN.exe'),
          StringStruct('ProductName', 'Lumen KVN'),
          StringStruct('ProductVersion', '{version}')
        ]
      )
    ]),
    VarFileInfo([VarStruct('Translation', [1033, 1200])])
  ]
)
""",
        encoding="utf-8",
    )


def _find_iscc() -> Path | None:
    candidates = [
        Path(os.environ.get("ISCC_PATH", "")),
        Path(os.environ.get("ProgramFiles(x86)", "")) / "Inno Setup 6" / "ISCC.exe",
        Path(os.environ.get("ProgramFiles", "")) / "Inno Setup 6" / "ISCC.exe",
    ]
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    found = shutil.which("ISCC.exe") or shutil.which("iscc")
    return Path(found) if found else None


def _copy_tree_merge(src: Path, dst: Path) -> None:
    """Copy src tree into dst, overwriting files where possible and skipping locked ones."""
    dst.mkdir(parents=True, exist_ok=True)
    for item in src.iterdir():
        if "Zone.Identifier" in item.name or item.name.lower().endswith(".bak"):
            continue
        target = dst / item.name
        if item.is_dir():
            _copy_tree_merge(item, target)
        else:
            try:
                shutil.copy2(str(item), str(target))
            except PermissionError:
                _print(f"  skipped (locked): {target.name}")


def _remove_legacy_files(root: Path, names: tuple[str, ...]) -> None:
    for name in names:
        target = root / name
        if target.exists():
            try:
                target.unlink()
                _print(f"  removed legacy file: {target}")
            except PermissionError:
                _print(f"ERROR: Cannot remove legacy file {target} — is LumenKVN.exe still running?")
                raise SystemExit(1)


def _install_subscription_fetcher(app_dir: Path = APP_DIR) -> Path:
    source = app_dir / f"{APP_NAME}.exe"
    if not source.is_file():
        raise RuntimeError(f"Main executable is missing: {source}")
    target = app_dir / SUBSCRIPTION_FETCHER_EXE_NAME
    shutil.copy2(source, target)
    return target


# ------------------------------------------------------------------
def ensure_venv() -> None:
    if VENV_PYTHON.exists():
        _print(f"venv OK: {VENV_PYTHON}")
        return
    _print("Creating virtual environment ...")
    _run([sys.executable, "-m", "venv", str(VENV_DIR)])
    _run([str(VENV_PYTHON), "-m", "pip", "install", "--upgrade", "pip"])
    _run([str(VENV_PYTHON), "-m", "pip", "install", "-r", str(ROOT / "requirements.txt")])


def clean() -> None:
    # build/ is purely temporary — safe to nuke
    if BUILD_DIR.exists():
        _print(f"Removing {BUILD_DIR}")
        try:
            shutil.rmtree(BUILD_DIR)
        except PermissionError:
            _print(f"ERROR: Cannot remove {BUILD_DIR} — is LumenKVN.exe still running?")
            _print("Close the app (tray -> Quit) and try again.")
            raise SystemExit(1)

    keep_dirs = {"data", "core", "zapret"}
    if APP_DIR.exists():
        for child in APP_DIR.iterdir():
            if child.name in keep_dirs:
                _print(f"Keeping {child}")
                continue
            try:
                if child.is_dir():
                    shutil.rmtree(child)
                else:
                    child.unlink()
            except PermissionError:
                _print(f"WARNING: Cannot remove {child}, skipping")
        _print(f"Cleaned {APP_DIR} (data/, core/, zapret/ preserved)")


def build_exe() -> None:
    ensure_venv()

    if not SPEC_FILE.is_file():
        raise SystemExit(f"Spec file not found: {SPEC_FILE}")

    _write_pyinstaller_version_file(_read_app_version())

    temp_dist = DIST_DIR / "_build_tmp"
    if temp_dist.exists():
        shutil.rmtree(temp_dist)

    cmd = [
        str(VENV_PYTHON), "-m", "PyInstaller",
        _windows_path(SPEC_FILE),
        "--noconfirm",
        "--clean",
        "--distpath", _windows_path(temp_dist),
        "--workpath", _windows_path(BUILD_DIR / "qml"),
    ]
    _run(cmd, cwd=str(ROOT))

    temp_app = temp_dist / SPEC_OUTPUT_NAME
    if not temp_app.is_dir():
        raise SystemExit(f"Expected PyInstaller output not found: {temp_app}")
    _print(f"Merging build output -> {APP_DIR}")
    _copy_tree_merge(temp_app, APP_DIR)
    shutil.rmtree(temp_dist, ignore_errors=True)
    fetcher_path = _install_subscription_fetcher(APP_DIR)
    _print(f"Installed direct subscription helper -> {fetcher_path}")

    dst_core = APP_DIR / "core"
    _print(f"Merging core -> {dst_core}")
    _copy_tree_merge(CORE_DIR, dst_core)
    _remove_legacy_files(dst_core, LEGACY_CORE_FILES)

    dst_zapret = APP_DIR / "zapret"
    if ZAPRET_DIR.is_dir():
        _print(f"Merging zapret -> {dst_zapret}")
        _copy_tree_merge(ZAPRET_DIR, dst_zapret)

    dst_templates = APP_DIR / "data" / "templates"
    if DATA_TEMPLATES_DIR.is_dir():
        _print(f"Merging templates -> {dst_templates}")
        _copy_tree_merge(DATA_TEMPLATES_DIR, dst_templates)

    droute_version = _validate_droute_bundle()
    dst_droute = APP_DIR / "data" / "external" / "droute"
    _print(f"Merging bundled droute {droute_version} -> {dst_droute}")
    _copy_tree_merge(DROUTE_BUNDLE_DIR, dst_droute)

    dst_assets = APP_DIR / "assets"
    if ASSETS_DIR.is_dir():
        _print(f"Merging assets -> {dst_assets}")
        _copy_tree_merge(ASSETS_DIR, dst_assets)

    for notice_file in NOTICE_FILES:
        if notice_file.is_file():
            shutil.copy2(str(notice_file), str(APP_DIR / notice_file.name))

    _print(f"Build complete: {APP_DIR / (APP_NAME + '.exe')}")


def _pack_zip(path: Path) -> None:
    if path.exists():
        path.unlink()
    _print(f"Creating {path} ...")
    base = APP_DIR.parent
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED, compresslevel=1) as zf:
        for item in APP_DIR.rglob("*"):
            zf.write(item, item.relative_to(base))


def pack_portable_zip() -> None:
    marker = APP_DIR / "portable"
    created_marker = not marker.exists()
    if created_marker:
        marker.write_text("", encoding="utf-8")
    try:
        _pack_zip(PORTABLE_ZIP_PATH)
    finally:
        if created_marker and marker.exists():
            marker.unlink()
    _print(f"Portable archive ready: {PORTABLE_ZIP_PATH}")


def build_installer() -> None:
    iscc = _find_iscc()
    if iscc is None:
        raise SystemExit(
            "Inno Setup compiler not found. Install Inno Setup 6 or pass --no-installer."
        )
    if INSTALLER_PATH.exists():
        INSTALLER_PATH.unlink()
    version = _read_app_version()
    version_info = _numeric_version(version)
    source_dir = _windows_path(APP_DIR)
    output_dir = _windows_path(DIST_DIR)
    _run(
        [
            str(iscc),
            f"/DAppVersion={version}",
            f"/DVersionInfo={version_info}",
            f"/DSourceDir={source_dir}",
            f"/DOutputDir={output_dir}",
            "/DAppId={{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}",
            "/DAppNameValue=Lumen KVN",
            "/DOutputBaseName=LumenKVN-Setup-windows-x64",
            _windows_path(INNO_SCRIPT),
        ],
        cwd=str(ROOT),
    )
    _print(f"Installer ready: {INSTALLER_PATH}")


# ------------------------------------------------------------------
def main() -> int:
    parser = argparse.ArgumentParser(description="Build Lumen KVN (QML edition)")
    parser.add_argument("--no-zip", action="store_true", help="skip zip creation")
    parser.add_argument("--no-installer", action="store_true", help="skip Inno Setup installer")
    parser.add_argument("--clean", action="store_true", help="only clean build artefacts")
    args = parser.parse_args()

    os.chdir(ROOT)

    if args.clean:
        clean()
        _print("Done.")
        return 0

    clean()
    build_exe()

    errors: list[BaseException] = []

    def _guard(fn):
        def wrapper():
            try:
                fn()
            except BaseException as exc:
                errors.append(exc)
        return wrapper

    threads = []
    if not args.no_zip:
        threads.append(threading.Thread(target=_guard(pack_portable_zip)))
    if not args.no_installer:
        threads.append(threading.Thread(target=_guard(build_installer)))
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    if errors:
        raise errors[0]

    _print("All done!")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
