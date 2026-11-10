# Rust port status (handoff for agents)

Read `PORTING.md` first (conventions). This file tracks *what is done* and
*what is left*, in dependency order. Update it in every porting commit.

Verification: `cd rust; cargo test -p lumen-core -p lumen-platform -p lumen-engines --lib`.
Last verified: lumen-core 198 tests, lumen-platform 11, lumen-engines 17
(cargo 1.99.1, Windows).

## Scope rule: engines stay in Go
xray-core and sing-box are NOT ported. They remain the original upstream Go
binaries (`xray.exe`, `sing-box.exe`) shipped next to Lumen. `lumen-engines`
only contains the Rust code that *manages* them: config builders, process
managers, operations, runtime planner, Clash API client, xray gRPC stats
client, core/resource updaters, zapret and discord proxy managers.

## Done

### lumen-core
- models, storage (DPAPI/passphrase), paths, path_utils, presets, constants
- i18n catalogs, release_notes, country_flags, traffic_history
- link_parser (all schemes, clash, json import, openvpn hook, wireguard)
- openvpn_import/normalization, wireguard_normalization, warp_generator
- config/: xray_builder, xray_inbounds, xray_fragments, multiplex,
  proxy_chain, bootstrap_dns, runtime_security (`bootstrap_dns.rs` = alias)
- crypt/: happ (+keys), incy
- subscription/: fetcher (safe redirects), presentation, text
- routing_presets, routing_rule_import, routing_runtime, node_transport,
  probe_capabilities, secret_scrubber, security
- http_utils: direct client (TUN bypass via /32 routes + raw UDP DNS,
  reqwest custom resolver), proxy client, proxy-first, safe redirects,
  connectivity_test, WarmHttpProbe (ports http_utils, direct_http,
  http_redirect_policy, connectivity_test, http_latency)
- log_utils, bounded_logs, logging (logging_setup)
- core_log_limiter: per-second quotas for normal and critical core logs,
  with summaries for lines withheld from the UI.
- diagnostics (bundle export + zip sanitizer), diagnostics_uploader
  (consent epochs, URL safety), pyjson (Python-compatible json.dumps)
- ping (ping_worker.py): tcp/udp/hysteria/http/icmp pings,
  `WindowsPingBypass` (= `_WindowsPingBypass`, also used by speed test),
  `PingWorker` with `run(&mut emit)` and `start()` -> `PingWorkerHandle`
  (events: `PingEvent::{Result, Progress, Completed}`)

### lumen-platform
- dpapi, idle, system_language, inet_settings, os_info
- subprocess (CREATE_NO_WINDOW, cp866/ANSI decoding, timeouts)
- windows_network (GetAdaptersAddresses), network_route_context (TTL cache,
  single flight), route_leases
- icmp: native IcmpSendEcho / Icmp6SendEcho2 via raw iphlpapi FFI
- runtime_priority: above-normal priority for owned cores and an opt-in
  UI priority raise that preserves any existing non-normal priority class.

### lumen-engines
- singbox/clash_api: `SingboxClashApiClient` (Bearer auth, selector switch
  with confirm + rollback, 8 MiB cap, quote(leaf, safe='')); 2 tests
  against an in-process fake API.
- xray/tun: `xray_wintun_dll` resolves the configured/default core location;
  `apply_xray_tun` adds the Wintun inbound to the existing Xray config and
  preserves explicit outbound bindings. It uses a pre-start physical route
  snapshot for interface binding and DNS, prepends the DNS/CIDR/QUIC rules,
  and validates imported config sections before mutation. The base Xray
  config builder is already in `lumen-core::config::xray_builder`.
- xray/stats_client: `XrayStatsClient::query` reuses a lazy loopback-only
  gRPC channel and reads inbound counters without resetting them. The port
  retains deadlines, retry backoff, cancellation on `close`, the 4 MiB limit,
  and strict counter validation. Diagnostic reasons use gRPC status codes.
  Tests cover the protobuf wire format and an in-process HTTP/2 service.
- xray/manager: `XrayManager` validates and writes runtime config, waits for
  local inbounds, retries once on readiness timeout, and stops the retained
  child. Lifecycle and rate-limited logs are available through `subscribe`.
  On Windows, stale-listener cleanup checks the executable path and keeps
  the process handle open while confirming the port owner. Runtime config
  replacement is atomic. `get_xray_version` queries the executable directly.
  Tests use a fixture executable and real Windows processes and sockets.

## Stubs still present (replace, do not keep)
- `lumen-core/src/speed_test.rs` (1 line). Decision: speed_test_worker.py
  depends on `native_test_config.py`, which needs the sing-box
  `build_singbox_outbound` and `_normalize_openvpn_outbounds` from
  `engines/singbox`. Port it into **lumen-engines** (`speed_test.rs` +
  `native_test_config.rs`) right after the sing-box config_builder and
  runtime_planner, then delete the lumen-core stub and its `pub mod`.
  `strict_proxy.py` maps to a reqwest client with `Proxy::all(url)` and no
  NO_PROXY bypass plus the safe redirect policy from http_utils.
- TODO in fetcher: "direct" transport should use `http_utils::direct_client`
  so subscription downloads bypass TUN like Python `DirectUrlOpener`.

## Left to port (Python source -> target crate)

### lumen-core
deeplinks, service_presets, process_presets, zapret_strategies,
zapret_server_scope, traffic_route_classifier, component_compatibility,
geodata_resources, active_profile_health, metrics_api,
state_snapshot, zip_utils, update_checker.

### lumen-platform
proxy_manager (WinINet system proxy, reuse inet_settings), startup
(schtasks/registry), single instance, power_reconnect, network_monitor,
win_proc_monitor, win_process_list, process_conflicts,
process_traffic_collector.

### lumen-engines (management of the Go binaries only)
xray: operations (221),
core_updater (719); sing-box: config_builder (1372), manager (1250),
operations (563), runtime_planner (2717);
core_resource_updater (1309),
zapret_manager (932), discord_proxy_manager (652), speed_test (796) +
native_test_config (97). Also provide the engine version probe used by
`diagnostics::collect_runtime_context`.

### lumen-app
app_controller (3285), application/* (node_service, connection_service,
worker_service, xray_runtime_service, profile_service, signature_service,
auto_switch_service, runtime_services, server_preflight), live_metrics_worker,
subscription_worker, app_updater (1173), qthread_utils equivalents.
`export_diagnostics` + upload: see app_controller.py:2970.

### lumen-ui (egui)
Pages from `qml_app/qml`: Dashboard, Nodes, Configs, Routing, Zapret,
Settings, Updates, History, Logs, About, Onboarding, dialogs; tray
(tray-icon/muda); theme; toasts. The view-model layer is
`qml_app/bridge/app_bridge.py` + `node_list_model.py` + `node_edit_helpers.py`.
`lumen-ui/src/main.rs` currently calls an empty `run()`.

## Known warnings to clean
happ.rs unused `cipher`; models.rs:624; paths.rs:459; traffic_history.rs
(`pending`, field `path`); storage.rs:33; i18n.rs `Value` naming;
diagnostics_uploader.rs SlotGuard.

## Tooling notes for agents
- Never put double curly braces in tool arguments (they get mangled);
  build strings with concatenation instead of `format!` escapes.
- Capture full cargo output with
  `cmd /c "cargo test -p lumen-core --lib > target\x.txt 2>&1"`.
- Tests that mutate the diagnostics upload epoch must hold
  `diagnostics_uploader::TEST_EPOCH_LOCK`.

## Release checklist (when everything above is done)
`cargo build --release -p lumen-ui`, run against a real xray/sing-box
core, compare generated configs with the Python app byte-for-byte, update
`windows/build*.py`/installer to ship the Rust binary.
