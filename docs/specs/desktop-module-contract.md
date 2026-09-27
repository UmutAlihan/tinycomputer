# Desktop Module Contract

## Purpose

Expose the vendored `agent-desktop` automation engine over TinyBus so a host can
offer desktop observation and interaction to an agent as typed tool calls,
without that host compiling the engine or any platform accessibility backend.

## Scope

This repository is an adapter. The engine's behavior — how a tree is walked, how
a ref is allocated and resolved, what counts as actionable — is upstream and
out of scope. In scope: the wire contract, the conversion between it and the
engine's argument types, the permission preflight, and the bus surface.

## Contract

### Members

- The interface `ai.tinyhumans.tinydesktop.Desktop` is served at
  `/ai/tinyhumans/tinydesktop/Desktop` with exactly fifty-six members,
  enumerated in dispatch order by `tinydesktop_bus::names::METHODS`.
- Every member takes at most one request payload and returns a
  `DesktopResponse`. Members taking no argument: `ListDisplays`,
  `ClipboardClear`, `Version`, `Status`.
- Members are named in `PascalCase`, matching the engine's command names where
  Rust allows it. `Type` is renamed explicitly because `type` is a keyword.
- `KeyDown`, `KeyUp`, `MouseDown`, and `MouseUp` are served, validate their
  arguments, and then fail closed. Holding a button across calls is stateful and
  this module is not, so a success would be unobservably wrong. They are served
  rather than omitted so the refusal is a structured reply naming the
  alternative, and so the names are already reserved for a stateful daemon.
- Session lifecycle, trace read and export, and the engine's bundled skills
  loader are out of contract version 1.0. Adding a member is a minor bump, which
  the bind rule in `tinydesktop_bus::version` permits.

### Jev control

- `ResolveIntent`, `RunGoal`, and `RunFlow` require TinyBus confidential
  delivery; `ValidateFlow` and `FlowGuide` touch neither the desktop nor Jev.
  Flows are specified in [`jev-intent-flows.md`](jev-intent-flows.md). Jev
  configuration arrives through sensitive module initialization or
  reinitialization; the module never returns, logs, or traces its API key.
- Jev chooses only from module-supplied operations and compatible refs. Text is
  caller-supplied, ordinary field values are withheld by default, and a
  destructive result always stops for confirmation. A local check of the
  *target's* label also forces confirmation for delete, send, purchase,
  payment, submission, overwrite, unsafe quit, trash, and sign-out actions; the
  goal's wording no longer does, so "write and send" still opens the compose
  window without confirmation.
- Exact endpoint overrides are limited to the selected provider's published
  route. Accessibility content is labeled as untrusted data, and observation
  visits at most 4,096 nodes and 64 levels before returning a bounded view.
- Execution gates on the selected option's probability, not Jev's distribution
  concentration. Exact accessible names may add deterministic identity evidence
  but never bypass risk checks.
- Goal runs stop at 40 actions, 80 evaluations, or three turns that changed
  nothing (compared without refs, which every snapshot re-mints). A failed
  action or a low-confidence turn is fed back and retried up to `max_retries`
  (default 2, at most 5); an element that fails twice is not offered again.

### The envelope

- Both outcomes travel in `DesktopResponse`: `ok` selects between `data` and a
  structured `error`. A command failure is never a `TinyBus` error.
- A `TinyBus` error means a transport or dispatch failure — an unknown member,
  an undecodable frame — and nothing else.
- `DesktopError` carries the engine's own code, message, suggestion, recovery
  hint, platform detail, structured details, and delivery disposition. None of
  those may be flattened into the message.
- The envelope's wire form is byte-identical to the `agent-desktop` CLI's stdout
  envelope, version `2.3`, so a host needs one parser rather than two.

### The contract crate

- `tinydesktop-bus` depends on `serde` and `serde_json` and nothing else. It may
  not depend on a transport, an async runtime, an HTTP client, a native library,
  or the engine. CI asserts the resolved dependency tree.
- It mirrors the engine's enumerations by wire form rather than importing them,
  and those mirrors carry no `#[non_exhaustive]`, so the module crate's
  conversions are exhaustive and a variant added upstream fails the build.
- `tinydesktop` depends on it and re-exports all of it, so the two crates name
  the same types rather than structural twins.

### Permissions

- Each member declares what it needs: nothing, accessibility, screen recording,
  or both. `Screenshot` needs both only when it targets a named application or
  window, because that target is resolved through the tree.
- The need is checked against the platform's report before the command runs. An
  unpermitted accessibility call typically returns an empty tree rather than an
  error, which would otherwise surface as "the element is not there".
- A member needing nothing does not fetch a report.
- `Permissions` prompts only when its `request` field is set; `Status` and
  `Permissions` report a denied permission rather than refusing to run.

### Configuration and concurrency

- The module's configuration is a JSON object with optional `session_id` and
  `trace_path` strings, `trace_strict` and `headed` booleans, and a `jev`
  object containing provider configuration and its API key. `null` and `{}`
  yield desktop defaults with Jev disabled; invalid recognized fields fail
  before the served object is replaced.
- TinyBus sensitive initialization and reinitialization carry the `jev` object.
  Reinitialization constructs the complete replacement service before
  `serve_at`, so a rejected key, endpoint, or desktop field leaves the existing
  service intact.
- Commands run on a blocking thread pool, not on the connection's dispatch task,
  because a dense snapshot or a thirty-second wait would otherwise stall every
  other caller.

## Verification

- Every test passes on a machine with no display server, no granted permission,
  and nothing running.
- Payload types pin their serde representation; the conversions assert every
  enumeration variant by the engine's own spelling.
- The served interface is exercised over TinyBus's in-memory transport,
  including a member with a payload, a member without one, a member that fails
  closed, and an unknown member.
- `crates/tinydesktop-examples/src/bin/verify_module.rs` loads the compiled `cdylib` through the real
  dynamic loader and calls `Version` before a release archive is accepted.
- The generated dispatch table and the embedded module manifest are both
  asserted against `tinydesktop_bus::names::METHODS`.
