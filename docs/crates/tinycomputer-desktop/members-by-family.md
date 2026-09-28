# Members by family

`Desktop` has one method per desktop operation the tinycomputer contract
defines: 54 of them. The code groups them into eight families, each in its
own file under
[`crates/tinycomputer-desktop/src/desktop/`](../../../crates/tinycomputer-desktop/src/desktop).
This page walks through each family with one short example. For the full
detail on any single member, read its rustdoc in the source file named
below, or run `cargo doc --no-deps -p tinycomputer-desktop --open`.

## Observation: reading the screen without touching it

File: [`observation.rs`](../../../crates/tinycomputer-desktop/src/desktop/observation.rs).

`snapshot`, `find`, `get`, `is`, `screenshot`. Covered in depth in
[Snapshots and refs](snapshots-and-refs.md). One example not shown there:

```rust
use tinycomputer_desktop::{Desktop, ScreenshotRequest};

let reply = Desktop::new().screenshot(ScreenshotRequest::default());
assert_eq!(reply.command, "screenshot");
```

Capturing a whole display needs only screen recording access. Capturing a
named application or window also needs accessibility access, because the
target has to be found in the tree before it can be framed. See
[Permissions](permissions.md).

## Interaction: acting on a ref

File: [`interaction.rs`](../../../crates/tinycomputer-desktop/src/desktop/interaction.rs).

`click`, `double_click`, `triple_click`, `right_click`, `clear`, `focus`,
`toggle`, `check`, `uncheck`, `expand`, `collapse`, `scroll_to`, `type_text`,
`set_value`, `select`, `scroll`. All of these take a ref (most through the
plain `RefRequest`) and perform the accessibility action that matches the
element's role: clicking a button, toggling a checkbox, expanding a
disclosure triangle.

```rust
use tinycomputer_desktop::{Desktop, TypeRequest};

let reply = Desktop::new().type_text(TypeRequest {
    ref_id: "@s1:e2".to_owned(),
    text: "hello".to_owned(),
    ..TypeRequest::default()
});

assert_eq!(reply.command, "type");
```

Two members are worth calling out because they trade off against each
other: `type_text` delivers keystrokes one at a time to the element, which
fires whatever per-character validation a form runs; `set_value` replaces
the whole value in one step, which is faster and safer for long text but
skips those handlers. Use `type_text` when the typing itself has to matter.

## Input: synthesized keyboard and mouse events

File: [`input.rs`](../../../crates/tinycomputer-desktop/src/desktop/input.rs).

`press`, `key_down`, `key_up`, `hover`, `drag`, `mouse_move`, `mouse_click`,
`mouse_down`, `mouse_up`, `mouse_wheel`. Unlike the interaction family, these
do not act on a ref through its accessibility action; they generate real
keyboard and mouse events, which is the only way to reach something the
accessibility tree cannot describe, such as a canvas or a custom-drawn
control.

```rust
use tinycomputer_desktop::{Desktop, MouseClickRequest};

let reply = Desktop::new().mouse_click(MouseClickRequest {
    x: 10.0,
    y: 20.0,
    count: 1,
    ..MouseClickRequest::default()
});

assert_eq!(reply.command, "mouse-click");
```

`key_down`, `key_up`, `mouse_down`, and `mouse_up` are the four exceptions
that always fail. They exist in the contract for a stateful daemon that can
hold a key or button down across calls, but this module has no state between
calls, so holding one open would be a lie a caller could only discover
through a broken drag later. Use `press` for a full key combination in one
call, and `drag` for a press-move-release gesture, both of which own their
whole action from start to finish. See
[Headed and headless input](headed-and-headless.md) for why this family
behaves differently from interaction members in general.

## Apps and windows: managing what is running

File: [`apps.rs`](../../../crates/tinycomputer-desktop/src/desktop/apps.rs).

`launch`, `close_app`, `list_apps`, `list_windows`, `list_displays`,
`list_surfaces`, `focus_window`, `resize_window`, `move_window`, `minimize`,
`maximize`, `restore`.

```rust
use tinycomputer_desktop::{Desktop, LaunchRequest};

let reply = Desktop::new().launch(LaunchRequest::new("Safari"));
assert_eq!(reply.command, "launch");
```

`list_apps`, `list_windows`, and `list_displays` need no permission at all:
they read the window server's own list, which every process is allowed to
see. That makes `list_apps` the right first call on a machine that has not
granted accessibility access yet, since it works regardless. `list_surfaces`
is how a caller checks whether a menu, sheet, or popover is currently open
and reachable before asking to snapshot it.

## Clipboard: the pasteboard

File: [`clipboard.rs`](../../../crates/tinycomputer-desktop/src/desktop/clipboard.rs).

`clipboard_get`, `clipboard_set`, `clipboard_clear`.

```rust
use tinycomputer_desktop::{ClipboardSetRequest, Desktop};

let reply = Desktop::new().clipboard_set(ClipboardSetRequest::text("hello"));
assert_eq!(reply.command, "clipboard-set");
```

An empty pasteboard is not treated as a failure: `clipboard_get` answers with
`found: false` rather than an error. All three members need no permission on
their own. The `Desktop` surface (see
[Desktop as a surface](desktop-as-a-surface.md)) uses these three under the
hood to paste typed text into a field: it reads whatever the pasteboard held
before, stages the new text, sends the paste, and puts the original contents
back afterward.

## Notifications: the notification center

File: [`notifications.rs`](../../../crates/tinycomputer-desktop/src/desktop/notifications.rs).

`list_notifications`, `notification_action`, `dismiss_notification`,
`dismiss_all_notifications`.

```rust
use tinycomputer_desktop::{Desktop, ListNotificationsRequest};

let reply = Desktop::new().list_notifications(ListNotificationsRequest::default());
assert_eq!(reply.command, "list-notifications");
```

Notifications are addressed by index within the current list, and that index
moves as new notifications arrive. Because of that, the mutating members
also take the application and title the caller expects to find at that
index, so a notification that has already scrolled off is not silently acted
on in place of a different one.

## Waiting: blocking until something is true

File: [`waiting.rs`](../../../crates/tinycomputer-desktop/src/desktop/waiting.rs).

One member, `wait`, with several modes: a plain sleep, waiting for an
element, a window, matching text, a surface (menu, closed menu, or
notification) to appear, or an event.

```rust
use tinycomputer_desktop::{Desktop, WaitRequest};

let reply = Desktop::new().wait(WaitRequest::sleep(1));
assert!(reply.ok);
```

A plain sleep needs no permission, because it touches no other application.
Every other mode watches something, so it needs accessibility access; without
it, the wait would run out its whole timeout watching a tree it is not
allowed to see, and time out looking like the condition never became true.
A wait that never becomes true fails with `TIMEOUT`, using the engine's own
default timeout of 30 seconds when the request leaves it unset.

## System: reporting on the module and the machine

File: [`system.rs`](../../../crates/tinycomputer-desktop/src/desktop/system.rs).

`version`, `status`, `permissions`.

```rust
use tinycomputer_desktop::Desktop;

let reply = Desktop::new().version();
assert!(reply.ok);
```

`version` needs nothing and touches nothing outside the process, which makes
it the cheapest possible way to confirm the module loaded and answers.
`status` reports permissions, the active session, and the most recent
snapshot in one call, which makes it the first thing to check when something
did not work and it is not obvious why. `permissions` reports what is granted
and, if asked, can trigger the operating system's own permission prompt; see
[Permissions](permissions.md) for why prompting only happens when explicitly
requested.

## All 54 at a glance

For the authoritative list with the exact request and response types each
member takes, see
[`docs/technical/specs/desktop-module-contract.md`](../../technical/specs/desktop-module-contract.md)
and the member constants in
`crates/tinycomputer-bus/src/names/mod.rs`. The four held-input members
(`key_down`, `key_up`, `mouse_down`, `mouse_up`) count toward the 54 even
though they always fail, for the reason given in the input family above.
