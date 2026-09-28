# Repository layout

A map of every top-level folder and file in tinycomputer, and where to read
more about each. If you are looking for how the system behaves rather than
where its pieces live, start at
[`docs/how-it-works.md`](../how-it-works.md) or
[`docs/architecture.md`](../architecture.md) instead.

## What tinycomputer is

tinycomputer is an installable [TinyBus](https://github.com/tinyhumansai/tinybus)
module that lets an agent use a computer: desktop applications through their
accessibility trees, and web pages through a real Chrome. It adapts two
vendored engines, [`agent-desktop`](https://github.com/lahfir/agent-desktop)
for the desktop and [`agent-browser`](https://github.com/vercel-labs/agent-browser)
for the browser, into typed tool calls, Jev-driven decision loops, and
background tasks. It is an adapter, not an engine: if a click lands on the
wrong element or a snapshot returns the wrong tree, that is a bug in the
vendored engine, fixed there and picked up here as a pinned-commit bump.

## Top-level folders

| Folder | What it holds |
|---|---|
| [`crates/`](#crates) | every Rust package in the workspace |
| [`vendor/`](vendor/README.md) | four pinned git submodules: the engines and clients this adapter wraps |
| [`docs/`](../README.md) | architecture, specs, plans, guides, and this project documentation |
| [`scripts/`](scripts/README.md) | developer tools: the lab, the docker lab, the journal inspector |
| [`docker/`](docker/README.md) | the Dockerfile the docker lab builds from |
| [`.github/`](ci-and-tooling/README.md) | CI workflows, the release workflow, and issue/PR templates |
| `.jev-journal/` | git-ignored; debug journals written by local runs (see [`docs/jev-journal.md`](../jev-journal.md)) |
| `worktrees/` | git-ignored; `git worktree` checkouts used for isolated feature work |

## Top-level files

| File | What it's for |
|---|---|
| `README.md` | the project's public-facing overview: what it does, the three levels a caller can use, a task example |
| `AGENTS.md` (symlinked as `CLAUDE.md`) | the single source of truth for how humans and coding agents work in this repository |
| `MODULE.md` | what the packaged module claims and serves once installed: its interface name, object path, and full member list |
| `ROADMAP.md` | what is shipped, what is next, and what is deliberately out of scope |
| `Cargo.toml` | the virtual workspace manifest: members, shared package metadata, shared dependencies, and lints (see [`docs/project/ci-and-tooling/README.md`](ci-and-tooling/README.md)) |
| `Cargo.lock` | the single lockfile for the whole workspace, kept committed for reproducible builds |
| `deny.toml` | `cargo-deny` configuration: advisories, license allow-list, dependency bans, allowed sources |
| `.env.example` | every environment variable any crate, test, or example reads, documented with a placeholder |
| `.gitmodules` | the four vendored submodules' URLs and tracked branches |
| `.gitignore` | build output, `.env`, `.jev-journal/`, and other local-only paths |
| `LICENSE` | GPL-3.0-only |
| `CONTRIBUTING.md` | the short path through `AGENTS.md`: setup, the four contract commands, and how to send a change |
| `CODE_OF_CONDUCT.md` | expected and unacceptable behavior for anyone participating in the project |
| `SECURITY.md` | how to report a vulnerability (never as a public issue) |
| `SUPPORT.md` | where to take a question, a bug, a feature idea, or a security report |

## crates/

Every Rust package lives under `crates/`, one directory per package, named
for the package it holds; there is no root package. Each crate has its own
README once its own documentation pass lands:

| Crate | Role |
|---|---|
| [`tinycomputer-bus`](../crates/tinycomputer-bus/README.md) | the wire contract: every type that crosses the bus, and the member names, with no runtime dependencies |
| [`tinycomputer-core`](../crates/tinycomputer-core/README.md) | shared, engine-free domain logic: the `Surface` trait, keys, safety rules, records and facts |
| [`tinycomputer-cursor`](../crates/tinycomputer-cursor/README.md) | the agent's on-screen cursor and the overlay window that draws it |
| [`tinycomputer-desktop`](../crates/tinycomputer-desktop/README.md) | the `agent-desktop` adapter: one method per desktop member, conversion, and the permission preflight |
| [`tinycomputer-browser`](../crates/tinycomputer-browser/README.md) | the `agent-browser` adapter: typed sessions over the linked engine |
| [`tinycomputer-engine`](../crates/tinycomputer-engine/README.md) | the agent runtime: Jev decision loops, `RunGoal`, intent flows, and the task controller |
| [`tinycomputer`](../crates/tinycomputer/README.md) | the module itself: TinyBus glue and the `cdylib`, no behavior of its own |
| [`tinycomputer-skills`](../crates/tinycomputer-skills/README.md) | the agent-facing `SKILL.md` and schemas for the task API |
| [`tinycomputer-examples`](../crates/tinycomputer-examples/README.md) | runnable examples, the lab binary, and the journal reader |

The crate split matters: `tinycomputer-bus` has no transport or engine, so a
host that only makes calls can depend on it alone; `tinycomputer-desktop` and
`tinycomputer-browser` wrap the vendored engines; `tinycomputer-engine`
builds the Jev loops on top of both; and `tinycomputer` serves everything
over the bus and re-exports the contract types, so `tinycomputer::SnapshotRequest`
and `tinycomputer_bus::SnapshotRequest` are the same type. See
[`docs/architecture.md`](../architecture.md) for the full picture and "How a
call travels."

## Where to go from here

- New to the project: [`README.md`](../../README.md) at the repository root,
  then [`docs/architecture.md`](../architecture.md)
- Giving the module a task to run: [`docs/giving-it-a-task.md`](../giving-it-a-task.md)
- How the system decides what to do on screen: [`docs/how-it-decides.md`](../how-it-decides.md),
  [`docs/decision-loops.md`](../decision-loops.md)
- Writing or reviewing a flow: [`docs/writing-flows.md`](../writing-flows.md),
  [`docs/flow-examples.md`](../flow-examples.md)
- What happens when a run goes wrong: [`docs/catching-mistakes.md`](../catching-mistakes.md),
  [`docs/rescue.md`](../rescue.md)
- What the module remembers between runs: [`docs/memory-and-saving.md`](../memory-and-saving.md)
- How the module reads the screen: [`docs/seeing-the-screen.md`](../seeing-the-screen.md)
- Data handling and permissions: [`docs/safety-and-privacy.md`](../safety-and-privacy.md)
- Watching a run happen live: [`docs/watching-a-run.md`](../watching-a-run.md)
- Unfamiliar term: [`docs/glossary.md`](../glossary.md)
- Running things on a real desktop or browser:
  [`docs/lab.md`](../lab.md) and [`docs/docker-lab.md`](../docker-lab.md),
  or the developer-tool detail in [`scripts/README.md`](scripts/README.md)
  and [`docker/README.md`](docker/README.md)
- The vendored engines this all sits on:
  [`vendor/README.md`](vendor/README.md)
- CI, releases, and root configuration:
  [`ci-and-tooling/README.md`](ci-and-tooling/README.md)
