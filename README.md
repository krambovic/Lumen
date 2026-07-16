# Lumen KVN

<p align="center">
  <img src="assets/LumenKVN.png" alt="Lumen KVN Logo" width="140">
</p>

<p align="center">
  <a href="https://github.com/krambovic/Lumen-KVN/releases"><img src="https://img.shields.io/github/v/release/krambovic/Lumen-KVN?style=for-the-badge&label=Release&labelColor=3A3A3A&color=8A2BE2" alt="Release"></a>
  <a href="https://github.com/krambovic/Lumen-KVN/releases"><img src="https://img.shields.io/github/downloads/krambovic/Lumen-KVN/total?style=for-the-badge&label=Downloads&labelColor=3A3A3A&color=17A673" alt="Downloads"></a>
  <img src="https://img.shields.io/badge/Platform-Windows%2011%20%7C%2010-29B6F6?style=for-the-badge&labelColor=3A3A3A" alt="Platform">
</p>

<p align="center">
  <b>Language:</b> English | <a href="README-RU.md">Русский</a>
</p>

---

Lumen KVN is a standalone Windows client for VPN/TUN, system proxy, routing, server management, and DPI bypass through zapret. It features a modern GPU-rendered QML interface with Mica/Acrylic effects.

> [!IMPORTANT]
> TUN/VPN modes and DPI bypass features (zapret) require Administrator privileges.

---

## Screenshots

<details>
<summary>Dashboard and theme previews</summary>
<br>

<img src="assets/screenshots/dashboard-dark.png" alt="Dashboard in dark theme" width="100%">
<br><br>
<img src="assets/screenshots/dashboard-red.png" alt="Dashboard with red accent theme" width="100%">
<br><br>
<img src="assets/screenshots/settings-rose-pine.png" alt="Settings in Rose Pine theme" width="100%">
<br><br>
<img src="assets/screenshots/dashboard-light.png" alt="Dashboard in light theme" width="100%">
<br><br>
<img src="assets/screenshots/zapret-dark.png" alt="Zapret DPI bypass screen" width="100%">

</details>

---

## Features

| Category | Components Used | Description |
| :--- | :--- | :--- |
| **DPI Bypass** | zapret / WinDivert | DPI circumvention for YouTube, Discord, and other services on packet level. |
| **TUN / VPN** | sing-box-extended | Fully-featured TUN mode with support for AmneziaWG (AWG 2.0), WireGuard, and XHTTP. |
| **Proxy** | xray-core | System proxy mode (VLESS, Trojan, Shadowsocks, VMess). |
| **Routing** | GUI presets | Convenient routing editor with presets, custom domains, IP rules, and per-service behavior. |
| **Discord Voice** | droute / SOCKS5 | Routes Discord voice and streams through the proxy without enabling full TUN mode. |
| **Diagnostics** | built-in tests | Latency (ping) and real download speed testing for servers. |
| **Interface** | PyQt6 / QML | Dynamic accent colors, custom theme presets, and wallpaper support. |

---

## Supported protocols

Lumen KVN supports importing and running these server types:

- **Xray / system proxy:** VLESS, VMess, Trojan, Shadowsocks, SOCKS, HTTP.
- **sing-box / TUN:** Hysteria, Hysteria2, TUIC, Mieru, MASQUE, WireGuard, AmneziaWG (AWG), WARP.
- **Custom configs:** raw Xray and sing-box JSON configs, including full sing-box config imports.

## Subscription support

- Regular subscription URLs and encrypted Happ links: `happ://crypt`, `happ://crypt2`, `happ://crypt3`, `happ://crypt4`, and `happ://crypt5`.
- HWID-protected subscriptions: Lumen can send the real Windows device HWID (enabled by default) or a custom HWID configured by the user.
- Happ Premium subscription metadata and supported controls are displayed directly in the server list and subscription properties.

> [!NOTE]
> Full `happ://crypt5` decryption support requires [Node.js](https://nodejs.org/) to be installed and available through `PATH`. Earlier `happ://crypt` formats are decrypted by Lumen itself.

## Installation

Go to the **[Releases](https://github.com/krambovic/Lumen-KVN/releases)** page and download the appropriate package:

* **Installer (`LumenKVN-Setup-windows-x64.exe`):** Recommended for most users.
* **Portable version (`LumenKVN-portable-windows-x64.zip`):** Standalone archive that runs without installation.

> [!CAUTION]
> **Windows Defender or another antivirus may report a false positive for Lumen KVN or its bundled components.** Lumen includes network tools such as Xray, sing-box and zapret, can create a TUN interface, and changes system proxy and routing settings. These capabilities, combined with the unsigned PyInstaller-packaged application, may trigger heuristic antivirus rules even when no malware is present. Download Lumen only from the official [GitHub Releases](https://github.com/krambovic/Lumen-KVN/releases) page. Do not disable antivirus protection globally; if a file is blocked, inspect the detection and submit it to the antivirus vendor as a false positive or add a local exception only after verifying the download source.

---

## Quick Start

1. Run Lumen KVN as Administrator.
2. Import a server link or a supported `.conf` file.
3. Choose the connection mode: system proxy, VPN/TUN, or zapret DPI bypass.
4. Select a routing preset and connect.

WARP, WireGuard, AmneziaWG, Hysteria, Hysteria2, TUIC, Mieru, and MASQUE configs are handled through TUN mode with `sing-box-extended`; VLESS, VMess, Trojan, Shadowsocks, SOCKS, and HTTP links can be used through system proxy mode.

---

## Build Instructions (for Developers)

<details>
<summary><b>Show Build Instructions</b></summary>

1. Install project dependencies:
   ```powershell
   pip install -r requirements.txt
   ```
2. Put core executables (`xray.exe`, `sing-box.exe`, `wintun.dll`, and GeoIP database files) in the `core/` directory.
3. Run the build script:
   ```powershell
   python build_qml.py
   ```
The build output will be located in the `dist/` directory.
</details>

---

## Star History

<a href="https://www.star-history.com/?repos=krambovic%2FLumen-KVN&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=krambovic/Lumen-KVN&type=date&theme=dark&legend=top-left&sealed_token=fW_XUyA3Qay011mKD7tuewBXpt8nzW6MbbuhvhOy-y-fr9jxvjrRZ_K88QIDpCds5soFksO_3iAvFQ9bkLGkB9My96Lkis7F7wxOS5LzxAb8FXS2yXAbrLbB-oBrdliut-myHmPUuPT8QPARlDbYrE7_dL2-sMUq6luZ_bOH15ALx_8XEKtC6iMCsI9f" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=krambovic/Lumen-KVN&type=date&legend=top-left&sealed_token=fW_XUyA3Qay011mKD7tuewBXpt8nzW6MbbuhvhOy-y-fr9jxvjrRZ_K88QIDpCds5soFksO_3iAvFQ9bkLGkB9My96Lkis7F7wxOS5LzxAb8FXS2yXAbrLbB-oBrdliut-myHmPUuPT8QPARlDbYrE7_dL2-sMUq6luZ_bOH15ALx_8XEKtC6iMCsI9f" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=krambovic/Lumen-KVN&type=date&legend=top-left&sealed_token=fW_XUyA3Qay011mKD7tuewBXpt8nzW6MbbuhvhOy-y-fr9jxvjrRZ_K88QIDpCds5soFksO_3iAvFQ9bkLGkB9My96Lkis7F7wxOS5LzxAb8FXS2yXAbrLbB-oBrdliut-myHmPUuPT8QPARlDbYrE7_dL2-sMUq6luZ_bOH15ALx_8XEKtC6iMCsI9f" />
 </picture>
</a>

---

## Contributors

[![Contributors](https://contrib.rocks/image?repo=krambovic/Lumen-KVN)](https://github.com/krambovic/Lumen-KVN/graphs/contributors)

---

## License

Lumen KVN is licensed under GPL-3.0. Integrated third-party components preserve their original licenses. See [LICENSE](LICENSE) and [NOTICE.md](NOTICE.md) for details.
