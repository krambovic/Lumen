# Lumen KVN

Lumen KVN is a Windows VPN/proxy client with Zapret integration, Xray/sing-box runtimes, routing presets, server management and a QML interface.

This project is a fork of `zapret-kvn`/`bebra-kvn`. Original copyright and license notices are preserved in `LICENSE` and `NOTICE.md`.

## Downloads

Releases are published at [krambovic/lumen-kvn](https://github.com/krambovic/lumen-kvn/releases).

- `LumenKVN-Setup-windows-x64.exe` - installer.
- `LumenKVN-portable-windows-x64.zip` - portable archive.

Lumen KVN is QML-only. There is no separate stable/nightly build in this repository.

## Build

```powershell
python build.py
```

The build script packages the QML application, copies runtime files from `core/`, bundles templates/assets and creates the installer and portable archive.
