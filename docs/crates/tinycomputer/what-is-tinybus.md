# What is TinyBus

TinyBus is a message bus, in the same family as D-Bus on Linux. If you have
never used D-Bus, the short version is: instead of one process calling a
library that another process happens to also load, processes announce
themselves on a shared bus by a well-known name, expose objects at named
paths, and answer typed method calls on those objects. Anyone else on the bus
can find that name and call those methods, without either side needing to
know how the other one is built.

tinycomputer is one of those processes, or more precisely, one of those things
that *can be* a process, more on that below. It claims the name
`ai.tinyhumans.tinycomputer.Desktop`, serves one object at
`/ai/tinyhumans/tinycomputer/Desktop`, and answers 67 methods on it: take a
snapshot of a window, click a button, run a whole multi-step task. A host,
whatever program is orchestrating an agent, talks to it the same way it would
talk to any other TinyBus service.

## Why not just an HTTP API, or a plain Rust library

Two reasons, and they pull in different directions.

The reason to avoid a plain library: automating a desktop or a browser needs
platform-specific accessibility APIs, native crashes are possible, and the
whole point is that a wedged or crashed automation engine should not be able
to take an entire kernel process down with it. TinyBus's answer is that every
integration is its own crash domain. If the module dies, the host finds out
because the bus reports the name released, not because a call hung until a
timeout.

The reason to avoid a bespoke HTTP API: TinyBus already gives you typed
methods, structured errors, a manifest a loader can verify before running
anything, and, importantly, an in-memory transport. That last part matters
more than it sounds: the *same* module code that would run out-of-process over
a Unix socket can instead be loaded directly into the host process, with calls
going through Rust function calls rather than a socket. A slim build gets the
crash isolation of a separate process without paying a network round trip for
every call. tinycomputer's tests exercise it exactly this way, over TinyBus's
in-memory transport, so nothing in this crate's test suite ever binds a real
socket.

## The pieces, mapped onto this crate

- **A well-known name.** `ai.tinyhumans.tinycomputer.Desktop`. A host asks the
  bus who owns this name and gets routed to tinycomputer, wherever it happens
  to be running.
- **An object path.** `/ai/tinyhumans/tinycomputer/Desktop`. One object, one
  path, in this module. A more elaborate service might serve several objects;
  tinycomputer does not need to.
- **An interface.** The set of methods the object answers, tied to the same
  name as above. `#[tinybus::interface(name = "ai.tinyhumans.tinycomputer.Desktop")]`
  on `DesktopService` in `crates/tinycomputer/src/tinybus_module/dispatch.rs`
  is what turns a plain `impl` block into that interface.
- **Methods.** Typed request in, typed response out, over the wire as
  serialized JSON-shaped payloads. `Click`, `Snapshot`, `RunGoal`, and 64
  others. See [members.md](members.md) for the full list.
- **Confidential delivery.** Some methods, `RunGoal` and the task-starting
  calls among them, are marked `#[tinybus(confidential)]`. TinyBus treats
  those calls, and the module's own configuration, as sensitive host-control
  traffic rather than ordinary bus chatter that a monitor tool might print.
  [members.md](members.md#confidential-members) explains which members and
  why.
- **A module ABI.** Beyond ordinary bus service, TinyBus defines a stable C
  ABI for dynamically loaded modules: an exported symbol set, an embedded
  manifest describing what the module provides, and an initialization
  function the loader calls with the host-supplied configuration.
  `tinybus_module::module_export!` in `crates/tinycomputer/src/tinybus_module/mod.rs`
  is what emits that for tinycomputer. This is what lets a host download a
  compiled `.dylib` or `.so` from a GitHub release and load it without
  compiling anything.

## Where to look for more

TinyBus itself, the broker, the wire protocol, the module loader, lives in
`vendor/tinybus` as a pinned submodule; its own `README.md` there covers the
bus in general terms, and `crates/tinybus/src/module/` inside it is the ABI
tinycomputer targets. That crate is out of scope for this repository: a bug in
the bus itself is fixed there, not here.

For how tinycomputer specifically uses TinyBus, read
[installing-and-loading.md](installing-and-loading.md) next, or
`crates/tinycomputer/src/tinybus_module/README.md` for the adapter code's own
explanation of why every method hands its work to a blocking thread pool
rather than running on the connection's dispatch task.
