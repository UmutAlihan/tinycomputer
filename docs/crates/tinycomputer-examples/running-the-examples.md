# Running the examples

Every binary lives under
[`src/bin/`](../../../crates/tinycomputer-examples/src/bin/) and is run with
`cargo run -p tinycomputer-examples --bin <name> -- <args>`. Two of them
(`lab` and `task_live`) have their own wrapper scripts that build the module
and set up the environment first; use those instead of calling `cargo run`
on them directly. The rest you can run as they are.

None of these binaries are part of the shipped module. They exist so a
change to the engine, the desktop adapter, or the browser adapter can be seen
doing something, rather than only passing a unit test.

## The one with nothing to set up: `basic`

```sh
cargo run -p tinycomputer-examples --bin basic
```

Calls `Desktop` directly, with no bus, no permission, and no running
application: version, permissions (reporting only, never a prompt), and the
window server's own app list. This is the first thing to run on a machine
you have not configured yet, and the one example that always works. There is
a commented-out snapshot call at the bottom you can turn on once
Accessibility is granted, to see a real tree.

## Proving a built module works: `verify_module` and `verify_github_release`

```sh
cargo build --release -p tinycomputer --lib
cargo run -p tinycomputer-examples --bin verify_module -- target/release/libtinycomputer.dylib
```

Loads a compiled `cdylib` through the real TinyBus dynamic loader (not the
in-process test path the unit tests use), waits for it to claim its bus
name, and calls `Version`. A release archive should not be trusted until
this passes against the exact artifact that would ship.

`verify_github_release` does the same check against a tagged GitHub release
instead of a local file: give it the release tag URL, the archive file name
for your platform, and the archive's SHA-256.

```sh
cargo run -p tinycomputer-examples --bin verify_github_release -- \
  https://github.com/tinyhumansai/tinycomputer/releases/tag/v0.1.4 \
  tinycomputer-0.1.4-ubuntu-24.04-x86_64.tar.gz \
  <sha256>
```

## The scored evaluation harness: `lab`

```sh
scripts/lab list
scripts/lab run mail-compose
```

The lab drives real desktop applications, scores whether a run actually did
the thing it was asked to, and writes a timeline you can read afterward. It
is its own page: see [the lab](the-lab.md). Always run it through
`scripts/lab`, not `cargo run` directly; the script builds the module in
release mode, attests it, and puts the clipboard helper beside it, all of
which the lab needs to behave like a real host.

## Reading a debug journal: `jev_journal`

```sh
cargo run -p tinycomputer-examples --bin jev_journal -- latest
```

Lists journaled runs, or summarises and prints one. See
[reading a journal](reading-a-journal.md).

## Whole tasks against real websites: `task_live` and `task_fixture`

```sh
scripts/docker-lab -- crates/tinycomputer-examples/tasks/run kashmir
```

`task_live` runs a plain-language task end to end: the planner writes a
flow, the task controller runs it with live Jev, and the run is followed
until it stops at the payment checkpoint. See [live tasks](live-tasks.md).

`task_fixture` runs the same kind of task, but against the local travel
fixture instead of a real website, so the result is exact and repeatable:

```sh
scripts/docker-lab -- crates/tinycomputer-examples/fixtures/run task_fixture
```

Both need `OPENROUTER_API_KEY` and a browser, so both run inside the Docker
lab; see [Docker lab](docker-lab.md) for why.

## Checking the browser stack without spending Jev credit: `browser_fixture`

```sh
scripts/docker-lab -- bash -c '
  python3 -m http.server 8765 --directory crates/tinycomputer-examples/fixtures/travel &
  cargo run -p tinycomputer-examples --bin browser_fixture'
```

Or more simply, since it does the same thing:

```sh
scripts/docker-lab -- crates/tinycomputer-examples/fixtures/run browser_fixture
```

Drives the travel fixture in a real browser and checks each layer the
browser surface relies on (snapshot parsing, result-card grouping and
ranking, payment-page detection) without asking Jev anything. It is the
fast check that the browser adapter itself still works, before spending
credit on a flow or task that exercises Jev on top of it.

`TINYCOMPUTER_FIXTURE_URL` overrides the fixture's address, and
`TINYCOMPUTER_BROWSER_EXECUTABLE` names the browser binary when discovery
would not find one on its own.

## Looking at a real page before writing a flow for it: `site_probe`

```sh
scripts/docker-lab -- cargo run -p tinycomputer-examples --bin site_probe -- \
  'https://example.com/search'
```

Opens each URL given on the command line and prints what a flow would see
there: the title, what is in front (a cookie sheet, say), the first
actionable controls, any repeated result cards, and whether a captcha or
login wall is blocking the page. This is the research step before pointing
`task_live` at a site you have not tried yet: it is how the notes in
[`tasks/kashmir/task.md`](../../../crates/tinycomputer-examples/tasks/kashmir/task.md)
and
[`tasks/emirates/task.md`](../../../crates/tinycomputer-examples/tasks/emirates/task.md)
were written.

A few extra arguments, given after a URL, act on the page that is already
open instead of opening a new one:

| Argument | Effect |
|---|---|
| `click=<name>` | clicks the first control whose name contains `<name>`, then prints the page again |
| `type=<text>` | types `<text>` with key presses into the focused field, but only after checking that field actually takes text; the text itself is never printed |
| `mouse=<x>,<y>` | clicks at that exact point with real mouse events |

`PROBE_ENDPOINT` attaches to a Chrome that is already running instead of
launching a fresh one; give the URL `current` to read the page it already
shows rather than navigating anywhere. `PROBE_WAIT_SECS` changes how long it
waits after navigating or acting before it reads the page (8 seconds by
default). `PROBE_JS` runs a script and prints the result; `PROBE_GREP`
prints the raw snapshot lines containing that text, case-insensitively.
Values are never printed for any control (only its role and name), so a
card number or password typed in an earlier step cannot leak into the
output.

## Touring the on-screen cursor: `cursor_demo`

```sh
cargo build -p tinycomputer-cursor --features overlay
cargo run -p tinycomputer-examples --bin cursor_demo -- calm 3
```

Moves the agent's cursor around a dozen made-up targets so its look and
motion can be judged without running an agent at all. Nothing is clicked;
the cursor is only drawn. Give it a pace (`brisk`, `natural`, or `calm`) and
a number of laps. See the cursor crate's own docs for what a pace changes.

## Direct, opt-in exercises against a loaded module

A handful of small binaries call a loaded module directly over the bus,
bypassing the lab and the task API, for narrow checks during development:

- **`live_probe`** — `live_probe <attested-module> <app> <name-fragment>`
  prints only the accessibility nodes matching a name fragment, and their
  parents. Read-only; no Jev, no permission beyond what a snapshot already
  needs.
- **`live_goal`** — `live_goal <probe|run> <attested-module>` exercises a
  bounded Calculator goal. `probe` just prints Calculator's controls;
  `run` needs `OPENROUTER_API_KEY` and performs a real scoped calculation,
  then checks the visible result.
- **`live_spotify`** — an opt-in, real-module Jev exercise against Spotify
  on macOS. It spends OpenRouter credit and changes Spotify's visible
  playback state, so run it only when you mean to.

All three need a module built and attested (`modules.toml` beside the
`.dylib`/`.so`), the same way `scripts/lab` builds one; point them at
`target/lab/libtinycomputer.dylib` (or the Linux equivalent) after a lab run
has built it, or build one the same way yourself.

## Environment variables these binaries read

Every variable is documented by name, with a placeholder, in
[`.env.example`](../../../.env.example) at the repository root. The ones
that matter most for this crate:

| Variable | Used by | For |
|---|---|---|
| `OPENROUTER_API_KEY` | `lab`, `task_live`, `task_fixture`, `live_goal`, `live_spotify` | Jev, and the optional LLM author or planner |
| `TINYCOMPUTER_MODULE` | `scripts/lab` | a module to load instead of building one |
| `TINYCOMPUTER_JEV_JOURNAL` | any of them | turns on the debug journal |
| `TINYCOMPUTER_FLOW_STRATEGY`, `TINYCOMPUTER_FLOW_DELIBERATION` | `lab`, `task_live`, `task_fixture` | narrow vs. wide asking, and how much a decision deliberates |
| `TINYCOMPUTER_BROWSER_EXECUTABLE`, `TINYCOMPUTER_BROWSER_ARGS`, `TINYCOMPUTER_BROWSER_USER_AGENT`, `TINYCOMPUTER_BROWSER_ENDPOINT` | any browser example | how a browser launches, or one to attach to |
| `TINYCOMPUTER_FIXTURE_URL` | `browser_fixture`, `task_fixture` | the travel fixture's address |
| `TINYCOMPUTER_BROWSER_PERCEPTION` | any browser example | read by sight (default) or by the accessibility tree alone |

[Live tasks](live-tasks.md) covers the variables specific to `task_live` in
full; there are more of those than fit here.
