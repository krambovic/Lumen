## Desktop v2.1.3

* Fixed clipboard subscription import, Dobry VPN auto-selection groups and server switching.
* Made HTTP GET the default latency method and excluded connection handshakes from warmed dashboard measurements.
* Updated sing-box extended to 2.7.3 on Desktop and Android, retaining Lumen MASQUE support; bundled droute 2.2.1 and restored online updates.
* Fixed false conflicts with Lumen-owned cores, Firefox proxy restoration, server-list updates and ping tooltips.
* Added a light rounded desktop icon and made the Desktop updater skip Android and incompatible releases.

Full Changelog: https://github.com/krambovic/Lumen/compare/v2.1.2...v2.1.3

## Desktop v2.1.2

* Fixed TUN traffic after switching between Xray and sing-box extended.

Full Changelog: https://github.com/krambovic/Lumen/compare/v2.1.1...v2.1.2

## What's Changed

* feat: implement Material 3 Expressive floating navigation dock with fluid spring-physics tab switching animations
* feat: add interactive drag-to-connect slider bar with dynamic wavy progress and real-time state feedback
* feat: add modular and customizable settings tiles hub with drag-and-drop ordering and multi-size tile layouts
* feat: add native MASQUE tunnel support with custom sing-box extended patches for censorship evasion
* feat: add built-in Cloudflare WARP profile generation supporting WireGuard, AmneziaWG, and MASQUE endpoints
* feat: add adaptive network profile for Iran with secure DoH resolvers (Cloudflare, Google, Quad9), HTTPS transport, and parallel queries
* feat: bundle offline Iranian geo-resources (`geoip-ir.srs` and `geosite-ir.srs`) for instant out-of-the-box rule matching
* feat: support inbound SOCKS5 and HTTP proxy listener with custom username and password authentication for LAN sharing
* feat: add Network Info inspection screen detailing public IP, ISP, ASN, location, and VPN leak status
* feat: add dynamic launcher icon switcher with System, Light, and Dark variants
* feat: implement high refresh rate display policy unlocking 90Hz, 120Hz, and 144Hz panels without resolution downscaling
* feat: introduce Material You dynamic color theming, AMOLED pure black, and custom accent palette options
* feat: add layout switcher between Compact and Detailed server cards with instant search and filter controls
* feat: add visual node editor modal supporting manual parameter configuration for VLESS, VMess, Trojan, SS, AWG, and MASQUE
* feat: integrate in-app camera QR scanner with flashlight support and fast on-device QR code generator
* feat: parse and display subscription metadata, traffic usage quotas, and expiry dates from provider response headers
* feat: add scheduled subscription auto-updates with custom intervals and regex node filtering
* feat: add granular reset controls to safely restore preferences, clear server database, or reset tile layouts
* fix: resolve cyclic back-gesture loop by unwinding tab history naturally and exiting only from the Home screen
* fix: enable true edge-to-edge rendering with transparent status and navigation bars and disabled contrast enforcement on Android 10+
* fix: center the 'L' logo mark and scale within the Android 12+ circular safe zone to eliminate corner clipping
* fix: implement multi-target HTTP latency probing with fallback endpoints and extended timeouts for filtered networks
* fix: protect against DNS poisoning and MITM on proxy endpoints via Server Address Pinning
* fix: patch sing-box extended to prevent nil interface-monitor crashes during network transitions
* fix: preserve selected split-tunneling apps and improve auto-selection for common messengers and browsers
* fix: ensure reliable tunnel reconnection and socket cleanup when switching between Wi-Fi and mobile data
* fix: validate proxy credentials and configuration bounds before starting the core to prevent silent startup failures
* perf: configure comprehensive R8 code and resource shrinking, reducing per-ABI release APK sizes from 105 MB to ~37 MB
* perf: optimize sing-box extended memory usage and JNI buffer lifecycle during high-throughput transfers
* perf: apply immutable UI models and state hoisting across Compose screens to prevent redundant recompositions

Full Changelog: https://github.com/krambovic/Lumen/compare/android-v1.2.1...android-v1.5.0
