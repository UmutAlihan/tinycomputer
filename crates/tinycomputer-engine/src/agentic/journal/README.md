# `agentic::journal` — the Jev debug journal

Opt-in, best-effort, append-only JSON Lines of every Jev exchange and every
flow timing, one file per run. The user-facing guide — turning it on, the
event table, reading a run with `jev_journal`, finding latency — is
[`docs/jev-journal.md`](../../../../../docs/jev-journal.md).

## Design

- **One door.** Every Jev call goes through `JevRuntime::evaluate`, which
  asks the client and then journals the exchange. Nothing else in the engine
  calls the client, so nothing escapes the journal.
- **A run is a scope on the runtime.** `JevRuntime` holds a `Journal`: the
  root directory (from `TINYCOMPUTER_JEV_JOURNAL`, or `with_journal`) and the
  open run, if any. `run_flow`, `run_goal`, and `resolve_intent` call
  `begin_run`, which opens a fresh run — or, when the runtime is already
  scoped (a task's runner calls `journaled_as("task-<id>")`; a goal
  continuation restores its pending run's journal), writes a `run` event into
  the existing one.
- **Free when off.** `Journal::record` takes a closure; with no open run it
  returns before building the event.
- **Never fatal.** A journal that cannot be opened prints one line to stderr
  and is dropped; a failed write is ignored. A run behaves identically with
  the journal on or off.
- **After masking.** The flow journals the request after `FlowRun::mask`, so
  secrets appear only as `${name}`.

## Public surface

`JevRuntime::with_journal`, `JevRuntime::journaled_as`,
`JevRuntime::journal_dir`, and the constants `JOURNAL_ENV`,
`JOURNAL_DEFAULT_DIR`, and `JOURNAL_FILE`, all re-exported from the crate
root. The reader lives in `tinycomputer-examples` (`src/journal/`, the
`jev_journal` binary), since only developers read journals.
