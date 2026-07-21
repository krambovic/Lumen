"""Build Lumen.

Lumen is QML-only. This wrapper keeps the old `python build.py` command
working while delegating all packaging to build_qml.py.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent


def main() -> int:
    cmd = [sys.executable, str(ROOT / "build_qml.py"), *sys.argv[1:]]
    return subprocess.run(cmd, cwd=str(ROOT)).returncode


if __name__ == "__main__":
    raise SystemExit(main())
