"""Run unit tests without touching the user's profile, network or cores."""
from __future__ import annotations
import os
import base64
import ast
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
_sandbox = tempfile.TemporaryDirectory(prefix="lumen-unit-tests-")
for _key in ("LOCALAPPDATA", "APPDATA", "HOME", "USERPROFILE", "TMP", "TEMP"):
    os.environ[_key] = _sandbox.name
os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"
os.environ["LUMEN_SAFE_TESTS"] = "1"

def _safe_stdio_fixture(args) -> bool:
    # Only the audited JSON/sleep stdio fixtures, never arbitrary processes.
    argv = args[1]
    if isinstance(argv, str) and os.name == "nt":
        import ctypes
        count = ctypes.c_int()
        parse = ctypes.windll.shell32.CommandLineToArgvW
        parse.argtypes = [ctypes.c_wchar_p, ctypes.POINTER(ctypes.c_int)]
        parse.restype = ctypes.POINTER(ctypes.c_wchar_p)
        ptr = parse(argv, ctypes.byref(count))
        if not ptr:
            return False
        try:
            argv = [ptr[i] for i in range(count.value)]
        finally:
            release = ctypes.windll.kernel32.LocalFree
            release.argtypes = [ctypes.c_void_p]
            release.restype = ctypes.c_void_p
            release(ptr)
    if isinstance(argv, (list, tuple)) and len(argv) == 4 and Path(argv[0]).resolve() == Path(sys.executable).resolve() and Path(argv[1]).resolve() == Path(__file__).resolve() and argv[2] == "--exec-fixture":
        # QML subprocesses execute with THIS SAME guard installed before app imports.
        return True
    if not isinstance(argv, (list, tuple)) or len(argv) != 3:
        return False
    if Path(argv[0]).resolve() != Path(sys.executable).resolve() or argv[1] != "-c":
        return False
    try:
        tree = ast.parse(argv[2])
        if any(not isinstance(stmt, (ast.Import, ast.Expr)) for stmt in tree.body):
            return False
        for node in ast.walk(tree):
            if isinstance(node, (ast.ImportFrom, ast.Lambda, ast.FunctionDef, ast.ClassDef, ast.Assign, ast.NamedExpr)):
                return False
            if isinstance(node, ast.Import) and any(a.name not in {"json", "sys", "time"} or a.asname for a in node.names):
                return False
            if isinstance(node, ast.Attribute):
                if not isinstance(node.value, ast.Name) or (node.value.id, node.attr) not in {("json", "load"), ("json", "dump"), ("sys", "stdin"), ("sys", "stdout"), ("time", "sleep")}:
                    return False
            if isinstance(node, ast.Call):
                if not isinstance(node.func, ast.Attribute) or node.func.attr not in {"load", "dump", "sleep"}:
                    return False
                if node.func.attr == "sleep" and (len(node.args) != 1 or not isinstance(node.args[0], ast.Constant) or not isinstance(node.args[0].value, (int, float)) or not 0 <= node.args[0].value <= 30):
                    return False
        return True
    except Exception:
        return False


def _guard(event, args):
    if event == "ctypes.dlsym" and args[1] in {
        "InternetSetOptionW", "InternetSetOptionA", "ShellExecuteW", "ShellExecuteExW",
        "CreateIpForwardEntry", "CreateIpForwardEntry2", "DeleteIpForwardEntry", "DeleteIpForwardEntry2",
        "SetIpForwardEntry", "RasSetEntryPropertiesW", "RegSetValueExW",
        "SetPerTcpConnectionEStats", "SetPerTcp6ConnectionEStats",
    }:
        raise PermissionError("Unit-test isolation blocked a native system mutation; mock this boundary")
    if event == "subprocess.Popen" and _safe_stdio_fixture(args):
        return
    if event == "subprocess.Popen" or event in {
        "os.system", "winreg.CreateKey", "winreg.DeleteKey", "winreg.DeleteValue",
        "winreg.SetValue", "winreg.SaveKey", "winreg.LoadKey",
    }:
        raise PermissionError(f"Unit-test isolation blocked {event}; mock this boundary")
    if event in {"socket.connect", "socket.sendto", "socket.getaddrinfo"}:
        address = args[0] if event == "socket.getaddrinfo" else args[-1]
        host = address[0] if isinstance(address, tuple) else address
        if str(host) not in {"127.0.0.1", "::1", "localhost", "None"}:
            raise PermissionError("Unit tests allow only loopback networking")

sys.addaudithook(_guard)
# Import constants without copying packaged or local developer state.
from xray_fluent import data_paths
_original_seed = data_paths.seed_user_data
data_paths.seed_user_data = lambda *_args, **_kwargs: None
from xray_fluent import constants
data_paths.seed_user_data = _original_seed
assert Path(constants.DATA_DIR).is_relative_to(_sandbox.name)

if __name__ == "__main__":
    os.chdir(ROOT)
    if len(sys.argv) == 3 and sys.argv[1] == "--exec-fixture":
        code = base64.b64decode(sys.argv[2], validate=True).decode("utf-8")
        exec(compile(code, "<guarded-qt-fixture>", "exec"), {"__name__": "__main__"})
        raise SystemExit(0)
    import pytest
    os.chdir(ROOT)
    raise SystemExit(pytest.main([*(sys.argv[1:] or ["tests", "-q"]), "-m", "not native_helper", "-o", "markers=native_helper: separately gated native executable integration check"]))
