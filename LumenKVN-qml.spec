# -*- mode: python ; coding: utf-8 -*-

from pathlib import Path

from PyInstaller.utils.hooks import collect_data_files


root = Path(SPECPATH)
qml_dir = root / "xray_fluent" / "qml_app" / "qml"

datas = [
    (str(qml_dir), "xray_fluent/qml_app/qml"),
]
datas += collect_data_files(
    "PyQt6",
    includes=[
        "Qt6/qml/QtQml/qmldir",
        "Qt6/qml/QtQml/plugins.qmltypes",
        "Qt6/qml/QtQml/qmlplugin.dll",
        "Qt6/qml/QtQml/Models/**",
        "Qt6/qml/QtQuick/qmldir",
        "Qt6/qml/QtQuick/plugins.qmltypes",
        "Qt6/qml/QtQuick/qtquick2plugin.dll",
        "Qt6/qml/QtQuick/Controls/qmldir",
        "Qt6/qml/QtQuick/Controls/plugins.qmltypes",
        "Qt6/qml/QtQuick/Controls/qtquickcontrols2plugin.dll",
        "Qt6/qml/QtQuick/Controls/Basic/**",
        "Qt6/qml/QtQuick/Controls/Universal/**",
        "Qt6/qml/QtQuick/Controls/impl/**",
        "Qt6/qml/QtQuick/Templates/**",
        "Qt6/qml/QtQuick/Layouts/**",
        "Qt6/qml/QtQuick/Window/**",
    ],
)


a = Analysis(
    [str(root / "run_qml.py")],
    pathex=[str(root)],
    binaries=[],
    datas=datas,
    hiddenimports=[
        "PyQt6.QtCore",
        "PyQt6.QtGui",
        "PyQt6.QtWidgets",
        "PyQt6.QtQml",
        "PyQt6.QtQuick",
        "PyQt6.QtOpenGL",
        "PyQt6.QtSvg",
        "PyQt6.QtNetwork",
        "win32comext",
        "win32comext.shell",
        "win32comext.shell.shellcon",
        "encodings.idna",
    ],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=["qfluentwidgets"],
    noarchive=False,
    optimize=0,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="LumenKVN",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    uac_admin=True,
    icon=[str(root / "assets" / "LumenKVN.ico")],
    manifest=str(root / "uac_admin.manifest"),
)
coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name="LumenKVN",
)
