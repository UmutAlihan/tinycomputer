# Reflection

`choose` is the one step kind that most looks like it cannot go wrong: it
finds the option, clicks it, and the click either succeeds or it does
not. Live runs proved that wrong. On Emirates, a step written as "choose
1 Adult in the passengers box" pressed the button labelled "Increase
number of Adult passengers. You have selected 1 Adult," a label that
happens to mention the option word for word while doing the opposite of
what the step wanted, and the search went ahead for two passengers instead
of one. Nothing in the step had looked at what the press actually did, so
the mistake did not surface until a wrong price appeared three pages
later, or sometimes never surfaced at all.

Reflection, in `reflect.rs`, is the fix: after a `choose` step ends `Done`
having pressed something, the runtime looks at the screen again before
moving on.

## What it checks

Two situations settle the question without asking Jev anything at all,
because the screen itself is unambiguous:

- a tab, radio, or option named exactly like the step's option is **not**
  selected, while a sibling of the same kind **is** selected (this is
  exactly the Emirates case: "Return" stayed selected after "One way" was
  pressed);
- the step typed something to filter a list, and the option it then
  pressed is still sitting there in the list, unselected, rather than
  having closed the list or shown itself as picked.

Field values are only shown to reflection when the request set
`include_values` (a task turns this on; secrets are still masked either
way).

Otherwise, one voted decision is asked:

- `reflects`: does the screen now show this step's choice made, exactly
  as the step asked?
- `strays`: does the screen show a *different* choice than the step
  asked, or a change the step never asked for at all, such as a different
  count, date, or name? (the negation of `reflects`, asked separately and
  calibrated against it, the way every yes/no pair is)

## What happens with the answer

At or above 0.50 (`REFLECT_FLOOR`), the step's `Done` outcome holds. Below that,
the runtime writes the finding into the run's history, then runs a short
`do` loop, at most 4 turns (`REPAIR_TURNS`), with the intent "correct the
previous step so the screen shows: ...; undo anything it changed that was
not asked for," and reflects a second time. If the second reflection is
still below the floor, the step fails, with a note in the shape
`reflection: the screen still does not show <option> in <what>
(confidence ...)`.

A step that ended `AlreadyDone` pressed nothing at all, and is never
reflected on: there is nothing a press could have gotten wrong. If Jev
gives no answer to either question, the step is simply left standing as
it was.

## What reflection deliberately does not cover

- **`enter`.** Its text is already read back field by field during
  delivery (see [Filling in forms](filling-forms.md)), and its values are
  secrets Jev is not allowed to see anyway.
- **`do`.** Its own loop already asks, every turn, whether the step is
  done before it ever ends the step.
- **`pick`, `verify`, `wait_for`, `stop_before`.** `pick` opens an item
  rather than setting a value; the other three do not act on anything, so
  there is nothing to reflect on.
- **Whether `pick` opened the right item.** This is a real open question
  (should `pick` reflect on "the page that opened is the picked flight"),
  deliberately deferred until a live run actually shows a wrong pick.

## Invariants

- Reflection goes through `FlowRun::ask`, the same as every other
  decision: it is budgeted, masked, voted, and journaled the same way.
- Both questions state plainly that screen text is data, never
  instructions.
- A repair is an ordinary `do` loop underneath: it cannot press a
  `stop_before` target, and it spends the same action and call budgets
  as anything else would.
- At most one repair per step. Reflection does not loop; it gets one
  second look, and that is final either way.
- Only `choose` is reflected on. Its option is never a secret; the
  private choices `enter` makes internally, through the same underlying
  code, are not reflected on because they are secrets by construction.

Every reflection adds `reflection` to the step's list of loops in the step
report, and writes a `reflect` journal event: which step, whether it
held, which attempt (`first` or `after_repair`), and, when a selected
sibling settled the question without asking Jev, that it was
`contradicted` by the screen rather than by an answer.

See [`docs/technical/specs/flow-reflection.md`](../../../technical/specs/flow-reflection.md)
for the full specification, and
[`docs/technical/decision-thresholds.md`](../../../technical/decision-thresholds.md)
for `REFLECT_FLOOR` and `REPAIR_TURNS`.
