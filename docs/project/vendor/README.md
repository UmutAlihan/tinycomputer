# vendor/

Four git submodules, each pinned to an exact commit by a gitlink in this
repository's tree. This is where the actual automation behavior lives:
tinycomputer is an adapter on top of these engines, not an engine itself. If
something here does the wrong thing on screen, that is a bug in the vendored
project, fixed there, not patched from this repository.

```sh
git submodule update --init --recursive
```

after cloning, to check them out.

| Path | Project | Branch tracked | What it supplies |
|---|---|---|---|
| `vendor/tinybus` | [`tinyhumansai/tinybus`](https://github.com/tinyhumansai/tinybus) | `main` | the host types, module ABI, and module-side SDK the `cdylib` is built against |
| `vendor/agent-desktop` | [`lahfir/agent-desktop`](https://github.com/lahfir/agent-desktop) | (pinned commit, no tracked branch) | the accessibility-tree engine: one core crate plus one backend per platform |
| `vendor/agent-browser` | [`tinyhumansai/agent-browser`](https://github.com/tinyhumansai/agent-browser) | `library-target` | the Chrome-over-CDP browser automation engine, linked in-process |
| `vendor/tinyinference` | [`tinyhumansai/tinyinference`](https://github.com/tinyhumansai/tinyinference) | (pinned commit, no tracked branch) | the Jev decisions client, and (behind the engine's `planner` feature) an LLM client |

## Why each one is vendored

**`vendor/tinybus`** gives the module the exact host types, macros, and frozen
module ABI that `crates/tinycomputer` builds its `cdylib` against
(`tinybus` and `tinybus-module` in `Cargo.toml`, taken with only the
`macros` and `modules` features; the socket and CLI features are not
needed here). Pinning it means the ABI this module exports cannot drift
out from under a host that loads it.

**`vendor/agent-desktop`** is the desktop automation engine this whole
adapter wraps: accessibility-tree observation, ref allocation, and every
command behind a desktop member of the bus interface. `agent-desktop-core`
is platform-neutral; `agent-desktop-macos`, `agent-desktop-windows`, and
`agent-desktop-linux` are taken as target-specific dependencies and supply
one accessibility backend each. A wrong tree, a click that lands on the
wrong element, or a missing platform capability is a bug here, not in
`crates/tinycomputer-desktop`.

**`vendor/agent-browser`** is the browser automation engine, linked as a
library rather than run out-of-process: `tinycomputer-browser` calls
`execute_command` over its `DaemonState` directly. It is vendored from the
`tinyhumansai/agent-browser` fork's `library-target` branch, not from
`vercel-labs/agent-browser` upstream, because the library entry point this
adapter needs does not exist upstream yet
([vercel-labs/agent-browser#2008](https://github.com/vercel-labs/agent-browser/issues/2008)).
Once that lands upstream, the gitlink can move back to the canonical repo.

**`vendor/tinyinference`** supplies `tinyinference-decisions`, the Jev
client used by every decision loop in `crates/tinycomputer-engine`, pinned
so provider validation and retry behavior stay reproducible across builds.
Only that one crate of the submodule is linked into the shipped module.
Behind the engine's optional `planner` feature, and in the lab's `--mode
authored`, the submodule's `tinyinference-llm` crate supplies the LLM
client used for planning and for authoring flows. If the Jev client itself
has a bug, it is `tinyinference-decisions` in this submodule that needs the
fix, not anything in `crates/tinycomputer-engine`.

## The rule: never edit vendored code from here

Do not patch anything under `vendor/` from a commit in this repository. Make
the change in the vendored project's own repository, push and land it there,
then update this repository's gitlink to the new commit in its own separate
commit. This applies especially to `agent-desktop`: patching a wrong tree or a
missing platform surface here would strand the fix the moment the gitlink
next moves, because a submodule update overwrites whatever local edit was
made. Keep the exact path dependencies and minimal features each submodule is
taken with in the root `Cargo.toml` unless a new module capability genuinely
requires more.

## Bumping a gitlink

1. Land the fix or update in the submodule's own repository and get its
   commit merged there.
2. From this repository:
   ```sh
   cd vendor/<submodule>
   git fetch
   git checkout <new-commit-or-branch-tip>
   cd ../..
   git add vendor/<submodule>
   git commit -m "Bump vendor/<submodule> to <short-sha>"
   ```
3. Run the four contract commands
   (`docs/project/ci-and-tooling/README.md#the-four-contract-commands`)
   against the new pin before opening a pull request; CI checks out
   submodules recursively and will run them again.

Check what is currently pinned, without changing anything, with:

```sh
git submodule status
```

Each submodule is its own workspace with its own lockfile (`vendor/tinybus`
notably so), which is why the root `Cargo.toml` excludes `vendor` from this
workspace's members rather than folding these crates in directly.

See also [`docs/architecture.md`](../../architecture.md) for how these
engines sit under the adapter crates, and the top-level "Vendored
dependencies" section of the repository's `AGENTS.md`/`CLAUDE.md` for the
same rule in the context of the whole contributor workflow.
