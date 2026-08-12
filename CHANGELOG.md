## What's Changed

- fix: unified the conflicting VPN/proxy dialog with the shared Fluent dialog layout
- fix: corrected conflict-dialog button alignment and spacing
- fix: stopped Lumen from identifying its own Xray or sing-box as a foreign VPN client
- fix: made conflict detection resilient to short PID snapshot races by matching executable paths
- test: added regression coverage for own-core false positives during restart

Full Changelog: https://github.com/krambovic/Lumen/compare/v1.9.7...v1.9.8
