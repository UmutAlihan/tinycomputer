# The lab: run, score, and debug flows on a real desktop

The lab is the development loop for Jev-driven desktop control. It loads the
module exactly as a production host does — built in release mode, attested by
`modules.toml`, loaded through the TinyBus dynamic loader, configured by private
reinitialization — and drives real applications on the machine it runs on.

## Setup

- macOS, with Accessibility granted to the terminal that runs the lab (and
  Screen Recording for screenshots).
- `OPENROUTER_API_KEY` exported, or in `.env` (see `.env.example`). Jev and the
  optional LLM author both go through OpenRouter.
- Scenarios drive Mail, Notes, TextEdit, Calculator, Finder, System Settings,
  and Spotify; Mail needs an account. The screen must be unlocked: a locked
  screen exposes no window to accessibility.

`scripts/lab` builds `crates/tinycomputer` in release mode into `target/lab/`,
places the engine's clipboard helper beside it (the engine only trusts it
there), writes `modules.toml`, and runs the `lab` binary from
`crates/tinycomputer-examples`.

## Commands

| Command | What it does |
|---|---|
| `scripts/lab list` | the scenario ladder |
| `scripts/lab run <scenario> [--mode flow\|goal\|authored]` | one run, with a timeline and a checker verdict |
| `scripts/lab eval <names\|all> --modes flow,goal --trials N` | many runs and a scorecard |
| `scripts/lab report <run-dir>` | re-print a run's timeline |
| `scripts/lab call <Member> '<json>'` | call any bus member directly, for probing |
| `scripts/lab validate <flow.json>` | check a flow |
| `scripts/lab guide` | print the flow authoring guide |

Flags: `--headed` (physical input; the target app needs a window to focus),
`--disable moves,undo,…` (turn decision loops off to measure them),
`--no-memory` (ignore grounding hints from earlier runs), `--flow <file>` (run
your own flow against a scenario's checker), `--strategy narrow|wide` (how
decisions are asked, `specs/jev-wide-turns.md`), `--send` (mail only; addresses
`TINYCOMPUTER_LAB_SELF_EMAIL` and nothing else).

## Modes

- **`goal`** — `RunGoal` on the scenario's single goal string: the baseline.
- **`flow`** — the scenario's hand-written, UI-agnostic flow
  (`crates/tinycomputer-examples/scenarios/<name>/flow.json`).
- **`authored`** — an LLM (`TINYCOMPUTER_LAB_MODEL`, default
  `anthropic/claude-sonnet-5`, through the vendored `tinyinference`) is given the
  brief and the flow guide, never the screen. It writes a flow; the lab
  validates and runs it, then shows the author a summary including anything
  `read` steps captured, and the author finishes or writes a flow for what
  remains, for up to three rounds.

## What a run leaves behind

`target/lab-runs/<scenario>/<mode>-<timestamp>/`:

- `request.json`, `result.json` — the exact request and result.
- `timeline.txt` — each step, its outcome, turns, Jev calls, the decision
  loops that contributed, and every action with its target and delivery path.
- `jev.jsonl` — every Jev exchange: the state it saw, the questions, the
  answers. This is how a wrong judgement is diagnosed: read what Jev was shown.
- With `TINYCOMPUTER_JEV_JOURNAL=1`, the module also journals every raw Jev
  call and every timing under `.jev-journal/`; see [`jev-journal.md`](jev-journal.md).
- `verdict.json` — the checker's verdict, which reads the application's real
  state rather than trusting the run's report.
- `authored-N.json` — in `authored` mode, each flow the LLM wrote.

`target/lab-runs/memory.json` keeps grounding hints between runs; the second
run of a scenario usually makes fewer Jev calls than the first.

## The loop

1. `scripts/lab eval all --modes flow,goal --trials 3`.
2. Pick a failure; `scripts/lab report` it; read its `jev.jsonl`.
3. Decide where the fault is: the observation (what Jev was shown), a decision
   loop (how it was asked), the flow (what was asked), or the engine (a wrong
   tree or a failed action). Engine faults go upstream to `agent-desktop`.
4. Write the failing test against the simulator in
   `crates/tinycomputer-engine/src/agentic/flow/test.rs`, fix, and re-run the scenario.
5. Record notable results in [`evals/`](evals/).
