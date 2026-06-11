# Lumen KVN QML

Lumen KVN now ships only the QML interface. The old QtWidgets edition was removed from source and release builds.

## Build

```powershell
python build.py
```

`build.py` delegates to `build_qml.py` and produces:

- `dist/LumenKVN-Setup-windows-x64.exe`
- `dist/LumenKVN-portable-windows-x64.zip`

The portable archive contains the `LumenKVN` directory and `LumenKVN.exe`.

## Release

The updater checks releases in [krambovic/lumen-kvn](https://github.com/krambovic/lumen-kvn/releases) and downloads the regular portable archive. There is no separate stable/nightly channel in Lumen KVN.
