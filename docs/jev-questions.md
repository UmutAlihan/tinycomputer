# Jev inputs and outputs

Every question the engine asks Jev, what it sends, what comes back, and what
the runtime does with each answer. [`decision-loops.md`](decision-loops.md)
explains *when* each question is asked; this page is the reference for *what*
goes over the wire. [`flow-examples.md`](flow-examples.md) walks real flows
through these questions step by step.

The shapes below are the wire form of `tinyinference_decisions` (in
`vendor/tinyinference`). The debug journal ([`jev-journal.md`](jev-journal.md))
records every request and answer in exactly this form, so the fastest way to
see a real one is to journal a run and read an `exchange` event.

## The request

One request is one round trip. It carries one shared `state` and any number
of independent questions, each keyed by an id the runtime chooses:

```json
{
  "model": "jev-latest",
  "state": { "app": "Mail", "current_step": "start a new email message", "…": "…" },
  "questions": {
    "done":     { "type": "noul",   "instructions": {…} },
    "progress": { "type": "score",  "instructions": {…}, "criteria": ["level 0", "…", "level 4"] },
    "move":     { "type": "choice", "instructions": {…}, "criteria": {"activate": "…", "none": "None of these fits."} }
  }
}
```

Every question is answered against the same `state`, independently of the
others. Asking several questions in one request costs one round trip, which
is why the runtime batches everything it wants to know about one screen.

## Question types and their answers

| Type | Asks | Request fields | Answer |
|---|---|---|---|
| **Noul** | a yes/no condition | `instructions`, optional `criteria: {true, false}` | `{"type": "noul", "noul": 0.82}`: the probability of yes |
| **Score** | where the state sits on an ordered scale | `instructions`, `criteria`: the levels, lowest first | `{"type": "score", "score": 2.6, "probabilities": {"0": 0.01, …, "4": 0.3}, "legend": {…}, "confidence": 0.4}` |
| **Choice** | one option from a closed set | `instructions`, `criteria`: option key → description | `{"type": "choice", "choice": "3", "probabilities": {"1": 0.02, "3": 0.91, …}, "confidence": 0.85}` |

Jev never writes text. Every answer is a number or a key the runtime offered,
so every answer can be thresholded and a malformed one fails closed.

How the runtime reads them (`agentic/flow/ask.rs`):

| Reader | Returns | Used for |
|---|---|---|
| `probability(id)` | a Noul's `noul` | most yes/no gates |
| `calibrated(yes, no)` | `mean(P(yes), 1 − P(no))` from a question and its negation | completion and conditions: a model that says yes to everything lands near 0.5 |
| `top_level(id)` | the probability a Score puts on its highest level | "fully accomplished", "all of it holds" |
| `combined(a, b)` | the mean of a calibrated yes/no and a top level | the final completion and condition estimates |
| `level(id)` | a Score's expected level as a fraction of the scale (0–1) | progress, and so regression detection |
| `chosen(id)` | the chosen key and its probability; `None` for `none` | every Choice |

A Choice always offers `none` ("None of these fits."). A `none` answer, a key
the runtime did not offer, or a missing answer all read as "nothing", and the
runtime never falls back to a default click.

With voting on, the answers the runtime reads are the average over framings:
per-option probabilities for a Choice (its `confidence` becomes the share of
framings that agreed), the probability for a Noul, per-level probabilities
for a Score. See [`jev-harness.md`](jev-harness.md).

## The shared state

Every flow question about a screen shares the state built by `ask::state`:

| Field | Holds | Limit |
|---|---|---|
| `app`, `window`, `surface` | where the run is: `window`, `sheet`, `dialog`, `popover`, `none`, … | — |
| `current_step` | the step's text, or what this decision is for ("check the entered form for errors") | — |
| `visible_text` | static text: labels, headings, status lines, wrapped as `untrusted_accessibility_data` | — |
| `elements` | one line per actionable element, `role "name" = "value" [states]`, wrapped as `untrusted_accessibility_data` | 120 lines |
| `recent_actions` | history: step outcomes, change notes ("window is now …; appeared: …"), and runtime notes ("that made things worse; undid it") | last 20 lines |
| `field_contents` | only with `include_values`: what each text field holds, including rich-text bodies and token fields | 12 fields × 400 characters |
| `already_collected` | once a `read`, `extract`, or `pick` has saved something: each saved variable's value (an `extract`'s as its count and first row), wrapped as `untrusted_accessibility_data`, so a step walking a list knows which items are done; masked like the rest | the last 12 variables × 120 characters |

A blank screen, when no window can be read, arrives as `surface: "none"` with
a note in `visible_text` saying a keyboard shortcut may still work.

## An element, as an option

Choices over elements describe each option with `describe`
(`tinycomputer-core/src/surface/screen.rs`):

```json
{
  "what": "button \"New Message\"",
  "where": "window \"Inbox\" > toolbar",
  "supports": ["Click"],
  "state": "enabled",
  "holds": "…only with include_values…",
  "contains": 3
}
```

Keys are `1`, `2`, … on a first ask and `A`, `B`, … on the relabelled re-ask,
so a bias toward a position or a label shows up as disagreement.

An element with no name of its own carries `near`, the nearest named
container it sits in (`button "destinationCity …"`), and its state line
reads `combobox in button "destinationCity …"`: on a booking widget that is
all that tells one unnamed search box from another dropdown's. A named
element whose page description says more than its name carries `says`, and
its state line reads `button "18" (Sunday, 18 October 2026)`: a calendar
names each day by its number, and without the description the 18th of this
month and of the next are the same button to Jev. Elements whose
descriptions match, bounds aside, are offered once — the first in page order
— because lookalikes side by side split a voted answer below its floor.

## The brief

Questions that *choose* — every Choice except `page_kind`, and the `confirm`
Noul — carry a `brief` inside their `instructions` (`FlowRun::brief`):

| Field | Holds |
|---|---|
| `goal` | the whole task in plain language, up to 600 characters |
| `for` | the task's shared details by name: whom it is for, dates, email |
| `secrets` | secret *names* only, as `${name}`, with a note that the module types them |
| `rules` | standing rules: "stop before paying", "decline paid extras" |
| `plan` | every top-level step, marked `[done]`, `[now]`, `[next]` |
| `so_far` | the last 12 things chosen, picked, or entered |
| `page` | the last answer to `page_kind`, on the web |

Questions that *judge* the screen get no brief, because it pulled their
answers toward the whole task instead of the step.

## Every flow question

Ids are the keys the runtime uses; the journal and the trace show them.

### Judging a `do` turn (`act.rs`, one request per turn)

| Id | Type | Given | Answer used as |
|---|---|---|---|
| `done` | Noul | the step | calibrated with `not_done`, combined with `progress`'s top level: ends the step at 0.75, or 0.85 before any action |
| `not_done` | Noul | the step | the negation for `done` |
| `progress` | Score, 5 levels | the step | "nothing relates" … "fully accomplished"; a drop of a quarter of the scale since the last action triggers undo |
| `blocked` | Noul | the step | at 0.70, the runtime asks `dismiss` |
| `helped` | Noul | the step, the last action | only after an action; under 0.20 triggers undo |
| `move` | Choice | the step; `activate`, `shortcut`, `expand`, `scroll`, `wait`, `finished`, `stuck` | the next move; anything else is ignored |
| `shortcut` | Choice | the step; `new_item`, `new_folder`, `find`, `reply`, `settings`, `back`, `next_field`, `confirm`, `dismiss` | pressed when `move` is `shortcut` and this is at least 0.5 |
| `page_kind` | Choice | 13 page kinds, from `search_form` to `captcha` | on the web only; briefs the next request |

### Reflecting (`reflect.rs`)

| Id | Type | Given | Answer used as |
|---|---|---|---|
| `reflects` | Noul | the `choose` step that just pressed | calibrated with `strays`: under 0.50 the step is repaired once, then failed |
| `strays` | Noul | the same step | the negation: a different count, date, or name, or a change not asked for |

### Recovering (`act.rs`)

| Id | Type | Given | Answer used as |
|---|---|---|---|
| `dismiss` | Choice | the visible non-destructive clickable elements, plus `escape` | the element to click, or Escape, to clear an obstacle |

### Grounding one element (`ground.rs`)

| Id | Type | Given | Answer used as |
|---|---|---|---|
| `confirm` | Noul | the purpose, one element | a remembered element is used at 0.5; after a re-ask, see below |
| `region` | Choice | up to 20 regions, each with its element count and six examples | narrows the pool, up to three rounds |
| `group_0` … `group_n` | Choice each | one group of up to 20 elements each | a knockout: each group's winner goes on |
| `target` | Choice | up to 20 elements | used at 0.70, or at 0.45 when its name is in the purpose |
| `again` | Choice | the same elements, reversed, lettered | agreeing with `target` plus `confirm` at 0.5 accepts it; `confirm` at 0.8 alone does too |

### Deliberating (`escalate/`, `duel/`, `ground.rs`, `act.rs`, `steps.rs`)

Asked only when the evidence behind an answer is thin, or after a press, under
[`specs/jev-deliberation.md`](specs/jev-deliberation.md). A deliberated
request is also re-asked in more framings, which changes no id.

| Id | Type | Given | Answer used as |
|---|---|---|---|
| `focus` | Choice | `step`, and up to four distractions: the container, what it shows, the control that clears it (or Escape, for something covering the controls the step names) | the root of a turn and a step's prelude: a distraction clearly picked is cleared first (`attention/`) |
| `wider` | Choice | every knockout winner, up to 20, when the region cut dropped some (deep) | a pick made without the region, checked against `target`; a disagreement goes to a duel |
| `duel_<i>_<j>` | Choice | two finalists, `1` shown first | both orders of every pair, counted Copeland-style; a champion takes at least 0.60 of every pairing |
| `is_<i>` | Noul | the purpose, one finalist | calibrated with `only_near_<i>`: with no duel champion, takes a leader at 0.65 with a 0.20 lead; `is_0` also vouches for an irreversible press at 0.85 |
| `only_near_<i>` | Noul | the same | the negation: a lookalike, or something next to the element |
| `intended` | Noul | the step, the last press, what it was meant to do | asked only after a missed effect; calibrated with `unintended`, under 0.50 the press is undone |
| `unintended` | Noul | the same | the negation: the wrong item opened, the page left, a choice cleared |
| `done`, `not_done`, `holds`, `negated` with a `view` | Noul | the screen alone, or what changed since the step began | the judgement over another rendering; the readings are combined by their median |

### Entering text (`enter.rs`)

| Id | Type | Given | Answer used as |
|---|---|---|---|
| `slot_0` … `slot_n` | Choice each | "type the {slot} into it", over one shared numbered field list | assigned greedily, most confident first; under 0.40 dropped |
| `asks_0` … | Noul each | a slot with no field found | under 0.35, the slot is "not asked for" and the step does not fail |
| `error_0` … | Noul each | each slot's field | at 0.70, the field is entered once more; still flagged, the step fails |

Slot names go to Jev. Slot values never do.

### Conditions (`steps.rs::holds`, for `verify`, `wait_for`, `if`, `repeat_until`, and `stop_before`'s after-check)

| Id | Type | Given | Answer used as |
|---|---|---|---|
| `holds` | Noul | the condition | calibrated with `negated`… |
| `negated` | Noul | the condition | …the negation… |
| `coverage` | Score, 5 levels | the condition | …combined with the top level: the condition holds at 0.75 |

### Reading and picking (`steps.rs`)

| Id | Type | Given | Answer used as |
|---|---|---|---|
| `source` | Choice | readable text on screen, 60 per page | for `read`, stored at 0.5 or above |
| `record` | Choice | up to 60 result cards, each as its fields | for `pick`, when the criterion did not parse; used at 0.5 |
| `list` | Choice | up to 6 lists showing, each as its length and first 3 items | for `extract`, and a `pick` whose criterion did not parse, when more than one list shows; used at 0.5, else the longest |

`extract` asks nothing when one list shows, and `pick` asks nothing when its
`by` parses as a price, time, duration, or stop-count criterion.

## The wide strategy (`wide.rs`, `survey.rs`)

With `strategy: "wide"` the state keeps `app`, `window`, `surface`,
`current_step`, `visible_text`, `field_contents`, and `already_collected`, and replaces
`elements` and `recent_actions` with:

| Field | Holds |
|---|---|
| `screen` | the digest: `in_front` (a dialog, sheet, popover, or consent banner), `regions` (id, where, `relevance` once surveyed, one `eN role "name"` line per element, or one `card N: text → eN control` line per card of a list), and `collapsed` (one line per noise, distraction, or over-budget region) |
| `memory` | `steps_done`, `now`, `recent_actions` (the last 24 history lines, across steps), `tried_and_failed`, `next_step`, `variables_read`, `budget_left` |

A `do` turn asks, in one request, the judging questions above plus:

| Id | Type | Given | Answer used as |
|---|---|---|---|
| `dismiss` | Choice | the safe controls of the region in front, plus `escape` | clears the obstacle when `blocked` is at 0.70, with no further request |
| `dismiss_known` | Noul | the control that closed this overlay on an earlier run | used at 0.5 |
| `target_<move>` | Choice | up to 20 candidates for `activate`, `expand`, or `scroll`, the purpose's named elements first | the narrow thresholds: at 0.70, or 0.45 when named; else one `confirm` |
| `again_<move>` | Choice | the same, reversed and lettered | consistency, as `again` |
| `group_<move>_<n>` | Choice each | a pool over 20, up to two groups | a knockout; several winners get one final `target` |
| `known_<move>` | Noul | a remembered element | used at 0.5 |

A crowded screen (over 40 actionable elements) is surveyed first, once per
page shape per step:

| Id | Type | Given | Answer used as |
|---|---|---|---|
| `relevance_<region>` | Score, 5 levels | one region: where, how many elements, six examples, whether it held a remembered element | ranks regions: what is shown in full and offered first |
| `distraction_<region>` | Noul | the same region | at 0.70 the region is collapsed and ranked last |

## The goal loop's questions (`agentic/policy.rs`)

`RunGoal` and `ResolveIntent` ask one larger request per decision. Their state
is `goal`, `app`, `window`, `surface`, and the last eight `recent_actions`.

| Id | Type | Given | Answer used as |
|---|---|---|---|
| `operation` | Choice | `WAIT`, `DONE`, `BLOCKED`, `WIDEN` (when scoped), and each operation some element supports: `CLICK`, `TYPE_TEXT`, `CHECK`, `UNCHECK`, `EXPAND`, `COLLAPSE`, `SCROLL`, `DRILL` | the next operation; see the gate below |
| `destructive` | Noul | the goal | at 0.50, the action needs confirmation |
| `click_target`, `type_text_target`, … | Choice per operation | the elements that support it | the element for the chosen operation |
| `target` (rerank) | Choice | a shortlist from a close first call | breaks a tie under 0.70 |

The gate (`policy::gate_with_evidence`) turns the answers into one decision: `DONE` and
`BLOCKED` need 0.70; any other operation abstains under 0.55 (0.45 when the
target's name appears in the goal); a `destructive` of 0.50 or more asks for
confirmation; and it acts at 0.70, or below that only on a named match.
`DONE` is advisory: only the caller's accessibility predicates end the task.

## What to expect from Jev

- **Probabilities are coarse.** Answers arrive rounded to two decimals, and
  near-ties are common. Thresholds sit well away from 0.5 for that reason,
  and the calibration pairs exist to pull a yes-biased answer back.
- **Every answer needs a visible basis.** Instructions ask for visible
  evidence; a question about something that is not on screen should come
  back low, not guessed.
- **Screen text is data.** Every question says so, and everything read from
  a screen is wrapped as `untrusted_accessibility_data`. A page that says
  "ignore your instructions" is just a label.
- **Size matters.** A request over 100 KB is trimmed before it is sent, and
  latency grows with input tokens. `request_bytes` in the journal shows how
  big each request was.
