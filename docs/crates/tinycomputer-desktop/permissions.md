# Permissions

## Why this crate checks first, instead of just trying

Desktop accessibility APIs fail in a particularly unhelpful way when a
process is not authorized to use them: they usually do not return an error.
They return an empty tree. On macOS and Windows, an unauthorized process
asking "what elements are in this window" gets back the answer "none," the
same answer it would get for a window that genuinely has nothing in it.

That means a snapshot from an unauthorized process looks exactly like a
snapshot of an empty window, and a click on an element that snapshot never
found looks exactly like the app not having that button. Nothing in the
response tells the difference between "you are not allowed to see this" and
"this is not here." A caller (or a decision loop acting on its behalf) has
no way to tell those apart without extra information.

`tinycomputer-desktop` closes that gap by asking the platform, before
running a command, whether the permission that command needs is actually
granted. If it is not, the command fails immediately with the code
`PERM_DENIED` and a suggestion naming the setting to change, instead of
running anyway and producing a misleadingly empty result.

## What each member needs

Every member is tagged with one of four needs, defined in
[`crates/tinycomputer-desktop/src/desktop/permission.rs`](../../../crates/tinycomputer-desktop/src/desktop/permission.rs):

| Need | Meaning | Example members |
|---|---|---|
| `Nothing` | Reads the module's own state, or asks the window server for a list every process may read. | `version`, `list_apps`, `list_windows`, `list_displays`, `clipboard_get` |
| `Accessibility` | Reads or drives another application's accessibility tree. | `snapshot`, `click`, `type_text`, `focus_window` |
| `ScreenRecording` | Captures pixels. | `screenshot` of a whole display |
| `AccessibilityAndScreenRecording` | Needs to resolve a target through the tree, then capture it. | `screenshot` of a named application or window |

A member tagged `Nothing` pays no cost for a permission report at all: no
round trip to the platform happens before it runs. This matters for
`list_apps` in particular, because it is meant to work as the very first
call on a machine where nothing has been granted yet.

## What happens on a denied permission

```rust
use tinycomputer_desktop::Desktop;

// On a machine without accessibility access granted, this fails with
// PERM_DENIED rather than returning an empty tree.
let reply = Desktop::new().snapshot(Default::default());
if !reply.ok {
    if let Some(error) = &reply.error {
        // error.code == "PERM_DENIED"
        // error.suggestion names the setting to grant
    }
}
```

The check happens in `Desktop::run_with_report`, the shared path every
member goes through
(in [`desktop/mod.rs`](../../../crates/tinycomputer-desktop/src/desktop/mod.rs)):
fetch the platform's permission report if the member needs one, check the
report against the member's need, and only then run the command. This is
also why the needs table mirrors the `agent-desktop` CLI's own policy: the
module and the CLI refuse the same commands for the same reasons, so a
result from one explains the other.

## The two members that report on permissions, not refuse over them

`status` and `permissions` (in
[`system.rs`](../../../crates/tinycomputer-desktop/src/desktop/system.rs))
are tagged `Nothing` even though their entire purpose is to talk about
permissions. That is deliberate: their job is to say whether a permission is
granted, so refusing to run them because a permission is denied would be
circular. They fetch the live report themselves and hand it back as data,
rather than treating a denial as a reason to fail.

`permissions` can also trigger the operating system's own permission
prompt, but only when the caller explicitly asks for it through
`PermissionsRequest::request`. A plain permission check never puts a system
dialog in front of whoever is at the machine; prompting is something a
caller has to opt into on purpose, because it interrupts a real person.

## Accessibility versus screen recording

Two separate permissions cover two separate capabilities:

- **Accessibility** lets the process read another application's UI
  structure and send it accessibility actions (click, focus, type). Almost
  every member in the observation and interaction families needs this.
- **Screen recording** lets the process capture pixels. Only `screenshot`
  needs it, and only screen recording on its own when capturing a whole
  display; capturing a named application or window needs accessibility too,
  because the target has to be found in the tree first before it can be
  framed as an image.

## Where this lives in the code

The need table and the preflight check are both in
[`permission.rs`](../../../crates/tinycomputer-desktop/src/desktop/permission.rs).
The platform-specific work of actually answering "is accessibility granted"
lives upstream in `vendor/agent-desktop`; this crate only decides which need
applies to which member and what to do when the report says no. See
[Errors and the envelope](errors-and-the-envelope.md) for how `PERM_DENIED`
is shaped once it reaches a caller, and
[Safety and privacy](../../safety-and-privacy.md) for the wider picture of
what tinycomputer is and is not allowed to touch.
