# Errors: what a caller should do next

Code: `crates/tinycomputer-browser/src/error/mod.rs` (the `Error` enum),
`crates/tinycomputer-browser/src/reply/mod.rs` (`classify`, which maps
agent-browser's own error strings onto it), `crates/tinycomputer-bus/src/
browser/errors/mod.rs` (the published wire names).

## The taxonomy is about the remedy, not the cause

`Error`'s variants are not a map of *where inside this crate* something
went wrong — a caller on the other side of the bus cannot use that
information anyway. They are a map of *what the caller should do about it*,
which is the one distinction that actually survives the trip across the
bus. That is deliberate enough to be written into the crate's own doc
comment: every variant maps to exactly one published wire name in
`tinycomputer_bus::browser::errors`, and that mapping (`Error::wire_name`)
lives in this crate rather than at the bus boundary, so a new variant can
never be added without someone deciding, on the spot, what a host is
supposed to see and do about it.

## The variants

| Variant | Wire name | What a caller should do |
|---|---|---|
| `InvalidInput` | `InvalidInput` | Fix the request — an unparseable URL, an empty expression, a screenshot quality outside 1–100. |
| `NoSuchSession` | `NoSuchSession` | Open a new session; this one is gone. |
| `NoSuchElement` | `NoSuchElement` | Take a fresh snapshot and choose again. |
| `StaleRef` | `StaleRef` | Same remedy as above, spelled out explicitly: the ref belonged to an earlier reading of this page. |
| `NotActionable` | `NotActionable` | The element exists but could not be acted on right now — covered, disabled, or off-document. The message names the obstruction where the browser could identify it. |
| `Timeout` | `Timeout` | The operation ran out of time; retrying with a longer deadline is reasonable. |
| `BlockedByPolicy` | `BlockedByPolicy` | Never retry. The session's `allowed_origins` refused this destination, and the answer will not change. |
| `BrowserUnavailable` | `BrowserUnavailable` | Not something a caller can fix by choosing differently — this is a host or deployment problem (no browser could be launched or reached). |
| `PageError` | `PageError` | The page itself raised a JavaScript exception, or the browser rejected a command. |
| `NoSuchOutput` | `NoSuchOutput` | The held screenshot or PDF being asked for is unknown or has expired. |
| `LimitExceeded` | `LimitExceeded` | A bound was hit: too many sessions, too many held outputs, or an output larger than the module will hold. |
| `ConnectionLost` | `NoSuchSession` (same wire name as `NoSuchSession`) | Open a new session. Kept as its own Rust variant — rather than folded into `NoSuchSession` at construction time — because it says something more specific ("the transport died") that is useful for the crate's own diagnostics, even though a caller across the bus is told exactly the same thing either way: don't retry into a socket that will never answer, open a fresh session instead. |
| `ModuleFailed` | `ModuleFailed` | Anything that does not fit the categories above. |

`errors::is_agent_recoverable` (in the bus crate) is the one further
decision that gets made on top of this table: whether a model can plausibly
recover by choosing differently, versus needing a human or an operator.
`InvalidInput`, `NoSuchElement`, `StaleRef`, `NotActionable`, `Timeout`, and
`PageError` are recoverable this way; `NoSuchSession`,
`BlockedByPolicy`, `BrowserUnavailable`, `NoSuchOutput`, `LimitExceeded`,
and `ModuleFailed` are not.

## Where the classification actually happens

agent-browser's own replies do not carry a taxonomy — they carry
`{success: false, error: "<sentence>"}`, a human-readable message and
nothing else. `reply::classify` (`reply/mod.rs`) is where those sentences
get sorted into the table above, matched against the engine's actual
message texts: `"Unknown ref: @e12 ..."` becomes `StaleRef`, anything
starting with `"could not locate element with role="` becomes `StaleRef`
too, `"... is not in the allowed domains list"` becomes `BlockedByPolicy`
(with the refused host pulled out of the message's own quoting),
`"... is covered by ..."` or `"not interactable"` becomes `NotActionable`,
and so on down the list in `classify`.

This is exactly as fragile as it sounds if agent-browser ever reworded one
of those messages — which is why every one of these mappings is pinned by
a test in `reply/test.rs`, so a message change upstream that would silently
break the classification instead breaks the build.

## Where errors surface for `BrowserSurface`

When a decision loop is driving the page through `BrowserSurface` rather
than calling `Browser` directly, an `Error` becomes a `DesktopResponse`
failure (`surface::mod::failure`), whose `code` is the error's wire name
rewritten from `PascalCase` to `SCREAMING_SNAKE_CASE` — `StaleRef` becomes
`STALE_REF` — matching the convention the desktop adapter's own errors use,
so a flow reading error codes does not need to know which surface it is
talking to.
