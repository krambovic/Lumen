## What's Changed

- feat: added the optional VPN conflict guard to the Startup and tests settings, enabled by default
- fix: allow Lumen to start alongside other VPN/proxy processes when the guard is disabled
- fix: hardened portable startup and version-file handling after updates
- fix: repaired Windows autostart registration without silently changing the user's preference
- test: added regression coverage for the conflict guard and updater startup paths

Full Changelog: https://github.com/krambovic/Lumen/compare/v1.9.8...v1.9.9
