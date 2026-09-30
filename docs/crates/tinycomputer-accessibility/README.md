# tinycomputer-accessibility

tinycomputer is a decision model (Jev) based harness for desktop and browser
automation. This crate is a small, separate piece for host applications: the
handful of operating-system answers a host wants **right now, in its own
process**, without going through the module bus.

A voice-dictation host is the typical user. When the user presses a hotkey it
asks which application and text field is focused, checks that focus has not
moved before it pastes, asks whether it may listen for input events or use the
microphone, and listens for the macOS Globe (Fn) key. Each is a synchronous
call that returns immediately.

## How it differs from the desktop surface

| | `tinycomputer-accessibility` | `tinycomputer-desktop` / the module |
|---|---|---|
| Caller | a host, in-process, Rust API | any bus client, typed members |
| Job | OS facts: focus, permissions, Globe key | drive other apps: snapshot, click, type |
| Runtime | none; blocking calls | engine, refs, safety checks |

## Platforms and features

macOS does the real work (accessibility APIs and a small Swift helper compiled
on first use). Other platforms answer with typed unsupported errors or states
so a host can call the same API everywhere. Fallible operations use the crate's
`Error` and `Result<T>` types. The `microphone-probe` feature uses `cpal` to
check for an input device, but device enumeration cannot confirm recording
authorization; macOS and Windows therefore report `Unknown` until a host opens
a capture stream. Leave the feature off if the host never records.

See the [crate README](../../../crates/tinycomputer-accessibility/README.md) for
the module map.
