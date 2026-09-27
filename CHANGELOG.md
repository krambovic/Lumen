## What's Changed

- feat: import Happ and INCY encrypted subscriptions without Node.js, with expanded crypt format and key support
- feat: preserve and edit imported sing-box configuration fields instead of replacing them with GUI defaults
- feat: import MagVPN subscriptions with both standard server links and Happ-compatible automatic groups
- fix: normalize Shadowsocks plugin settings and mux values across links, subscription formats and runtime configs
- fix: improve subscription refresh reconciliation, retain selected servers and avoid partial updates on provider errors
- fix: restore missing Zapret strategy dependencies and validate preset assets before launch
- fix: improve sing-box core packaging, runtime migration, routing and configuration compatibility
- fix: make subscription links easier to select and copy in the desktop UI
- docs: refresh English and Russian project guides and release links
- test: add regression coverage for encrypted imports, subscriptions, runtime configs and Zapret assets

Full Changelog: https://github.com/krambovic/Lumen/compare/v1.9.15...v1.9.16
