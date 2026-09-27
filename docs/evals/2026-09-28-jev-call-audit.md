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

## After: the wide strategy

Same fixture, same code, both strategies (`TINYCOMPUTER_FLOW_STRATEGY`):

| | narrow | wide |
|---|---|---|
| outcome | pass, stopped at payment | pass, stopped at payment |
| decisions / calls | 14 / 70 | 13 / 65 |
| decisions per `do` turn | mean 1.33, most 2 | mean 1.00, most 1 |
| mean request | 7.0 KB | 10.7 KB |
| largest call | 4,556 tokens (14%) | 5,614 tokens (17%) |
| wall | 26.7 s (Jev 6.0 s) | 28.2 s (Jev 5.8 s) |

The fixture's pages are small, so the wide turn's gain there is one round
trip per hesitant turn; settling still dominates the wall time.

What the first wide runs found, each fixed and pinned by a simulator test:

| Symptom | Cause | Fix |
|---|---|---|
| a payment page judged "seat selection skipped" at 0.48 (narrow: 0.97) | the ledger kept one line per finished step, dropping the previous step's clicks — the only evidence the seat page was dealt with | `recent_actions` reaches back across steps; 0.95 on replay |
| a target Jev answered `none` was asked again through narrow grounding | no record that it had been asked | `Prepared::Nothing` |
| `stop_before` could not find the Pay button (both strategies borderline) | asked to "perform: paying", Jev weighed the brief's rule to stop before paying (0.44); position bias put Pay last (0.01) | ask to *find, without pressing it* (1.00 on replay); purpose-named elements first in wide pools |

## The Kashmir task (live, Google Flights and goindigo.in)

`tasks/run kashmir`, the planner's 28-step flow replayed with `FLOW_FILE`
for both strategies. No run reached the payment checkpoint; each fix below
moved both strategies further:

| Stopped at | Cause | Fix |
|---|---|---|
| step 4, `pick` on Google Flights: "could not open the picked item" | the card's own duration text covers its "Select flight" link; the label is rendered twice (a hidden tab) and carries a double space, so the same-card check never matched; `pick` had no covered-click recovery | the check finds the target by its label under the point with whitespace collapsed; `pick` uses the `do` loop's uncover-and-retry and reports the engine's reason |
| step 10, "open the destination city search box" (wide) | the purpose names the destination button, so it led the options and was pressed again, closing the dropdown; its name grows once open | the element pressed last turn goes last, matched by a label one extends the other by |
| step 10, a request at 28,362 tokens (88% of the window) | a 40,000-byte digest and an 80-candidate knockout on a dense page | 24,000 bytes and 40 candidates; the survey keeps answers by region name and is not re-asked when a dropdown opens |
| step 11, `enter` "no field was found for: destination city search" (both) | nine unnamed, identical search boxes in the dropdown: the unnamed one could not be told apart from other dropdowns', and, once described, the voted framings each picked a different lookalike | an unnamed element is described by its nearest named container (`near`); lookalikes are offered once |

After those fixes both strategies reach the same place: Google Flights
compared, the ₹7,339 IndiGo fare picked and opened, goindigo.in open, the
cookie banner dismissed, One Way and Delhi confirmed, the destination
dropdown opened (wide 0.78, narrow 0.90 — the wide judge sits closer to the
0.75 bar here). Step 11 then fails for both. With lookalikes collapsed Jev
picks the destination search box at 0.99, but three fills do not land in
IndiGo's search combobox: that is text delivery into the widget, not a
decision, and is the next thing to take up (with the step-10 judgement, and
the dropdown closing again before step 11 on one wide run).
