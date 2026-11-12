# Rust port status (handoff for agents)

Read `PORTING.md` first (conventions). This file tracks *what is done* and
*what is left*, in dependency order. Update it in every porting commit.

Verification: `cd rust; cargo test -p lumen-core -p lumen-platform -p lumen-engines -p lumen-app -p lumen-ui --lib --locked`.
Last verified: lumen-core 261 tests, lumen-platform 67, lumen-engines 195, lumen-app 138, lumen-ui 113
(cargo 1.99.1, Windows). Note: lumen-app tests that toggle or observe the
diagnostics consent epoch hold `lumen_app::test_support::CONSENT_LOCK`
(shutdown() revokes consent globally; the globals are process-wide).

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
- subscription/: fetcher (safe redirects), presentation, text, HTTP request and
  response parsing. The response parser keeps conditional cache headers,
  decodes link lists, preserves structured JSON for the node parser, and merges
  quota, Happ directives and provider metadata in Python precedence order.
  Request preparation resolves Happ and Incy links, expands converter templates,
  validates HWID values and builds the provider client profiles.
- routing_presets, routing_rule_import, routing_runtime, node_transport,
  probe_capabilities, secret_scrubber, security
- http_utils: direct client (TUN bypass via /32 routes + raw UDP DNS,
  reqwest custom resolver), proxy client, proxy-first, safe redirects,
  connectivity_test, WarmHttpProbe (ports http_utils, direct_http,
  http_redirect_policy, connectivity_test, http_latency)
- log_utils, bounded_logs, logging (logging_setup)
- traffic_route_classifier: classifies Clash and runtime chains by outbound
  type, selector choices and detours. Recorded members take precedence over
  later selector switches. Mixed, cyclic and unrecognized routes are unknown.
- core_log_limiter: per-second quotas for normal and critical core logs,
  with summaries for lines withheld from the UI.
- component_compatibility: pinned policy digest, minimum engine versions
  and required geodata codes.
- update_checker: HTTPS release feeds, channel selection and SHA-256 metadata;
  update downloads use idle read timeouts and proxy-to-physical-route fallback.
- zip_utils: checks all member paths before extraction, including Windows
  drive, device and reparse-point paths.
- diagnostics (bundle export + zip sanitizer), diagnostics_uploader
  (consent epochs, URL safety), pyjson (Python-compatible json.dumps)
- ping (ping_worker.py): tcp/udp/hysteria/http/icmp pings,
  `WindowsPingBypass` (= `_WindowsPingBypass`, also used by speed test),
  `PingWorker` with `run(&mut emit)` and `start()` -> `PingWorkerHandle`
  (events: `PingEvent::{Result, Progress, Completed}`)
- geodata_resources: RuNetFreedom archive entries and the China/Iran raw
  GitHub rule-set feeds; `singbox_rule_set_path` lands inside
  `singbox_rule_set_dir()`.
- state_snapshot: `snapshot_state(&AppState)` payload + cloned settings;
  payload keeps raw UTF-8 (Python used `ensure_ascii=True`, content equal).
- deeplinks (deeplinks.py): `lumen://` parsing with Python-compatible
  `urlsplit`/`parse_qs`/`unquote` helpers, instance-message encode/decode;
  the Python tests were ported 1:1.
- metrics_api: `ClashApiClient` over a raw loopback HTTP/1.1 socket —
  bounded reads, deadline per chunk, `close()` cancels the active read,
  non-200 bodies scanned for unsupported-transport markers. Reasons carry
  Python exception names (TimeoutError/OSError/BadStatusLine/...).
- active_profile_health: `probe_active_profile` (async: the warm probe is
  reqwest-based) with strict no-NO_PROXY proxy GET, safe redirect policy
  and the UNKNOWN/FAILED semantics of the Python module.
- process_presets, service_presets: preset tables 1:1 (`icon` is the FIF
  name as a string).
- zapret_strategies: blob registry (74 entries incl. zero64), lua extension
  map, catalog parsing with mtime/size cache and local overrides, strategy
  binding helpers, `node_server_kind`. Shipped-catalog tests resolve the
  repo checkout via `CARGO_MANIFEST_DIR` (runtime path stays `base_dir()`,
  the exe directory).
- zapret_server_scope: preset->server profiles (`--ipset-ip`, handshake-only
  payloads, `--new` merging of WinDivert port filters), strategy scopes,
  node transports and the 24h resolve cache. `make_server_scope_in` adds a
  catalog_dir override for tests; `make_server_scope` matches Python.
- New dependency: `unicode-general-category` (workspace + lumen-core) for
  `clean_strategy_name` (So/Sk/Cs/Co/Cf filtering).

### lumen-platform
- dpapi, idle, system_language, inet_settings, os_info
- subprocess (CREATE_NO_WINDOW, cp866/ANSI decoding, timeouts)
- windows_network (GetAdaptersAddresses), network_route_context (TTL cache,
  single flight), route_leases
- icmp: native IcmpSendEcho / Icmp6SendEcho2 via raw iphlpapi FFI
- runtime_priority: above-normal priority for owned cores and an opt-in
  UI priority raise that preserves any existing non-normal priority class.
- win_process_list: native Toolhelp snapshot of running executable names.
- single_instance: keeps the `Local\Lumen.SingleInstance` mutex for the
  lifetime of the primary. A user-only Windows pipe accepts bounded messages
  and recovers from abandoned clients. Secondary launches verify the pipe
  owner's user before forwarding and can notify the legacy LumenKVN pipe.
  Retry deadlines preserve the ordinary and elevated relaunch behavior.
- startup (startup.py incl. single-instance rebrand cleanup): legacy tree
  removal, Run + StartupApproved entries, elevated schtasks with UTF-16-BOM
  XML and the ONLOGON fallback, `list2cmdline` (exact CPython algorithm),
  `relaunch_as_admin` and the always-run-as-admin AppCompatLayers toggles.
  Note: the Python test expecting `WorkingDirectory == C:\Program Files`
  is stale; the module (and the port) emit `C:\Program Files\Lumen`.
- power_reconnect: `WindowsPowerEventFilter` state machine (suspend/resume
  arming, duplicate-resume guard, busy retries, shutdown marking) and
  `power_broadcast_event`; Qt timer modeled as `schedule`/`fire_timer_if_due`.
- network_monitor: `NetworkMonitor` with the UDP-to-8.8.8.8 fingerprint
  (fallback "0.0.0.0") on a worker thread.
- win_proc_monitor: GetExtendedTcpTable + Get/SetPerTcpConnectionEStats
  with the 96-byte ROD struct, creation-time PID cache, estats retry
  backoff, core exclusions.
- process_conflicts: Toolhelp snapshot with tasklist fallback, descendant
  PID walk, netstat LISTENING parsing, conflict tables, foreign system
  proxy scan, `scan_network_conflicts` -> JSON, terminate helper.
- process_traffic_collector: full aggregation engine (baseline deltas with
  `max_delta` clamp, route totals, closed-connection carryover, hidden
  cores, top-host, speeds, epoch invalidation). Cross-crate glue is trait
  injected: lumen-app supplies `CoreServices` forwarding to
  `lumen_core::metrics_api::ClashApiClient`, `RouteClassifier::new` and
  `i18n::tr`. `by_proc`, closed-only processes and top-host ties keep the
  Python dict insertion order; collector tests share `TEST_STATE_LOCK`
  (pytest runs sequentially, Rust tests do not).
- New dependency: `url` added to lumen-platform (system-proxy URL parsing).
- proxy_manager: WinINet LAN proxy on the mixed port, version-2 recovery
  receipts compatible with Python, and PID/creation-time ownership checks.
  Automatic restoration verifies matching settings and preserves static
  proxy, PAC and auto-detect flags. Rust serializes mutations with a
  runtime-directory lock and archives the receipt after a verified explicit
  reset. Optional Firefox-family integration preserves unrelated preferences
  and keeps restoration pinned in `user.js` until the browser exits.
  Tests simulate registry mutations and use isolated browser profiles.

### lumen-engines
- singbox/manager: `SingBoxManager` (tokio broadcast `SingboxEvent`
  replaces Qt signals) with config validation, retry loop classified by
  Wintun race (2 attempts for WARP/MASQUE, 3 for plain TUN, 1 for
  proxy-only), pre-launch PowerShell hook, TUN rotation/ready waits,
  bounded runtime logs via `CoreLogLimiter("sing-box", 20, 10)`,
  `get_singbox_version` for diagnostics, orphaned TUN adapter cleanup,
  hot TUN-name rotation `singbox_tun-{pid:x}-{hex6}`. Fixture exe tests:
  38. Unconditional Python errors at start() surface as `Error` events
  instead of uncaught OSError.
- xray/operations + singbox/operations: start_proxy/start_tun/start_
  runtime/restart/hot-switch flows behind the `OperationsController`
  trait (lumen-app implements it; lumen-engines cannot depend on lumen-app).
  `try_hot_switch_selector` keeps every Python guard with the
  rollback-to-restart fallback; `validate_runtime_plan` keeps the 5-iteration
  prune loop; Russian user-facing strings are byte-identical. The deferred
  planner test (`test_planner_runtime_error_is_reported_as_a_connection_error`)
  landed here, plus the hot-switch validation trio and xray flow tests: 9.
- singbox/config_builder: full outbound conversion for every scheme —
  TLS/uTLS/certificate pins, ws/grpc/httpupgrade/xhttp transports with
  their alias tables and `extra` merging, Shadowsocks-2022 key sizes,
  vmess security, vless vision, SIP003/v2ray plugins, multiplex.
  `build_singbox_outbound(&Node, tag, multiplex_enabled, concurrency)` is
  the speed-test entry point. 24 tests.
- singbox/runtime_planner (module folder): `plan_singbox_runtime` with
  `PlanRuntimeOptions`, hybrid planning, xray sidecar plans, proxy chains
  and hot-switch selectors, endpoint DNS bootstrap caches, DNS/TUN runtime
  contracts (legacy UDP->DoH migration, fakeip/hosts, reject rules),
  metrics contract, `node_runtime.rs` matrix (`is_xray_exclusive_node`).
  `normalize_openvpn_outbounds` lives in `runtime_planner/sidecar.rs` (the
  speed-test worker imports it there). Python object-identity dials are
  expressed via `DialPath`. 53 tests; the one Python test that needs
  `operations.start_runtime` moves to operations once that module lands.
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
- xray/core_updater: GitHub stable/beta/nightly release selection and custom
  HTTPS feeds, SHA-256 verification (asset metadata or `.dgst`), and
  candidate version and compatibility checks before stopping services.
  `XrayCoreUpdateWorker` reports progress, waits for an explicit disconnect
  acknowledgement, and supports cancelling HTTP reads and the acknowledgement
  wait. Installation stages Xray and optional Wintun, preserves Lumen's
  geodata, saves `.exe.bak`, and rolls back on replacement failure.
- singbox/document: inspect editor text and parse a runtime document with
  its source path, SHA-256 text hash and proxy-outbound flag.
- core_resource_updater: startup checks and update worker for regional Xray
  geodata, sing-box rule sets, sing-box extended/Cronet and Droute. Checks
  archive hashes and rule-set files, coordinates disconnect before installing
  sing-box, and cancels stalled HTTP reads.
- zapret_manager: winws2 lifecycle and events, preset parsing and editing,
  referenced resource checks, server-scoped rules and WinDivert cleanup.
- discord_proxy_manager: Droute bundle discovery and installation, Discord
  variant detection, legacy marker migration, and enabling/disabling the
  local SOCKS proxy through Discord's Droute payload.
- speed_test: `SpeedTestWorker` runs speed and cold/warm HTTP latency tests
  through temporary Xray and sing-box processes. It uses strict proxy HTTP,
  supports AUTO profiles and reuses the native sing-box builder through
  `native_test_config`. The former `lumen-core` stub is removed.

### lumen-app
- app_controller (module folder): the Python AppController ported whole —
  `AppEvent` signal enum, queued `ControllerTask` event loop with worker/
  subscription/metrics/resource polling and deadline timers, transition
  machinery, session capture, persistence with all state migrations and the
  save queue, startup/admin reconciliation, config/template path resolution
  for both engines, the full connection flow incl. the Zapret group (trusted
  DoH pool, server binding, autostart), proxy/Discord wiring, subscriptions
  dispatch, workers, logging filters, master-password lock, diagnostics and
  shutdown. Implements `OperationsController`, `RuntimeController`,
  `ConnectionController`, `ShutdownController` and `PowerReconnectController`
  so the ported flows plug in unchanged. 15 controller tests. Documented
  deviations: Qt signals -> `AppEvent` + queued tasks, import/update calls
  go through the subscription dispatcher (event-reported), and
  `owned_core_process_pids` returns an empty set (managers keep handles
  private; path-based ownership remains the working signal).
- health_listener: authenticated loopback probes for both cores. A new
  sing-box probe uses its target selector's configured default and adds a
  pinned route only when the classifier confirms a proxy path.
- xray_runtime_service: inspects loaded profile text and builds runtime JSON
  from it and the selected node in `AppState`. Reports repairs to stored
  nodes for persistence, applies proxy chains and GUI routing, and preserves
  the observatory and balancer of full AUTO profiles. Replaces imported API
  services with a loopback StatsService and adds authenticated, outbound-pinned
  health probes for supported selected proxies. The full AUTO test compares
  the resulting JSON with the Python service output.
- config_profiles, config_documents, profile_service: resolve profile paths
  under each engine's directory, select, import and reset templates, and
  sync a newer template with a `.bak` copy. Writes replace files atomically
  and preserve Windows text line endings. The sing-box document cache tracks
  path, modification time and size. Editor apply mirrors an explicit
  `route.final` to routing settings and emits state-save and transition
  events when required by the connection context.
- session_state, transition_engine: active-session snapshot and the rules for
  connect, disconnect, system-proxy updates, Xray and sing-box hot swaps, and
  reconnect. Tests cover blocked transition signatures, action priority, and
  the sing-box selector in both TUN and system-proxy mode. `XrayRuntimeConfig`
  is already defined in `xray_runtime_service`.
- node_runtime_service: selects the core and fastest eligible node, repairs and
  validates stored outbounds, and resolves country codes. Xray runtime setup
  uses the shared validation and reports repairs for persistence. The country
  resolver groups duplicate hosts and runs one cancellable task per controller.
- signature_service: routing, transition, Xray and TUN signatures from the
  selected node, settings and existing profile inspections. Keeps Xray-layer
  signatures independent of system-proxy toggles and sing-box editor TUN
  signatures independent of the selected node. Signatures use sorted compact
  ASCII JSON. Proxy-chain hops without a link use Python-style `repr` in the
  Rust JSON model's map order.
- auto_switch_service: counts distinct fresh HTTP failures for the active
  profile and separately tracks sustained download speed drops. Keeps the
  replacement in the current group, preserves cooldown and attempt limits
  across automatic reconnects, and emits selection, save and transition
  events in Python order. Cooldown starts at the first actual switch because
  Rust uses a process-relative monotonic clock.
- update_service: owns the Xray update worker and forwards progress, results
  and status events. An apply request on an active connection checks first
  and reuses the live proxy for installation. The worker replaces files only
  after a disconnect acknowledgement. The service requests reconnect after
  completion when it stopped a connection. Shutdown can cancel and join the
  active worker.
- server_preflight: validates XHTTP and Reality settings, native endpoint
  fields, and full sing-box and Xray AUTO profiles. `leastPing` and
  `leastLoad` require an observatory. Classic proxy protocols retain the
  Xray fallback after a native conversion error. `prepare_connection_node`
  repairs and validates the node before checking the configured core.
- live_metrics_worker: polls authenticated Clash or Xray stats on a dedicated
  thread. Reuses the Clash connections snapshot for process totals and work
  logs, counts only active Xray inbound tags, and filters implausible proxy
  process counters. Health polling stays active when process details are
  hidden and resets on profile changes. The poll delay includes the Windows
  thread CPU budget and a cooldown for slow local API responses.
- single_instance: launcher API wraps the Windows transport and existing
  deep-link codec. The primary receives typed activation, tray and deep-link
  requests. A true autostart stays in the tray, while a relaunch or incoming
  deep link requests foreground activation.
- diagnostics_service: captures state, recent logs and upload consent before
  queuing a background export. Uses the existing redacted ZIP builder and
  engine version probes. Upload is optional and tied to the captured consent.
  The archive name includes a unique suffix for concurrent exports.
- node_service: manual and file-reference imports, node edits, group moves
  and deletion, ordering and selection. Uses Unicode case folding to deduplicate
  links within groups. Group deletion also removes linked subscriptions and
  their nodes. Emits save and connection transition requests for the controller.
- subscription_service: prepares downloaded text with include/exclude filters
  and reconciles it with the saved nodes. Refreshes preserve identities and
  unchanged measurements, reject partial parses, and retain a missing active
  node. The preparation is bound to the input text and filter settings.
- subscription_worker: downloads an ordered import/refresh batch and prepares
  nodes off the async executor. Negotiates provider profiles and sends an
  install-scoped Windows HWID or the configured override. Handles conditional
  responses, provider notices and retries, resolves Happ AUTO placeholders and
  requires both MagVPN representations before preparing nodes. Reports scrubbed
  errors per job and cancels pending network work on stop.
- subscription_cache and subscription_lifecycle: build refresh jobs from saved
  settings, apply worker results through NodeService, and request a save after
  committing nodes and records in memory. Keep validators and quota on HTTP 304,
  force a full fetch after a parser revision change or local node removal, and
  honor Retry-After when scheduling automatic refreshes. Successful refreshes
  merge provider metadata, preserve node identities, and handle URL migration.
- subscription_dispatcher and subscription_refresh_timer: queue manual imports,
  deep links and refreshes on one worker, apply results on the state owner, and
  emit import status, node deltas and localized notices. The recurring timer
  skips locked profiles and subscriptions in backoff. Cancellation and shutdown
  drop queued batches and active network work. The dispatcher exposes a deadline
  and a cancellation-safe receive for the application event loop.
- worker_service and probe_targets: select endpoint or full-profile measurement
  targets, including Xray balancers and nested sing-box selectors, and run
  TCP/ICMP, HTTP and real latency, speed, and connectivity checks. The service
  applies current-worker results on the state owner, retains bounded history,
  emits progress and save requests, and keeps cancelled workers until cleanup.
- connection_service and runtime_services: validate initial connections, route
  proxy and TUN startup through the existing Xray and sing-box operations,
  capture runtime session data, and coordinate disconnect and reconnect.
  Handle unexpected core exit and fail-closed proxy behavior, metrics and
  traffic history, and ordered shutdown with one worker deadline. The
  controller interfaces expose the state, events and manager hooks used by
  these services.
- app_update_check: discovers stable and prerelease Windows releases, selects
  a setup or portable asset, reads its SHA-256 from release metadata or a
  sidecar, and prevents automatic installation of a rollback.
- app_updater (module folder): the download/install/restart half. Segmented
  range downloads over a proxy/plain/direct opener with the shared safe
  redirect policy, cancellation-aware chunk reads and flushed files before
  assembly or SHA-256 verification. The elevated PowerShell scripts resolve
  portable markers, legacy paths and the uninstall registry. Portable updates
  merge bundled resources into the existing data directory before replacing
  program files. Both scripts stop only named processes under the current
  or target installation directory. ShellExecuteW "runas" launches the
  scripts, and the worker reports progress through its event channel.

### lumen-ui
- view_models: the bridge and node editor, node/process/log models, history,
  configs and Zapret helpers. The bridge consumes the existing AppEvent
  stream, including subscription imports, diagnostics, WARP and update checks.
  The controller pump keeps manager subscriptions alive between iterations.
- shell: eframe entry point, themed window chrome and navigation, tray and
  toasts, saved window geometry, Windows power handling, single-instance
  activation and deep links. PageContext carries the bridge, theme, and
  page events. Country flags and the window/tray icon are available in the
  standalone binary.
- Logs (`LogsPage.qml`) and `DiagnosticsExportDialog.qml`: searchable and
  selectable live entries, severity filters, autoscroll, clear marker and log
  actions. Export offers the original archive sections and consent-gated
  upload. The shell opens the output directory on successful export.
- History (`HistoryPage.qml`): period selection, refresh, reset, summary and
  independently scrolling session, daily and process tables with three-state
  column sorting. The page reads and clears the controller's live traffic
  history, using its existing asynchronous writer.
- Updates (`UpdatesPage.qml`): application and Xray channel selection,
  application release notes, and checks, installation, status and progress for
  the app, Xray, sing-box extended, GeoIP/GeoSite and droute. The page tracks
  update events while hidden and during Xray's check-to-install transition.
- Configs (`ConfigsPage.qml`, `CodeEditor.qml`): independent sing-box and Xray
  drafts, active config and template selection, native JSON picker, template
  reset, discard confirmation, and formatting, validation, saving and applying
  with the existing controller. The editor has a fixed line-number gutter,
  current-line highlighting, error navigation and Ctrl+S.
- Routing (`RoutingPage.qml`): built-in and custom presets, LAN bypass, default
  TUN route, Discord Voice, application and service routing, and domain/IP rules.
  Includes the rule and preset dialogs, domain normalization preview, text and
  UTF-8 file import, and clipboard export.
- Zapret (`ZapretPage.qml`): preset search, preview, import, editing, rename
  and delete, manual start/stop, and searchable server-specific strategy and
  preset bindings. The page shares the plain-text editor with Configs.
- About and onboarding: project links, support dialogs with the bundled USDT
  QR image, replayable first-run setup, and upgrade notes. Setup applies the
  selected language, appearance, routing, imports, WARP and system settings
  through the existing bridge. The egui modal uses a dimmed backdrop.
- Dashboard: connect and mode controls, quick connect, searchable server list,
  batch test and server actions, shared protocol editor and QR dialog, live
  rates, latency history, traffic totals and the routing preset picker.
- Nodes: sortable and resizable server table, selection and bulk operations,
  node and subscription inspectors, group and subscription dialogs, clipboard
  and file import, tests, and link, QR and JSON export.
- The dashboard and server table render the original flag sprite, with the
  existing stripe and emoji fallbacks for countries outside the sprite.
- The Windows GUI records Rust panics and window initialization failures in
  the crash log. The next primary launch imports the previous report into
  the application log.
- Settings: appearance, networking, startup and tests, subscriptions, DNS,
  updates and data, including the native file selectors, color and reset
  dialogs, profile passwords and backup handling.

## Left to port (Python source -> target crate)

All ten Windows pages and the onboarding flow are wired to the Rust shell.
The Windows release and prerelease workflows package the Rust executable
through `windows/build.py`. The portable archive and Inno Setup installer
carry the same executable, bundled cores, assets and templates, with the
`LumenKVN.exe` alias for updates from older installs.

## Known warnings to clean
None — the lumen-core list was cleaned in the same pass as the direct-route
fetcher fix (2026-11-10). lumen-platform/lumen-engines/lumen-app warnings
are handled inside their porting commits.

## Tooling notes for agents
- Never put double curly braces in tool arguments (they get mangled);
  build strings with concatenation instead of `format!` escapes.
- Capture full cargo output with
  `cmd /c "cargo test -p lumen-core --lib > target\x.txt 2>&1"`.
- Tests that mutate the diagnostics upload epoch must hold
  `diagnostics_uploader::TEST_EPOCH_LOCK`.
- Subagents run at most 2 concurrently (account limit); resume failed
  agents with SendMessage after a slot frees.

## Release verification
- The Windows SDK build embedded the manifest, icon and version metadata.
  The staged executable reported the expected version. The portable ZIP
  and Inno Setup installer both built using real Xray and sing-box extended
  binaries, and each core accepted its bundled default template.
- The Rust and Python clients generated equivalent JSON configurations for
  VLESS, Trojan, Shadowsocks and Hysteria2 in four proxy/TUN scenarios after
  masking the random Clash API secret. Both cores accepted all seven
  generated configurations.
- Xray and sing-box extended both relayed a real HTTP request through an
  isolated local VLESS server from each of the generated SOCKS and HTTP
  proxy listeners.
- The portable update applied the release ZIP to an isolated profile. The
  script preserved the profile and its custom template, deployed the bundled
  resources and passed the new executable's version probe.
- The live TUN, full desktop launch and Inno Setup upgrade scenarios require
  desktop verification.
