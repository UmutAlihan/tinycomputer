# Errors and the envelope

## Every method returns a value, never a Rust error

Look at the signature of any member on `Desktop`:

```rust
pub fn click(&self, request: RefRequest) -> DesktopResponse;
```

Not `Result<DesktopResponse, Error>`. Just `DesktopResponse`. That is true
for all 54 members. A click that fails because the ref went stale, an
application that is not running, a permission that was not granted: none of
these are Rust errors here. They are ordinary return values, and the caller
is expected to look at them and decide what to do next.

## Why

A stale ref is not really a failure in the sense a caller cannot recover
from; it is a result that says "take a fresh snapshot and try that ref
again." A denied permission says "grant this setting, then retry." An
ambiguous application name (say, two windows both named "Untitled") says
"here are the candidates, pick one." Each of these carries specific,
structured information a caller can act on automatically. Turning them into
a string inside a `Result::Err` would throw that structure away, and a bus
adapter forwarding the error to a remote caller would have nothing left but
text.

So `Error`, the crate's actual Rust error type (in
[`crates/tinycomputer-desktop/src/error/mod.rs`](../../../crates/tinycomputer-desktop/src/error/mod.rs)),
is reserved for something much narrower: the module failing to even start a
command. Its three variants are a configuration blob that was not valid
JSON, a configuration field of the wrong type, and the engine failing to set
up a command context (for example, a session directory it could not open).
None of these describe anything about an application, a ref, or a
permission; they describe the module itself being unable to begin.

## What is inside a `DesktopResponse`

Every reply has the same shape:

- `command`: the name of the member that produced it, for example `"click"`.
- `ok`: whether the command succeeded.
- `data`: the result, when `ok` is true.
- `error`: a `DesktopError`, when `ok` is false.

And every `DesktopError` carries:

- `code`: a stable, machine-readable string such as `STALE_REF`,
  `PERM_DENIED`, `ELEMENT_NOT_FOUND`, `PLATFORM_NOT_SUPPORTED`, or
  `AMBIGUOUS_TARGET`. A caller matches on this, not on the message text.
- `message`: a human-readable description, mostly useful for logs.
- `suggestion`: a short, specific next step, such as "Grant Accessibility
  permission and retry."
- `recovery`: a hint describing the recommended strategy, whether the
  command is safe to retry, and whether it needs a fresh snapshot first.
- `platform_detail` and `details`: extra structured context specific to the
  error, such as the list of candidates for an `AMBIGUOUS_TARGET`.
- `disposition`: whether the action might have partially reached the
  application before failing, which matters for deciding whether a retry
  could double an effect (like sending a message twice).

```rust
use tinycomputer_desktop::{Desktop, RefRequest};

let reply = Desktop::new().click(RefRequest::new("@stale:e2"));

if !reply.ok {
    let error = reply.error.expect("a failed reply carries an error");
    // error.code, error.suggestion, error.recovery all describe what to do
}
```

## Where the mapping happens

The engine's own error type, `agent_desktop_core::AppError`, already carries
a code, a message, a suggestion, and a delivery disposition for the errors
that matter. Rather than re-deriving that mapping and risking it drifting
from the engine's, `tinycomputer-desktop` reuses the engine's own
`ErrorPayload` conversion and just relabels the fields into the contract's
types. That happens in
[`reply.rs`](../../../crates/tinycomputer-desktop/src/desktop/reply.rs),
in the function `desktop_error`. Every command funnels through
`Desktop::run` and `Desktop::run_with_report`
(in [`desktop/mod.rs`](../../../crates/tinycomputer-desktop/src/desktop/mod.rs)),
which is what guarantees every member produces the envelope the same way,
rather than each of the 54 methods building its own.

## This is agent-desktop's own wire format

`DesktopResponse` and `DesktopError`'s field names match, byte for byte,
the JSON the `agent-desktop` CLI already writes to stdout. That means a host
that already knows how to parse the CLI's output does not need a second
parser for the module's replies, and a response coming out of this crate can
be handed to an agent unchanged. See
`crates/tinycomputer-bus/src/envelope/mod.rs` for the full reasoning and
`docs/technical/specs/desktop-module-contract.md` for the pinned wire form.

## Related pages

- [Snapshots and refs](snapshots-and-refs.md) for the specific case of
  `STALE_REF`.
- [Permissions](permissions.md) for `PERM_DENIED` and why the check happens
  before a command runs rather than after it fails.
- [Catching mistakes](../../catching-mistakes.md) for how a flow or task
  built on top of this crate reacts to these errors automatically.
