# Snapshots and refs

## The problem with coordinates

The obvious way to automate a desktop is to take a screenshot, find a button
in the picture, and click its pixel coordinates. That breaks the moment
anything on screen moves: a window shifts a few pixels, a sidebar resizes, a
banner appears above the button you wanted. The click still happens, it just
lands on whatever is now sitting at that spot.

tinycomputer does not do this for anything it can avoid. Instead it asks the
operating system directly: "what elements exist in this window, and what can
each one do?" That answer is a snapshot.

## What a snapshot is

Calling `Desktop::snapshot` walks an application's accessibility tree, the
same structural information screen readers use, and returns it as a compact
tree of elements. Every element that can be acted on gets a *ref*: a short
id like `@s8f3k2p9:e1`.

```rust
use tinycomputer_desktop::{Desktop, FindRequest, RefRequest};

let desktop = Desktop::new();

let found = desktop.find(FindRequest {
    app: Some("Safari".to_owned()),
    role: Some("button".to_owned()),
    name: Some("Save".to_owned()),
    first: true,
    ..FindRequest::default()
});

if found.ok {
    let reply = desktop.click(RefRequest::new("@s8f3k2p9:e1"));
    assert_eq!(reply.command, "click");
}
```

A ref is not a coordinate and not a CSS-style selector re-evaluated at click
time. It is a handle bound to one specific snapshot: `s8f3k2p9` names the
snapshot, `e1` names the element inside it. Two different snapshots of the
same window produce different ids for what might be the same button, because
each ref carries the moment it was described in.

## Why the binding matters: STALE_REF

Because a ref belongs to a snapshot, acting on one that is no longer current
fails on purpose, with the code `STALE_REF`, rather than guessing. If the
window closed, the element was removed, or enough time passed that the
snapshot is considered too old to trust, the click does not happen at all.
The reply's recovery hint says to take a fresh snapshot and try again.

This is deliberate: the alternative, clicking whatever now occupies that
position, is exactly the coordinate-guessing behavior refs exist to avoid.
`docs/technical/architecture.md`'s "Safety, in one place" section describes
the same principle from the engine's point of view. See
[Errors and the envelope](errors-and-the-envelope.md) for how `STALE_REF` and
other codes are structured in the response.

## Keeping snapshots small: skeletons and depth

A dense application (Xcode, Slack, a browser tab full of nested divs) can
have thousands of accessibility nodes. Describing all of them costs tokens
and time for no benefit if a decision loop only needs to know a handful are
there. `SnapshotRequest` has a few knobs for this:

- `max_depth` limits how far the tree walk goes; the engine's own default is
  10 when the field is left unset.
- `skeleton` asks for a shallow overview first: enough to see the shape of
  the window without every leaf element inside it.
- `root_ref` re-roots the walk at a ref from an earlier snapshot, so a caller
  that already knows roughly where something is can drill into just that
  part of the tree.
- `interactive_only` and `compact` trim elements that carry no action and
  shrink the JSON shape of what remains.

A node whose subtree was cut short by `max_depth` is marked as truncated
rather than silently dropped, so a caller (or the `Surface` layer described in
[Desktop as a surface](desktop-as-a-surface.md)) knows there is more to see if
it asks.

## Finding without snapshotting first

`Desktop::find` runs the same underlying tree walk as `snapshot`, but filters
as it goes: by role, name, description, value, text, or a boolean state, and
can ask for the first, last, nth, or every match. It is the right call when
the caller already knows roughly what it wants ("the button named Save") and
does not need the whole tree back.

## Reading one thing about one ref

`Desktop::get` reads a single property (text, value, title, bounds, role, or
states) off one ref, and `Desktop::is` tests a single boolean state
(visible, enabled, checked, focused, expanded, selected). Both take a ref the
same way an interaction member does, and both fail with `STALE_REF` under the
same condition.

## Where this lives in the code

The observation members (`snapshot`, `find`, `get`, `is`, `screenshot`) are in
[`crates/tinycomputer-desktop/src/desktop/observation.rs`](../../../crates/tinycomputer-desktop/src/desktop/observation.rs).
The engine's own tree walk and ref allocation live upstream in
`vendor/agent-desktop`; this crate only shapes the request going in and the
response coming out. See [Members by family](members-by-family.md) for the
rest of the interaction members that consume refs, and
[Seeing the screen](../../seeing-the-screen.md) for how a snapshot becomes
what a Jev decision loop is actually shown.
