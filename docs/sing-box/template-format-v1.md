# sing-box Template Format V1

## Purpose

This document defines the first template format for editing `sing-box` configs
inside Bebra VPN through a plain text editor.

This format is intentionally simple:

- the user edits text;
- the template stays valid JSON;
- the app injects only a few runtime-managed fragments;
- the final compiled JSON is what gets launched.

## Baseline

- target runtime: `sing-box 1.14.x`
- template syntax: JSON
- placeholder system: [../template-editor-v1.md](../template-editor-v1.md)

## Chosen Ownership Model

### User-owned in template

The template should own:

- `log` choices if the user wants to edit them;
- user-defined outbounds other than the active runtime proxy outbound;
- `route.final`;
- user route rules;
- DNS servers and DNS final selection;
- optional extra `experimental` sections except app-managed entries.

### App-owned at compile time

The app injects:

- TUN inbound fragments;
- hybrid protect inbound when needed;
- active-node proxy outbound;
- required runtime route pre-rules;
- required Clash API object.

This keeps the editor text-first without losing the ability to switch active
nodes and runtime mode safely.

## Supported Placeholders

V1 supports these placeholders for `sing-box`.

### `${APP_INBOUNDS}`

Context:

- array splice placeholder

Placement:

```json
"inbounds": [
  "${APP_INBOUNDS}"
]
```

Expansion:

- native mode: one `tun` inbound
- hybrid mode: one `tun` inbound plus one local `shadowsocks` protect inbound

### `${APP_PROXY_OUTBOUND}`

Context:

- array item placeholder

Placement:

```json
"outbounds": [
  "${APP_PROXY_OUTBOUND}",
  {
    "type": "direct",
    "tag": "direct"
  }
]
```

Expansion:

- native mode: converted active-node outbound tagged `proxy`
- hybrid mode: local SOCKS relay outbound tagged `proxy`

### `${APP_ROUTE_RULES}`

Context:

- array splice placeholder

Placement:

```json
"rules": [
  "${APP_ROUTE_RULES}",
  {
    "domain_suffix": ["discord.com"],
    "outbound": "proxy"
  }
]
```

Expansion:

- `sniff`
- `hijack-dns` for DNS protocol
- protected-process bypass rule
- native mode:
  - active server endpoint bypass rule
- hybrid mode:
  - protect inbound direct rule

Notes:

- this placeholder is intended for app-managed pre-rules;
- user rules come after it in the template array.

### `${APP_CLASH_API}`

Context:

- value placeholder

Placement:

```json
"experimental": {
  "clash_api": "${APP_CLASH_API}"
}
```

Expansion:

- one object containing app-managed Clash API settings, currently at least
  `external_controller`

## Required Placeholders

For V1, these placeholders are required:

- `${APP_INBOUNDS}`
- `${APP_PROXY_OUTBOUND}`
- `${APP_ROUTE_RULES}`
- `${APP_CLASH_API}`

Compile must fail if any of them is missing.

## Required Runtime Tags

After compile, the template must contain these tags and references:

- outbound tag `proxy`
- outbound tag `direct`
- outbound tag `block`
- DNS server tag `bootstrap-dns`
- DNS server tag `proxy-dns`

The template may define `direct`, `block`, `bootstrap-dns`, and `proxy-dns`
itself. The app only verifies that they exist and that references are valid.

## Recommended Minimal Template

```json
{
  "log": {
    "level": "warn",
    "timestamp": true
  },
  "inbounds": [
    "${APP_INBOUNDS}"
  ],
  "outbounds": [
    "${APP_PROXY_OUTBOUND}",
    {
      "type": "direct",
      "tag": "direct",
      "domain_resolver": "bootstrap-dns"
    },
    {
      "type": "block",
      "tag": "block"
    }
  ],
  "route": {
    "auto_detect_interface": true,
    "default_domain_resolver": "proxy-dns",
    "final": "proxy",
    "rules": [
      "${APP_ROUTE_RULES}",
      {
        "domain_suffix": ["discord.com"],
        "outbound": "proxy"
      }
    ]
  },
  "dns": {
    "servers": [
      {
        "tag": "bootstrap-dns",
        "type": "udp",
        "server": "1.1.1.1"
      },
      {
        "tag": "proxy-dns",
        "type": "tcp",
        "server": "8.8.8.8",
        "detour": "proxy"
      }
    ],
    "final": "proxy-dns"
  },
  "experimental": {
    "clash_api": "${APP_CLASH_API}"
  }
}
```

## Validation Rules

### Template-level

- template parses as valid JSON;
- placeholders appear only as full string values;
- placeholders appear only in supported structural contexts.

### Compile-level

- active node exists;
- active node can be rendered in native or hybrid mode;
- every required placeholder exists exactly once;
- no unknown placeholder exists.

### Final-config-level

- final JSON contains one or more inbounds from `${APP_INBOUNDS}`;
- final JSON contains `proxy`, `direct`, and `block` outbounds;
- `route.final` references a real outbound;
- `default_domain_resolver` and `dns.final` point to real DNS server tags;
- final JSON is accepted by the `sing-box` validator.

## What This Format Avoids

This format intentionally avoids:

- GUI field editors for route rules;
- GUI field editors for DNS;
- GUI field editors for transport and TLS;
- ad hoc text macros embedded inside arbitrary strings;
- direct editing of the exact launched config as the only source of truth.

## Migration Note

This format is the preferred V1 path for `sing-box`.

If the same editor model works well, other engines can adopt the same pattern:

- engine-native text
- small placeholder set
- app-managed compile stage
- final compiled preview

## Related Documents

- [README.md](./README.md)
- [runtime-config.md](./runtime-config.md)
- [editor-integration.md](./editor-integration.md)
- [../template-editor-v1.md](../template-editor-v1.md)
