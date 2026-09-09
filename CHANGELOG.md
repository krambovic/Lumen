## What's Changed

- fix: fully support Shadowsocks SIP002, SIP003 and SIP008 links, JSON profiles and subscriptions
- fix: normalize legacy, AEAD and Shadowsocks 2022 cipher names before starting sing-box extended
- fix: preserve plugins, escaped options, native fields and credentials while editing or exporting Shadowsocks servers
- fix: reject corrupted Shadowsocks credentials and unsupported methods without breaking the remaining subscription or AUTO group
- fix: keep UDP-over-TCP and multiplex settings compatible and preserve dependency chains during hot-switches
- test: add cross-platform regression coverage for Shadowsocks parsing, editing, exporting and sing-box configuration

Full Changelog: https://github.com/krambovic/Lumen/compare/v1.9.13...v1.9.14
