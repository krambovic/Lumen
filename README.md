<p align="center">
  <img src="windows/assets/banner.png" alt="Lumen — Xray / sing-box extended cross-platform client" width="100%">
</p>

<p align="center">
  <a href="https://github.com/krambovic/Lumen/releases?q=v"><img src="https://img.shields.io/github/v/release/krambovic/Lumen?filter=v*&amp;sort=semver&amp;style=for-the-badge&amp;label=Desktop&amp;labelColor=1C1C1C&amp;color=8A2BE2" alt="Latest Desktop release"></a>
  <a href="https://github.com/krambovic/Lumen/releases?q=android-v"><img src="https://img.shields.io/github/v/release/krambovic/Lumen?filter=android-v*&amp;sort=semver&amp;style=for-the-badge&amp;label=Android&amp;labelColor=1C1C1C&amp;color=3DDC84" alt="Latest Android release"></a>
  <a href="https://github.com/krambovic/Lumen/releases"><img src="https://img.shields.io/github/downloads/krambovic/Lumen/total?style=for-the-badge&label=Downloads&labelColor=1C1C1C&color=17A673" alt="Downloads"></a>
  <img src="https://img.shields.io/badge/Windows%20%7C%20Android-29B6F6?style=for-the-badge&labelColor=1C1C1C&label=Platform" alt="Platform">
  <img src="https://img.shields.io/badge/GPL--3.0-F5A623?style=for-the-badge&labelColor=1C1C1C&label=License" alt="License">
</p>

<p align="center">
  <b>English</b> · <a href="README-RU.md">Русский</a>
</p>

---

<p align="center">
  <b>Your connection. Your rules.</b><br>
  An open-source VPN, proxy and anti-censorship client for Windows and Android.
</p>

<p align="center">
  <a href="https://github.com/krambovic/Lumen/releases"><b>Download</b></a> ·
  <a href="#features">Features</a> ·
  <a href="#protocols">Protocols</a> ·
  <a href="#settings">Settings</a> ·
  <a href="#screenshots">Screenshots</a> ·
  <a href="https://github.com/krambovic/Lumen/issues">Support</a>
</p>

## Get started

| | Windows | Android |
| :--- | :--- | :--- |
| **Requires** | Windows 10/11 · x64 | Android 8.0+ |
| **Download** | Installer or portable ZIP | ARM64, ARMv7, x86_64 or universal APK |
| **Modes** | TUN · System proxy · Local proxy · Zapret | Device VPN · Local proxy |
| **Core** | sing-box extended + Xray-core | sing-box extended |

1. Download Lumen from [Releases](https://github.com/krambovic/Lumen/releases). Extract the entire archive if using the portable build.
2. Import a server link, configuration file or subscription.
3. Select a server, choose a routing preset and connect.

<sub>Lumen does not provide VPN servers or subscriptions. Most Android phones use the ARM64 APK.</sub>

## Features

### Route what matters

Choose global proxy, regional bypass or custom rules. Route domains, IP ranges and applications through **proxy**, **direct** or **block** actions, with GeoIP/GeoSite, LAN bypass and ad blocking.

Windows includes regional profiles for **Russia, China and Iran**, service/process rules and reusable routing presets. Android offers per-app allow/disallow lists.

### Keep your servers organized

Import subscriptions, create groups, search and filter servers, and keep your selection and sorting across restarts. Use AUTO groups for automatic server selection.

Scheduled refresh supports **ETag/304**, retry backoff and reconciliation. Updates preserve server selection when possible and identify subscriptions with changes or errors. Provider usage and expiry information appear when supplied.

### Know how your connection performs

Run **TCP**, **ICMP**, **HTTP GET**, real-proxy latency or download-speed tests. View live upload/download rates, session traffic, peak speed and traffic history, with per-application accounting in supported modes.

TCP/ICMP checks measure address reachability; real-proxy checks test traffic through the profile.

### Built for each platform

**Windows** — switch compatible sing-box servers without restarting the core in TUN or system-proxy mode. Use Xray for profiles that need it, zapret presets for DPI bypass, Discord routing, tray controls and profile backups.

**Android** — connect through Android VPN or run a local proxy. Control the connection from widgets and Quick Settings, configure SOCKS5 credentials and receive lightweight update notifications. Manual server changes restart the connection.

**Both** — native interfaces, theme presets, automatic updates, and **English, Russian, Persian and Chinese** translations.

## Protocols

| Family | Supported protocols |
| :--- | :--- |
| **V2Ray / Xray** | VLESS · VMess · Trojan |
| **Shadowsocks** | Legacy · AEAD · Shadowsocks 2022 · Supported plugins |
| **QUIC & TLS** | Hysteria · Hysteria2 · TUIC · AnyTLS · Mieru |
| **HTTP-based** | NaïveProxy · MASQUE |
| **WireGuard** | WireGuard · AmneziaWG, including AWG 3.x · Cloudflare WARP |
| **Other** | OpenVPN · SOCKS4/4a/5 · HTTP/HTTPS · Snell through compatible native configs |

<details>
<summary>Transports, security & import formats</summary>

**Transports**

TCP/RAW, WebSocket, gRPC, HTTP/2, HTTPUpgrade, XHTTP, mKCP and supported QUIC transports.

**Security and connection options**

TLS, REALITY, uTLS fingerprints, Vision, supported VLESS Encryption modes, multiplexing and Shadowsocks UDP-over-TCP.

Support depends on the protocol, platform and bundled core; not every combination is valid.

**Import**

- Share links and plain/Base64 subscription lists.
- Clash/Mihomo YAML and Xray/sing-box JSON.
- Shadowsocks SIP002/SIP003 links and SIP008 JSON.
- WireGuard/AmneziaWG `.conf` and OpenVPN `.ovpn`.
- Supported encrypted Happ links and provider metadata.
- Clipboard, files, QR codes and platform-specific deep links.

**Edit and export**

The editor retains protocol-specific fields. Export uses a canonical share link when it can preserve the profile; otherwise, it keeps JSON or a native configuration format.

</details>

## Settings

Expand your platform for the available controls. Some advanced options appear only when their parent feature is enabled or supported by the selected core.

<details>
<summary><b>Windows</b> · Appearance, DNS, routing, TUN, startup & more</summary>

### Appearance

- Light, dark or system theme; color presets, accent and custom base tone.
- Wallpaper with opacity, blur and brightness controls.
- Interface density, scale, corner radius and animations.
- Compact/full settings mode and interface language.
- Windows 11 backdrop effects and transparency strength.

### Subscriptions

- Automatic refresh interval and loading through the active proxy/TUN.
- Include/exclude regular expressions.
- Custom User-Agent, real Windows HWID or a manually supplied HWID.
- Subscription converter enable/disable and service URL.

### DNS

- System or core DNS mode.
- Separate direct/proxy resolvers, each with UDP, TCP, TLS or HTTPS transport.
- Independent IPv4/IPv6 resolution strategies.
- Parallel queries, optimistic cache and GeoSite-based DNS selection.
- TUN DNS interception, Fake DNS/FakeIP and hosts overrides.

### Routing & network

- Russia, China or Iran regional profile and its available quick presets.
- Domain, IP/CIDR, GeoSite, GeoIP, service and process rules.
- Proxy/direct/block actions, fallback behavior and custom routing presets.
- LAN bypass, ad blocking and Firefox proxy integration.
- Reconnect on network changes, IPv6 preference and kill-switch.
- TLS fragmentation: packet selection, fragment length and delay.
- Multiplexing enable/disable and concurrency.
- Low-speed server switching: enable/disable, speed threshold, delay and cooldown.

### Local proxy & TUN

- SOCKS/mixed and HTTP ports.
- Optional proxy authentication with a custom username/password.
- LAN access and routing-only sniffing.
- TUN stack: mixed, system or gVisor.
- MTU, Strict Route, QUIC/HTTP3 blocking and endpoint-independent NAT.

### Startup & tests

- Windows autostart and starting in the tray at Windows sign-in.
- Always request Administrator rights.
- Auto-connect to the last server or after importing a server.
- Restore VPN after sleep/hibernation and automatically start zapret.
- Check for conflicting VPN/proxy processes.
- Ping method, speed-test URL/preset and concurrent checks.

### Updates & data

- Application update checks, automatic installation and stable/beta channel.
- Core and GeoIP/GeoSite update checks; manual resource updates.
- Xray stable/beta channel and custom Xray/sing-box executable paths.
- Profile password and backup import/export.
- Diagnostic telemetry toggle.
- Reset application/routing settings while retaining servers and subscriptions.

</details>

<details>
<summary><b>Android</b> · Themes, DNS, traffic, ping, AUTO & more</summary>

### Appearance

- Light/dark theme presets, Material You colors and AMOLED black.
- Dashboard layout: default, slider or centered.
- Launcher icon: system, light or dark.
- Interface language and haptic feedback.

### Subscriptions

- User-Agent and optional HWID with an editable value.
- Loading through the active VPN and permission to use HTTP subscriptions.
- Scheduled refresh and interval.
- Include/exclude regular expressions.
- Subscription converter enable/disable and URL.
- Allow or reject subscription-provided setting overrides.

### DNS

- Automatic, Android, secure or custom JSON mode.
- Direct/proxy resolvers with UDP, TCP, TLS or HTTPS transport.
- Independent IPv4/IPv6 strategies and proxy IPv4-only option.
- DNS interception, FakeIP, parallel queries and optimistic cache.
- Geo-based DNS selection, hosts and hostname-to-IPv4 override.

### Traffic & connection

- Multiplexing: concurrency, minimum streams, smux/yamux/h2mux and padding.
- TCP Brutal enable/disable and upload/download rates where supported.
- TLS fragmentation, MTU, IPv6 preference and QUIC blocking.
- TCP Fast Open, TCP MultiPath and UDP fragmentation.
- Shadowsocks UDP-over-TCP and outbound connection timeout.

### Ping & AUTO

- TCPing, ICMP, HTTP GET or Real HTTP; test URL and presets.
- Timeout, concurrency, attempts and retry delay.
- Best, average or median result; good/fair latency thresholds.
- Test on opening the server list and optional unreachable-node cleanup with its threshold.
- Restore default ping settings.
- AUTO test URL, interval, switching tolerance and idle timeout.
- Whether AUTO switching interrupts existing connections.

### Routing & local proxy

- Domain/IP rules, regional presets, Geo resources, LAN bypass and ad blocking.
- Per-app allow/disallow lists.
- Proxy-only mode and local proxy enable/disable.
- SOCKS5/HTTP ports and LAN sharing.
- SOCKS5 authentication toggle and editable username/password.

### Application & updates

- Reconnect on network change and automatic connection on boot.
- Proxy data-path validation.
- Speed statistics, VPN notification and notification speed display.
- Logging and diagnostic telemetry toggles.
- Automatic/manual update checks, installed/bundled core version display and APK selection for the device architecture.

</details>

## Screenshots

<details>
<summary>Windows & Android</summary>

**Windows**

<img src="windows/assets/screenshots/windows-dashboard-dark.png" alt="Lumen for Windows — dashboard" width="100%">

<img src="windows/assets/screenshots/windows-routing-light.png" alt="Windows routing in light theme" width="100%">

<img src="windows/assets/screenshots/windows-zapret-dark.png" alt="Windows zapret presets" width="100%">

<img src="windows/assets/screenshots/windows-dashboard-rose-wallpaper.png" alt="Windows custom theme and wallpaper" width="100%">

**Android**

<p align="center">
  <img src="android/assets/screenshots/android-dashboard-dark.jpg" alt="Android dashboard" width="32%">
  <img src="android/assets/screenshots/android-nodes-dark.jpg" alt="Android servers" width="32%">
  <img src="android/assets/screenshots/android-settings-light.jpg" alt="Android settings" width="32%">
</p>

</details>

## Good to know

**Permissions & traffic.** Windows TUN and zapret require Administrator rights. System proxy only captures applications that honor proxy settings; other traffic and DNS may go directly. Android may need permission to run in the background.

**Profiles & privacy.** Use trusted providers, keep exported credentials private and enable authentication for LAN proxy sharing. Other VPN clients may conflict with Lumen.

## Development

<details>
<summary>Build Windows or Android from source</summary>

**Windows** uses Python and PyQt6/QML. Network cores, geodata and packaging assets are also required for a complete build.

```powershell
cd windows
python -m pip install -r requirements.txt
python run_qml.py
python build_qml.py
```

**Android** uses Kotlin and Jetpack Compose. Configure JDK 17 and the Android SDK.

```powershell
cd android
./gradlew assembleDebug
```

For a signed release, configure `android/keystore.properties` and run `./gradlew assembleRelease`.

To rebuild the patched sing-box extended core, use `android/tools/build_singbox_extended.ps1` with Go, Git and Android NDK r28 installed.

</details>

Bug reports and pull requests are welcome. Include your platform, Lumen version, connection mode and sanitized logs when opening an [issue](https://github.com/krambovic/Lumen/issues).

---

Built with [Xray-core](https://github.com/XTLS/Xray-core), [sing-box extended](https://github.com/shtorm-7/sing-box-extended), [zapret-kvn](https://git.zapret.moe/zapretkvn/zapret-kvn), [Wintun](https://www.wintun.net/), [WinDivert](https://reqrypt.org/windivert.html) and [RuNetFreedom routing data](https://github.com/runetfreedom/russia-v2ray-rules-dat).

[GPL-3.0](LICENSE) · [Third-party notices](NOTICE.md) · [Releases](https://github.com/krambovic/Lumen/releases) · [Issues](https://github.com/krambovic/Lumen/issues)
