# scripts/

Everything here is a developer tool. None of it ships in the module or runs
in production; it exists to build, run, and inspect tinycomputer while you
work on it.

| Path | What it is |
|---|---|
| `scripts/lab` | builds the module and runs it against a real desktop |
| `scripts/docker-lab` | runs a command in a Linux container with Chromium, for anything that launches a browser |
| `scripts/debug-ui` | a small Vite app for reading Jev debug journals |

## scripts/lab

`scripts/lab` is the development loop for Jev-driven desktop control. It
builds `crates/tinycomputer` in release mode, copies the resulting library into
`target/lab/`, writes a `modules.toml` beside it with the library's SHA-256
hash, and (on macOS) builds and installs the engine's clipboard helper next to
the module, because the engine only trusts that helper when it sits beside the
module it was built for. It then runs the `lab` binary from
`crates/tinycomputer-examples`, loading the module exactly the way a real host
would: built in release mode, attested by `modules.toml`, loaded through the
TinyBus dynamic loader.

If a `.env` file exists at the repository root, the script exports it before
running anything (see `docs/project/ci-and-tooling/README.md` for what
`.env.example` documents). Nothing from it is printed.

Commands (`list`, `guide`, and `report` never build or load the module):

| Command | What it does |
|---|---|
| `scripts/lab list` | print the scenario ladder |
| `scripts/lab run <scenario> [--mode flow\|goal\|authored]` | one run, with a timeline and a checker verdict |
| `scripts/lab eval <names\|all> --modes flow,goal --trials N` | many runs and a scorecard |
| `scripts/lab report <run-dir>` | re-print a run's timeline |
| `scripts/lab call <Member> '<json>'` | call any bus member directly, for probing |
| `scripts/lab validate <flow.json>` | check a flow |
| `scripts/lab guide` | print the flow authoring guide |

Useful flags: `--headed` (send physical input; the target app needs a window
to focus), `--disable moves,undo,…` (turn off named decision loops to measure
their effect), `--no-memory` (ignore grounding hints from earlier runs),
`--flow <file>` (run your own flow against a scenario's checker),
`--strategy narrow|wide` (how decisions get asked), `--send` (mail scenario
only, and only ever to `TINYCOMPUTER_LAB_SELF_EMAIL`).

Requirements: macOS, with Accessibility granted to the terminal running the
lab (Screen Recording too, for screenshots), an unlocked screen, and
`OPENROUTER_API_KEY` set, because Jev and the optional LLM author both go
through OpenRouter. A background shell with no Accessibility grant will fail
scenarios with `PERM_DENIED`; run the lab from a terminal you have granted
Accessibility to.

Full detail, including the scenario ladder and how to read a run, lives in
[`docs/technical/lab.md`](../../technical/lab.md).

## scripts/docker-lab

`scripts/docker-lab` runs a command inside a Linux container built from
`docker/lab/Dockerfile`, which has a Rust toolchain plus Playwright's Chromium
and its system libraries. Use it for anything that launches a browser, or
that writes and runs throwaway browser automation scripts, because some of
those scripts can trip macOS's malware protection when run directly on a Mac.
Inside the container nothing touches the host except the source tree you
mount in.

```sh
# a shell in this repository, inside the container
scripts/docker-lab

# this repository's contract commands, but on Linux
scripts/docker-lab -- cargo test --all-features

# the browser stack against the travel fixture, then a full task
scripts/docker-lab -- crates/tinycomputer-examples/fixtures/run browser_fixture
scripts/docker-lab -- crates/tinycomputer-examples/fixtures/run task_fixture

# a different checkout entirely, e.g. agent-browser's own crate
scripts/docker-lab --src ../agent-browser --workdir cli -- cargo test --profile ci
```

| Option | Meaning |
|---|---|
| `--src DIR` | directory mounted read-write at `/src` (default: this repository) |
| `--workdir SUBDIR` | directory under `/src` the command runs in |
| `--rebuild` | rebuild the image even when one with the same tag exists |
| `CMD` | the command to run (default: an interactive shell) |

Build output goes to `<workdir>/target/docker-lab`, next to but never mixed
with the host's own `target/`, because Linux and macOS build artifacts are not
interchangeable. The cargo registry is cached in a named Docker volume
(`tinycomputer-lab-cargo`) so repeated runs don't redownload crates. Only two
environment variables ever cross into the container, and only by name:
`OPENROUTER_API_KEY` and `TINYCOMPUTER_LAB_MODEL`; no value is ever written to
a command line.

The image tag is derived from a hash of the Dockerfile, so editing the
Dockerfile builds a fresh image the next time you run the script; you rarely
need `--rebuild` yourself.

More detail and more example invocations live in
[`docs/technical/docker-lab.md`](../../technical/docker-lab.md).

## docker/lab/Dockerfile

This is the image `scripts/docker-lab` builds from; see
[`docs/project/docker/README.md`](../docker/README.md) for what it contains.

## scripts/debug-ui

A local Vite single-page app for exploring the JSONL files written by the Jev
debug journal (see [`docs/technical/jev-journal.md`](../../technical/jev-journal.md)
for the event format and how to turn journaling on). It is a plain developer
tool: nothing here is part of the shipped module, and it never uploads
anything to a cloud service.

In development, its Vite dev server discovers run folders under this
checkout's `.jev-journal/` directory, lists every run, and loads the most
recently modified one automatically. The dev server's API is read-only and
only ever serves files named `journal.jsonl` under that directory. A built
static preview has no such API; it still lets you open journal files by hand.

Run it from the repository root:

```sh
cd scripts/debug-ui
npm install
npm run dev
```

Then open the local URL Vite prints. What it gives you:

- a run list, and a searchable, filterable event timeline for the selected run
- request and answer tabs for each Jev exchange, with expandable JSON trees
  (expand-all and collapse-all controls included)
- summary timing, plus per-run and per-call input and output token counts
- a folder picker (including nested run folders), a file picker for
  individual `journal.jsonl` files, and drag-and-drop; use "Add JSONL files"
  to add more files to what is already loaded
- malformed lines are skipped and reported, without losing the valid events
  around them

The dev server does not watch the filesystem, so a journal that grows while
you are looking at it will not refresh on its own: click the run-list refresh
button to discover new runs, or refresh the selected run to pick up appended
events while keeping your current selection and filters.

To build a static copy instead of running the dev server:

```sh
npm run build
npm run preview
```

Journal files can contain screen text and other data captured from a live
run, so treat anything you open here the same way you'd treat the journal
itself: see [`docs/safety-and-privacy.md`](../../safety-and-privacy.md) and
[`docs/technical/jev-journal.md`](../../technical/jev-journal.md).

The app is a small React + TypeScript project (`package.json`, `vite.config.ts`,
`src/main.tsx`, `src/ui/App.tsx`); there is nothing generated or vendored
under `scripts/debug-ui/src`.
