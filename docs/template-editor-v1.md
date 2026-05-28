# Template Editor V1

## Purpose

This document fixes the intended editor model for runtime engine configs in
Bebra VPN.

The design goal is explicit:

- use one plain text editor inside the app;
- use templates instead of GUI forms;
- do not rebuild engine configs from dozens of per-field widgets;
- let the app inject only the runtime fragments that cannot be authored
  statically.

## Non-Goals

V1 does not try to:

- build a form-based config designer;
- hide engine syntax from the user;
- invent a fake cross-engine schema;
- make the raw launched config fully user-owned when some parts are inherently
  runtime-generated.

## Core Model

Each engine config goes through three stages:

1. `Template`
2. `Compile`
3. `Final runtime config`

### Template

The user edits engine-native text with a small placeholder mechanism.

Examples:

- `sing-box` template: JSON
- `xray` template: JSON
- other engines: their own native text format if needed

### Compile

The app resolves placeholders using current runtime context:

- active node
- selected engine mode
- generated local ports
- generated TUN adapter name
- app-managed protection fragments
- local API listen addresses

### Final runtime config

This is the document passed to the actual engine process.

It may differ from the saved template because runtime-managed fragments are
materialized during compile.

## Why This Model

This keeps the product simple:

- user edits text, not forms;
- the app still owns dynamic runtime details that cannot safely be hardcoded;
- the same editor model can later be reused for multiple cores.

## Template Syntax

V1 uses a very small placeholder system.

### Rule 1

The template must remain valid in the engine's native syntax before
placeholder expansion.

For JSON-based engines, this means the template itself must be valid JSON.

### Rule 2

A placeholder is a full string value with this exact shape:

```txt
${NAME}
```

Examples:

```json
"${APP_PROXY_OUTBOUND}"
"${APP_ROUTE_RULES}"
"${APP_CLASH_API}"
```

### Rule 3

Embedded placeholders are not allowed.

Invalid examples:

```json
"proxy-${NODE_ID}"
"C:\\temp\\${NAME}.json"
"prefix ${APP_ROUTE_RULES}"
```

The compiler only replaces full placeholder nodes, not string substrings.

## Placeholder Contexts

V1 supports two structural placeholder modes.

### Value placeholder

Used where one JSON value is expected.

Example:

```json
{
  "clash_api": "${APP_CLASH_API}"
}
```

The compiler replaces that string node with exactly one JSON value, usually an
object.

### Array splice placeholder

Used as one element inside an array.

Example:

```json
{
  "rules": [
    "${APP_ROUTE_RULES}",
    {
      "domain_suffix": ["discord.com"],
      "outbound": "proxy"
    }
  ]
}
```

The compiler replaces that one string element with zero, one, or many array
items.

This lets templates stay valid JSON while still allowing the app to inject
multiple objects into arrays.

## Placeholder Resolution Rules

### Known placeholder only

Every placeholder name must be known to the selected engine compiler.

Unknown placeholder is a compile error.

### No implicit fallback

If a required placeholder is missing from the template, compile fails.

### Type-safe replacement

The compiler must reject placeholders inserted in a context that does not match
their replacement type.

Examples:

- array-splice placeholder used as an object value -> error
- value placeholder used where an array splice is required -> error

### No text substitution

Compilation should be structural, not naive string replacement.

For JSON engines, the intended flow is:

1. parse template JSON;
2. walk the tree;
3. replace placeholder nodes structurally;
4. serialize final JSON;
5. validate final JSON.

## Engine Contract

Each engine declares:

- supported template file format;
- supported placeholders;
- which placeholders are required;
- which placeholders expand to one value;
- which placeholders splice array items;
- which tags or structural invariants must exist after compile.

## Editor UX Requirements

The app editor page should stay minimal.

V1 needs:

- template list;
- text editor;
- syntax highlighting if available;
- validate button;
- final-config preview button;
- apply button;
- error list with line/column when available.

V1 does not need:

- generated forms for route rules;
- generated forms for outbound fields;
- special per-field editors for DNS, TLS, transport, or routing.

## Validation Pipeline

### Stage 1: template validation

- parse template in native syntax;
- collect placeholders;
- reject malformed placeholders.

### Stage 2: app compile validation

- active node exists if required;
- engine mode can be materialized;
- required runtime data exists;
- required placeholders are present.

### Stage 3: final engine validation

- compiled config is serialized;
- compiled config passes engine-specific checks;
- app-level structural invariants are still satisfied.

## Error Classes

The editor must report errors by class.

### Template syntax error

Examples:

- invalid JSON;
- duplicate key if parser reports it;
- malformed placeholder string.

### Compile error

Examples:

- unknown placeholder;
- missing required placeholder;
- active node missing;
- hybrid/native runtime mismatch;
- placeholder in wrong structural context.

### Final config error

Examples:

- engine rejects compiled config;
- required tag missing after compile;
- invalid regex, CIDR, or transport option.

## V1 Direction

This is the chosen V1 direction for config editing in Bebra VPN:

- one text editor;
- template files;
- engine-native syntax;
- small structural placeholder system;
- preview of compiled final config;
- no per-field GUI model.

## Related Documents

- [sing-box/README.md](./sing-box/README.md)
- [sing-box/editor-integration.md](./sing-box/editor-integration.md)
- [sing-box/template-format-v1.md](./sing-box/template-format-v1.md)
