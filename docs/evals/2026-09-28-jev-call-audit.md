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

## The typing failure at step 11

Instrumented live (goindigo.in, 1280×800), the "search box" Jev chose was
never a text field. Each city in IndiGo's dropdown is a
`div role="combobox"` whose `aria-labelledby` points nowhere, so it is
unnamed and holds its city as a value; the dropdown's only real input
(`placeholder="Search"`) sits in a mobile header that is `display: none` at
desktop width, and neither inserted text nor key presses filter the list,
which offers nine popular cities and not Srinagar. The general faults, and
their fixes:

| Fault | Fix |
|---|---|
| the browser's editable check accepted any focused element with a `combobox`, `searchbox`, or `textbox` role | only a text-like `input` (not read-only or disabled), a `textarea`, or a `contenteditable` region takes text; a fill or a paste into anything else is refused as `NOT_A_TEXT_FIELD` before a key is sent |
| `enter` tried a refused field again every round, then the next row of the same list | a field that refuses the text is struck with every element of its kind (role, label, place), for the rest of the step |
| the reveal loop then pressed a refused row and chose Mumbai | no `do` move in the step presses an element of a refused kind |
| an unnamed control's own text never reached Jev | an unnamed, valueless, non-text-entry control is described by the text inside it |
| the failure read "no field was found" | "no field that takes text was found for: …; N element(s) the page offered as fields refused the text" |

After the fixes, both strategies refuse the row once, press nothing they
should not, and fail step 11 with that note. Srinagar cannot be entered in
this widget at desktop width from a headless browser; that is the site, not
the harness.

## Sight: reading the page without relying on ARIA

The typing failure came from trusting the page's markup: rows that *declare*
`combobox` and a label that points nowhere. Most sites mark up partly or
wrongly, so the browser surface now reads the rendered page the way a person
looks at it ([`specs/browser-sight.md`](../specs/browser-sight.md)): what is
drawn and on top, the words on and beside each control, and which boxes
really take text. The accessibility tree is the fallback.

Read with a local headless Chrome (1280×800), one pass each:

| Page | What sight shows | The tree showed |
|---|---|---|
| goindigo.in, destination open | the rows as `button "Mumbai Chhatrapati Shivaji Maharaj International Airport BOM"` …, the field as `button "To Search by place/airport"`, the offers under the list `covered`; no text box, since none is drawn | nine unnamed `combobox` rows offered as text fields |
| goindigo.in form | each radio, date, passenger, and payment control once (the page draws a second radio and a wrapper around each) | two of each, one named `oneWay` |
| Google Flights results | the fields as `textbox "Where from? New Delhi DEL" = "New Delhi"`; the cards grouped per list, not marked covered by their own text | the same fields and cards |
| travel fixture | every field named by the label above it | the same |

One reading takes 10–60 ms in the page.

Live, in the Docker lab, with sight (the default):

| Run | Outcome | Jev |
|---|---|---|
| travel fixture, narrow | pass, stopped at payment; 11 `seen:` refs acted on, no tree refs | 15 decisions, 75 calls (tree: 14 / 70) |
| Kashmir, replayed plan, narrow | steps 1–10 as with the tree, then step 11: "no field that takes text was found for: destination city search" | — |

At step 11 sight never offered a city row as a field, so nothing was typed
into one and none was pressed; the tree reached the same failure only after
the row refused the text. Step 4 showed the one flaw the run found: Google
Flights renders two unnamed lists at one place, and their first cards were
read as one card; unnamed lists are now numbered in page order (`list`,
`list 2`). IndiGo's desktop widget still draws no search box, so Srinagar
still cannot be entered there.

## Batching independent decisions

Two narrow-strategy chains asked Jev things that did not depend on each
other, one round trip at a time. `FlowRun::ask_batch` now sends independent
requests together (every framing of every request in flight at once), and
the turn and grounding use it ([`jev-harness.md`](../jev-harness.md)):

- a step's first turn asks the judge and grounding's first round together;
- narrowing asks the region question and a knockout cut along the regions
  together, instead of up to three region rounds and then a knockout.

Same code otherwise, both runs twice each, alternating, in the Docker lab
(sight, narrow, votes 5). "Jev wait" is each round trip's slowest request,
summed: the time a step actually waited for Jev.

| | fixture, before | fixture, batched | Kashmir, before | Kashmir, batched |
|---|---|---|---|---|
| outcome | pass ×2 | pass ×2 | step 11 ×2 | step 11 ×2 |
| wall | 28.4 / 27.6 s | 28.0 / 27.9 s | 41.2 / 40.4 s | **35.9 / 34.0 s** |
| Jev wait | 7.5 / 6.9 s | 6.7 / 6.6 s | 20.6 / 20.5 s | **16.8 / 15.2 s** |
| round trips | 15 | 12 | 42 | **27** |
| decisions / calls | 15 / 75 | 15 / 75 | 42 / 210 | 39 / 195 |
| round trips per `do` turn | mean 1.50, most 2 | mean 1.00, most 1 | mean 2.75, most 5 | **mean 1.50, most 2** |

On Kashmir's crowded pages the Jev wait fell by about a fifth and the run
by about 14%; the batched run also made fewer calls, since one knockout
replaces the extra region rounds. On the fixture, whose pages are small,
each first turn saves one round trip, but settling (20 s of the 28) hides
it. Settling is now the largest cost on both: 14–21 s per run.
