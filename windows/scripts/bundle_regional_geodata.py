"""Prepare offline regional presets for the native Windows package."""

from hashlib import sha256
import json
from pathlib import Path
import shutil
import sys
import tempfile
from urllib.request import Request, urlopen

WINDOWS_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(WINDOWS_DIR))

from xray_fluent.geodata_resources import CHINA_SINGBOX_RULE_SETS, IRAN_SINGBOX_RULE_SETS


METADATA_NAME = "lumen-geodata.json"


def fetch(url: str) -> bytes:
    request = Request(url, headers={"User-Agent": "Lumen-Windows-Packaging"})
    with urlopen(request, timeout=60) as response:
        return response.read()


def prepare_bundle(region: str, repository: str, rule_sets: dict[str, str]) -> None:
    api = f"https://api.github.com/repos/{repository}"
    release = json.loads(fetch(f"{api}/releases/latest"))
    assets = {asset["name"]: asset for asset in release["assets"]}
    destination = WINDOWS_DIR / "core" / "geodata" / region
    with tempfile.TemporaryDirectory(prefix=f"lumen-{region}-geodata-") as temp:
        stage = Path(temp)
        rules_dir = stage / "rule-sets"
        rules_dir.mkdir()
        dat_hashes = {}
        for name in ("geoip.dat", "geosite.dat"):
            asset = assets[name]
            body = fetch(asset["browser_download_url"])
            digest = sha256(body).hexdigest()
            if len(body) < 1024 or asset.get("digest") != f"sha256:{digest}":
                raise ValueError(f"Invalid {repository} release asset: {name}")
            (stage / name).write_bytes(body)
            dat_hashes[name] = digest

        rule_hashes = {}
        for key, url in rule_sets.items():
            name = f"{key.replace(':', '-')}.srs"
            body = fetch(url)
            if len(body) < 64 or not body.startswith(b"SRS"):
                raise ValueError(f"Invalid sing-box rule-set: {name}")
            (rules_dir / name).write_bytes(body)
            rule_hashes[name] = sha256(body).hexdigest()

        license_info = json.loads(fetch(f"{api}/license"))
        (stage / "LICENSE-geodata.txt").write_bytes(fetch(license_info["download_url"]))
        metadata = {
            "schema": 3,
            "region": region,
            "version": release["tag_name"],
            "rule_sets": rule_sets,
            "sha256": dat_hashes,
            "rule_set_sha256": rule_hashes,
        }
        shutil.copytree(stage, destination, dirs_exist_ok=True)
        (destination / METADATA_NAME).write_text(
            json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
        )


if __name__ == "__main__":
    prepare_bundle("china", "Loyalsoldier/v2ray-rules-dat", CHINA_SINGBOX_RULE_SETS)
    prepare_bundle("iran", "Chocolate4U/Iran-v2ray-rules", IRAN_SINGBOX_RULE_SETS)
