# Lumen KVN

<p align="center">
  <img src="assets/LumenKVN.png" alt="Lumen KVN" width="160">
</p>

**Language:** English | [Русский](README-RU.md)

Lumen KVN is a standalone Windows client for VPN/TUN, system proxy, routing, server management, and DPI bypass through zapret.

The project preserves copyright and license notices for source materials and bundled third-party components. See [LICENSE](LICENSE) and [NOTICE.md](NOTICE.md).

## What It Does

Lumen KVN combines xray-core, sing-box-extended, zapret, routing presets, and a QML interface without requiring users to edit raw JSON configs for common tasks.

Main features:

- import VLESS, Trojan, Shadowsocks, VMess, WARP, WireGuard, and AWG/AmneziaWG configs;
- system proxy mode through xray-core;
- VPN/TUN mode through sing-box-extended;
- native XHTTP, WARP, WireGuard, and AWG 2.0 support through sing-box-extended;
- built-in zapret/DPI bypass;
- server ping and download speed tests;
- routing by domains, IP ranges, services, and applications;
- routing presets: global, blocked-only, and everything except Russia;
- Discord voice proxy through droute without TUN;
- compact and full UI modes;
- application, xray-core, sing-box-extended, geoip.dat, and geosite.dat updates;
- GPU-rendered QML interface.

## Installation

Download the latest release from [Releases](https://github.com/krambovic/lumen-kvn/releases).

- `LumenKVN-Setup-windows-x64.exe` - regular Windows installer.
- `LumenKVN-portable-windows-x64.zip` - portable version without installation.

TUN/VPN and zapret require Administrator rights.

## sing-box-extended

Lumen KVN uses `sing-box-extended` as the main TUN core.

It powers:

- WARP and WireGuard `.conf`;
- AWG 2.0 / AmneziaWG `.conf`;
- native XHTTP for supported servers;
- TUN routing by domains, IP ranges, services, and applications.

WARP/WireGuard/AWG configs are available only in TUN mode on sing-box-extended. They are intentionally unavailable in system proxy mode because xray-core cannot run these configs directly.

## Importing Configs

Regular proxy links can be pasted from the clipboard.

For WARP/WireGuard/AWG:

1. Open the Servers tab.
2. Click `Import .conf`.
3. Select a file containing `[Interface]` and `[Peer]` sections.

You can also paste the `.conf` text, a local file path, or a `file:///...` URL into import.

AWG 2.0 options such as `Jc`, `Jmin`, `Jmax`, `S1-S4`, and `H1-H4` are read from `[Interface]` automatically.

## Build

```powershell
python build_qml.py
```

The build creates:

- `dist/LumenKVN-Setup-windows-x64.exe`
- `dist/LumenKVN-portable-windows-x64.zip`

Before a release build, `core/` should contain `xray.exe`, `sing-box.exe`, `wintun.dll`, `geoip.dat`, and `geosite.dat`.

## Third-Party Components

Bundled or integrated components keep their own licenses and notices:

- [Xray-core](https://github.com/XTLS/Xray-core)
- [sing-box-extended](https://github.com/shtorm-7/sing-box-extended)
- zapret/WinDivert bundle under `zapret/`
- droute helper for Discord voice proxy, downloaded/used separately

See [NOTICE.md](NOTICE.md) for details.

## License

GPL-3.0. See [LICENSE](LICENSE).