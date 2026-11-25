"""Prepare the offline China preset for the native Windows package."""

from hashlib import sha256
import json
from pathlib import Path
import shutil
import sys
import tempfile
from urllib.request import Request, urlopen

WINDOWS_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(WINDOWS_DIR))

from xray_fluent.geodata_resources import CHINA_SINGBOX_RULE_SETS


LOYALSOLDIER_API = "https://api.github.com/repos/Loyalsoldier/v2ray-rules-dat/releases/latest"
LOYALSOLDIER_LICENSE = "https://raw.githubusercontent.com/Loyalsoldier/v2ray-rules-dat/master/LICENSE"
METADATA_NAME = "lumen-geodata.json"


def fetch(url: str) -> bytes:
    request = Request(url, headers={"User-Agent": "Lumen-Windows-Packaging"})
    with urlopen(request, timeout=60) as response:
        return response.read()


def prepare_bundle(destination: Path) -> None:
    release = json.loads(fetch(LOYALSOLDIER_API))
    assets = {asset["name"]: asset for asset in release["assets"]}
    with tempfile.TemporaryDirectory(prefix="lumen-cn-geodata-") as temp:
        stage = Path(temp)
        rules_dir = stage / "rule-sets"
        rules_dir.mkdir()
        dat_hashes = {}
        for name in ("geoip.dat", "geosite.dat"):
            asset = assets[name]
            body = fetch(asset["browser_download_url"])
            digest = sha256(body).hexdigest()
            if len(body) < 1024 or asset.get("digest") != f"sha256:{digest}":
                raise ValueError(f"Invalid Loyalsoldier release asset: {name}")
            (stage / name).write_bytes(body)
            dat_hashes[name] = digest

        rule_hashes = {}
        for key, url in CHINA_SINGBOX_RULE_SETS.items():
            name = f"{key.replace(':', '-')}.srs"
            body = fetch(url)
            if len(body) < 64 or not body.startswith(b"SRS"):
                raise ValueError(f"Invalid sing-box rule-set: {name}")
            (rules_dir / name).write_bytes(body)
            rule_hashes[name] = sha256(body).hexdigest()

        (stage / "LICENSE-Loyalsoldier.txt").write_bytes(fetch(LOYALSOLDIER_LICENSE))
        metadata = {
            "schema": 3,
            "region": "china",
            "version": release["tag_name"],
            "rule_sets": CHINA_SINGBOX_RULE_SETS,
            "sha256": dat_hashes,
            "rule_set_sha256": rule_hashes,
        }
        shutil.copytree(stage, destination, dirs_exist_ok=True)
        (destination / METADATA_NAME).write_text(
            json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
        )


if __name__ == "__main__":
    prepare_bundle(WINDOWS_DIR / "core" / "geodata" / "china")
