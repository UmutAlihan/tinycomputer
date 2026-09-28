# tinycomputer-examples

This crate has no product code in it. Everything here exists to run
tinycomputer against something real and show you what happened: a
demonstration binary, a scored evaluation harness (the lab), a live task
runner, fixtures to test against without touching a real website, and a
reader for the debug journal. If you want to see tinycomputer do something,
or check that a change to the engine did not quietly make it worse at doing
that thing, this is the crate you run.

## Who this is for

Anyone changing `tinycomputer-engine`, `tinycomputer-desktop`, or
`tinycomputer-browser` and wanting to see the effect on a real run, rather
than trusting a unit test. Anyone trying to understand why a run clicked the
wrong thing. Anyone verifying a packaged release actually loads and answers
before it ships.

## A tour

| Folder | What it is |
|---|---|
| `src/bin/` | the runnable binaries, one file (or folder) per binary; see [running the examples](running-the-examples.md) |
| `src/lab/` | the lab's own library code: loading the module, the scenario ladder, writing run artifacts |
| `src/journal/` | the debug journal reader's parsing and summarising logic, shared by the `jev_journal` binary |
| `scenarios/` | one folder per lab scenario: a brief, a hand-written flow, and (in code) a checker |
| `fixtures/travel/` | a small static booking site for browser runs that need to be exact and repeatable |
| `tasks/` | saved live-task inputs (a task in plain language, its facts, a recorded plan) for the two travel-booking examples |
| `tests/` | this crate's own integration test |

Two shell scripts outside this crate drive the binaries here with the right
build and environment: `scripts/lab` (see [the lab](the-lab.md)) and
`scripts/docker-lab` (see [Docker lab](docker-lab.md)).

## Where to start

- New to the repository and want to see it do something without spending
  Jev credit or granting any permission: run `basic`, in
  [running the examples](running-the-examples.md).
- Changing a decision loop or a flow and want to measure the effect on real
  applications: [the lab](the-lab.md).
- Testing anything that opens a browser: [Docker lab](docker-lab.md) first,
  it explains why the browser examples insist on a container.
- Running a whole plain-language task end to end, against a real website or
  your own signed-in Chrome: [live tasks](live-tasks.md).
- A run did something surprising and you want to see exactly what Jev was
  shown and answered: [reading a journal](reading-a-journal.md).
- Building or checking the travel fixture the browser examples use:
  [fixtures](fixtures.md).

## Cross-links

- [How tinycomputer works](../../how-it-works.md) — the big picture these
  examples exercise pieces of.
- [Giving it a task](../../giving-it-a-task.md) — the task API that
  `task_live` and `task_fixture` drive from the outside.
- [Writing flows](../../writing-flows.md) — the language the scenario and
  task flows are written in.
- [How it decides](../../how-it-decides.md) — what Jev is, and what a
  decision loop is.
- [Catching mistakes](../../catching-mistakes.md) and
  [rescue](../../rescue.md) — what happens when a step goes wrong, which
  `--disable` and `TASK_RESCUES` let you turn off to measure.
- [Memory and saving](../../memory-and-saving.md) — the grounding hints
  `target/lab-runs/memory.json` carries between runs.
- [Seeing the screen](../../seeing-the-screen.md) — what a flow or task
  actually reads, which the journal and the trace both record.
- [Safety and privacy](../../safety-and-privacy.md) — why `--send` only ever
  addresses your own inbox, why payment is never entered, and why journals
  are git-ignored.
- [Watching a run](../../watching-a-run.md) — the cursor and other ways to
  watch a run live, beyond the timeline and the journal.
- [Glossary](../../glossary.md) — short definitions of terms used across
  these pages (Jev, flow, task, decision loop, and so on).
- [`docs/technical/lab.md`](../../technical/lab.md),
  [`docs/technical/docker-lab.md`](../../technical/docker-lab.md), and
  [`docs/technical/jev-journal.md`](../../technical/jev-journal.md) — the
  technical reference these friendlier pages point back to for detail.
