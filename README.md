# tinydesktop

An installable TinyBus module that lets an agent use a computer: desktop
applications through their accessibility trees, and web pages through a real
Chrome. A host loads one `cdylib`, and an agent behind that host can click
through Mail or fill in a booking form without screenshots, pixel matching, or
any knowledge of the application's interface.

The module works at three levels, and a caller picks whichever fits:

- **Primitives.** 54 typed desktop members (`Snapshot`, `Click`, `SetValue`,
  `Press`, `Launch`, …) over the vendored
  [`agent-desktop`](https://github.com/lahfir/agent-desktop) engine. The caller
  decides everything.
- **Flows.** A short, plain-language script of what to accomplish ("start a new
  email message", `enter` a recipient and subject, `stop_before` sending). The
  module works out how on the live screen by asking Jev, TypeSafe's decision
  model, many small questions.
- **Tasks.** One call hands over a whole job, such as "find the cheapest flight
  from Delhi to Srinagar on 14 October and fill in my details up to payment". It
  runs in the background across the browser and desktop apps, pauses for
  missing details and approvals, and always stops at payment.

## A task, from the caller's side

```json
{"member": "StartTask", "args": [{
  "task": "Find the cheapest one-way flight from Delhi to Srinagar on 14 October and fill in my details up to payment",
  "facts": {"first name": "Asha", "last name": "Raina", "email": "asha@example.com"},
  "constraints": {"surfaces": ["browser"]}
}]}
```

`StartTask` returns at once with a task id. The caller long-polls `AwaitTask`
until the status asks for something:

- `needs_input`: a detail is missing; answer with `ContinueTask({inputs})`.
- `needs_approval`: the task found an irreversible action (send, delete,
  confirm a booking); answer with `ContinueTask({approve})`.
- `needs_human`: a captcha, a one-time code, or a login wall; a person deals
  with it, then `ContinueTask`.
- `checkpoint`: the task reached the payment page and stopped. A person pays.
- `done`, `failed`, or `cancelled`.

Every view carries a one-sentence `summary`, progress, the current step, and
`next`, the calls that make sense from here.

The caller's details are typed into fields locally. Jev and the planner only
ever see their names. Card data is refused outright.
[`docs/tasks.md`](docs/tasks.md) walks through a full session.

## How it works

The core of the module is a loop that grounds a plain-language step on a
screen it has never seen:

```text
 look at the screen ─► ask Jev small questions ─► act through a closed operation
        ▲                                                   │
        └──────── check what changed, recover if worse ◄────┘
```

Jev never writes text and never plans. It answers three kinds of closed
question: yes/no (a Noul), a position on a scale (a Score), or a pick among
labelled options (a Choice), each with probabilities. Deterministic Rust does
the rest. For a step like "start a new email message" one turn asks, in a
single request:

- is the step done, and is it still not done (averaged, to cancel a model's
  lean toward "yes");
- how far along it is, on five levels;
- whether something unrelated is in the way;
- which kind of move to make: press a control, use a standard shortcut,
  expand, scroll, wait, or give up.

Pressing a control needs one element out of possibly hundreds. The runtime
never shows Jev more than 20 options at once. It narrows by screen region,
re-asks with the options reversed and relabelled when the first answer is not
confident, and asks "is this the right element?" before using a doubtful pick.
After acting it compares the screen before and after, bans a control that did
nothing, undoes one that made things worse, and dismisses dialogs that get in
the way.

Text goes in by setting the field's value and reading it back. If it did not
arrive, the runtime pastes it and reads it back again. Irreversible controls
are never pressed by an ordinary step, and a screen with card fields is never
clicked through.

[`docs/decision-loops.md`](docs/decision-loops.md) explains every step kind,
question, and threshold in detail.

## Architecture

```text
 tinydesktop (cdylib)        TinyBus glue: manifest, dispatch, config
        │
 tinydesktop-engine          Jev runtime, flow runtime, workspace, tasks, planner
        │                 │
 tinydesktop-desktop    tinydesktop-browser       adapters, one Surface each
        │                 │
 vendor/agent-desktop   vendor/agent-browser      the engines, pinned by gitlink

 tinydesktop-core            Surface trait, screen model, keymap, safety, facts
 tinydesktop-bus             the wire contract, with no runtime at all
```

Dependencies point one way: `bus` ← `core` ← {`desktop`, `browser`} ←
`engine` ← `tinydesktop`. The decision loops are written once against the
`Surface` trait in `tinydesktop-core`, so they run the same over a desktop
window and a browser tab. A `Workspace` joins the two, so one flow can search
flights on the web and then write an email in Mail.

| Crate | Holds |
|---|---|
| `tinydesktop-bus` | every type that crosses the bus: member names, payloads, the `DesktopResponse` envelope, the Agent and browser types, the flow grammar and guide, the contract version |
| `tinydesktop-core` | the `Surface` trait and `Screen`; verified text delivery; result cards; the per-OS keymap; the safety classifier and payment detector; facts; price, time, duration, and stop parsers |
| `tinydesktop-desktop` | `Desktop`, one typed method per desktop member, with a permission preflight; `Desktop` as a `Surface` |
| `tinydesktop-browser` | `Browser` sessions over agent-browser linked in-process; `BrowserSurface` |
| `tinydesktop-engine` | Jev, `RunGoal`, `ResolveIntent`, the flow runtime, the workspace, the task controller, the optional planner |
| `tinydesktop` | the TinyBus module; no behaviour of its own |
| `tinydesktop-skills` | agent-facing `SKILL.md` and schemas for the task API |
| `tinydesktop-examples` | examples, the lab, the travel fixture, live verifiers |

A host that only makes calls depends on `tinydesktop-bus` alone and compiles no
engine, no TinyBus, and no accessibility backend. `tinydesktop` re-exports the
contract, so `tinydesktop::SnapshotRequest` and
`tinydesktop_bus::SnapshotRequest` are the same type rather than structural
twins. The adapters are plain libraries that another host can take without the
engine.

[`docs/architecture.md`](docs/architecture.md) covers how a call travels
through the layers, threading, configuration, and the safety checks.

## The members

All 67 members are served on `ai.tinyhumans.tinydesktop.Desktop` at
`/ai/tinyhumans/tinydesktop/Desktop`. `tinydesktop_bus::names::METHODS` lists
them in dispatch order; here they are by family:

| Family | Members |
| --- | --- |
| Tasks | `Describe` `PlanTask` `StartTask` `AwaitTask` `ContinueTask` `CancelTask` `TaskReport` `ListTasks` |
| Goals and flows | `ResolveIntent` `RunGoal` `RunFlow` `ValidateFlow` `FlowGuide` |
| Observation | `Snapshot` `Find` `Get` `Is` `Screenshot` |
| Interaction | `Click` `DoubleClick` `TripleClick` `RightClick` `Type` `SetValue` `Clear` `Focus` `Select` `Toggle` `Check` `Uncheck` `Expand` `Collapse` `Scroll` `ScrollTo` |
| Input | `Press` `KeyDown` `KeyUp` `Hover` `Drag` `MouseMove` `MouseClick` `MouseDown` `MouseUp` `MouseWheel` |
| Apps and windows | `Launch` `CloseApp` `ListApps` `ListWindows` `ListDisplays` `ListSurfaces` `FocusWindow` `ResizeWindow` `MoveWindow` `Minimize` `Maximize` `Restore` |
| Clipboard | `ClipboardGet` `ClipboardSet` `ClipboardClear` |
| Notifications | `ListNotifications` `NotificationAction` `DismissNotification` `DismissAllNotifications` |
| Waiting | `Wait` |
| System | `Version` `Status` `Permissions` |

`StartTask`, `ContinueTask`, `TaskReport`, `RunGoal`, `ResolveIntent`, and
`RunFlow` are confidential: they carry the caller's values and page data, so
the bus requires an attested module and keeps them away from monitors.

The browser's typed contract (`OpenSession`, `Navigate`, `Snapshot`,
`Perform`, `ReadPage`, `Screenshot`, downloads, and outputs) is defined in
`tinydesktop-bus` and implemented by `tinydesktop-browser`, but not yet served
as bus members. Today the browser is reached through tasks.

### Primitives: observe, then act on what you observed

`Snapshot` walks an application's accessibility tree and returns a compact
description in which every element carries a ref, such as `@s8f3k2p9:e1`.
Interaction members take refs. They do not take coordinates, and they do not
take selectors evaluated fresh at click time.

That indirection is the design. A ref is bound to the snapshot it came from,
so acting on one either reaches the element that was described or fails with
`STALE_REF` and asks for a fresh snapshot. It will not click whatever has since
moved into that position.

```rust,ignore
use tinydesktop_bus::{names, DesktopResponse, RefRequest, SnapshotRequest};

let proxy = connection.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;

// `skeleton` caps the walk at three levels: structure without leaf detail,
// enough to decide where to look before spending a full walk on a subtree.
let tree: DesktopResponse = proxy
    .call(names::methods::SNAPSHOT, (SnapshotRequest {
        app: Some("Safari".to_owned()),
        skeleton: true,
        ..Default::default()
    },))
    .await?;

let clicked: DesktopResponse = proxy
    .call(names::methods::CLICK, (RefRequest::new("@s8f3k2p9:e1"),))
    .await?;
```

Ref actions go through the platform's accessibility API rather than
synthesized input, so by default they do not steal focus, move the cursor, or
touch the clipboard. A run can proceed while someone else is using the
machine. Headed mode and the `Input` members exist for the cases that need a
real cursor.

Every member returns a `DesktopResponse`, on success and on failure, carrying
either the data or a structured error with its code, a suggestion, whether a
retry is safe, and how to recover. A TinyBus error is reserved for a transport
or dispatch failure. See
[`crates/tinydesktop-bus/README.md`](crates/tinydesktop-bus/README.md).

`KeyDown`, `KeyUp`, `MouseDown`, and `MouseUp` validate their arguments and then
refuse: holding a button down is stateful and this module is not. They are
served rather than omitted so the refusal is an explained reply naming what to
use instead, rather than an `UnknownMethod`.

### Flows

`RunFlow` runs a flow: an `app`, optional `vars`, and steps that say what to
accomplish, never how.

```json
{ "app": "Mail",
  "steps": [
    { "open": "Mail" },
    "start a new email message",
    { "enter": { "recipient": "sam@example.com", "subject": "Friday", "message body": "Hi Sam, …" } },
    { "verify": "the draft shows the recipient, the subject, and the message body" },
    { "stop_before": "sending the email" } ] }
```

The step kinds are `do` (or a plain string), `open`, `browse`, `enter`,
`choose`, `read`, `extract`, `pick`, `verify`, `wait_for`, `stop_before`,
`repeat_until`, and `if`. `FlowGuide` returns the authoring guide as prompt
text, `ValidateFlow` checks a candidate without touching anything, and the
guide itself is
[`crates/tinydesktop-bus/src/flow/guide.md`](crates/tinydesktop-bus/src/flow/guide.md).
Nothing irreversible happens without `allow_destructive`. The design and its
rationale are in [`docs/specs/jev-intent-flows.md`](docs/specs/jev-intent-flows.md).

### Goals

`RunGoal` is the older single-goal loop for the desktop. The host sends one
bounded goal with an exact window, allowed operations and target labels,
prepared text, and success predicates the module checks on the accessibility
tree after every action. By default an action judged hard to undo stops with a
one-use `confirmation_id`; the host shows it to a person and calls again with
`continuation: {"id": "...", "approve": true}`. Approval takes a fresh
snapshot and acts only if exactly one element still matches the original
target. `ResolveIntent` grounds one described element, optionally acting on it.
See [`crates/tinydesktop-engine/src/agentic/README.md`](crates/tinydesktop-engine/src/agentic/README.md).

## Configuration

The host passes the module a JSON object at load and on reinitialization.
TinyBus treats it as sensitive host-control traffic: monitors never receive it,
and its buffers are zeroized after use. Reinitialization replaces the served
object only after the whole new configuration validates.

| Key | Meaning |
|---|---|
| `session_id`, `trace_path`, `trace_strict`, `headed` | agent-desktop's session, trace, and input mode |
| `jev` | Jev provider, API key, and optional model, endpoint, timeout, retries, and `sdk_name` |
| `planner` | an OpenRouter `api_key` and optional `model`; absent means tasks need a flow |
| `browser.executable` | the Chrome or Chromium binary to launch |

Without `jev`, the Jev-driven members answer `JEV_NOT_CONFIGURED` and the
primitives keep working. For the TinyHumans proxy, the host supplies
`jev.sdk_name`, and the Jev client sends it as `x-sdk-name` only to that exact
endpoint.

## Safety

Several independent checks keep a run from doing something that cannot be
undone. None of them relies on a model's judgement.

- An ordinary step refuses to press a control whose label reads as
  destructive, that the flow's own `stop_before` names, or that is an unnamed
  button in a confirmation sheet. Only `stop_before` reaches one, and it
  presses only with `allow_destructive` or an approval.
- A payment control, or any click on a screen showing card fields, stops the
  run at a checkpoint. Nothing ever types payment data.
- Jev and the planner see fact names, never values. Values are redacted from
  every summary.
- Screen text is always passed to Jev as untrusted data, and a move Jev was
  not offered fails closed.
- Budgets cap a task's actions, Jev calls, and time across all its runs, and
  constraints confine it to chosen surfaces and origins.

## Permissions and platforms

Desktop automation needs permissions a person grants: accessibility access,
and screen recording for captures. The module checks before it acts, because
an accessibility API called by an unauthorized process usually returns an
empty tree rather than an error. The snapshot succeeds, finds nothing, and
the click reports the element is not there, which looks exactly like an
application with no such button. Checking first turns that into `PERM_DENIED`
naming the setting to change.

macOS and Windows have full accessibility backends. Linux builds, loads, and
answers, but implements no surfaces yet, so desktop observation there fails
with `PLATFORM_NOT_SUPPORTED`. That comes from the vendored engine and will
follow it. The browser works wherever Chrome or Chromium runs.

## The lab

`scripts/lab` builds and attests the module, runs scenarios against real
macOS applications, checks the real application state, and writes a timeline
and every Jev exchange for each run:

```sh
scripts/lab run mail-compose                  # a hand-written flow
scripts/lab run mail-compose --mode goal      # the single-goal RunGoal baseline
scripts/lab run mail-compose --mode authored  # an LLM writes the flow
scripts/lab eval all --modes flow,goal --trials 3
```

Anything that launches Chromium runs in a Linux container instead of on the
host:

```sh
scripts/docker-lab -- crates/tinydesktop-examples/fixtures/run task_fixture
```

See [`docs/lab.md`](docs/lab.md) and [`docs/docker-lab.md`](docs/docker-lab.md).

## Layout

```text
Cargo.toml                 # virtual workspace: members, shared metadata, lints
crates/
├── tinydesktop-bus/       # the wire contract
│   └── src/
│       ├── names/         # interface, object path, one constant per member
│       ├── envelope/      # DesktopResponse and DesktopError
│       ├── agent/         # the task API: TaskView, TaskStatus, requests
│       ├── agentic/       # RunGoal, ResolveIntent, Jev configuration
│       ├── flow/          # the flow grammar and guide.md
│       ├── browser/       # the browser contract
│       └── observation/ interaction/ input/ apps/ clipboard/ …
├── tinydesktop-core/      # surface/ keymap/ safety/ records/ facts/
├── tinydesktop-desktop/   # desktop/ (the 54 members) and surface/
├── tinydesktop-browser/   # sessions/ convert/ reply/ linked/ outputs/ surface/
├── tinydesktop-engine/
│   └── src/
│       ├── agentic/       # RunGoal and ResolveIntent; flow/ is the flow runtime
│       ├── workspace/     # the desktop and the browser as one surface
│       ├── task/          # the task controller
│       └── planner/       # the optional LLM planner
├── tinydesktop/           # the cdylib: tinybus_module/ dispatch, runner, manifest
├── tinydesktop-skills/    # SKILL.md for agents
└── tinydesktop-examples/  # bins, the lab, scenarios, fixtures/travel
vendor/
├── tinybus/               # TinyBus host types and module SDK
├── agent-desktop/         # the desktop engine
├── agent-browser/         # the browser engine, linked as a library
└── tinyinference/         # the Jev client, and the LLM client for the planner
docs/                      # architecture, loops, tasks, lab, specs, plans, ADRs
```

Within each crate, feature areas are directory modules: `mod.rs` for the
implementation and exports, `types.rs` for substantial types, and `test.rs` for
unit tests. [`AGENTS.md`](AGENTS.md) holds the full repository guidance, and
`CLAUDE.md` is a symlink to it.

## The vendored engines

The four submodules are pinned by gitlink, and this repository never edits
them. A wrong tree or a wrong click is fixed upstream and arrives here as a
gitlink bump in its own commit. `vendor/agent-browser` currently points at the
`library-target` branch of `tinyhumansai/agent-browser`, which adds the
library target this module links; it moves back to upstream once that change
lands there.

## Development

Clone with submodules, or initialize them before building:

```sh
git submodule update --init --recursive
```

The four checks CI runs:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
```

Useful extras:

```sh
cargo run -p tinydesktop-examples --bin basic
cargo build -p tinydesktop --release --lib   # the installable cdylib
cargo doc --no-deps --all-features           # CI builds it with RUSTDOCFLAGS="-D warnings"
cargo deny check all                         # supply chain; see deny.toml
.github/scripts/check-file-coverage.sh 90 coverage.json   # needs cargo-llvm-cov

# Load the built cdylib through the real TinyBus dynamic loader:
cargo run -p tinydesktop-examples --bin verify_module -- \
  target/release/libtinydesktop.so
```

The test suite needs no display server, no granted permission, no browser, and
no network. It covers the contract's wire form, the conversions, the
permission preflight, the browser adapter against a scripted engine, the flow
runtime against a simulated application and an oracle Jev, the task
controller against scripted runs, and the served interface over the in-memory
transport. Anything that drives a real application or site is a live run, kept
out of the suite.

## Releasing

Run the **Release** workflow from the Actions tab with a `patch`, `minor`, or
`major` bump. It validates the workspace and opens a version pull request for
the one `[workspace.package]` version every member inherits. Merge that PR
after required checks pass, then run the workflow with `current` to tag the
checked version, build `crates/tinydesktop` as a TinyBus `cdylib`, and create
the GitHub release. Use `current` again to resume an interrupted release.

Assets are named `tinydesktop-<version>-<platform>.<tar.gz|zip>` and contain
the native module, its SHA-256 `modules.toml`, the license, and
[`MODULE.md`](MODULE.md). Every release also publishes `checksum.toml`, which
TinyBus uses to verify an archive before extracting it. The workflow loads the
published Ubuntu archive through TinyBus's GitHub release API and calls its
`Version` member before declaring the release successful.

The native matrix covers Ubuntu 22.04 and 24.04 on x86_64 and ARM64; Fedora 43
and 44 on x86_64 and ARM64; Arch Linux on x86_64; macOS 15 and 26 on Intel and
Apple Silicon; Windows Server 2022 and 2025 on x86_64; and Windows 11 on ARM64.
Do not hand-edit the version in the root `Cargo.toml`.

## Documentation

- [`docs/architecture.md`](docs/architecture.md): the layers, how a call
  travels, threading, configuration, safety
- [`docs/decision-loops.md`](docs/decision-loops.md): how the flow runtime
  grounds each step, question by question
- [`docs/tasks.md`](docs/tasks.md): the task API, pausing and resuming,
  private values, the planner
- [`docs/lab.md`](docs/lab.md) and [`docs/docker-lab.md`](docs/docker-lab.md):
  live runs
- [`docs/specs/`](docs/specs/README.md), [`docs/plans/`](docs/plans/README.md),
  [`docs/adr/`](docs/adr/0001-record-architecture-decisions.md): specs, plans,
  and decision records
- [`AGENTS.md`](AGENTS.md), [`CONTRIBUTING.md`](CONTRIBUTING.md),
  [`SECURITY.md`](SECURITY.md)

## License

GPL-3.0-only. See [LICENSE](LICENSE). The vendored `agent-desktop` and
`agent-browser` engines are Apache-2.0; TinyBus and TinyInference are vendored
under their own licenses.
