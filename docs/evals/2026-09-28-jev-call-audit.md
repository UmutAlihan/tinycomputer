# 2026-09-28 — how many Jev calls one loop makes

A baseline for [`specs/jev-wide-turns.md`](../specs/jev-wide-turns.md): what
the flow runtime spends on Jev today, counted from the code and measured on a
live run. Jev: `jev-latest` through OpenRouter, `votes` 5 (the default).

## Counted from the code

A decision is one request asked in `votes` framings at once
(`agentic/flow/vote.rs`); it waits for its slowest framing. Decisions inside
one turn run one after another.

| One `do` turn | Decisions in sequence | Calls |
|---|---|---|
| a shortcut move (`judge` only) | 1 | 5 |
| `activate`, pool of 20 or fewer, confident | 2 (`judge`, `target`) | 10 |
| `activate`, pool of 20 or fewer, under 0.70 | 3 (+ `again` and `confirm`) | 15 |
| `activate`, larger pool (up to three `region` rounds, a knockout) | up to 7 | up to 35 |
| plus an obstacle (`blocked` at 0.70, then `dismiss`) | +1 | +5 |
| a grounding-memory hit | 2 (`judge`, `confirm`) | 10 |

Every decision in a turn resends the same `state`; only the questions differ.

## Measured: the travel fixture (`task_fixture`, Docker lab)

`scripts/docker-lab -- env TINYCOMPUTER_JEV_JOURNAL=1 crates/tinycomputer-examples/fixtures/run task_fixture`
— passed, stopped at payment.

| | |
|---|---|
| wall time | 28.0 s |
| Jev | 6.4 s (22%): 15 decisions, 75 calls; p50 362 ms, p90 437 ms |
| acting | 21.4 s (76%), of which **20.6 s is settling**: about 1.6 s after each of 13 actions, including each field of a form |
| request size | mean 7.0 KB, 2,671 input tokens per call; largest 4,556 tokens |
| window use | 8% mean, 14% largest, of a 32K-token window |

| Step | Decisions | Calls | What they were |
|---|---|---|---|
| enter (3 fields) | 2 | 10 | `slot_*`; `error_*` |
| do "search for flights" | 2 | 10 | judge; `target` |
| wait_for | 1 | 5 | `holds` |
| enter (4 fields) | 2 | 10 | `slot_*`; `error_*` |
| do "continue past the traveller details" | 5 | 25 | judge, `target`, judge, `target`, judge |
| do "skip the seat selection" | 1 | 5 | judge (already done) |
| stop_before | 2 | 10 | `target`; `again` + `confirm` |

## What it says

- **Requests are small.** One call uses about a twelfth of Jev's window;
  the harness can show far more of a page, and ask far more per request.
- **Decisions are serial.** A normal `do` turn is two round trips, a
  hesitant pick three, a crowded screen up to seven, all over one screen.
  Folding them into one request per turn is the main latency and call lever
  on the Jev side.
- **Settling costs more than Jev does on the browser.** 1.6 s per action,
  paid once per form field, is three times the Jev time of the whole run.
  That lever is outside the wide-turn change and is recorded here to be
  taken up on its own.
- macOS scenarios (`scripts/lab`) could not be run from a background shell
  without Accessibility permission (`PERM_DENIED` at launch); the counts
  above for crowded desktop screens come from the code and the calculator
  run in [`2026-09-27-first-runs.md`](2026-09-27-first-runs.md) (30 calls
  for 9 key presses).
