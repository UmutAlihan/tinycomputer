# Documentation

This directory holds documentation that does not belong in rustdoc: the shape
of the system, the reasoning behind it, and the constraints a reader needs
before touching the code. API reference lives in doc comments next to the code,
where it cannot drift.

## Layout

```text
docs/
├── README.md      # this index
├── architecture.md, decision-loops.md, tasks.md   # how the system works
├── jev-harness.md, jev-journal.md                  # the Jev stack; debugging it
├── jev-questions.md, flow-examples.md             # Jev's inputs and outputs; flows traced
├── lab.md, docker-lab.md                          # live runs
├── evals/         # recorded live results
├── specs/         # behavior and architecture specifications
├── plans/         # implementation plans derived from approved specs
└── adr/           # architecture decision records, numbered and immutable
```

- **[`specs/`](specs/README.md)** — one file per feature, module, or subsystem,
  describing its behavior, public surface, invariants, and acceptance criteria.
- **[`plans/`](plans/README.md)** — implementation-ordered, test-first steps for
  delivering an approved specification. Plans name exact files and verification
  commands, and are updated as the work progresses.
- **`adr/`** — a dated record per significant decision. Use
  [`adr/0001-record-architecture-decisions.md`](adr/0001-record-architecture-decisions.md)
  as the template. An accepted ADR is not edited; it is superseded by a later
  one.

Complex modules also carry a module-level `README.md` inside `src/<module>/`
covering their design, public surface, and important constraints.

Coding agents: [`AGENTS.md`](../AGENTS.md) maps each kind of change to the
documents to read first ("Read The Right Document First").

Start with these, in order:

- [`architecture.md`](architecture.md): the crates, how a call travels from
  the bus to a click, threading, configuration, and the safety checks.
- [`jev-harness.md`](jev-harness.md): the map of the Jev stack — every layer
  from a request to a Jev call, what each adds, where the time goes, and how
  each is tested and observed.
- [`decision-loops.md`](decision-loops.md): how the flow runtime grounds each
  step on a live screen and every question it asks Jev;
  [`decision-thresholds.md`](decision-thresholds.md) lists every threshold.
- [`jev-questions.md`](jev-questions.md): every input Jev receives, every
  question id with its type and options, the answer shapes, and how each
  answer is thresholded.
- [`flow-examples.md`](flow-examples.md): real flows traced decision by
  decision, what each step kind costs, and what common failures look like.
- [`jev-journal.md`](jev-journal.md): the debug journal — every Jev exchange
  and timing of a run on disk — and how to use it to find latency.
- [`tasks.md`](tasks.md): the task API for outside agents, pausing and
  resuming, private values, budgets, and the planner.
- [`../scripts/debug-ui/`](../scripts/debug-ui/): the local Vite SPA for
  inspecting Jev journal JSONL files.
- [`lab.md`](lab.md) and [`docker-lab.md`](docker-lab.md): running flows on a
  real desktop, and anything that launches Chromium in a container.
- [`evals/`](evals/): recorded results of live runs.

The contract for what this module serves, and why it is shaped that way, is in
[`specs/desktop-module-contract.md`](specs/desktop-module-contract.md), with its
implementation sequence in
[`plans/desktop-module-contract.md`](plans/desktop-module-contract.md). The
packaging and release contract is in
[`specs/tinybus-module-release.md`](specs/tinybus-module-release.md), with its
sequence in
[`plans/tinybus-module-release.md`](plans/tinybus-module-release.md). Intent
flows are specified in [`specs/jev-intent-flows.md`](specs/jev-intent-flows.md),
the browser, the task API, and the planner in
[`specs/unified-agent.md`](specs/unified-agent.md), and the wide strategy —
one request per turn over a digest of the screen, with a survey and a working
memory — in [`specs/jev-wide-turns.md`](specs/jev-wide-turns.md)
([`plans/jev-wide-turns.md`](plans/jev-wide-turns.md)). How the browser
surface reads a page by what is drawn rather than by its ARIA markup is in
[`specs/browser-sight.md`](specs/browser-sight.md), and how a `choose` checks
and repairs what its press left in
[`specs/flow-reflection.md`](specs/flow-reflection.md). How every decision is
deliberated on its evidence — escalated, dueled, checked after acting, and
undone and retried when wrong — is in
[`specs/jev-deliberation.md`](specs/jev-deliberation.md)
([`plans/jev-deliberation.md`](plans/jev-deliberation.md)). How a failed step
is handed to a reasoning model for guidance before a task fails is in
[`specs/task-rescue.md`](specs/task-rescue.md)
([`plans/task-rescue.md`](plans/task-rescue.md)). How a run remembers what it
saved, and how a finished task returns it in the caller's JSON shape, is in
[`specs/task-output.md`](specs/task-output.md).

## Conventions

- Keep every Markdown file at 500 lines or fewer. When a topic outgrows that,
  split it into focused files and link them from the nearest `README.md`.
- Update documentation in the same commit as the behavior it describes.
- Prefer a concrete example over an abstract description.
- Link between documents rather than duplicating content; one fact lives in one
  place.
- Write a specification before a plan: the spec defines the outcome and
  constraints, while the plan defines the implementation sequence.
