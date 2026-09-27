## What's Changed

- fix: keep the TUN default route limited to unmatched traffic and show inherited service routes as “Default” instead of fake per-service overrides
- fix: make the blocked-only TUN preset send unmatched traffic directly, while retaining explicit user fallback choices
- fix: keep the built-in blocked-preset service routes active when a service is reset to “Default”
- fix: replace the 32-bit Cygwin runtime with the matching x64 DLL required by winws2, fixing Zapret startup for all strategies
- test: validate matching x64 architectures for winws2 and its runtime DLLs and cover the TUN fallback migration

Full Changelog: https://github.com/krambovic/Lumen/compare/v1.9.16...v1.9.17
