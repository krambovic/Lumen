## What's Changed

- fix: keep the active server selected when a subscription temporarily omits it or returns 304
- fix: report real subscription changes and name failed subscriptions without showing false success
- fix: harden system proxy restoration and avoid interfering with proxy settings Lumen does not own
- fix: recover slow or colliding TUN startup without deleting existing adapters
- fix: restore subscription banner accessors and prevent duplicate QML page initialization
- fix: improve updater, administrator relaunch and shutdown edge cases
- test: add guarded regression coverage for subscription reconciliation, proxy safety and TUN recovery
- ci: keep guarded Windows checks isolated before release builds

Full Changelog: https://github.com/krambovic/Lumen/compare/v1.9.12...v1.9.13
