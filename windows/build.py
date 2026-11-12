"""Build and package the Rust Windows desktop client."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent


def main() -> int:
    cmd = [sys.executable, str(ROOT / "build_rust.py"), *sys.argv[1:]]
    return subprocess.run(cmd, cwd=str(ROOT)).returncode


if __name__ == "__main__":
    raise SystemExit(main())
