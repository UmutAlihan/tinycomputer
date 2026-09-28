# The debug journal

The journal is how you find out what a run actually did after it is over:
every question Jev was asked, every answer it gave, how long each call took,
and how much wall time each part of a run spent. It lives in
`crates/tinycomputer-engine/src/agentic/journal/mod.rs`, and it is built
around one promise, stated in the module's own doc comment: **it is off
unless it is switched on, and switched on it never changes what a run
does.** Every write is best effort; a write that fails is silently dropped
rather than failing the run it is describing.

This page covers the journal from inside this crate. For how to actually
read one afterwards (the CLI tool, what a summary looks like, how to spot
where time went), see [`docs/technical/jev-journal.md`](../../technical/jev-journal.md).

## Turning it on

Two ways, both wired to the same `Journal` type:

- **The environment variable**, `TINYCOMPUTER_JEV_JOURNAL`. `1` (or `true`,
  `on`, `yes`) writes under `.jev-journal/` in the working directory
  (`JOURNAL_DEFAULT_DIR`); any other non-empty, non-falsy value is taken as
  the directory to write under instead. `JevRuntime::configure` reads this
  automatically, so a module that does nothing special still gets a journal
  the moment the variable is set in its environment.
- **In code**, `JevRuntime::with_journal(dir)` switches it on
  unconditionally, ignoring the environment variable, always writing under
  `dir`.

```
TINYCOMPUTER_JEV_JOURNAL=1 scripts/lab run <scenario> --mode flow
```

## Where a run's journal lives

Each run gets its own directory, `<dir>/<run id>/`, holding one file:
`journal.jsonl` (`JOURNAL_FILE`), JSON Lines, one event per line. A run id
sorts by start time and is safe as a single path segment:
`20260928T101530Z-flow-a1b2c3`: a compact UTC timestamp, the run's kind
(`flow`, `goal`, `intent`, `goal-continuation`), and three random bytes so
two runs started in the same second never collide.

`JevRuntime::journal_dir()` reports where the *current* run is writing, if
the journal is on and a run has actually begun; it returns `None` before
that, and always when the journal is off.

## Runs, and sharing one file across several

A `JevRuntime::begin_run(kind, label)` call opens a run: if the runtime is
not already writing one, it starts a fresh directory named for `kind` and
the current time; if it is, the `run` event is appended to the run already
open. This is what lets `resolve_intent`, `run_goal`, and `run_flow` each
call `begin_run` at their own top without worrying about whether they are
the first thing to touch the journal.

The task controller uses a different entry point,
`JevRuntime::journaled_as(run_id)`, so that every flow run belonging to one
*task*, the first run, and every rescue's or approval's continuation of it,
writes to the same journal file, named after the task rather than after
whichever run happened to start it. Reading one task's whole story back
later means reading one file, not stitching several together.

## What is in a line

Every event carries `event` (its kind), `seq` (a per-run sequence number),
`at` (a wall-clock RFC 3339 timestamp), and `elapsed_ms` (time since the
run's journal was opened). Each is written by `Journal::record`, which
every event kind goes through. Two kinds matter most:

- **`run`**: written once, when a run begins: its `kind`, a human `label`
  (the goal or intent text, or the task's), the Jev `model` in use, and the
  process id.
- **`exchange`**: written for every single Jev call, win or lose: the
  optional `step` label, which `questions` were asked, the request's byte
  size, the full `request` sent, and either `{"ok": true, latency_ms,
  attempts, request_id, model, input_tokens, output_tokens, answers}` or
  `{"ok": false, latency_ms, attempts, error}`.

The exact field list for every event kind (including the ones the flow
runtime and its grounding, voting, and deliberation logic add on top of
these two) is catalogued in
[`docs/technical/jev-journal.md`](../../technical/jev-journal.md); this page
only covers the two written from inside `JevRuntime` itself.

## Why the journal is git-ignored

A journal holds exactly what Jev was shown: screen text, element names, and
the caller's own goal or task text. Facts are masked before a request is
ever built (see [tasks.md](tasks.md#facts-and-secrets)), so a secret value
is not in a journal, but ordinary personal data visible on screen can be.
`.jev-journal/`, the default directory, is git-ignored for exactly that
reason. A journal, a trace, or a transcript should never be pasted into
a commit, an issue, or a pull request.

## Source

- `crates/tinycomputer-engine/src/agentic/journal/mod.rs`, `Journal`,
  `RunJournal`, `record`, `exchange`, `fresh_id`, `sanitize`.
- `crates/tinycomputer-engine/src/agentic/runtime.rs`, `JevRuntime::begin_run`,
  `with_journal`, `journaled_as`, `journal_dir`.
- [`docs/technical/jev-journal.md`](../../technical/jev-journal.md), every
  event kind's fields, the CLI reader, and how to read a summary.
- [watching-a-run.md](../../watching-a-run.md), the less technical version
  of this page.
