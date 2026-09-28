# tinycomputer-desktop

`tinycomputer-desktop` is the desktop half of tinycomputer, a decision model
(Jev) based harness for desktop and browser automation written in Rust. Jev
answers the small closed questions about what to press next; the harness
around it reads the screen, asks, checks the answer, acts, verifies, and
enforces safety. This crate is the part of the harness that lets it control a
real desktop: click a button in Safari, read a field in a Finder window, type
into a form in System Settings. It wraps the vendored
[agent-desktop](https://github.com/lahfir/agent-desktop) engine, which does the
actual accessibility-tree work, and exposes it as one Rust type, `Desktop`,
with one method per operation.

If you are new to the repository, start with
[how tinycomputer works](../../how-it-works.md) first. This page assumes you
already know that tinycomputer runs flows and tasks on top of a "surface" (a
desktop or a browser), and it explains what the desktop surface actually is
and how it behaves.

## Who needs this page

- Someone wiring a host application to tinycomputer's desktop calls directly,
  without going through a flow or a task.
- Someone debugging why a click failed, why a snapshot came back empty, or why
  a permission error showed up.
- Anyone curious what "the desktop adapter" in
  [`docs/architecture.md`](../../technical/architecture.md) actually contains.

If you only want to describe a job in plain language and let tinycomputer
figure out the clicks, you want
[giving it a task](../../giving-it-a-task.md) instead, and this crate is
working underneath without you ever calling it directly.

## What this crate is, in one paragraph

`Desktop` takes a typed request (say, "click this ref"), asks the operating
system's accessibility API to do it, and returns a typed response that always
succeeds as a value, even when the click itself failed. A stale reference, a
missing permission, an application that is not running: all of these come
back as data with a code and a suggestion, not as a crash or a raw error
string. This crate holds no bus, no agent loop, and no model. It is a
translator between tinycomputer's contract types and the engine's own
argument types, plus the permission check that runs before anything touches
the screen.

## Why this crate exists separately

The instinct might be to let `tinycomputer-engine` (which runs the actual
decision loops) talk to `agent-desktop` directly. Two things stop that:

1. **The contract must not depend on a platform backend.** Requests and
   responses live in `tinycomputer-bus`, a crate with no accessibility code in
   it at all, so a host that only sends bus messages never links macOS or
   Windows accessibility libraries. `tinycomputer-desktop` is the only place
   that imports both the contract types and the engine types, and converts
   between them.
2. **Permission checking and error shaping belong in one place.** Every one of
   the 54 members below goes through the same preflight and the same
   response-building code, so a caller gets the same shape of answer whether
   it called `click` or `snapshot`.

See [`docs/technical/architecture.md`](../../technical/architecture.md),
section "How a call travels," for where this crate sits between the bus glue
and the vendored engine.

## A tour of the crate

| Path | What is there |
|---|---|
| [`crates/tinycomputer-desktop/src/desktop/`](../../../crates/tinycomputer-desktop/src/desktop) | `Desktop` itself, split into one file per family of members: observation, interaction, input, apps and windows, clipboard, notifications, waiting, system. |
| [`.../desktop/convert.rs`](../../../crates/tinycomputer-desktop/src/desktop/convert.rs) | Turns a contract type (from `tinycomputer-bus`) into the matching engine argument type. |
| [`.../desktop/permission.rs`](../../../crates/tinycomputer-desktop/src/desktop/permission.rs) | Decides what a member needs granted, and fails it early with a clear reason if it is not. |
| [`.../desktop/reply.rs`](../../../crates/tinycomputer-desktop/src/desktop/reply.rs) | Wraps an engine result, success or failure, into the response envelope every member returns. |
| [`crates/tinycomputer-desktop/src/surface/`](../../../crates/tinycomputer-desktop/src/surface) | `Desktop` acting as a `tinycomputer-core::Surface`, the interface the Jev decision loops actually call. |
| [`crates/tinycomputer-desktop/src/error/`](../../../crates/tinycomputer-desktop/src/error) | The small `Error` enum for cases where a command cannot even start (bad configuration, for example), separate from the normal "the click failed" responses. |

Each of these has its own page below, going deeper than this overview.

- [Snapshots and refs](snapshots-and-refs.md): why tinycomputer clicks
  elements by reference instead of by screen coordinates, and what happens
  when a reference goes stale.
- [Members by family](members-by-family.md): a short example of every one of
  the 54 desktop operations, grouped the way the code groups them.
- [Permissions](permissions.md): why this crate checks accessibility and
  screen recording access before doing anything, and what happens when it is
  missing.
- [Errors and the envelope](errors-and-the-envelope.md): why every method
  returns a value instead of a Rust `Result`, and how to read one.
- [Desktop as a surface](desktop-as-a-surface.md): how `Desktop` becomes the
  thing a Jev decision loop actually looks at and acts through.
- [Platforms](platforms.md): what works today on macOS, Windows, and Linux.
- [Headed and headless input](headed-and-headless.md): the difference between
  a ref action and a synthesized mouse or keyboard event, and when each one
  runs.

## How this fits into the rest of tinycomputer

```
host / agent
     |
     v
tinycomputer (TinyBus glue)          <- crates/tinycomputer
     |
     v
tinycomputer-engine (Jev, flows, tasks)  <- crates/tinycomputer-engine
     |
     v
tinycomputer-desktop (this crate)     <- Desktop, one method per member
     |
     v
vendor/agent-desktop (the engine)     <- accessibility backend per platform
     |
     v
the operating system's accessibility API
```

A host that only wants raw desktop calls, with no decision-making on top, can
depend on `tinycomputer-desktop` alone and skip `tinycomputer-engine`
entirely. That is the whole reason the crate boundary is drawn here rather
than folded into the engine.

## Related reading

- [How tinycomputer works](../../how-it-works.md): the big picture, for
  someone who has not seen any of this before.
- [Giving it a task](../../giving-it-a-task.md): the level most people
  actually use, built on top of this crate.
- [Writing flows](../../writing-flows.md): a level between raw calls and full
  tasks.
- [Seeing the screen](../../seeing-the-screen.md): how a snapshot becomes what
  a decision loop sees.
- [Safety and privacy](../../safety-and-privacy.md): what never reaches a
  screen or a model.
- [Glossary](../../glossary.md): short definitions of terms like ref, surface,
  and snapshot used throughout this page.
- [`docs/technical/architecture.md`](../../technical/architecture.md): the
  precise, less friendly version of "how a call travels."
- [`docs/technical/specs/desktop-module-contract.md`](../../technical/specs/desktop-module-contract.md):
  the wire contract every request and response in this crate follows.
- [`crates/tinycomputer-desktop/README.md`](../../../crates/tinycomputer-desktop/README.md):
  the crate's own short README, aimed at contributors rather than newcomers.
