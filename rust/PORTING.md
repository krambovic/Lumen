# Lumen Python → Rust Porting Conventions

All agents porting Python modules into the Rust workspace MUST follow these rules.

## Workspace layout

- `rust/crates/lumen-core` — domain logic: parsing, config building, storage, i18n, crypto, presets, subscriptions, diagnostics.
- `rust/crates/lumen-platform` — Windows OS primitives (registry, WinINet, Toolhelp, ICMP, DPAPI, power events, single instance, startup).
- `rust/crates/lumen-engines` — xray/sing-box process management, Clash API, gRPC stats, core updaters.
- `rust/crates/lumen-app` — controller/orchestration (headless; tokio).
- `rust/crates/lumen-ui` — egui frontend.

Python sources live in `windows/xray_fluent/`. Port modules 1:1 by file: `link_parser.py` → `crates/lumen-core/src/link_parser.rs` (or a module folder `link_parser/mod.rs` + submodules for big files).

## Code style

- Rust 2021, edition from workspace. `snake_case` functions, `PascalCase` types.
- JSON data is dynamic: use `serde_json::Value` (objects, arrays) exactly like Python dicts. Do NOT invent typed structs for xray/sing-box payloads unless stated.
- Reuse helpers from `crate::models`: `str_field`, `str_or`, `opt_str`, `int_field`, `opt_int`, `opt_f64`, `bool_field`, `str_list`, `obj_list`, `str_map`, `utc_now_iso`. Import `use crate::models::str_field;` etc. For nested JSON in engines use `serde_json::Value` directly with `.get()/as_str()/as_object_mut()`.
- Constants from `crate::constants` (e.g. `PROXY_HOST`, `DEFAULT_SOCKS_PORT`). Ports are `u16` in platform, `i64` in models (JSON).
- Errors: `anyhow::Result` for general, `thiserror` for typed error enums. Match Python exception names in error types.
- No stubs: every function must be a complete port of the Python logic, including defaults, normalization, error messages (Russian user-facing strings stay Russian, byte-identical where practical).
- Comments only for non-obvious logic; copy meaningful Python comments.
- Tests: port Python test expectations where cheap (`#[cfg(test)]`); at least one happy-path test per public function family.

## JSON parity

State/config files must remain byte-compatible with the Python client:
- Serialization key names identical (`to_dict()` → `json!({...})` with same keys).
- Numeric types: ports/MTU as `i64` in models, `u16` when passed to sockets.
- `Value::Null` ↔ Python `None`.

## Process/subprocess ports (engines/platform)

- Spawn cores with `std::process::Command` or `tokio::process::Command`; on Windows add `CREATE_NO_WINDOW` (0x08000000) via `CREATE_NO_WINDOW` constant through `CommandExt::creation_flags`.
- Console tools (`netstat`, `tasklist`, `taskkill`, `sc`, `schtasks`, `route`, `powershell`) run via `std::process::Command` with args; decode output as UTF-8 with lossy fallback.

## Windows APIs

- Use the `windows` crate (`workspace = true`) with only the features already listed in the workspace `Cargo.toml`. If you need a missing feature, ADD it to the root `rust/Cargo.toml` windows dependency list (alphabetical order) and mention it in your report.
- Registry: `winreg` crate.

## Building

- After writing, run `cargo check -p <crate>` from `rust/`. It MUST pass with zero errors before you finish. Fix all errors. Warnings are acceptable but avoid dead code by keeping only what's used.
- Do NOT edit `rust/crates/*/src/lib.rs` (the coordinator owns module registration) — create your file(s) only. Your module declaration will already exist in lib.rs.
- Do NOT run `cargo test` for the whole workspace; `cargo check -p <crate>` plus `cargo test -p <crate> <name filter>` for your own tests only.
- Commit nothing; the coordinator commits.

## Report format

End your run with: files created, public API surface (function signatures), any Python behavior intentionally simplified, missing features you could not port.
