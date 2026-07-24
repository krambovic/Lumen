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

from xray_fluent.constants import APP_VERSION, SUBSCRIPTION_FETCHER_EXE_NAME


def _is_subscription_fetcher() -> bool:
    return (
        "--subscription-fetcher" in sys.argv[1:]
        or Path(sys.executable).name.casefold() == SUBSCRIPTION_FETCHER_EXE_NAME.casefold()
    )

if __name__ == "__main__":
    if _is_subscription_fetcher():
        from xray_fluent.subscription_fetcher import cli_main

        sys.exit(cli_main())
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
    from xray_fluent.qml_app.main_qml import main

    sys.exit(main())
