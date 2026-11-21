# Rust port status (handoff for agents)

Read `PORTING.md` first (conventions). This file tracks *what is done* and
*what is left*, in dependency order. Update it in every porting commit.

Verification: `cd rust; cargo test -p lumen-core -p lumen-platform -p lumen-engines -p lumen-app -p lumen-ui --lib --locked`.
Last verified: lumen-core 264 tests, lumen-platform 68, lumen-engines 196, lumen-app 138, lumen-ui 144
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
- Sing-box imports and runtime share WireGuard/AWG/WARP endpoint normalization.
  Legacy profiles with a null address retain the `local_address` value.
- config/: xray_builder, xray_inbounds, xray_fragments, multiplex,
  proxy_chain, bootstrap_dns, runtime_security (`bootstrap_dns.rs` = alias)
- Xray DNS expands Lumen's local exclusion list through the shared routing
  parser. An IP-only list leaves the DNS fallback on the proxy resolver.
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
  Qt signals use `AppEvent` and queued tasks. Import and update calls go
  through the subscription dispatcher. Conflict detection excludes the
  active core processes and temporary measurement processes owned by the
  client, including cancelled workers that are still finishing.
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
- Visual parity with the QML client is checked with
  `rust/tools/uicmp/uicmp.py` (see "Visual comparison" below). Shared pieces:
  - The window is created transparent with a zero clear colour, so the DWM
    Mica/acrylic backdrop shows through the translucent `windowBase` veil as
    in `Main.qml`. Before this the veils were blended over eframe's dark
    default and every surface was about 11 levels too dark. Dithering is off
    because Qt paints flat fills.
  - `Font.DemiBold` text uses `seguisb.ttf` through `theme::semibold` /
    `theme::semibold_family`. egui's `RichText::strong()` only changes the
    colour, so it is not used for weight.
  - `shell/motion.rs` ports the QML `Behavior`/`NumberAnimation` timings
    (`follow`, `follow_color`, `toggle`, `progress`) and honours
    `Theme.animations`. Wired so far: rail width and burger, nav button
    fill/pill/ink, the `LazyPageLoader` fade + 6 px slide on page change and
    `chromeSettleAnim` (`ShellApp::chrome_settle`): when the onboarding card
    starts closing, the rail, the pages and their panels go from 0.35
    opacity and a 0.975 scale to 1 over 900 ms. The scale is a layer
    transform (`Context::set_transform_layer`). It does not run at start-up,
    as in the QML client.
  - `shell/widgets.rs` holds the hand-painted Fluent controls:
    `FluentButton` (`AccentButton.qml` with the colour and press-scale
    animations), `combo` (`FluentCombo.qml` field and flyout), `card_frame`,
    `section_title`, `BigValue`, `bordered_rect` (a QML border replaces the
    fill at the edge; egui strokes blend over it) and `StatusRing`
    (`StatusRing.qml` with the busy spin and settle animations).
    The dashboard and the routing preset toolbar use them. Other pages keep
    stock egui buttons and combos, restyled in `theme.rs` (small corners, no
    hover expansion, Fluent borders, accent open state, overlay scrollbars,
    110 ms widget animations) with the `widgets::combo_chevron` glyph painted
    over stock combos. Their hover/press colour animations are egui's, not
    the `AccentButton.qml` ones.
  - The settings and routing `Switch` follows the Universal style: the off
    state is a transparent track with a 2 px outline at 60% of the text
    colour and a knob at 80%; the knob slides in 120 ms.
  - Divider lines are allocated as exact 1 px items with a symmetric inset.
    They used to sit in a `horizontal` row, which added about 19 px per
    divider on the settings and routing pages.
  - Compared at 1280x760, 960x640 and 640x480 on the dashboard, routing and
    settings pages. Known gaps: the QML rail scrolls when the window is too
    short while the Rust rail keeps its fixed groups, and the dashboard
    search field overflows its card by a few pixels at 640 px width.
  - Window frame (`win_native::install_custom_frame`): the frameless
    window is subclassed for `WM_NCHITTEST` (6 px resize borders and
    corners) and `WM_NCCALCSIZE`. With an auto-hidden taskbar the work area
    equals the monitor, and a client that covers the monitor exactly is
    treated as fullscreen: it loses per-pixel alpha, so the backdrop turns
    black and stays black until the process restarts. The maximized client
    therefore keeps one spare row at the bottom.
  - Text: the QML client draws text and icon glyphs with ClearType (RGB
    subpixel antialiasing), which at 96 DPI has three times the horizontal
    resolution of egui's grayscale text and is why the port looked low
    resolution. `rust/vendor` patches epaint and egui_glow to rasterize
    glyphs into RGB coverage and blend each channel separately (dual-source
    blending, OpenGL 3.3); see `rust/vendor/README.md`. Stroke weight
    matches the QML client within 3% on the settings page
    (`target/uicmp/hb_inkcmp.py`). `apply_visuals` keeps linear coverage
    (no transfer curve, which would discard the colour channels) with
    sub-pixel glyph binning and hinting. Labels are not selectable.
    Text styles are reinstalled when the font scale changes
    (`theme::font_key`). Microsoft YaHei is the CJK fallback. Setting and
    routing rows scale their height with the font scale.
  - Bitmaps (navigation outlines, country flags) are rasterized at their
    device pixel size and drawn on whole device pixels. Minifying 72-96 px
    bitmaps with the bilinear sampler made them look jagged. Flags keep
    their aspect ratio and get the QML 20% black rounded border.
  - Controller access from the UI thread: a connect or disconnect runs on
    the controller thread and holds the controller while the cores start.
    Bridge getters called every frame (`profile_loaded`,
    `routing_preset_options`, `node_details`, `history_data`, Zapret and
    proxy-chain reads) check `controller_free()` and serve their last value
    while it is held. The check must not wait: a 4 ms timed lock per read
    added up to tens of milliseconds per frame during a connect. `pump()` still delivers status, transition, connection
    and log events then and defers the others. New per-frame getters that
    lock the controller must follow the same pattern, or the window freezes
    for the length of a connect. `target/uicmp/hb_lockdiag.py` builds a
    preview that logs every UI-thread wait over 30 ms with its call site.
  - Tray (`tray.rs`, `tray_menu.rs`): events are handled as they arrive and
    wake the hidden window; only the left button release toggles it, and the
    visibility comes from `IsWindowVisible` because egui's viewport info is
    stale while the window is hidden. The context menu (`tray_menu.rs`) is
    its own popup window, because Windows draws the frame, padding and
    square corners of a native menu itself. It paints the
    `_apply_menu_style` stylesheet (1 px border, 6 px padding, 34 px items,
    13 px separators, 210 px minimum width; 244 x 244 like the Qt menu),
    gets the small Windows 11 corner radius and shadow from DWM, opens the
    routing submenu 6 px beside its row, handles the arrow, Enter and Escape
    keys, and closes on deactivation or a press outside. The
    hidden-to-tray notice is a balloon on the tray icon
    (`QSystemTrayIcon.showMessage`); the tray window is subclassed to catch
    `NIN_BALLOONUSERCLICK`, which shows the window. The PowerShell toast is
    only the fallback.
  - Zapret page: the preset list is read from disk at most every 2 s
    (`AppBridge::zapret_presets`, cleared by every preset edit) and the
    binding pickers are rebuilt only when the settings, the server or a
    second change. Re-reading the files several times per frame cost about
    14 ms per frame.
  - Servers inspector: the subscription and server tabs follow
    `NodesPage.qml` (tiles, detail rows, ring captions, link style).
  - Page changes cross-fade like `LazyPageLoader`: every page follows
    `opacity: current ? 1 : 0` (220 ms) and a 6 px offset (260 ms) through
    `motion::follow`, and a page is rendered while its opacity is above
    zero. Leaving pages render through `pages::render_leaving_page` in a
    non-interactive area.
  - Settings tabs: `TabTransition` in `pages/settings.rs` is the
    `tabExit` / `tabEnter` state machine of `activateTab` (a click during
    the exit only changes the destination, a click during the enter starts
    a new exit from the current values). The earlier timestamp version
    swapped the page without an exit when tabs were switched quickly.
  - Diagnostics export (`pages/diagnostics_export.rs`) follows
    `DiagnosticsExportDialog.qml`: 540 px flyout, 20 px margins, 12 px
    blocks, title and subtitle rows with switches, divider, ghost and
    accent buttons, 40% black backdrop, and fresh defaults on every open.
  - `shell/wheel.rs` ports the `FluentScroll.qml` wheel glide (92 px per
    notch, 540 ms `OutQuint`) by rewriting wheel notches in
    `raw_input_hook`, so every scroll area follows it. Rendering is on the
    GPU through glow; a release build spends about 1 ms of CPU per frame
    on the settings page at 144 Hz. Measured 2026-11-13 with
    `target/uicmp/hb_perf.ps1` on a 12-core machine: idle 0.1-0.4% of total
    CPU (the logic tick repaints at 10 Hz), and wheel scrolling 1-3% on
    every page with 0.5-1.4 ms of CPU per frame and 7 ms frame gaps.
    During a sing-box proxy connect frames take about 0.9 ms (at most
    1.7 ms) and the status ring holds 144 Hz. Page switches produced no
    frame over 6 ms of CPU and no missed vsync (`hb_navcycle.py`).
  - Toasts and the file-drop overlay are painted on a foreground layer
    with explicit geometry. An auto-sized `Area` kept toasts at the widest
    width seen and covered only part of the window with the drop overlay.
    The subscription "updating" toast is reported once, by the dispatcher.
  - Onboarding (`OnboardingOverlay.qml`) was rebuilt on explicit grids: it
    starts below the title bar, which stays usable, and matches the QML
    card, step rail, logo mark and all six steps at 1280x760 and 640x480.
    Animations follow the QML: the card enters with opacity, a 0.94 scale
    and a 28 px offset (620 ms) and leaves in 380 ms while the backdrop
    fades after 140 ms and the dashboard settle animation restarts; steps
    cross-fade with a 56 px slide (300 / 520 ms); page content fades in
    from the top down over 1100 ms (`stagger_reveal`, which derives the
    QML `rv(order)` from each shape's position); the backdrop also covers
    the title strip. The card scale and the stagger are applied to the
    painted shapes of the layer after the frame is built. The backdrop is
    the blurred live UI (`shell/backdrop_blur.rs`): a paint callback copies
    the framebuffer below the overlay, builds mipmaps and draws a Gaussian
    over mip level 6 with the QML desaturation and `Theme.bg` tint; it
    matches the QML backdrop within one level at eight sample points.
    Without OpenGL 3.3 it falls back to a flat `Theme.bg`. The backdrop
    covers the whole window, and the title bar's caption and
    buttons are copied from its layer back on top (TitleBar.qml has z 50,
    the overlay z 40).
- Parity pass of 2026-11-20 (user report, checked against the QML client with
  `rust/tools/uicmp/uicmp.py`):
  - Links: eframe is built without egui-winit's `links` feature, so
    `Context::open_url` did nothing. `pages::about::open_url` goes through
    `win_native::open_url` (ShellExecuteW on a worker thread, http(s), tg and
    mailto only).
  - `link_parser::unquote` decoded every `%XX` byte on its own, so a name
    like `%F0%9F%87%A9%F0%9F%87%AA%20test` became eight replacement
    characters. Escaped runs are decoded together, as in Python.
  - Exit from the tray: `NetworkMonitor` slept a whole 5 s interval before
    looking at its stop flag and shutdown joined it on the UI thread. It now
    waits on a channel. `on_exit` hides the window and drops the tray icon
    before the shutdown work, and the controller logs
    `AppController shutdown finished in N ms (phase=ms ...)`.
  - Window corners: `DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND` is part
    of the backdrop ops (`MicaOp::DwmCornerRound`).
  - Subpixel text: the LCD shader lifts coverage by the luminance of the
    premultiplied colour, so muted (translucent) text no longer comes out
    7-9% heavier than in Qt (see `rust/vendor/README.md`).
  - egui 0.37 strokes circles outside the radius. Radio rings, step dots,
    swatches, the nav badge, the status ring track and the subscription
    traffic ring were drawn for a centred stroke and were too large.
    Pass `radius - stroke_width` for a border inside a QML circle and
    `radius - stroke_width / 2` for a track centred on an arc.
  - `widgets::busy_indicator` and `widgets::indeterminate_progress` port the
    Universal `BusyIndicator` and indeterminate `ProgressBar` from
    `qquickuniversalbusyindicator.cpp` / `qquickuniversalprogressbar.cpp`
    (phase tables and Bezier easings). Do not use `egui::Spinner`.
  - Onboarding: the flow opens once the window is visible (the dashboard
    shows first, as in Qt) and runs on its own clock that advances at most
    50 ms per frame; the first page takes about 0.3 s to build in a debug
    build and the entrance used to appear mostly finished.
  - `shell/fluent_menu.rs` ports FluentMenu / FluentMenuItem /
    FluentMenuSeparator (icons, hints, danger items, submenus that can act
    on a click, 90 ms fade). Used by the servers context menu, the Add and
    More toolbar menus, the dashboard server menu and the text-field menu.
    `fluent_menu::publish_theme` makes the frame's `Theme` (now `Clone`)
    available to widgets that are not given one.
  - The subscription import dialog and the node editor follow
    `importProgressDialog` and `NodeEditDialog.qml`.
  - Segoe UI Emoji is a font fallback after the UI fonts (Qt draws the
    regional indicators of flag sequences as small letters; egui showed a
    missing-glyph box).
  - The Updates tab probed `xray -version` and `sing-box version` on the UI
    thread on its first visit. `AppBridge::prefetch_component_versions` runs
    them on a worker thread from `start_deferred`.
  - Settings rows lay out right to left: the info icon goes after the
    control in code so that it appears before it, as in QML.
  - Servers page source picker (`pages/nodes/source.rs`) follows
    `sourceButton` / `sourcePopup`: title with the coloured subscription line
    (`Status::segments`, the QML `lineHtml`) or "Серверов: N", a 360 px popup
    aligned to the button's right edge with headers, active marker, usage
    bars, counts, the hover delete button for groups and the import / new
    group footer. `set_selected_subscription_id` now also updates the cached
    settings it is read from; the selection used to show only after an
    unrelated settings refresh.
- Second parity pass (2026-11-20):
  - `widgets::card_frame` returns a `Card` that ports the Card.qml hover:
    border `borderSolid` -> `divider`, a 1.7% overlay and a 3 px shadow at
    60% of `shadowOpacity` scaled by the card's alpha, over `animFast`.
    Dashboard cards are hoverable too (QML disables it there; requested).
  - Text placement (vendored epaint, see `rust/vendor/README.md`): glyphs
    align to the unrounded row height, half-pixel galley positions round up
    the screen, and rows made only of icon-font glyphs use that face's
    height. Labels and icons now sit at Qt's offsets in buttons.
  - `shell/fluent_dialog.rs` ports FluentDialog.qml (`FluentDialog::new`,
    `custom` for dialogs with their own layout, `text_field`). Note the
    Universal style's `verticalPadding: 18` wins over `padding`, so content
    bottom padding is 18. Ported onto it: dashboard modeHelp, About support
    and USDT, server QR, nodes group / info / delete group / move
    (`pages/nodes/dialogs.rs`), settings reset, Zapret delete / discard,
    network conflict, the routing dialogs and the configs discard overlay.
    The colour pickers moved to `shell/color_picker.rs` (see below).
  - `widgets::FluentTip::fluent_tip` replaces `on_hover_text`
    (FluentToolTip.qml: 450 ms delay, 100 ms fade, centred 4 px above).
  - Onboarding pages record explicit reveal blocks (`rv(ui, order, dy, ..)`)
    with the QML `rv(order)` values and slide distances instead of deriving
    the order from shape positions. The action bar has a fixed id scope:
    shifting auto ids restarted the button colour animations at every step
    change (the buttons flashed). Widgets whose animation state is keyed by
    their response id must not sit after content whose widget count varies.
  - `shell/frame_clock.rs` stamps frames with the next vblank from
    `DwmGetCompositionTimingInfo` so animation and wheel-glide steps are
    evenly spaced like Qt's vsync-driven animations.
  - `find_main_window` caches the shell HWND (it enumerated every window of
    the session several times per frame).
  - egui only draws the focused text-field border and caret while the OS
    window has focus; uicmp captures never show them.
- Visual check of the second pass (2026-11-20), side by side with QML:
  - Verified: onboarding step reveal and timing, the action bar (no flash),
    routing dialogs, Zapret delete, server QR (by source: the QML side
    cannot render QR without the `qrcode` module), info tooltips.
  - Settings rows lay out right to left, so multi-control rows listed their
    controls in reverse (password, backup, core paths, fragment fields,
    speed URL). Code order is now the reverse of the QML order, with a
    comment at each row. Core paths have their subtitles.
  - `StyledField` / `StyledArea` (settings): `Theme.card` fill, control
    height, 10 px padding (they used egui's near-black extreme background).
  - Two-column GridLayouts of `fillWidth` columns (DNS servers, regex
    filters) share the width in proportion to the columns' implicit widths;
    `layout_columns` reproduces that, and ColumnLayout's default 5 px
    spacing. Lone-title `SettingRow`s centre the title on the row.
  - One Universal `Switch` (`settings::switch_widget`) for settings, routing
    and onboarding: disabled is a transparent track with baseLow (20 %)
    border and knob, plus the hover / pressed colours. The tray row dims to
    0.55 while launch at login is off. The theme option reads «Авто».
  - `InfoIcon`: hover background and a 250 ms tip delay
    (`FluentTip::fluent_tip_after`).
  - `display_core_path` resolves relative paths against the app folder like
    Python's `resolve()`, so `core/sing-box-extended/sing-box.exe` shows as
    `core\sing-box.exe`.
  - Routing dialogs: `DnsField` is text height + 12 (not the 32 px control
    height) and the «Действие» label centres on the combo.
  - Zapret preset card: `padding: 0`, 14 px header margins, full-width
    divider, 10 px editor margins (the header was 16 px taller).
  - Dashboard hero: the top row is max(ring 52, text block), so it is 52 px
    until a server caption appears (it was fixed at 66, 14 px too tall).
  - Not checkable in captures: dialogs behind typed input (configs discard)
    and the network conflict dialog, since the test windows never have focus.
- Remaining popups and controls (2026-11-20), compared with QML:
  - `shell/color_picker.rs` ports ColorPickerDialog.qml for the settings
    accent (with «Системный»), the base tint (heading, darkness, `base|mute`
    source) and the onboarding custom accent. The SV square, hue and
    darkness bars are per-pixel textures with anti-aliased corners, so they
    blend like Qt's stacked gradients. Backdrop: Universal `baseLowColor`.
  - `pages/search_combo.rs`: `SearchCombo` (search field, keyboard
    navigation, opens upwards without room below, 90 ms fade) for the Zapret
    bindings and the proxy chain rows, and the speed-test preset flyout.
    Their areas are not constrained to the window, like Qt popups with
    `margins: -1`.
  - `FluentSpin` (130 px, ‹ / › steppers, centred editable value) replaces
    `DragValue` in all ten settings spin boxes; `PercentSlider` draws the
    Universal slider (2 px track, accent fill, 8 x 24 handle, step 5,
    commits while dragging) without egui's value box.
  - The SearchCombo list shows its scroll bar only on hover (egui), QML
    keeps it visible.
- Synced with `main` (2026-11-20): merged `origin/main` (desktop 2.1.4,
  `fix(windows): repair HTTP latency checks and AWG 3.x probes`) and ported
  the Python changes:
  - HTTP GET ping: a `Connection: close` response reconnects outside the
    timer instead of failing; `cp.cloudflare.com/generate_204` is tried when
    the primary target fails; cold samples are at least 1 ms; sing-box
    probes get the 12 s setup budget AUTO profiles had.
  - The connected AWG / WireGuard / WARP node is pinged through its health
    listener (`SpeedTestWorker::with_active_profile`), never with a second
    handshake on the same key.
  - `WarmHttpProbe` accepts `Connection: close` (an untimed GET reopens the
    tunnel, since reqwest cannot reconnect without a request).
  - ICMP no longer falls back to a TCP handshake.
  - Native test configs apply the AWG 3.x Windows bind workaround.
  - AWG links and .conf files keep `random_trailers` / `disable_cookies`.
  - Release notes and locales: «Пинг и AWG».
- Emoji and flags (2026-11-21):
  - Colour emoji: `epaint`'s `color_fonts` feature is on (lumen-ui
    Cargo.toml), so COLR glyphs from Segoe UI Emoji are painted in colour
    and ZWJ sequences are shaped, as Qt / DirectWrite draw them. Before, the
    monochrome outline was used, and Segoe UI Emoji's outline of a colour
    glyph is often a placeholder box: 🏳️ in server names showed as an empty
    rectangle. Regional indicators still render as small letters (verified).
  - Flag sprites are rasterized at 4x and box-averaged
    (`flag::SVG_SUPERSAMPLE`): resvg under-covered the US stars (canton
    brightness 0.27 vs Qt 0.33, now 0.325). Qt still draws them with a bit
    more contrast; its rasterizer cannot be reproduced exactly.
  - `flag::FlagBorder`: the 20% black border sits under the sprite where QML
    declares the image with `z: 1` (servers table, dashboard hero), over it
    in the dashboard server list, and is absent on onboarding cards. Rust
    drew it over every sprite, darkening the edge pixels by 20%.
- Startup subscription refresh (2026-11-21): Python `load` also refreshes
  every subscription 3 s after the profile opens
  (`QTimer.singleShot(3000, _on_sub_auto_update)`), not only on the
  recurring timer. Rust waited the full interval, so the subscription servers
  were not refreshed and moved to the end of the stored list as in Python.
  This made «Все серверы» list them in a different order when `sort_order`
  ties. Ported as `AppController::schedule_startup_subscription_refresh` plus
  `SubscriptionDispatcher::refresh_automatically`, which skips locked
  profiles and subscriptions in backoff. Verified on the same profile: the
  Rust list order now matches QML.
- Onboarding card border (2026-11-21): `cardBorder` is a child of
  `cardLayer` in QML and fades with it. Rust painted it through the layer
  painter, which ignores the card opacity, so a bright outline stayed visible
  while the card flew in and out. It now uses the card UI's painter;
  fly-out captures show the border fading with the card.
- Mica after a restart (2026-11-21): `main_qml.py` connects
  `_schedule_backdrop_refresh` (reapply now and after 150 ms) to
  `visibilityChanged`, `activeChanged` and `windowStateChanged`. Rust applied
  the backdrop once on the first frame, while the window was still hidden, so
  DWM could drop it and the light theme's translucent veil was left over
  black. The shell now reapplies it on the same window state changes. The
  user confirmed the light theme with Mica after a tray exit.
- Python test audit and differential corpora (2026-11-21): every Python test
  is mapped in `rust/TEST_PARITY.md` (753 matched, 58 ported, 86 existing, 57
  corpus, 78 n/a, 70 not ported with reasons). `rust/tools/parity` records
  every input the Python suite feeds the parser, the server editor and pure
  helpers, adds generated configs for every scheme/transport/security/format,
  and the Rust tests replay Python's results: parser 2115 cases, runtime
  configs 15891 (Xray, sing-box outbound and plan across presets, DNS, TUN,
  fragments, multiplex, hot switch), editor 6385, helpers 9609. Fixes found:
  JSON key order (serde_json `preserve_order`, `shift_remove` like `dict.pop`),
  sing-box group order, CPython base64/JSON errors, China-preset rules treated
  as user rules, IP-list form, VLESS/socks/http/naive/XHTTP/AWG share links,
  editor saves, scrubber JSON spacing, two-letter country codes, and a corrupt
  traffic history being overwritten. `rust/tools/env_matrix.py` starts the
  packaged build installed and portable, from Cyrillic/space and 212-char
  folders, with proxy environment variables, and in light Persian (RTL), dark
  Chinese and compact English profiles: all start in under 1 s (7 s on the
  first seeded run), stay up and write their data folder without panics.
- Version 3.0.0 (2026-11-21): the native client has its own version in
  `rust/Cargo.toml` (`[workspace.package]`) and the `APP_VERSION` fallback
  in `lumen-core/src/constants.rs`. `windows/build_rust.py` reads it from
  `Cargo.toml` instead of the Python `constants.py`, which stays at 2.1.4.
  The 3.0.0 "What's new" notes and their en/fa/zh translations live in
  `lumen-core`; a test checks the current version has 3-4 translated notes.

- Logs (`LogsPage.qml`) and `DiagnosticsExportDialog.qml`: searchable and
  selectable live entries, severity filters, autoscroll, clear marker and log
  actions. Export offers the original archive sections and consent-gated
  upload. The shell opens the output directory on successful export.
  The page uses the QML search and action row, severity buttons, right-aligned
  autoscroll switch and inset log card. Entry height follows wrapped text and
  optional actions. At the minimum window width the filter buttons reduce
  horizontal padding to keep the toolbar controls accessible. Compared the
  running clients in the dark theme at 1280×760 and 640×360 with the same
  sample entries, and exercised severity filtering, the autoscroll switch
  and opening the diagnostics dialog in the Rust client.
- History (`HistoryPage.qml`): period selection, refresh, reset, summary and
  independently scrolling session, daily and process tables with three-state
  column sorting. The page reads and clears the controller's live traffic
  history, using its existing asynchronous writer.
  The toolbar and proportional summary cards follow QML. Table cards keep
  fixed heights and pinned headings. Columns shrink to fit narrow windows.
  Daily totals map the storage tuple in upload/download order, with separate
  display and numeric sort values for each column.
  Compared both running clients with the same traffic history in the dark
  theme at 1280×760 and 640×360. Exercised period changes, sorting, scrolling
  inside a table and scrolling the page to the process table in Rust.
- Updates (`UpdatesPage.qml`): application and Xray channel selection,
  application release notes, and checks, installation, status and progress for
  the app, Xray, sing-box extended, GeoIP/GeoSite and droute. The page tracks
  update events while hidden and during Xray's check-to-install transition.
  Cards follow the QML version rows, right-aligned Stable/Beta selectors,
  Fluent action glyphs, 120-point release notes panel and 10-point progress
  bars. The channel selector test covers the application beta key and blocks
  clicks during an update. Compared both running clients in the dark theme
  with an available app update and 42% Xray progress at 1280×760 and with idle
  cards at 640×360. Scrolled to the droute card in the minimum-size Rust window.
- Configs (`ConfigsPage.qml`, `CodeEditor.qml`): independent sing-box and Xray
  drafts, active config and template selection, native JSON picker, template
  reset, discard confirmation, and formatting, validation, saving and applying
  with the existing controller. The editor has a fixed line-number gutter,
  current-line highlighting, error navigation and Ctrl+S.
  The core switch, aligned source fields and editor toolbar follow QML. The
  editor uses Consolas when installed, and the toolbar and status line stay
  visible at the minimum window size. Compared both clients at 1280×760 and
  640×360 with the same fixtures and checked core switching and validation.
- Routing (`RoutingPage.qml`): built-in and custom presets, LAN bypass, default
  TUN route, Discord Voice, application and service routing, and domain/IP rules.
  Includes the rule and preset dialogs, domain normalization preview, text and
  UTF-8 file import, and clipboard export. Reworked to the QML layout: the
  ghost help button sits on the title row, the preset grid stretches three
  columns (stacking below 850pt with the same icon-only toolbar row), the
  bypass/TUN/Discord rows pin their switches and combo to the right edge,
  process and domain tables use the 150pt action column with icon delete
  buttons and the 12% warning band. Compared both clients at 1280×760 and
  640×360 and toggled the bypass switch in Rust.
- Settings (`SettingsPage.qml`): reworked to the QML structure — the icon tab
  bar with the half-width accent underline and 1220pt label threshold, cards
  of SettingRow (glyph + title/subtitle on the left, controls pinned right),
  accent/base-tint swatches, 170pt percent sliders, pills, switches, info
  icons, the two-column regex/DNS grids and the danger reset button. All
  seven tabs render through the same setting-row helper. Compared both
  clients at 1280×760 and 640×360 and switched tabs in Rust.
- Zapret (`ZapretPage.qml`): preset search, preview, import, editing, rename
  and delete, manual start/stop, and searchable server-specific strategy and
  preset bindings. The page shares the plain-text editor with Configs.
  The desktop layout puts the preset list beside the preview and bindings,
  with the original sidebar width range and fixed-height preset rows. At the
  minimum width the cards stack so their actions remain reachable. Compared
  both clients in the dark theme at 1280×760 and 640×360 using the same
  presets. Checked selection, editor opening and scrolling to bindings in Rust.
  Re-verified after the routing/settings rework.
- About and onboarding: project links, support dialogs with the bundled USDT
  QR image, replayable first-run setup, and upgrade notes. Setup applies the
  selected language, appearance, routing, imports, WARP and system settings
  through the existing bridge. The onboarding overlay now mirrors the QML
  "window inside the window": full-window backdrop, 980×640 card with the
  272pt brand + stepper side panel (compact progress pills below 760pt),
  page headers, language pills, feature tiles, theme/palette/accent pickers,
  region and routing choice cards, toggle rows and the summary + support
  card on the last page, above the 76pt action bar. The About page follows
  QML spacing, link actions and support dialogs. Links wrap in narrow
  windows and the QR adapts to the available dialog height. Compared both
  clients at 1280×760 and walked every onboarding step in Rust.
- Dashboard geometry is laid out from the `DashboardPage.qml` numbers with
  painted rows instead of nested egui layouts: 20 px hero card with the
  66 px status column, 180 px connect button, segmented mode switch and
  right-aligned rates; pivot tabs with the animated underline; the quick
  connect header scrolling with the 44 px server rows; 96 px latency and
  session cards; animated process bars. At 1280x760 the overlay against the
  QML client differs only in glyph anti-aliasing (Qt uses ClearType).
- Dashboard: connect and mode controls, quick connect, searchable server list,
  batch test and server actions, shared protocol editor and QR dialog, live
  rates, latency history, traffic totals and the routing preset picker.
  The layout uses the QML server column width and resize breakpoints, group
  tabs, status ring and right-aligned transfer rates. The app traffic card
  fills the available height and the routing picker stays at the bottom.
- The shell keeps the title bar across the viewport, reserves space for the
  bottom navigation items and renders the five outline icons from
  `NavButton.qml`. The bundled Segoe icon font supplies the other glyphs.
- Nodes: sortable and resizable server table, selection and bulk operations,
  node and subscription inspectors, group and subscription dialogs, clipboard
  and file import, tests, and link, QR and JSON export.
  The layout follows `NodesPage.qml` with two toolbar rows, the original
  table column priorities and horizontal scrolling at the minimum window
  size. The 300-point inspector appears at a page width of 1100 points and
  keeps subscription actions at the bottom. Compared with the running QML
  client in the dark theme at 1654×940, 1060×700 and 640×360 with the same
  server and subscription fixture. An egui input test drags a header and
  checks the manual layout written to controller settings.
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

## Visual comparison
`rust/tools/uicmp/uicmp.py` (tracked) runs either client against an
isolated profile and captures it:
- `python rust/tools/uicmp/uicmp.py run rust|qml --profile NAME [--fresh |
  --clone OTHER] --size 1280x760 --shots 1.5,2 --out name`. LOCALAPPDATA and
  APPDATA point into `rust/target/uicmp/profiles/NAME`, so the real profile
  is never touched. Shots land in `rust/target/uicmp/shots/`. `stage` links
  `core`, `zapret`, `assets` and `data/templates` into `target/debug`.
- The QML client starts with `--side-by-side` (no single-instance mutex or
  pipe) under a Python whose PyQt6 matches the Qt minor pinned in
  `windows/requirements.txt` (`qml_python()`); the vendored QWindowKit DLLs
  need it. With another Qt (PyQt6 6.12 on this machine's Python 3.11) QWKCore
  fails with "entry point not found" and the QML window loses its custom
  frame: captures made that way show a 7 px border and cannot be dragged. On an empty profile it copies `windows/data` (state path
  migration); `--clone` a profile made by the Rust client to give both the
  same state. `--arg` passes extra client arguments (for example a
  `lumen://add?url=...` deep link).
- Rust input (debug builds): `--input "0.3,click,25,152;0.9,move,300,222;
  1.0,rclick,300,222"` is handed over through `LUMEN_PREVIEW_INPUT_FILE`
  when the window appears (`shell/preview_input.rs`), so input and captures
  share one clock. Coordinates are egui points; send a `move` before a
  right click so egui sees the hover first. QML input: `--post` posts
  window messages in client pixels; Qt places context menus at the real
  cursor position.
- `--mode print` (default) uses PrintWindow and works while the window is
  covered, but shows no DWM backdrop, corners or shadow, and it waits for
  the UI thread (a stalled frame delays the capture). The QML window is
  layered, and its first PrintWindow frames read lighter than on screen.
  `--mode top` makes the window topmost without activation for the grab;
  `--mode screen` brings it to the front. Both capture whatever else is on
  screen in `--margin`, so keep the margin at 0 on a machine in use.
- The `hb_*` helpers mentioned above (`hb_inkcmp.py`, `hb_lockdiag.py`,
  `hb_perf.ps1`, ...) lived in the untracked `target/uicmp` and were lost
  when `target` was cleaned; only `uicmp.py` is kept in the repository.
- Never stop clients by process name: the user's own Lumen may be running.
  The tool terminates only the process it started.
- `windows/scripts/create_side_by_side_shortcut.ps1` puts an elevated
  "Lumen (Python, admin)" shortcut on the desktop for manual comparison next
  to the Rust client. Both clients share `%LOCALAPPDATA%\Lumen\data`; the
  last one to save wins.

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
  The staged executable reported the expected version through the packaging
  probe and the update script's `--version-file` command. The update script
  waits for the GUI executable and passes the output path as a single
  argument, including spaces. The portable ZIP
  and Inno Setup installer both built with Xray, the patched sing-box extended
  core, its Cronet DLL for NaiveProxy, Wintun and the regional geodata bundle.
  Each core accepted its bundled default template.
- The Rust and Python clients generated equivalent JSON configurations for
  VLESS, Trojan, Shadowsocks and Hysteria2 in four proxy/TUN scenarios after
  masking the random Clash API secret. Both cores accepted all seven
  generated configurations. They also accepted four configurations with the
  blocked-only and except-Russia presets against the shipped geodata.
- Xray and sing-box extended both relayed a real HTTP request through an
  isolated local VLESS server from each of the generated SOCKS and HTTP
  proxy listeners.
- The portable update applied the release ZIP to an isolated profile. The
  script preserved the profile and its custom template, deployed the bundled
  resources and passed the new executable's version probe.
- Python-created plain, DPAPI and passphrase profiles loaded in Rust and
  remained readable in Python after being updated and saved by Rust.
- The release executable opened a visible, responsive desktop window from
  the extracted portable directory. Startup uses NUL-terminated UTF-16
  strings for Win32 calls, including registration of the `lumen:` protocol.
- The live TUN and Inno Setup upgrade scenarios require desktop verification.
