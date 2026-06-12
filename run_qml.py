"""Top-level launcher for the Qt Quick (QML) frontend.

This exists so the app can be started both as a module and as a PyInstaller
entry point. PyInstaller executes the entry script as ``__main__`` (not as a
package submodule), which would break the relative imports inside
``xray_fluent.qml_app.main_qml``. Using absolute imports here avoids that.

Dev usage:
    python run_qml.py
    # or
    python -m xray_fluent.qml_app.main_qml
"""
from __future__ import annotations

import sys
from pathlib import Path

from xray_fluent.constants import APP_VERSION
from xray_fluent.qml_app.main_qml import main

if __name__ == "__main__":
    if "--version-file" in sys.argv[1:]:
        try:
            index = sys.argv.index("--version-file")
            Path(sys.argv[index + 1]).write_text(APP_VERSION, encoding="utf-8")
            sys.exit(0)
        except Exception:
            sys.exit(2)
    if "--version" in sys.argv[1:]:
        print(APP_VERSION)
        sys.exit(0)
    sys.exit(main())
