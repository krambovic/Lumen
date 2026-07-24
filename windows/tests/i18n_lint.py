from __future__ import annotations

import ast
import io
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PKG = ROOT / "xray_fluent"
LOCALES = PKG / "locales"
QML_DIR = PKG / "qml_app" / "qml"
PY_FUNCS = {"tr", "translate"}
QML_CALL = re.compile(r'I18n\.t\(\s*(["\'])(.*?)\1')

BS = chr(92)
SENT = chr(0)


def _unescape(text: str) -> str:
    return (
        text.replace(BS + BS, SENT)
        .replace(BS + "n", "\n")
        .replace(BS + "t", "\t")
        .replace(BS + "r", "\r")
        .replace(BS + '"', '"')
        .replace(BS + "'", "'")
        .replace(SENT, BS)
    )


def _py_keys() -> tuple[set[str], int]:
    keys: set[str] = set()
    dynamic = 0
    for path in PKG.rglob("*.py"):
        try:
            tree = ast.parse(path.read_text(encoding="utf-8"))
        except SyntaxError:
            continue
        for node in ast.walk(tree):
            if (
                isinstance(node, ast.Call)
                and isinstance(node.func, ast.Name)
                and node.func.id in PY_FUNCS
                and node.args
            ):
                first = node.args[0]
                if isinstance(first, ast.Constant) and isinstance(first.value, str):
                    keys.add(first.value)
                else:
                    dynamic += 1
    return keys, dynamic


def _qml_keys() -> set[str]:
    keys: set[str] = set()
    for path in QML_DIR.rglob("*.qml"):
        for match in QML_CALL.finditer(path.read_text(encoding="utf-8")):
            keys.add(_unescape(match.group(2)))
    return keys


def _load_catalog(path: Path) -> dict[str, str]:
    data = json.loads(path.read_text(encoding="utf-8"))
    return {k: v for k, v in data.items() if isinstance(k, str) and not k.startswith("__")}


def main() -> int:
    py_used, dynamic = _py_keys()
    qml_used = _qml_keys()
    used = py_used | qml_used
    print(f"Source strings used: {len(used)} (py={len(py_used)}, qml={len(qml_used)}, dynamic-skipped={dynamic})")
    catalogs = sorted(LOCALES.glob("*.json"))
    if not catalogs:
        print(f"No locale files found in {LOCALES}")
        return 1
    failed = False
    for path in catalogs:
        catalog = _load_catalog(path)
        missing = sorted(used - set(catalog))
        orphan = sorted(set(catalog) - used)
        print(f"\n[{path.stem}] {len(catalog)} entries | missing={len(missing)} orphan={len(orphan)}")
        for key in missing:
            print(f"  MISSING: {key!r}")
        for key in orphan[:50]:
            print(f"  orphan:  {key!r}")
        if len(orphan) > 50:
            print(f"  ... and {len(orphan) - 50} more orphans")
        if missing:
            failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")
    raise SystemExit(main())
