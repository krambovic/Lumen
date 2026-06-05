from __future__ import annotations

from pathlib import Path
import zipfile


def safe_extract_zip(archive: zipfile.ZipFile, target_dir: Path) -> None:
    root = target_dir.resolve()
    root.mkdir(parents=True, exist_ok=True)
    for member in archive.infolist():
        member_path = Path(member.filename)
        if member_path.is_absolute() or ".." in member_path.parts:
            raise RuntimeError(f"unsafe zip member path: {member.filename}")
        destination = (root / member_path).resolve()
        if root != destination and root not in destination.parents:
            raise RuntimeError(f"unsafe zip member path: {member.filename}")
    archive.extractall(root)
