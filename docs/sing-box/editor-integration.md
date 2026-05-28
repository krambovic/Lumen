# Text Editor Integration Plan For sing-box

## Problem Statement

Today the app stores routing intent in GUI fields and builds the final
`sing-box` JSON at connect time.

The next product direction is to move config authoring into a text editor
inside the app. If we do that naively and let users edit the raw runtime JSON
directly, we will immediately mix together two different kinds of data:

- author-owned intent;
- app-owned runtime materialization.

That split must be made explicit first.

Version baseline for this plan:

- target runtime: `sing-box 1.14.x`

So the editor design can rely on the modern route-rule model and should not be
shaped around deprecated `geoip` / `geosite` route fields.

## The Core Decision

The editor should not be designed as "a textbox over the exact final runtime
JSON" unless we are also willing to support placeholders and a materialization
pipeline.

The safer design is:

1. user edits source text;
2. app resolves selected node and local runtime fields;
3. app shows compiled runtime preview;
4. app validates the compiled result;
5. app launches with the compiled result.

This can still feel like "editing config in a text editor", but it avoids
making every transient runtime detail user-owned.

Chosen V1 direction:

- one text editor;
- template files in engine-native syntax;
- structural placeholders;
- compiled final preview;
- no per-field config GUI.

## Recommended Ownership Split

### User-owned

The editor may safely own:

- route rules;
- DNS server choices;
- DNS strategy and final server tag;
- whether unmatched traffic ends in `direct` or `proxy`;
- protocol-specific outbound details for expert mode;
- transport and TLS options that are part of the actual remote node intent.

### App-owned

The app should continue to own:

- generated TUN interface name;
- local protect port and password in hybrid mode;
- local SOCKS relay port used by hybrid mode;
- local Clash API listen address;
- loop-prevention rules for protected processes;
- loop-prevention rule for the active server endpoint in native mode;
- materialization of the selected `Node` into the `proxy` outbound;
- logging defaults and local debug knobs.

## Chosen Product Shape

The editor stores a template-like source document. The app injects the
runtime-managed fragments before launch and shows the compiled result in a
preview panel.

This is the preferred V1 shape for Bebra VPN because it keeps the UX text-only
without pretending that runtime-generated fragments are static user text.

## Practical Template Boundaries

For `sing-box`, these are the most useful injection points:

- `${APP_INBOUNDS}`
- `${APP_PROXY_OUTBOUND}`
- `${APP_ROUTE_RULES}`
- `${APP_CLASH_API}`

The concrete V1 placeholder rules are specified in
[template-format-v1.md](./template-format-v1.md).

## Validation Pipeline

For the future editor page, validation should happen in layers.

### Layer 1: text validation

- the document parses;
- the root type is correct;
- required arrays and objects exist;
- tags are unique where needed.

### Layer 2: app semantic validation

- the selected node exists;
- the selected node can be rendered in native or hybrid mode;
- required app-managed placeholders or slots are present;
- the document does not remove mandatory runtime tags used elsewhere by the
  app;
- TUN mode requirements are satisfied.

### Layer 3: compiled config validation

- the final materialized `sing-box` JSON is built;
- the final document is optionally formatted;
- the final document is checked by upstream validation before start.

This layered validation is important because parse success alone is not enough.
The app can still produce a broken runtime if tags or injection points do not
line up.

## Error Model To Expose In The UI

The editor page should distinguish these classes of errors:

### Syntax errors

- malformed JSON;
- wrong scalar type;
- duplicate keys if the parser reports them.

### App semantic errors

- unknown selected node;
- required outbound tag missing;
- template slot missing;
- hybrid-only fragment present in native mode;
- native-only assumptions used with an unsupported transport.

### Upstream config errors

- invalid field values;
- unknown transport option;
- bad regex in `process_path_regex`;
- invalid CIDR or address;
- invalid TLS structure.

These categories should not be collapsed into one generic "config error"
message.

## Migration Stages

### Stage 1

Keep current GUI as the source of truth, but add:

- export of generated `sing-box` runtime JSON;
- read-only preview pane;
- local documentation links from the editor page.

### Stage 2

Introduce a source editor for `sing-box` while still compiling from current
state models.

At this stage, the editor can be the preferred interface, but the app still
owns node materialization and local runtime fragments.

### Stage 3

Move `RoutingSettings` from form fields to stored text documents and keep only
small helper widgets around the editor.

### Stage 4

Apply the same ownership model to other engines:

- Xray
- tun2socks-sidecar generation
- Zapret presets and related text assets

This is where "all cores in a text editor" becomes a consistent product
feature instead of a one-off JSON textbox.

## Recommendation For The First Implementation

For the first in-app text editor iteration:

- keep `Node` storage unchanged;
- keep active-node selection unchanged;
- introduce a `sing-box` source document per profile or per routing preset;
- compile that source into the runtime JSON used today;
- show the compiled JSON in a preview panel;
- block launch if the compiled result fails app semantic validation or upstream
  validation.

This gives us the editor UX now without forcing a full rewrite of the runtime
pipeline.

## References

- [runtime-config.md](./runtime-config.md)
- [template-format-v1.md](./template-format-v1.md)
- [../template-editor-v1.md](../template-editor-v1.md)
- [../profile-format-v1.md](../profile-format-v1.md)
- `xray_fluent/singbox_config_builder.py`
- `xray_fluent/app_controller.py`
