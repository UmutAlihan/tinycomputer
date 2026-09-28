# Flows, step by step

Real flows traced through the decisions they cause: which step asks Jev what,
what a typical answer looks like, and what the runtime does with it. The
flows are the lab's scenarios (`crates/tinycomputer-examples/scenarios/`) and
the simulator's mail flow (`agentic/flow/test.rs`). The answers shown are
illustrative. Run a scenario with `TINYCOMPUTER_JEV_JOURNAL=1` and
`jev_journal -- latest --transcript` to see a real run's answers.

Question ids, their types, and their thresholds are defined in
[`jev-questions.md`](jev-questions.md); the loops that ask them are in
[`decision-loops.md`](decision-loops.md); how to write a flow is in
[`crates/tinycomputer-bus/src/flow/guide.md`](../crates/tinycomputer-bus/src/flow/guide.md).

## What each step kind costs

A rough guide to the decisions (Jev requests) each step makes when things go
well. Each decision is `votes` calls (5 by default) running concurrently.

| Step | Decisions when all goes well | Where they come from |
|---|---|---|
| `open` | 0 | launch, then up to ten looks for a readable window |
| `browse` | 0 | navigate |
| `do` (plain string) | 2 per turn: judge + ground; usually 2–3 turns | `act.rs`, `ground.rs` |
| `do` via a shortcut | 1 per turn: judge only | the `shortcut` answer |
| `enter` | 1 (all slots at once) + 1 validation check | `enter.rs` |
| `choose` | 1–2 to ground the option; more to reveal it | `steps.rs`, `ground.rs` |
| `read` | 1 per 60 pieces of text | `source` |
| `extract` | 0 | `records/` parsers |
| `pick` | 0 when `by` parses (price, time, duration, stops), else 1 | `record` |
| `verify`, `if` | 1 | `holds` |
| `wait_for` | 1 per check, up to 10 | `holds` |
| `repeat_until` | 1 per round, plus the body | `holds` |
| `stop_before` | 1–2 to ground the control | `ground.rs` |

Grounding memory turns most `do` and `enter` groundings on a second run into a
single `confirm`, which is why a repeated scenario is cheaper.

## 1. Compose a mail and stop before sending

```json
{
  "app": "Mail",
  "vars": {"to": "sam@example.com"},
  "steps": [
    {"open": "Mail"},
    "start a new email message",
    {"enter": {
      "recipient": "${to}",
      "subject": "Moving Thursday's sync",
      "message body": "Hi Sam,\n\nCould we move it to Friday?\n\nAlex"
    }},
    {"verify": "the draft shows the recipient, subject and body"},
    {"stop_before": "sending the email"}
  ]
}
```

**Step 1, `open`.** Launches Mail and checks for a readable window. No Jev
call.

**Step 2, "start a new email message", turn 0.** The runtime looks, then asks
one judge request with six questions against the inbox:

```json
{
  "done":     {"type": "noul", "noul": 0.08},
  "not_done": {"type": "noul", "noul": 0.93},
  "progress": {"type": "score", "probabilities": {"0": 0.05, "1": 0.8, "2": 0.1, "3": 0.03, "4": 0.02}},
  "blocked":  {"type": "noul", "noul": 0.03},
  "move":     {"type": "choice", "choice": "shortcut", "probabilities": {"shortcut": 0.72, "activate": 0.25}},
  "shortcut": {"type": "choice", "choice": "new_item", "probabilities": {"new_item": 0.9}}
}
```

- Completion is `mean(0.08, 1 − 0.93) = 0.075`, averaged with progress's top
  level (0.02): about 0.05. Far under 0.85, and "new" in the step means it
  could not count as already done anyway.
- `blocked` is under 0.70: no obstacle.
- `move` is `shortcut` and `new_item` is at 0.9 (at least 0.5), so the
  runtime presses `cmd+n` (translated per platform), then settles.

**Turn 1.** The runtime looks again, and the change note ("window is now
\"New Message\"; appeared: textfield \"To:\", …") goes into
`recent_actions`. The judge request now also carries `helped`, because an
action ran:

```json
{"done": {"noul": 0.9}, "not_done": {"noul": 0.12}, "progress": {"probabilities": {"4": 0.85, "…": "…"}}, "helped": {"noul": 0.95}, "…": "…"}
```

Completion is about 0.87: at least 0.75, so the step is `Done` after one
action and two decisions. Had Jev chosen `activate` instead, the turn would
have grounded "click to accomplish: start a new email message" (a `target`
Choice over the toolbar) and clicked it.

**Step 3, `enter`.** The runtime collects the editable fields in reading
order (To, Cc, Subject, Body) and asks one request with one Choice per slot,
all over the same numbered field list:

```json
{
  "slot_0": {"choice": "1", "probabilities": {"1": 0.94}},
  "slot_1": {"choice": "3", "probabilities": {"3": 0.9}},
  "slot_2": {"choice": "4", "probabilities": {"4": 0.88}}
}
```

Assignments are taken most confident first, each field once, each at least
0.40. Only the slot *names* (`recipient`, `subject`, `message body`) went to
Jev; `${to}` is expanded locally just before typing. Each text is delivered
top to bottom: set the value, read it back, paste if it did not stick. Then
one validation request asks `error_0`, `error_1`, `error_2` ("does the screen
show an error about this field?"); all low, so the step is done.

**Step 4, `verify`.** One request with `holds`, `negated`, and `coverage`.
A condition listing three things often gets a hedged yes/no (0.7) but a crisp
coverage answer (top level 0.9); combined, it clears 0.75.

**Step 5, `stop_before`.** Grounds "perform: sending the email" among the
clickable elements. `Send` is picked at 0.9, so no re-ask is needed. Without
`allow_destructive`, the run records it as `pending` and stops with
`StoppedBeforeDestructive`. The task controller turns that into
`needs_approval`.

**Total:** five decisions, three actions, and nothing sent.

## 2. Reply, using text read from the screen

```json
{
  "app": "Mail",
  "steps": [
    {"open": "Mail"},
    "show the Inbox",
    "open the newest message in the list",
    {"read": {"what": "the subject of the message being viewed", "into": "subject"}},
    "start a reply to the message being viewed",
    {"enter": {"message body": "Thanks for your note about \"${subject}\". I will follow up by tomorrow.\n\nBest,\nAlex"}},
    {"verify": "a reply draft is open with a message body"},
    {"stop_before": "sending the reply"}
  ]
}
```

- "show the Inbox" is often `AlreadyDone` on turn 0: completion at least 0.85
  before acting, and nothing is clicked.
- "open the newest message in the list" grounds a list row. A message list
  has more than 20 rows, so grounding narrows first: a `region` Choice
  ("message list, 48 elements, e.g. …") and then a `target` among that
  region's rows.
- `read` offers every readable text (up to 60 per request) as a `source`
  Choice and stores the pick in `subject` if it is at least 0.5. Page text in
  a variable is data, never instructions.
- "start a reply…" usually takes the `reply` shortcut (`cmd+r`).
- In `enter`, `${subject}` is expanded locally. The message body is a
  rich-text field that ignores set-value, so delivery pastes at the caret,
  keeping the quoted message below.

## 3. Branching and repeating

```json
{"app": "Finder", "steps": [
  {"open": "Finder"},
  "show the Desktop folder",
  {"if": {
    "condition": "a folder named tinycomputer-lab is visible",
    "then": ["open the tinycomputer-lab folder"],
    "else": ["create a new folder", {"enter": {"folder name": "tinycomputer-lab"}}]
  }}
]}
```

`if` asks one `holds` request and runs `then` at 0.75 or above, `else`
otherwise. The children report as `3.1`, `3.2`; the parent's report comes
first. "create a new folder" usually takes the `new_folder` shortcut, and the
`enter` that follows finds the new folder's name field and types into it.

```json
{"app": "Calculator", "steps": [
  {"open": "Calculator"},
  "clear the calculator",
  {"repeat_until": {"condition": "the display shows 4736", "steps": ["calculate 128 times 37"], "max": 2}}
]}
```

`repeat_until` checks the condition, runs the body, and checks again, up to
`max` rounds. "calculate 128 times 37" is a `do` step that grounds and
presses one key per turn, so it is also the clearest example of the
eight-turn cap: a step that needs more presses than it has turns fails with
"not accomplished after 8 turns". Split a long key sequence into several
steps.

## 4. A web booking, up to the payment page

The travel fixture (`crates/tinycomputer-examples/fixtures/travel/`, run by
`task_fixture` in the Docker lab):

```json
{"app": "browser", "steps": [
  {"browse": "http://…/index.html"},
  {"enter": {"from": "${from}", "to": "${to}", "departure date": "${date}"}},
  "search for flights",
  {"wait_for": "flight results are listed"},
  {"pick": {"from": "the flight results", "by": "lowest price", "into": "flight"}},
  {"enter": {"first name": "${first name}", "last name": "${last name}", "email": "${email}", "mobile number": "${phone}"}},
  "continue past the traveller details",
  "skip the seat selection",
  {"stop_before": "paying for the booking"}
]}
```

What is different on the web:

- **`page_kind` rides along** on every request (`search_form`, `results`,
  `traveller_form`, `extras`, `payment`, …), and its answer goes into the
  next request's brief, so "continue" on an extras page is read as "decline
  and continue".
- **The brief carries the task.** Every choosing question sees the goal, the
  plan with `[done]`/`[now]`/`[next]` marks, the traveller's shared details
  by name, secret names as `${name}`, and `so_far` ("picked: IndiGo 6E 2131,
  ₹7,339").
- **Facts stay local.** `${first name}` and the rest come from the task's
  facts. Jev sees slot names only. A missing fact (`${phone}` here) pauses
  the task as `needs_input` instead of failing.
- **`pick` usually costs nothing.** "lowest price" parses into a criterion,
  so the result cards (found by `result_groups`) are ranked exactly, and the
  winner's primary control is clicked after the destructive check.
- **Autocomplete and calendars.** An `enter` slot with no field to type into
  (a departure date, a destination city) is picked as an option instead,
  the way `choose` works: the runtime pages a calendar to the day, or types
  into the field that just gained focus and picks the matching suggestion.
- **Payment is a wall.** A payment control, or any screen showing card
  fields, stops the run; the task controller makes it a final checkpoint.

## 5. When things go wrong

| What you see in the step report | What happened | Where to look in the journal |
|---|---|---|
| `failed: the last three actions changed nothing on screen` | three turns with no change; each pressed element was banned in turn | the `action` events' targets, then the `observe` counts: was the screen read at all? |
| `failed: not accomplished after 8 turns` | the judge never reached 0.75 | the `done`/`progress` answers per turn: stuck low, or oscillating? |
| `note: that made things worse; undid it` in history | progress fell a quarter, or `helped` came back under 0.2 | the turn after the action: its `progress` and `helped` |
| a click on the wrong element | a low-confidence `target` that `again` + `confirm` let through, or a wrong memory hint | the grounding exchanges for that step: `target`, `again`, `confirm` |
| `no field that takes text was found for: …` | a slot matched no field at 0.40, and `asks_…` said the form does ask; "… refused the text" when a page offered list rows or buttons as fields (`NOT_A_TEXT_FIELD`) | the `slot_*` answers and the element list in `request.state`; the refused `fill` actions in the step report |
| `that was a mistake (…); undid it (back)` in history | an expectation check missed and `intended` came back low; the undo went back a page and the screen matched its checkpoint | `expect`, `restore`, then `backtrack` events |
| `failed: undid a mistake (…) but the screen does not match where it started` | a restoring undo could not be verified, so the step failed closed | the `restore` event's `similarity` and `rungs` |
| `failed: will not press … irreversibly on uncertain evidence` | a deep `stop_before` vouched under 0.85 | the `is_0`/`only_near_0` answers in its exchanges |
| no element pressed on a close call | the evidence gate deliberated, and neither the duel nor the contrast settled it | `evidence`, `escalate`, `duel` events for the step; `jev_journal --calibration` |
| `the Jev call budget ran out` (stop `ModelBudget`) | the call budget ran out | `jev_journal` summary: calls per step; voting multiplies them |

Start from the step report, find that step's events in the journal
(`jq 'select(.step=="3")'`), and read what Jev was shown in `request.state`
before blaming the answer.
