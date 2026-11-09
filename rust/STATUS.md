# Rust port status (handoff for agents)

Read `PORTING.md` first (conventions). This file tracks *what is done* and
*what is left*, in dependency order. Update it in every porting commit.

Verification: `cd rust; cargo test -p lumen-core; cargo test -p lumen-platform`.
Last verified: lumen-core 178 tests, lumen-platform 7 tests, all green
(cargo 1.99.1, edition 2024).

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

### lumen-platform
- dpapi, idle, system_language
- subprocess (CREATE_NO_WINDOW, cp866/ANSI decoding, timeouts)
- windows_network (GetAdaptersAddresses), network_route_context (TTL cache,
  single flight), route_leases

## Stubs still in lumen-core (replace, do not keep)
- `diagnostics.rs`, `logging.rs`, `ping.rs`, `speed_test.rs` (1-line files).
- TODO in fetcher: "direct" transport should use `http_utils::direct_client`
  so subscription downloads bypass TUN like Python `DirectUrlOpener`.

## Left to port (Python source -> target crate)

### lumen-core
log_utils, logging_setup, bounded_logs, diagnostics, deeplinks,
service_presets, process_presets, zapret_strategies, zapret_server_scope,
traffic_route_classifier, component_compatibility, geodata_resources,
ping_worker (logic), speed_test_worker (logic).

### lumen-platform
proxy_manager (WinINet system proxy + registry), startup (schtasks/registry),
single instance, power_reconnect, win_proc_monitor, process_conflicts,
process_traffic_collector, ICMP ping.

### lumen-engines
xray: config_builder glue, manager, operations, tun, core_updater;
sing-box: config_builder, manager, operations, runtime_planner, clash_api;
xray_stats_client (tonic gRPC), core_resource_updater, zapret_manager,
discord_proxy_manager.

### lumen-app
app_controller, application/* (node_service, connection_service,
worker_service, xray_runtime_service, profile_service, signature_service,
auto_switch_service, runtime_services, server_preflight), live_metrics_worker,
app_updater, diagnostics_uploader.

### lumen-ui (egui)
Pages from `qml_app/qml`: Dashboard, Nodes, Configs, Routing, Zapret,
Settings, Updates, History, Logs, About, Onboarding, dialogs; tray
(tray-icon/muda); theme; toasts. The view-model layer is
`qml_app/bridge/app_bridge.py` + `node_list_model.py` + `node_edit_helpers.py`.
`lumen-ui/src/main.rs` currently calls an empty `run()`.

## Release checklist (when everything above is done)
`cargo build --release -p lumen-ui`, run against a real xray/sing-box
core, compare generated configs with the Python app byte-for-byte, update
`windows/build*.py`/installer to ship the Rust binary.
