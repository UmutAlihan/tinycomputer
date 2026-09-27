# TinyBus Adapter

This module is the boundary between the engine in `src/desktop/` and TinyBus
module ABI v1. `DesktopService` exposes each of `Desktop`'s methods as a typed
bus member, and `setup` registers its object and claims the well-known interface
name. Neither the name, the object path, nor the payload types are spelled here:
they come from `tinydesktop-bus`, so a rename is a compile error in every
consumer instead of an `UnknownMethod` at runtime.

## Why the members are written out

`dispatch.rs` is fifty-six near-identical `async fn`s. They cannot be generated
by a `macro_rules!` inside the `impl` block: `#[tinybus::interface]` reads that
block's items to build its dispatch table, and a macro invocation there is still
unexpanded when the attribute runs. Writing them out is what lets the macro see
them.

The order they appear in is the order of `tinydesktop_bus::names::METHODS`, and
`test.rs` asserts the generated dispatch table against that list.

## Why every member blocks elsewhere

Accessibility APIs are synchronous, and some calls are slow on purpose — a dense
snapshot walks thousands of elements, a wait blocks for up to thirty seconds.
Running one on the connection's dispatch task would stall every other caller for
that whole time, so each member hands its work to `tokio::task::spawn_blocking`.

That is affordable because `Desktop` is four small fields and constructs its
platform adapter per call: nothing platform-specific has to cross a thread
boundary or survive an `await`. The module asks for two worker threads for the
same reason — one to run a blocking command on, one to keep answering on.

## Configuration

The module takes its configuration from the loader as a JSON object, parsed into
`Desktop` by `Desktop::from_config`: `session_id` and `trace_path` as strings,
`trace_strict` and `headed` as booleans. `browser.executable` names the
browser binary and `browser.cursor` the pace of the agent's drawn cursor
(`off`, `brisk`, `natural`, `calm`). An optional `jev` object configures the
provider, model, endpoint, and API key before the service is registered.
TinyBus treats initial and replacement module configuration as sensitive
host-control traffic. An unreadable configuration fails before replacing the
served object rather than falling back to defaults.

## The manifest

`tinybus_module::module_export!` emits the descriptor, embedded manifest, and
initialization symbols consumed by the dynamic loader. The manifest method list
must stay aligned with the interface macro's dispatch table and with
`tinydesktop_bus::names::METHODS`; the unit tests check both relationships. The
manifest side is checked by reading the literals `module_export!` was handed out
of this module's own source, because reading them back out of the exported
`extern "C"` function would need `unsafe`, which this workspace forbids.

Integration tests use TinyBus's in-memory transport, and
`crates/tinydesktop-examples/src/bin/verify_module.rs` loads a compiled `cdylib` through
the real dynamic loader before a release archive is accepted.
