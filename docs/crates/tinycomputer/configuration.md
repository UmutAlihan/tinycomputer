# Configuration

The module reads its configuration from the TinyBus loader as a single JSON
object, both on first load and on any later reinitialization. Every top-level
key is optional. `null` and `{}` both mean "use the desktop defaults, with Jev
turned off", which is a working, if limited, configuration: everything except
the Jev-driven and task members still answers.

Configuration is delivered as TinyBus confidential, sensitive host-control
traffic, never as an ordinary bus call a monitor tool might print, and the
module never logs, traces, or echoes back an API key it was given. If a key
you send is the wrong type, or an object you send does not parse, the whole
reinitialization fails before it replaces the currently served object, so a
typo in a live reconfiguration cannot leave the module half-configured.

## `session_id` and `trace_path`

```json
{
  "session_id": "checkout-run-42",
  "trace_path": "/var/log/tinycomputer/checkout-run-42.jsonl"
}
```

Both strings, both for the underlying desktop engine. `session_id` names the
run for whatever bookkeeping the engine does per session; `trace_path` is
where it writes its own trace, separate from the Jev debug journal described
in [`../../technical/jev-journal.md`](../../technical/jev-journal.md). Leave
both out for an unnamed session with no trace file.

## `trace_strict` and `headed`

```json
{
  "trace_strict": true,
  "headed": false
}
```

Both booleans. `trace_strict` makes the engine fail loudly if it cannot write
the trace, rather than continuing without one. `headed` controls whether the
*desktop* engine's interactions that need a real cursor run with a visible
cursor. This is a different setting from the `cursor` key below, which is
about the on-screen overlay the agent draws to show where it is about to act,
and it is a different setting from `constraints.headed` on a task, which
controls the *browser's* visibility for that one task.

## `jev`

```json
{
  "jev": {
    "api_key": "sk-...",
    "provider": "open_router",
    "model": "jev-latest",
    "endpoint_url": null,
    "timeout_ms": 30000,
    "max_retries": 2,
    "sdk_name": "my-host"
  }
}
```

Only `api_key` is required inside this object; every other field falls back
to a documented default. `provider` picks which response contract the client
validates against: `type_safe` (the default, TypeSafe's first-party API),
`open_router` (OpenRouter's Jev-compatible decisions endpoint), or
`tiny_humans_open_router` (Tiny Humans' authenticated OpenRouter proxy).
`endpoint_url` overrides the provider's own route, but only to another route
the same provider publishes; it is not a way to point Jev at an arbitrary
host. `model` defaults to `jev-latest` when omitted.

## What happens without `jev`

Leave `jev` out and the module still serves every desktop member (`Snapshot`,
`Click`, `Launch`, and so on) exactly as before. What stops working:

- `ResolveIntent`, `RunGoal`, and `RunFlow` all reply with an error carrying
  the code `JEV_NOT_CONFIGURED` and the message "Jev must be supplied through
  private module configuration", rather than a `TinyBus` transport failure.
  You can still call them; you just get a structured no rather than a crash.
- Every task started through `StartTask` runs no further than its plan: the
  flow itself needs Jev to make any decision, so a task without Jev
  configured still reports what it tried and fails the same way.
- `Describe`'s `Capabilities.jev_configured` field reports `false`, which is
  the intended way for a caller to check this before starting a task rather
  than discovering it from a failed call.

`ValidateFlow` and `FlowGuide` need neither Jev nor the desktop, so they work
regardless.

## `planner` and `rescue_model`

```json
{
  "planner": {
    "api_key": "sk-or-...",
    "model": "anthropic/claude-sonnet-5",
    "rescue_model": "openai/gpt-6-luna"
  }
}
```

This is the one configuration key that unlocks plain-language tasks. Without
it, `StartTask` still runs, but only when given a `flow` directly rather than
free-text `task`; a plain-language `task` with no planner configured pauses
immediately asking the caller to supply a flow (`needs_plan`), because there
is nothing to turn the words into a plan.

`api_key` and everything else here is an OpenRouter credential and model
selection, not a TypeSafe or Jev one; the planner and Jev are configured
separately even though they might point at the same underlying provider.
`model` is the model the planner drafts a flow with; leave it out and the
engine's own default (`PLANNER_MODEL`) is used.

`rescue_model` rides on the same `planner` object because rescuing is the
planner's other job: when a task's step fails outright, it is handed to this
model for guidance before the task gives up on it, up to five times by
default (`TaskBudget.max_rescues`, per request, from 0 to 5; the module's own
default is also 5). Leave `rescue_model` out and the engine's default
(`openai/gpt-6-luna`) is used. Set `max_rescues` to `0` on a `StartTask`
request to turn rescues off for that one task without touching this
configuration.
[`../../technical/specs/task-rescue.md`](../../technical/specs/task-rescue.md)
covers the rescue mechanism itself.

## `browser.executable`

```json
{
  "browser": {
    "executable": "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
  }
}
```

Names the Chrome or Chromium binary a task's browser session should launch,
for when the platform's own discovery would not find one, for example a
headless container with Chrome installed somewhere nonstandard. Most
installs never need this: leave `browser` out entirely and the linked
`agent-browser` engine looks for Chrome itself. `browser` is only ever an
object with this one optional key; sending anything else under `browser` is
rejected rather than ignored.

This is independent of `constraints.browser_endpoint` on an individual
`StartTask` request, which instead attaches to an *already-running* Chrome
over its DevTools endpoint (for example the user's own signed-in browser)
rather than launching a fresh one.

## `cursor`

```json
{ "cursor": "brisk" }
```

or

```json
{
  "cursor": {
    "pace": "natural",
    "overlay": "/opt/tinycomputer/tinycomputer-cursor-overlay"
  }
}
```

Either form is accepted. As a bare string, it is one of `off`, `brisk`,
`natural` (the default when `cursor` is left out entirely), or `calm`. As an
object, `pace` is the same four choices and `overlay` is the filesystem path
to the `tinycomputer-cursor-overlay` helper that draws the moving cursor on
screen; release archives for macOS and Windows ship this helper beside the
module, and it is found automatically there without setting `overlay`
yourself. This one cursor is shared between the desktop engine and every
task's browser session, so a task that moves between an application window
and a browser tab shows one continuous cursor rather than two independent
ones. `off` draws nothing and every interaction runs exactly as it would
without a visible cursor at all.
[`../../technical/specs/virtual-cursor.md`](../../technical/specs/virtual-cursor.md)
covers the cursor's own design.

## Reinitialization

A host is free to send the module a new configuration object after the first
one, for example to rotate an API key or change the cursor pace mid-run. The
module builds the entire replacement service, including a fresh `jev`
runtime and task store, before calling `serve_at` again, so a configuration
that fails to parse or fails to validate never tears down the service that
was already running. Existing in-flight tasks and their workspaces are tied
to the old service instance and are not silently migrated onto the new one;
plan a reinitialization the way you would plan any other momentary service
restart.

## Next

[members.md](members.md) is the full list of what each of these
configurations lets you call, and [calling-it.md](calling-it.md) works
through actual request and response bodies.
