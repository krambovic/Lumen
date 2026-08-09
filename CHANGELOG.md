## What's Changed

- feat: added proxy authentication and limited non-admin mode
- feat: added HTTP GET latency tests and resizable server columns
- feat: expanded subscription, AnyTLS, Snell, Clash and OpenVPN TCP imports
- fix: fixed regional presets, service actions and first-start routing state
- fix: prevented DNS leaks and stale sing-box route/DNS rules
- fix: renamed the sing-box TUN adapter to `tun0` and improved startup cleanup
- fix: fixed real latency and speed test lifecycle
- fix: fixed VPN client conflict handling and false own-Xray detection
- fix: fixed Windows 10 autostart visibility, Mica logging and window frame layout
- fix: prevented PowerShell cleanup errors during Windows shutdown
- fix: fixed portable startup, single-instance activation and non-admin launch
- fix: fixed installer builds and replacement of existing release assets
- security: protected stored proxy credentials with Windows DPAPI
- security: hardened updates, resource hashes and diagnostics redaction

Full Changelog: https://github.com/krambovic/Lumen/compare/v1.9.6...v1.9.7
