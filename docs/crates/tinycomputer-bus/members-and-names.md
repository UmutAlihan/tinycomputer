# Members and names

Source: [`crates/tinycomputer-bus/src/names/`](../../../crates/tinycomputer-bus/src/names/)
and the per-family modules: `observation/`, `interaction/`, `input/`, `apps/`,
`clipboard/`, `notifications/`, `waiting/`, `system/`.

## The interface and the object path

The whole desktop surface is served as one bus interface at one object path:

```rust,ignore
pub const INTERFACE: &str = "ai.tinyhumans.tinycomputer.Desktop";
pub const OBJECT_PATH: &str = "/ai/tinyhumans/tinycomputer/Desktop";
```

A "member" here is one callable operation on that interface, the rough
equivalent of a method name. There are 56 desktop members plus 8 Agent (task)
members served on the same interface (their names never collide, since they
were designed together). Every one of them is a named Rust constant, never a
bare string written out at a call site:

```rust,ignore
use tinycomputer_bus::names;
assert_eq!(names::methods::CLICK, "Click");
```

That is not a style preference. If a member is ever renamed, every place that
spells it as `names::methods::CLICK` fails to compile; every place that spells
it as the literal string `"Click"` keeps compiling and starts failing at
runtime with an "unknown method" the first time it actually runs. The
constants turn a footgun into a compile error.

`names::METHODS` is the full list, in the exact order the interface dispatches
them, and `crates/tinycomputer` (the module itself) asserts its own dispatch
table and its embedded manifest against that list. So the three copies — the
constants here, the module's dispatch table, and the module's manifest —
cannot drift apart without a test failing.

## Reading the observation-then-action pattern

Almost every family below follows the same two-step rhythm: first you
*observe*, which hands you a `ref` (a short opaque string like `@s8f3k2p9:e1`
naming one element in the accessibility tree); then you *act*, naming that
ref in the next call. Coordinates exist as a fallback for the rare cases where
there is no accessible element to name, but the ref is the normal path.

Refs are tied to the snapshot that minted them. Act on a stale one (the
screen changed since you looked) and you get `STALE_REF` back, with a
suggestion to re-snapshot — see
[The envelope and errors](envelope-and-errors.md).

## The families

### Observation: `Snapshot`, `Find`, `Get`, `Is`, `Screenshot`

Source: [`observation/types.rs`](../../../crates/tinycomputer-bus/src/observation/types.rs)

- **`Snapshot`** walks an accessibility tree and hands back a ref for every
  element it finds. `SnapshotRequest` is all-defaults: the empty object
  snapshots the focused application's focused window at full depth. The
  `skeleton` flag is the token-budget lever: it caps the walk at three levels
  and returns structure without leaf detail, useful for deciding where to
  look before spending a full walk on one subtree (follow up with `root_ref`
  pointed at the interesting container).
- **`Find`** searches instead of walking: give it a role, a name, states to
  require, and get back matches. Its filter fields all combine with AND. Its
  selection fields (`count`, `first`, `last`, `nth`) are mutually exclusive —
  asking for two of them at once is an `INVALID_ARGS` error, not a silent
  pick of one.
- **`Get`** reads one property (`text`, `value`, `title`, `bounds`, `role`,
  `states`) of one ref.
- **`Is`** tests one boolean state (`visible`, `enabled`, `checked`,
  `focused`, `expanded`, `selected`) of one ref. A property that does not
  apply to that element's role comes back as inapplicable rather than
  `false`, so "this checkbox is unchecked" reads differently from "this is
  not a checkbox."
- **`Screenshot`** captures an application, a window, or a display. With
  `output_path` set, the image is written to disk and the reply carries the
  path; without it, the image comes back base64-encoded inline. Prefer the
  path for anything bigger than a single control — inlining a full-screen PNG
  is megabytes of base64 in a bus frame.

### Interaction: the ref-addressed actions

Source: [`interaction/types.rs`](../../../crates/tinycomputer-bus/src/interaction/types.rs)

`Click`, `DoubleClick`, `TripleClick`, `RightClick`, `Clear`, `Focus`,
`Toggle`, `Check`, `Uncheck`, `Expand`, `Collapse`, and `ScrollTo` all take the
same `RefRequest { ref_id, snapshot_id, timeout_ms }` — there is one struct
because there is nothing more to say for any of them.

A few members need more:

- **`Type`** (`TypeRequest`) delivers text to the named element specifically,
  not to whatever currently holds keyboard focus, so a window that steals
  focus mid-run does not silently receive it.
- **`SetValue`** (`SetValueRequest`) replaces a field's value in one step
  rather than typing it character by character, so it skips whatever
  per-keystroke handlers a form has. Use `Type` when the typing itself
  matters (an autocomplete, a live search box); use `SetValue` when only the
  end result matters.
- **`Select`** (`SelectRequest`) picks an option in a list, menu, or picker
  by its accessible name.
- **`Scroll`** (`ScrollRequest`) scrolls a specific container by a direction
  and an amount, distinct from `ScrollTo` which just brings a ref into view.

### Input: synthesized keyboard and mouse

Source: [`input/types.rs`](../../../crates/tinycomputer-bus/src/input/types.rs)

- **`Press`** sends a key combination written the way a menu would show it:
  `cmd+shift+p`, `ctrl+c`, `escape`, `f5`. The engine normalizes and
  validates it and refuses a combo the platform reserves unless `force` is
  set.
- **`Hover`**, **`Drag`**, **`MouseMove`**, **`MouseClick`**, **`MouseWheel`**
  work at either a ref or raw screen coordinates. `Drag` is the one member
  that holds a mouse button down for a whole gesture, and it can only do that
  safely because it owns both endpoints within a single call.
- **`KeyDown`**, **`KeyUp`**, **`MouseDown`**, and **`MouseUp`** are reserved
  for a future stateful daemon and currently fail closed: a stateless call
  cannot safely hold a key or button down across separate requests, so rather
  than pretend to, these members validate their payload and then refuse.
  `HoverRequest.duration_ms` has the same problem for the same reason: it is
  accepted (so the shape is stable) and rejected (any positive value fails),
  with the engine telling the caller to hover, then issue a separate wait.

### Apps: applications, windows, displays

Source: [`apps/types.rs`](../../../crates/tinycomputer-bus/src/apps/types.rs)

`Launch`, `CloseApp`, `ListApps`, `ListWindows`, `ListDisplays`,
`ListSurfaces`, `FocusWindow`, `ResizeWindow`, `MoveWindow`, `Minimize`,
`Maximize`, `Restore`.

Worth knowing: `LaunchRequest.attach_if_running` defaults to `true`, so
launching an already-open application attaches to it rather than starting a
second copy; set it to `false` to require a fresh process (and get a
structured error naming the existing pid if one is already running).
`LaunchRequest.cdp_port` opens a Chrome DevTools Protocol port on a
Chromium-based application, which is the seam a browser automation library
uses to drive that app's web content while its native menus and dialogs stay
on the accessibility path. `CloseAppRequest` never closes something the
engine considers protected (a window server, a login agent), whatever `force`
says. `FocusWindowRequest` narrows by `window_id`, `app`, and `title`
together; a combination matching more than one window fails with
`AMBIGUOUS_TARGET` and lists the candidates rather than guessing.

### Clipboard, notifications, waiting, system

Source: [`clipboard/`](../../../crates/tinycomputer-bus/src/clipboard/types.rs),
[`notifications/`](../../../crates/tinycomputer-bus/src/notifications/types.rs),
[`waiting/`](../../../crates/tinycomputer-bus/src/waiting/types.rs),
[`system/`](../../../crates/tinycomputer-bus/src/system/types.rs)

- **`ClipboardGet`** / **`ClipboardSet`** / **`ClipboardClear`**: reading an
  image never inlines it (it is written to a file and reported by path,
  like `Screenshot`); writing takes exactly one of `text`, `image`, or
  `file_urls`, and asking for none or more than one is `INVALID_ARGS`. A read
  that finds nothing in the requested format is not an error, it just reports
  `found: false`.
- **`ListNotifications`**, **`NotificationAction`**, **`DismissNotification`**,
  **`DismissAllNotifications`**: `NotificationActionRequest` and
  `DismissNotificationRequest` both carry an `expected_app` and
  `expected_title` alongside the plain index, so a caller can assert what it
  believes it is acting on and get a structured error if the notification
  list moved between reading it and acting on it, rather than silently
  dismissing the wrong entry.
- **`Wait`** (`WaitRequest`): exactly one of `ms`, `element`, `window`,
  `text`, `surface`, or `event` selects what is being waited for; a wait that
  times out fails with `TIMEOUT` rather than returning as if it had
  succeeded.
- **`Permissions`** (`PermissionsRequest`): `request: false` (the default)
  just reports the current permission state without putting anything on
  screen; `request: true` prompts for whatever is missing.
- **`Version`** and **`Status`** take no argument at all.

## Where to look next

- The shared enums these payloads are built from (`Surface`, `Modifier`,
  `MouseButton`, and so on) are in [The shared vocabulary](vocabulary.md).
- How every one of these members replies, on success and failure, is in
  [The envelope and errors](envelope-and-errors.md).
- The higher-level `Flow`, `RunGoal`, and Agent task interfaces are built on
  top of exactly these members; see [Writing flows](flows.md),
  [The goal loop](goal-loop.md), and [The Agent and task types](agent-and-tasks.md).
- For what actually happens on the desktop when one of these members runs,
  see [docs/technical](../../technical/architecture.md) and
  [crates/tinycomputer-desktop/README.md](../../../crates/tinycomputer-desktop/README.md).
