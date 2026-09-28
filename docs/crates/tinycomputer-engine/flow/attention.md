# Attention

Before a step is judged, or an element is grounded for it, the runtime asks
one thing first: what on this screen needs attention, the step itself, or
something in the way of it? This runs at the top of every `do` turn, and
once before a `choose`, `enter`, `pick`, `read`, `extract`, or
`stop_before` step. It lives in `attention/`.

## Why this exists

A cookie banner, a promo toast, a newsletter prompt, or an app-install
card left in place does two things at once: it takes up a share of Jev's
attention when it is asked to judge the screen, and it can physically sit
on top of the element the step actually needs, turning a click meant for
a result card into a click on the toast that covers it. Live on Emirates,
a promo toast ("Unlimited date changes...", with a Close button) covered
the lower results while the flow kept pressing the fare card sitting above
it, one turn after another, never touching what it was actually clicking
on because it never noticed the toast was in the way. Attention exists to
catch this class of problem before it wastes a turn.

## Finding candidates without asking anyone

Distractions are found deterministically, with no Jev call, before the
attention question is even built. A container counts as a distraction
candidate when any of these hold:

- it holds a plain dismiss control: an X, "Close," "Not now," "No thanks,"
  "Got it," "Reject all," "Accept essential only," and similar;
- it is reported as being in front (the digest's own notion of an
  overlay);
- its labels mark it as a cookie, consent, privacy, newsletter, subscribe,
  offer, promo, app, or survey card.

A container is deliberately **not** offered as a distraction when:

- it is bigger than 12 elements (`MAX_DISTRACTION_SIZE`), or holds more
  than one text field, because that shape is the page itself, or a real
  form, not a distraction. Live on Emirates, the booking form's own
  clear-field close icons sat directly under the page's `main` region, and
  without this size cutoff they would have been offered as things to
  dismiss.
- an "Accept" or "Reject" control sits in ordinary page content rather than
  in one of the shapes above: that is the step's own business, not a
  distraction to clear out of its way.
- the step itself already names the exact distraction ("dismiss the
  cookie banner"), since that is not a distraction from the step, it is the
  step.
- the control looks irreversible, or was already pressed once this step.

Separately, when the surface marks elements as `covered` and one of them
is something the step actually names, whatever is sitting on top of it (a
calendar or list an earlier step left open, with no dismiss control of its
own) is offered too, and it is cleared with Escape, once per step. Live on
Emirates, a date calendar that stayed open over the booking form covered
the Class button the very next step needed to press.

## Asking, and only when there is something to ask about

A clean screen with no candidates costs nothing: no attention question is
asked at all. When there is at least one candidate, one Choice is asked,
"the step" against each distraction, describing what each one shows and
what would clear it.

## Clearing, on clear evidence only

A distraction is cleared only when the evidence behind the answer is
strong (the same evidence-gate reasoning covered in
[Deliberation](deliberation.md), with a floor of 0.5, `ATTENTION_FLOOR`).
It is cleared with its **least committal** control: rejecting or
essential-only first, closing plainly next, accepting outright last. At
most 3 distractions are cleared per step (`MAX_CLEARED`), and the screen
is looked at again after each one, since clearing one overlay can reveal
another underneath it.

## How this differs from the obstacle check in the do loop

The `do` loop's own `blocked` question (see [The do loop](the-do-loop.md))
still exists, and still catches a dialog the step's own judge sees as
blocking it. Attention runs earlier and catches a different case: a
distraction that is in the *screen's* way without necessarily being in the
*judge's* way, something the step's own judging question might never
mention because it is not obviously "blocking" in the way a modal dialog
is, just sitting where a click needs to land.

See [`docs/technical/decision-thresholds.md`](../../../technical/decision-thresholds.md)
for `MAX_DISTRACTIONS`, `MAX_CLEARED`, `MAX_DISTRACTION_SIZE`, and
`ATTENTION_FLOOR`, and
[`docs/technical/specs/jev-deliberation.md`](../../../technical/specs/jev-deliberation.md)
for how attention fits into the wider deliberation design.
