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

`extract` asks nothing, and `pick` asks nothing when its `by` parses as a
price, time, duration, or stop-count criterion.

## The goal loop's questions (`agentic/policy.rs`)

`RunGoal` and `ResolveIntent` ask one larger request per decision. Their state
is `goal`, `app`, `window`, `surface`, and the last eight `recent_actions`.

| Id | Type | Given | Answer used as |
|---|---|---|---|
| `operation` | Choice | `WAIT`, `DONE`, `BLOCKED`, `WIDEN` (when scoped), and each operation some element supports: `CLICK`, `TYPE_TEXT`, `CHECK`, `UNCHECK`, `EXPAND`, `COLLAPSE`, `SCROLL`, `DRILL` | the next operation; see the gate below |
| `destructive` | Noul | the goal | at 0.50, the action needs confirmation |
| `click_target`, `type_text_target`, … | Choice per operation | the elements that support it | the element for the chosen operation |
| `target` (rerank) | Choice | a shortlist from a close first call | breaks a tie under 0.70 |

The gate (`policy::gate`) turns the answers into one decision: `DONE` and
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
