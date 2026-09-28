# tinycomputer-bus

tinycomputer is a Jev-based harness for desktop and browser automation: Jev
decides what to press or type by answering small closed questions, and the
harness does everything else, reading the screen, asking, checking the
answer, acting, and verifying. `tinycomputer-bus` is the dictionary the
harness and a host use to talk to each other. It defines every message that
can cross the wire: what a request looks like, what a reply looks like, and
the exact names used to ask for something. It holds no logic and touches no
screen. It is just the shapes of things.

## Who needs this

If you are writing the code that loads tinycomputer as a module and calls
into it, this crate is what you depend on. You get typed requests
(`SnapshotRequest`, `RunFlowRequest`, `StartTaskRequest`, and so on) and typed
replies, and the compiler catches a typo in a member name or a missing field
before you ever run the program. You do not need the desktop engine, the
browser engine, or any of the heavier crates that actually do the automation:
`tinycomputer-bus` depends on nothing but `serde` and `serde_json`.

If you are only using tinycomputer through a higher-level tool (an agent
framework, a chat assistant with tools attached), you probably do not need to
read this crate at all. Start with [How tinycomputer works](../../how-it-works.md)
and [Giving it a task](../../giving-it-a-task.md) instead. Come back here when
you want to know exactly what a field is called, what its default is, or what
a reply's JSON looks like byte for byte.

## Where this sits

```
a host program  --(loads and calls)-->  tinycomputer (the module)
                                            depends on and re-exports
                                         tinycomputer-bus (this crate)
```

tinycomputer ships as a single loadable file (a `cdylib`, in Rust terms). A
host can load that file and call into it, but it cannot `use` any Rust types
out of it directly. So the vocabulary of requests and replies is published
separately, as an ordinary library anyone can depend on: this crate.

The direction matters. `tinycomputer` (the crate with the actual engine)
depends on `tinycomputer-bus` and re-exports everything in it, so
`tinycomputer::SnapshotRequest` and `tinycomputer_bus::SnapshotRequest` are
the exact same type, not two lookalike structs that drift apart over time. A
host that only wants to make calls depends on `tinycomputer-bus` alone, and
gets none of the weight of the real engine: no platform accessibility
backend, no browser, no async runtime.

For the fuller architecture picture (how a call actually travels from a host,
through the bus, into the engine, and back), see
[docs/technical/architecture.md](../../technical/architecture.md).

## What is deliberately not here

- **No behavior.** A type here describes what a message carries, never what
  the module does with it. The desktop engine lives in `tinycomputer-desktop`,
  the browser engine in `tinycomputer-browser`, the decision loops in
  `tinycomputer-engine`.
- **No transport.** This crate does not know how to open a connection, retry
  one, or trace one. A host already has its own answer for that.
- **No parallel host-side types.** There is exactly one `SnapshotRequest`,
  defined once, here at the bottom of the dependency graph.

See the crate's own [rustdoc](../../../crates/tinycomputer-bus/src/lib.rs) for
the terse, load-bearing version of this explanation, and
[crates/tinycomputer-bus/README.md](../../../crates/tinycomputer-bus/README.md)
for the engineering write-up this page is a friendlier tour of.

## A short tour

| Folder | What it holds | Read next |
|---|---|---|
| `names/` | The one interface name, the one object path, and one constant per member (80 in total: 54 desktop, 5 Jev-driven, 8 task, and 13 browser) | [Members and names](members-and-names.md) |
| `envelope/` | `DesktopResponse`, the reply shape every desktop member uses, and the structured `DesktopError` it carries on failure | [The envelope and errors](envelope-and-errors.md) |
| `vocabulary/` | The small shared enums every payload draws from: surfaces, mouse buttons, modifier keys, element properties | [The shared vocabulary](vocabulary.md) |
| `observation/`, `interaction/`, `input/`, `apps/`, `clipboard/`, `notifications/`, `waiting/`, `system/` | One module per family of desktop members: reading the screen, acting on a ref, synthesizing keys and clicks, managing windows and applications, and so on | [Members and names](members-and-names.md) |
| `flow/` | `Flow`, the app-agnostic script format, and its authoring guide | [Writing flows, the wire format](flows.md) |
| `agentic/` | `RunGoal` and `ResolveIntent`: the lower-level, bounded desktop-control loop that flows are built on | [The goal loop](goal-loop.md) |
| `agent/` | The Agent (task) interface: hand over a job in plain language, get back a paused-or-finished view | [The Agent and task types](agent-and-tasks.md) |
| `browser/` | The 13 `Browser…` members' own vocabulary, served on the same interface as the desktop members: sessions, snapshots, actions, screenshots, downloads | [Browser types](browser.md) |
| `version/` | `CONTRACT_VERSION` and the rule a host uses to decide whether it can bind to a given module | [Versioning and compatibility](versioning.md) |

Each page below explains the *why*, links to the source for the exact field
list, and shows real JSON pulled from the crate's own tests (the tests pin the
wire format, so what you see there is what actually goes over the wire).

## Pages

- [The envelope and errors](envelope-and-errors.md): every desktop reply's shape, and how a failure is described.
- [Members and names](members-and-names.md): all 80 members, grouped by family, and why names are constants rather than strings.
- [The shared vocabulary](vocabulary.md): surfaces, buttons, modifiers, and the other small enums every payload reuses.
- [Writing flows, the wire format](flows.md): the `Flow` grammar as JSON, `RunFlowRequest`, and how a run reports itself.
- [The goal loop](goal-loop.md): `RunGoal` and `ResolveIntent`, the bounded loop underneath flows.
- [The Agent and task types](agent-and-tasks.md): `StartTask`, `ContinueTask`, `TaskStatus`, and the rest of the task API's wire shapes.
- [Browser types](browser.md): sessions, snapshots, actions, and screenshots for the 13 browser members.
- [Versioning and compatibility](versioning.md): `CONTRACT_VERSION`, `ENVELOPE_VERSION`, and the bind rule.

For the cross-cutting story of how a task decides what to do, see
[How it decides](../../how-it-decides.md) and
[docs/technical/decision-loops.md](../../technical/decision-loops.md). For
what happens when something goes wrong mid-run, see
[Catching mistakes](../../catching-mistakes.md) and
[Rescue](../../rescue.md). For what is and is not shown to the decision model,
see [Safety and privacy](../../safety-and-privacy.md).
