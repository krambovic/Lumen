from __future__ import annotations

import csv
import os
import subprocess


_CREATE_NO_WINDOW = 0x08000000 if os.name == "nt" else 0
_KNOWN_CONFLICTS = {
    "amneziawg.exe": "AmneziaWG",
    "amneziavpn.exe": "AmneziaVPN",
    "clash-verge.exe": "Clash Verge",
    "clash-verge-rev.exe": "Clash Verge Rev",
    "clash.exe": "Clash",
    "hiddify.exe": "Hiddify",
    "mihomo.exe": "Mihomo",
    "nekobox.exe": "NekoBox",
    "nekoray.exe": "NekoRay",
    "nordvpn.exe": "NordVPN",
    "openvpn.exe": "OpenVPN",
    "outline-client.exe": "Outline",
    "protonvpn.exe": "Proton VPN",
    "psiphon3.exe": "Psiphon",
    "v2rayn.exe": "v2rayN",
    "wireguard.exe": "WireGuard",
}


def find_conflicting_network_apps() -> list[str]:
    if os.name != "nt":
        return []
    try:
        result = subprocess.run(
            ["tasklist", "/FO", "CSV", "/NH"],
            capture_output=True,
            timeout=4,
            check=False,
            creationflags=_CREATE_NO_WINDOW,
        )
        text = result.stdout.decode("utf-8", errors="replace")
    except Exception:
        return []
    found: set[str] = set()
    for row in csv.reader(text.splitlines()):
        if not row:
            continue
        label = _KNOWN_CONFLICTS.get(row[0].strip().lower())
        if label:
            found.add(label)
    return sorted(found, key=str.casefold)
