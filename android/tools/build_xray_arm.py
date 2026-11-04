"""Build the Android/ARMv7 PIE missing from upstream's current binary releases."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import urllib.request
import zipfile

VERSION = "v26.10.31"
ROOT = Path(__file__).resolve().parents[1]
work = Path(tempfile.gettempdir()) / "lumen-xray-build"
work.mkdir(exist_ok=True)
go = work / "go/bin/go.exe"
if not go.exists():
    releases = json.load(urllib.request.urlopen("https://go.dev/dl/?mode=json&include=all"))
    release = next(r for r in releases if r["version"] == "go1.27.3")
    package = next(f for f in release["files"] if f["os"] == "windows" and f["arch"] == "amd64" and f["kind"] == "archive")
    archive = work / package["filename"]
    urllib.request.urlretrieve("https://go.dev/dl/" + package["filename"], archive)
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == package["sha256"]
    with zipfile.ZipFile(archive) as z:
        z.extractall(work)
source = work / "source"
if not source.exists():
    subprocess.run(["git", "clone", "--depth", "1", "--branch", VERSION,
        "https://github.com/XTLS/Xray-core.git", str(source)], check=True)
ndk = Path(os.environ["LOCALAPPDATA"]) / "Android/Sdk/ndk/28.0.13004108/toolchains/llvm/prebuilt/windows-x86_64/bin"
env = os.environ.copy()
env.update(GOOS="android", GOARCH="arm", GOARM="7", CGO_ENABLED="1", GOMAXPROCS="2",
           CC=str(ndk / "armv7a-linux-androideabi26-clang.cmd"))
target = ROOT / "app/src/main/jniLibs/armeabi-v7a/libxray.so"
subprocess.run([str(go), "build", "-p", "2", "-trimpath", "-buildmode=pie", "-ldflags=-s -w -buildid= -checklinkname=0",
    "-o", str(target), "./main"], cwd=source, env=env, check=True)
print("Android ARMv7", VERSION, hashlib.sha256(target.read_bytes()).hexdigest())
