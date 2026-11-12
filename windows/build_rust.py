"""Build and package the native Windows client using the existing Inno installer."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

import build_qml as packaging


def _resource_compiler() -> Path:
    if path := shutil.which("rc.exe"):
        return Path(path)
    program_files = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)"))
    sdk = Path(os.environ.get("WindowsSdkDir", program_files / "Windows Kits" / "10"))
    versions = sorted(sdk.glob("bin/*/x64/rc.exe"), reverse=True)
    if versions:
        return versions[0]
    raise RuntimeError("Windows SDK resource compiler (rc.exe) was not found")


def _compile_resources(work: Path, version: str) -> Path:
    resource_file = work / "lumen.rc"
    resource_object = work / "lumen.res"
    numeric = packaging._numeric_version(version).replace(".", ",")
    icon = (packaging.ASSETS_DIR / "Lumen.ico").as_posix()
    manifest = (packaging.ROOT / "app.manifest").as_posix()
    resource_file.write_text(
        f'''1 ICON "{icon}"
1 24 "{manifest}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILETYPE 1
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "FileDescription", "Lumen"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "Lumen"
            VALUE "OriginalFilename", "Lumen.exe"
            VALUE "ProductName", "Lumen"
            VALUE "ProductVersion", "{version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
''',
        encoding="utf-8-sig",
    )
    packaging._run([str(_resource_compiler()), "/nologo", "/fo", str(resource_object), str(resource_file)])
    return resource_object


def build_executable(work: Path, version: str) -> Path:
    rust = packaging.REPO_ROOT / "rust"
    resources = _compile_resources(work, version)
    env = {**os.environ, "LUMEN_BUILD_VERSION": version}
    packaging._run(
        ["cargo", "rustc", "--locked", "--release", "-p", "lumen-ui", "--bin", "Lumen",
         "--target-dir", str(rust / "target"), "--", "-C", f"link-arg={resources}",
         "-C", "link-arg=/MANIFEST:NO"],
        cwd=str(rust),
        env=env,
    )
    return rust / "target" / "release" / "Lumen.exe"


def stage_release(executable: Path, app_dir: Path) -> None:
    app_dir.mkdir(parents=True, exist_ok=True)
    if any(app_dir.iterdir()):
        raise RuntimeError(f"The staging directory must be empty: {app_dir}")
    shutil.copy2(executable, app_dir / "Lumen.exe")
    packaging._install_legacy_update_bridge(app_dir)
    packaging._copy_tree_merge(packaging.CORE_DIR, app_dir / "core")
    packaging._remove_legacy_files(app_dir / "core", packaging.LEGACY_CORE_FILES)
    packaging._copy_tree_merge(packaging.ZAPRET_DIR, app_dir / "zapret")
    packaging._copy_tree_merge(packaging.DATA_TEMPLATES_DIR, app_dir / "data" / "templates")
    packaging._validate_droute_bundle(packaging.DROUTE_BUNDLE_DIR)
    packaging._copy_tree_merge(packaging.DROUTE_BUNDLE_DIR, app_dir / "data" / "external" / "droute")
    packaging._copy_tree_merge(packaging.ASSETS_DIR, app_dir / "assets")
    for notice in packaging.NOTICE_FILES:
        if notice.is_file():
            shutil.copy2(notice, app_dir / notice.name)


def verify_executable(app_dir: Path, work: Path, version: str) -> None:
    for flag in ("--startup-probe-file", "--version-file"):
        probe = work / f"{flag[2:]}.txt"
        probe.unlink(missing_ok=True)
        subprocess.run([str(app_dir / "Lumen.exe"), flag, str(probe)],
                       cwd=app_dir, check=True, timeout=45)
        actual = probe.read_text(encoding="utf-8").strip()
        if actual != version:
            raise RuntimeError(f"Packaged executable reports {actual!r} for {flag}, expected {version!r}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--no-zip", action="store_true", help="skip portable archive creation")
    parser.add_argument("--no-installer", action="store_true", help="skip the Inno Setup installer")
    parser.add_argument("--stage-dir", type=Path, help="retain the staged application in an empty directory")
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("the native Windows package must be built on Windows")

    for relative in ("xray.exe", "wintun.dll", "sing-box-extended/sing-box.exe"):
        if not (packaging.CORE_DIR / relative).is_file():
            parser.error(f"missing bundled core file: {packaging.CORE_DIR / relative}")

    version = packaging._read_app_version()
    packaging.DIST_DIR.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="lumen-rust-", dir=packaging.DIST_DIR) as temporary:
        work = Path(temporary)
        app_dir = args.stage_dir.resolve() if args.stage_dir else work / "Lumen"
        if args.stage_dir is None and args.no_zip and args.no_installer:
            app_dir = Path(tempfile.mkdtemp(prefix="Lumen-", dir=packaging.DIST_DIR))
        executable = build_executable(work, version)
        stage_release(executable, app_dir)
        verify_executable(app_dir, work, version)
        if not args.no_zip:
            packaging.pack_portable_zip(app_dir)
        if not args.no_installer:
            packaging.build_installer(app_dir, version)
        if args.stage_dir or (args.no_zip and args.no_installer):
            print(f"Staged Windows application: {app_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
