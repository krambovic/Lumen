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

from xray_fluent.qml_app.main_qml import main

if __name__ == "__main__":
    sys.exit(main())
