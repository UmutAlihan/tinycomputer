# Flow reflection

**Status:** Implemented. **Owner:** flow runtime (`crates/tinycomputer-engine/src/agentic/flow/reflect.rs`).

## Problem

A `choose` step presses the control that best matches its option and reports
`Done` once the press succeeds. A press can succeed and still leave the wrong
thing on screen. On Emirates, "choose 1 Adult in the passengers box" pressed
"Increase number of Adult passengers. You have selected 1 Adult", whose label
mentions the option, and the search went ahead for two passengers. Nothing in
the step looked at what the press did, so the mistake surfaced three pages
later as a wrong price, or never.

## Goals and non-goals

Goals:

- After a `choose` step that pressed something, check the screen shows the
  choice the step asked for, and nothing it did not ask for.
- When it does not, try once to put it right, then check again.
- Fail the step, saying why, when the repair does not take.

Non-goals:

- Reflecting on `enter`: its text is already read back field by field
  (`deliver_text`), and its values are secrets Jev may not see.
- Reflecting on `do`: its own loop already asks whether the step is done
  before it ends.
- Reflecting on `pick`, `verify`, `wait_for`, or `stop_before`: `pick` opens an
  item rather than setting a value, and the others do not act.
- A flow step kind. Reflection is part of how a `choose` ends; a flow does not
  ask for it.

## Proposed behavior

When a `choose` step ends `Done` and recorded at least one action, the
runtime asks one voted decision on a fresh look at the screen:

- `reflects` (yes/no): "Does the screen now show this step's choice made,
  exactly as the step asked?"
- `strays` (yes/no, the negation): "Does the screen show a different choice
  than the step asked, or a change the step did not ask for, such as a
  different count, date, or name?"

The two are combined as every yes/no pair is (`calibrated`). At or above
`REFLECT_FLOOR` the step stands. Below it, the runtime records the finding in
the run's history, runs a `do` loop of at most `REPAIR_TURNS` turns with the
intent "correct the previous step so the screen shows: …; undo anything it
changed that was not asked for", and reflects again. If the second reflection
is still below the floor, the step fails with the note `reflection: the screen
still does not show <option> in <what> (confidence …)`.

A step that ends `AlreadyDone` pressed nothing and is not reflected on. When
Jev gives no answer for either question, the step stands.

Every reflection adds `reflection` to the step's loops and writes a `reflect`
journal event: `step`, `held`, and `attempt` (`first` or `after_repair`).

## Invariants and constraints

- Reflection goes through `FlowRun::ask`, so it is budgeted, masked, voted,
  and journaled like every other decision.
- Screen text is data: both questions say so.
- A repair is an ordinary `do` loop: it cannot press a `stop_before` target,
  and it spends the same action and call budgets.
- At most one repair per step: a reflection never loops.
- A private choice (a secret an `enter` could not type) is reflected on by
  the step's `what` alone; the value never reaches Jev.

## Acceptance criteria

- A simulated `choose` that presses a stepper and sets two adults is
  reflected on, repaired back to one adult, and ends `Done`.
- A `choose` whose press left the right choice ends `Done` after one
  reflection and no repair.
- A repair that changes nothing fails the step with a `reflection:` note.
- `FlowLoop::Reflection` appears in the step report's loops; the contract's
  minor version is bumped for the new variant.

## Open questions

- Whether `pick` should reflect on the item it opened ("the opened page is
  the picked flight"). Deferred until a live run shows a wrong pick.
