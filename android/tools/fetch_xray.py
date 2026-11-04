"""Fetch pinned official Xray Android binaries, verifying upstream SHA256 digests."""
import hashlib
import io
import re
import urllib.request
import zipfile
from pathlib import Path

VERSION = "v26.10.31"
ROOT = Path(__file__).resolve().parents[1] / "app/src/main/jniLibs"

def fetch(abi, asset):
    url = f"https://github.com/XTLS/Xray-core/releases/download/{VERSION}/{asset}.zip"
    data = urllib.request.urlopen(url, timeout=120).read()
    digest = urllib.request.urlopen(url + ".dgst", timeout=30).read().decode()
    expected = hashlib.sha256(data).hexdigest()
    assert expected.lower() in re.sub(r"\s", "", digest).lower(), "Upstream SHA256 mismatch"
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        binary = next(n for n in archive.namelist() if Path(n).name == "xray")
        target = ROOT / abi / "libxray.so"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(archive.read(binary))
        if abi == "arm64-v8a":
            license_name = next(n for n in archive.namelist() if Path(n).name == "LICENSE")
            license_target = ROOT.parent / "assets/licenses/xray.txt"
            license_target.parent.mkdir(parents=True, exist_ok=True)
            license_target.write_bytes(archive.read(license_name))
        print(abi, VERSION, hashlib.sha256(target.read_bytes()).hexdigest(), flush=True)

if __name__ == "__main__":
    from concurrent.futures import ThreadPoolExecutor
    with ThreadPoolExecutor(max_workers=2) as pool:
        futures = [pool.submit(fetch, abi, name) for abi, name in [
            ("arm64-v8a", "Xray-android-arm64-v8a"), ("x86_64", "Xray-android-amd64")]]
        for future in futures:
            future.result()
