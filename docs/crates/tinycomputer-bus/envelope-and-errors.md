# The envelope and errors

Source: [`crates/tinycomputer-bus/src/envelope/`](../../../crates/tinycomputer-bus/src/envelope/)

Every desktop member, whether it clicks a button or lists open windows,
answers with the same shape: a `DesktopResponse`. Not a Rust `Result`, not a
raw value, always this one envelope. That choice is deliberate, and worth
explaining before the fields, because it shapes everything else in this
crate.

## Why one envelope, not a `Result`

A `Result` is fine when a failure is uninteresting: the operation either
worked or it did not, and the caller does not need to act differently
depending on how it failed. Desktop automation does not get that luxury. When
a click fails because the element reference has gone stale, the right move is
"take a fresh snapshot and try again." When it fails because a permission
was never granted, the right move is "ask the person to grant it." Those are
different next steps, and folding both into one generic error string would
throw away exactly the information a caller needs to pick between them.

So every reply carries a `DesktopError` on failure, not a string. `Error`, the
crate-wide Rust error type used elsewhere in tinycomputer, is reserved for a
different and rarer problem: the module failing to even start a command
(a transport hiccup, a decode failure). Anything a caller can act on belongs
in the envelope, never in that Rust error type.

## `DesktopResponse`

```rust,ignore
pub struct DesktopResponse {
    pub version: String,        // always "2.5" right now (ENVELOPE_VERSION)
    pub ok: bool,
    pub command: String,        // "click", "list-apps" — the engine's own spelling
    pub data: Option<Value>,    // present when ok
    pub error: Option<DesktopError>, // present when not ok
}
```

Exactly one of `data` and `error` is present, and which one is picked by `ok`.
`command` is always there, on success or failure, so a caller batching several
calls can tell which reply belongs to which request without keeping its own
side table.

A successful reply, straight from the crate's test suite:

```json
{
  "version": "2.5",
  "ok": true,
  "command": "list-apps",
  "data": { "apps": [] }
}
```

Note there is no `error` key at all, not `"error": null`. The type skips
absent optional fields on serialization, which keeps replies small and keeps
"absent" and "null" from meaning two different things on the wire.

## `DesktopError`

```rust,ignore
pub struct DesktopError {
    pub code: String,               // "STALE_REF", "PERM_DENIED", …
    pub message: String,            // human-readable
    pub suggestion: Option<String>, // what to do instead
    pub recovery: Option<RecoveryHint>,
    pub platform_detail: Option<String>,
    pub details: Option<Value>,     // code-specific structured detail
    pub disposition: Delivery,      // how far it got, and whether retrying is safe
}
```

Match on `code`, never on `message`. The message is for a person reading
logs; the code is the part that is guaranteed to stay `STALE_REF` and not
quietly become "Stale Ref" or get a comma added in front of it.

A real failure, taken verbatim from the crate's own test (it is literally the
JSON `agent-desktop click @s1:e2 --json` prints for a stale reference, which
is what a host actually has to parse):

```json
{
  "version": "2.5",
  "ok": false,
  "command": "click",
  "error": {
    "code": "STALE_REF",
    "message": "Ref @s1:e2 is no longer valid",
    "suggestion": "Take a fresh snapshot",
    "recovery": {
      "strategy": "refresh_snapshot_then_retry_original",
      "retryable": true,
      "requires_fresh_snapshot": true
    },
    "disposition": { "delivery": "not_delivered", "retry": "safe" }
  }
}
```

`RecoveryHint.strategy` is a named strategy string (like
`refresh_snapshot_then_retry_original`) rather than a bare boolean, because
"you can retry" and "you can retry, but only after re-snapshotting" are
different instructions and a caller should not have to infer the second one
from context.

## Delivery and retry: the question that actually matters

When a command fails, the question a caller really has is simple: "did this
already happen, and can I safely try it again?" Retrying a click that never
reached the button is harmless. Retrying a click that reached the button but
whose confirmation was lost could double-submit a form. `Delivery` exists to
answer exactly that question, and it deliberately keeps two related fields
rather than one, so a caller does not have to re-derive the mapping itself:

```rust,ignore
pub struct Delivery {
    pub delivery: DeliveryDisposition, // how far the command got
    pub retry: RetryDisposition,       // whether repeating it is safe
}
```

`DeliveryDisposition` has five states: `unknown` (the default; the engine
cannot say), `not_delivered`, `delivery_uncertain`, `delivered_unverified`,
and `delivered_verified`. `RetryDisposition` collapses those into the answer
that matters: `unknown`, `safe` (nothing happened, so retrying cannot
duplicate an effect), or `unsafe` (something may have happened, so retrying
could duplicate it). The mapping is fixed by `Delivery::of`: `not_delivered`
maps to `safe`; the three "delivered" variants all map to `unsafe`;
`unknown` maps to `unknown`. A caller checking `disposition.retry ==
RetryDisposition::Safe` never has to reason about the five delivery states
directly.

When a reply is decoded without a `disposition` field at all (an older
engine, or a hand-built test fixture), it defaults to `unknown`/`unknown`
rather than to some falsely reassuring "safe":

```json
{ "code": "INTERNAL", "message": "boom" }
```

decodes with `disposition == { "delivery": "unknown", "retry": "unknown" }`.

## Two version numbers, on purpose

`ENVELOPE_VERSION` (currently `"2.5"`, a string) describes the *shape* of
`DesktopResponse` itself: the field names, what is optional, what
`disposition` looks like. It tracks the underlying `agent-desktop` engine's
own output format, because this crate's envelope is that format, byte for
byte — a host that already parses that engine's CLI output needs no second
parser.

`CONTRACT_VERSION` (a `(u32, u32)` tuple, see
[Versioning and compatibility](versioning.md)) describes something different:
the member set and the payload shapes of this whole crate. The two version
numbers can and do move independently. Do not confuse a bump to one with a
bump to the other.

## Building responses and errors

The constructors on both types exist so calling code does not have to spell
out every field by hand:

```rust,ignore
DesktopResponse::ok("version", serde_json::json!({"version": "0.8.3"}));
DesktopResponse::err("click", DesktopError::new("STALE_REF", "ref expired"));
DesktopError::new("INVALID_ARGS", "amount must be positive").with_suggestion("use a positive number");
```

For the Agent (task) interface, the reply shape is different again —
`AgentResponse<T>` and `AgentError`, covered in
[The Agent and task types](agent-and-tasks.md) — because a task-level failure
needs a different vocabulary (a `hint` in one sentence, a `recoverable`
flag) than a desktop-level one does. The two envelopes are not meant to be
confused: `DesktopResponse` answers a desktop member, `AgentResponse`
answers an Agent member.
