## What's Changed

- feat: add ping and speed checks for nested AUTO server groups, including OpenVPN endpoints, without testing through a direct fallback
- fix: recover Lumen-owned system proxy settings after interrupted sessions and add an explicit reset action for conflicting Windows proxy settings
- fix: reduce system proxy startup delays and avoid false readiness-probe errors from sing-box
- fix: keep connection logs flowing after clearing the log, show v2rayN-style connection entries and suppress session-traffic counters and noisy repeats
- fix: keep the desktop UI and network cores responsive under heavy CPU load, with bounded core logs and deferred startup work
- fix: show effective direct/proxy routes for services and visually separate Zapret strategy columns
- test: cover AUTO probes, proxy recovery, live logging, startup responsiveness and core log limits

Full Changelog: https://github.com/krambovic/Lumen/compare/v1.9.17...v1.9.18
