# Repository Guidelines

## Project Structure & Module Organization

Lumen is a multi-platform application supporting Windows and Android:
- **`windows/`**: Windows Desktop Python/PyQt6 QML client (`run_qml.py`, `xray_fluent/`).
- **`android/`**: Android Jetpack Compose native client (`:app`, `:ui`, `:core:config`, `:core:database`, `:core:engine`, `:core:vpn`).

### Windows Client (`windows/`)
The main launcher is `windows/run_qml.py`, which enters `windows/xray_fluent/qml_app/main_qml.py`. Core application logic lives in `windows/xray_fluent/`: `application/` contains service orchestration, `engines/xray/` and `engines/singbox/` build and run proxy/TUN backends, and `qml_app/bridge/` exposes Python models and actions to QML. QML views are in `windows/xray_fluent/qml_app/qml/`.

### Android Client (`android/`)
Modular Jetpack Compose application built with Kotlin, Room DB, sing-box extended Go core, and Android VpnService.
- `:app`: Android application module, navigation, and viewmodels.
- `:ui`: Jetpack Compose UI components, design tokens, and screens.
- `:core:config`: Link parsing, AmneziaWG metadata normalization, sing-box and Xray config builders.
- `:core:database`: Room DB entities, DAOs, and migrations.
- `:core:engine`: Dual-engine manager and sing-box native driver interface.
- `:core:vpn`: Android `VpnService` implementation, split tunneling, and QS Tile service.

## Build, Test, and Development Commands

### Windows
```powershell
cd windows
pip install -r requirements.txt
python run_qml.py
pytest
python build_qml.py
```

### Android
```powershell
cd android
./gradlew assembleDebug
```

## Coding Style & Naming Conventions

- **Python (Windows)**: 4-space indentation, type hints, `snake_case` for functions/modules, `PascalCase` for classes and QML models.
- **Kotlin (Android)**: Standard Android Kotlin conventions, Jetpack Compose idiomatic state handling, `@Immutable` UI models, coroutine `StateFlow` streams.

## Commit Guidelines

Use Conventional Commit style subjects, for example `feat(android): add native domain routing screen`, `feat(repo): organize client modules into android and windows`, `chore: release version 1.8.1`.
