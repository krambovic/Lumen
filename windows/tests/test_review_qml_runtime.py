import base64
from pathlib import Path
import subprocess
import sys


def test_dashboard_instantiates_with_unknown_route_in_guarded_subprocess():
    root = Path(__file__).parents[1]
    script = 'import time\nfrom pathlib import Path\nfrom PyQt6.QtCore import QUrl\nfrom PyQt6.QtWidgets import QApplication\nfrom PyQt6.QtQml import QQmlComponent, QQmlEngine, qmlRegisterSingletonInstance\nimport xray_fluent\nfrom xray_fluent.app_controller import AppController\nfrom xray_fluent.qml_app.bridge.app_bridge import AppBridge\napp = QApplication([])\ncontroller = AppController()\ncontroller.start_deferred_services = lambda: None\ncontroller._cleanup_tun_adapter = lambda **kwargs: None\ncontroller.proxy.disable = lambda **kwargs: False\nbridge = AppBridge(controller)\nqmlRegisterSingletonInstance("App", 1, 0, "App", bridge)\nengine = QQmlEngine()\nqml = Path(xray_fluent.__file__).parent / "qml_app" / "qml"\nengine.addImportPath(str(qml))\nbridge.processModel.set_stats([{"exe": "sandbox.exe", "upload": 100, "download": 200, "unknown_bytes": 300, "route": "unknown"}])\ncomponent = QQmlComponent(engine, QUrl.fromLocalFile(str(qml / "DashboardPage.qml")))\nfor _ in range(100):\n    app.processEvents()\n    if component.status() != QQmlComponent.Status.Loading:\n        break\n    time.sleep(0.01)\nassert not component.isError(), [e.toString() for e in component.errors()]\nitem = component.create()\nassert item is not None, [e.toString() for e in component.errors()]\nfor width in (720, 1000):\n    item.setProperty("width", width)\n    item.setProperty("height", 900)\n    for _ in range(10):\n        app.processEvents()\nprint("Dashboard instantiated at 720 and 1000 pixels with unknown-route process data")\nbridge.shutdown()\ncontroller.finalize_shutdown()\nitem.deleteLater()\nengine.deleteLater()\napp.processEvents()\n'
    result = subprocess.run(
        [sys.executable, str(root / "scripts" / "run_safe_tests.py"), "--exec-fixture",
         base64.b64encode(script.encode("utf-8")).decode("ascii")],
        cwd=root, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=35,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert "Dashboard instantiated" in result.stdout
