# A host's view of calling it

This page shows what actually goes over the wire, from the caller's side.
Everything here assumes the module is already loaded and configured; see
[installing-and-loading.md](installing-and-loading.md) and
[configuration.md](configuration.md) if it is not.

## The envelope every desktop and Jev member uses

Both success and failure travel in the same shape, `DesktopResponse`. There
is no separate error channel; `ok` tells you which of the other two fields is
populated.

A successful call:

```json
{
  "ok": true,
  "command": "click",
  "data": { "ref": "@s8f3k2p9:e1" }
}
```

A failed one, for example clicking a ref from a snapshot that is no longer
current:

```json
{
  "ok": false,
  "command": "click",
  "error": {
    "code": "STALE_REF",
    "message": "the ref @s8f3k2p9:e1 no longer resolves to an element",
    "suggestion": "take a fresh snapshot and use a ref from it",
    "recovery_hint": "retry",
    "platform_detail": null,
    "details": null,
    "disposition": "retryable"
  }
}
```

The only thing that ever comes back as a `TinyBus` transport-level error
rather than this envelope is the module failing to reach the point of
running a command at all, for example calling a member name the module does
not serve. Anything the *engine* rejects, a stale ref, a denied permission,
an application it cannot find, comes back as `ok: false` with a structured
`error`, never as a thrown exception on your side. Branch on `ok` and read
`error.code`, not on whether the call itself raised something.

## Observe, then act on what you observed

The whole interaction model in one example: take a snapshot, find an
element, click it by ref.

```json
// Find request
{ "app": "Safari", "role": "button", "name": "Save", "first": true }
```

```json
// Find response
{
  "ok": true,
  "command": "find",
  "data": {
    "elements": [
      { "ref": "@s8f3k2p9:e1", "role": "button", "name": "Save" }
    ]
  }
}
```

```json
// Click request, using that ref
{ "ref": "@s8f3k2p9:e1" }
```

The ref `@s8f3k2p9:e1` is bound to the snapshot it came from. `Click` either
reaches the exact element `Find` described, or fails with `STALE_REF` and
asks for a fresh snapshot. It will never click whatever has since moved into
that screen position, which is the entire point of addressing elements by
ref instead of by coordinates.

## Calling a Jev-driven member

`RunGoal` needs `jev` configured (see
[configuration.md](configuration.md#jev)) and is confidential, so send it
over whatever your TinyBus client library calls confidential delivery, not a
plain call.

```json
// RunGoal request
{
  "app": "Mail",
  "goal": "Archive every message from newsletters@example.com",
  "allowed_operations": ["click", "select"],
  "confirm_destructive": true
}
```

Without `jev` configured, this comes back immediately as:

```json
{
  "ok": false,
  "command": "run-goal",
  "error": {
    "code": "JEV_NOT_CONFIGURED",
    "message": "Jev must be supplied through private module configuration",
    "suggestion": "configure jev in the module's private configuration",
    "recovery_hint": "manual"
  }
}
```

## Calling a browser member

The 13 `Browser…` members take one object, the session beside the member's
own fields, and answer in the same `DesktopResponse` envelope the desktop
members use. `BrowserOpenSession` first, then act on the session it hands
back:

```json
// BrowserOpenSession request
{}
```

```json
// BrowserOpenSession response
{
  "ok": true,
  "command": "browser-open-session",
  "data": { "id": "s-1", "launched": true, "headless": true }
}
```

```json
// BrowserNavigate request
{ "session": "s-1", "url": "https://example.com/next" }
```

```json
// BrowserPerform request, clicking a ref BrowserSnapshot named
{
  "session": "s-1",
  "action": "click",
  "target": { "kind": "ref", "value": "e3" }
}
```

`BrowserScreenshot` hands back an output id rather than an inline image;
read it back in chunks with `BrowserReadOutput` until `eof`, then
`BrowserReleaseOutput`. `crates/tinycomputer/src/tinybus_module/test/browser.rs`
runs this exact open-navigate-click-close sequence over the in-memory bus.
See [docs/crates/tinycomputer-bus/browser.md](../tinycomputer-bus/browser.md)
for every field and error code.

## Calling a task member

Task members answer `AgentResponse<T>` instead of `DesktopResponse`: same
`ok`-then-`data`-or-`error` idea, but `error` here carries `code`, `message`,
`hint`, and `recoverable` rather than the desktop envelope's fields.

Start a task:

```json
// StartTask request
{
  "task": "Book the cheapest one-way flight from BLR to DEL next Friday",
  "facts": {
    "traveller_name": "Jordan Alvarez",
    "email": "jordan@example.com"
  },
  "constraints": {
    "payment": "stop_at_payment",
    "surfaces": ["browser"]
  },
  "budget": { "max_actions": 60 }
}
```

```json
// StartTask response
{
  "ok": true,
  "data": {
    "id": "task-7f2a",
    "status": { "state": "running" },
    "summary": "Searching for one-way flights BLR to DEL, next Friday",
    "progress": 0.05,
    "next": ["AwaitTask", "CancelTask"]
  }
}
```

`StartTask` is confidential, both because it is where secret facts (a card
number, say) enter the system as templates rather than literal values, and
because a plain-language `task` string can describe anything. Poll or block
with `AwaitTask`, using the id you got back:

```json
// AwaitTask request
{ "id": "task-7f2a", "timeout_ms": 15000 }
```

Eventually the task either finishes, or pauses and tells you exactly what it
needs, for example a payment checkpoint:

```json
{
  "ok": true,
  "data": {
    "id": "task-7f2a",
    "status": {
      "state": "needs_approval",
      "action": "Pay ₹4,820 for the selected IndiGo flight"
    },
    "summary": "Ready to pay; waiting for approval",
    "progress": 0.8,
    "next": ["ContinueTask", "CancelTask"]
  }
}
```

Answer it with `ContinueTask`, and this call is confidential for the same
reason `StartTask` is: it is the other place secret facts and approvals flow
in.

```json
// ContinueTask request
{ "id": "task-7f2a", "approve": true }
```

`TaskReport` (also confidential, since it can carry the full trace) gives you
the whole history once you are done:

```json
// TaskReport request
{ "id": "task-7f2a" }
```

## Discover what you can call before calling it

Rather than hard-coding this page's examples, call `Describe` first. It takes
no argument and needs no permission, and returns `Capabilities`: whether Jev,
the planner, and rescues are configured, which surfaces are usable right now,
every task member's JSON Schema, worked examples of its own, and a
`catalogue` listing every one of the module's 80 members (task, flow,
desktop, and browser alike) with its family and a one-sentence summary.
Building a host against `Describe`'s output rather than against fixed
assumptions means your integration keeps working if a future contract
version adds a member or a field, since `Capabilities.members` and
`Capabilities.catalogue` are generated from the same source this page's
`members.md` was written from.

## Next

[`../../giving-it-a-task.md`](../../giving-it-a-task.md) goes deeper on the
task lifecycle: facts and secrets, budgets, payment modes, and what pausing
actually looks like end to end. [members.md](members.md) is the full member
list this page draws its examples from.
