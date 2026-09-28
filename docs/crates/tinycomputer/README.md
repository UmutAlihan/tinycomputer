# crates/tinycomputer

tinycomputer is the TinyBus module that ships tinycomputer, a decision model
(Jev) based harness for desktop and browser automation written in Rust. Jev
answers the small closed questions about what to press; the harness does
everything else: reading the screen, asking, checking the answer, acting,
verifying, and enforcing safety. This crate is where the harness gets built
into one native library a TinyBus host can load, so it is what you are
depending on if you install a release archive and call it from another
program. Everything else in the workspace, `tinycomputer-bus`,
`tinycomputer-desktop`, `tinycomputer-engine`, `tinycomputer-browser`,
`tinycomputer-cursor`, exists to be assembled here.

## What it is

`tinycomputer` compiles to two things from the same source: an `rlib` for Rust
code that wants to link against it directly, and a `cdylib`, a `.so`, `.dylib`,
or `.dll` depending on platform, that a TinyBus host loads at runtime without
either side knowing about the other's build. The `cdylib` is the module: it
claims the bus name `ai.tinyhumans.tinycomputer.Desktop`, serves one object at
`/ai/tinyhumans/tinycomputer/Desktop`, and answers 80 members, one call for
each thing an agent can ask the desktop, the browser, or a running task to do:
54 desktop primitives, 5 Jev-driven members, 8 task members, and 13 browser
primitives prefixed `Browser`.

The crate holds no automation logic of its own. `tinycomputer-desktop` wraps
the vendored accessibility engine, `tinycomputer-engine` wraps the decision
loops that drive it, and `tinycomputer-browser` wraps a Chrome session. This
crate's job is narrower: take those pieces, read the host's configuration,
dispatch each bus call to the right method, and export the module in a shape
the loader recognizes. `crates/tinycomputer/src/tinybus_module/README.md`
covers that adapter layer in more depth, and
[`docs/technical/specs/desktop-module-contract.md`](../../technical/specs/desktop-module-contract.md)
is the frozen contract this crate implements.

## Who needs this crate

- Someone building or operating a TinyBus host that wants to give an agent
  desktop and browser control. You install the release archive, point the
  loader at it, and call it. See [installing-and-loading.md](installing-and-loading.md).
- Someone writing Rust code in this workspace that needs the wire types
  (`SnapshotRequest`, `DesktopResponse`, and so on) without pulling in the
  bus or the engine. Depend on `tinycomputer-bus` alone; this crate re-exports
  the same types rather than defining twins, so `tinycomputer::SnapshotRequest`
  and `tinycomputer_bus::SnapshotRequest` are one type, not two that happen to
  look alike.
- Someone debugging why a call from a host behaves a certain way. The dispatch
  code, the configuration parsing, and the task runner all live under
  `src/tinybus_module/` in this crate.

If instead you want to change how a snapshot is taken, how a click resolves an
element, or anything else about the underlying automation, this is the wrong
crate. That behavior lives in `vendor/agent-desktop` and `vendor/agent-browser`,
fixed upstream and picked up here by a gitlink bump.

## How a host loads it

A TinyBus host does not compile against this crate. It downloads or is handed
a release archive containing the compiled library plus a `modules.toml` that
pins the library's SHA-256 digest, and the loader refuses to start a module
whose file does not match that digest. Configuration, including any API key,
travels from the host to the module as a JSON object at load time, and is
never echoed back, logged, or written to disk.

```sh
tinybus modules load-github \
  https://github.com/tinyhumansai/tinycomputer/releases/tag/v0.2.1 \
  tinycomputer-0.2.1-ubuntu-24.04-x86_64.tar.gz \
  <archive-sha256>
```

The full walkthrough, including what the archive contains and how to verify a
build locally, is in [installing-and-loading.md](installing-and-loading.md).

## A tour of this crate's folders

```text
crates/tinycomputer/
├── src/
│   ├── lib.rs                 crate docs and the entire public re-export surface
│   └── tinybus_module/
│       ├── mod.rs             setup() and the module_export! manifest
│       ├── dispatch/          one async fn per member, in contract order
│       │   ├── mod.rs         the served interface: one impl block
│       │   ├── service.rs     building the service from its configuration
│       │   └── browser.rs     what the browser members share
│       ├── config.rs          the `browser` and `cursor` configuration keys
│       ├── runner.rs          the task runner: one workspace per task
│       ├── tinybus_module_tests.rs  unit tests over the in-memory bus
│       ├── tinybus_module_tests/    one <topic>_tests.rs per topic
│       └── README.md          why the adapter is shaped the way it is
└── tests/                     integration tests against the public API only
```

`dispatch/mod.rs` is long on purpose. Read
[what's confidential and why](members.md#confidential-members) before assuming
any given member is safe to call from an untrusted caller.

## Pages in this section

- [what-is-tinybus.md](what-is-tinybus.md): what TinyBus is, in plain terms,
  and why this module rides on top of it instead of exposing a plain HTTP API.
- [installing-and-loading.md](installing-and-loading.md): release archives,
  `modules.toml`, checksum verification, and `verify_module`.
- [configuration.md](configuration.md): every configuration key this module
  reads, with examples, and what happens when you leave one out.
- [members.md](members.md): all 80 members grouped by family, and which ones
  need confidential delivery.
- [calling-it.md](calling-it.md): a host's-eye view of calling the module,
  worked examples for a plain desktop call and for a task.
- [releases.md](releases.md): how versioning and the release pipeline work,
  and the full platform matrix.

## Related reading

- [`../../how-it-works.md`](../../how-it-works.md) for the system as a whole.
- [`../../giving-it-a-task.md`](../../giving-it-a-task.md) for the task API
  from the perspective of the caller, rather than the module.
- [`../../technical/architecture.md`](../../technical/architecture.md) for the
  layered architecture this crate sits at the top of.
- [`../../technical/specs/desktop-module-contract.md`](../../technical/specs/desktop-module-contract.md)
  and [`../../technical/specs/tinybus-module-release.md`](../../technical/specs/tinybus-module-release.md)
  for the exact, versioned contract this crate is held to.
