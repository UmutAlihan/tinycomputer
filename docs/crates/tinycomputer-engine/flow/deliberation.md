# Deliberation

Every decision described so far, grounding an element, judging whether a
step is done, checking a condition, ultimately comes down to a probability
compared against a fixed bar: 0.70 to act on a target, 0.75 to call a step
done. That works most of the time. It also has a specific, measured
weakness: a Choice's probability tells you how concentrated Jev's answer
is, not how likely that answer is to be right, and a single number cannot
tell those two things apart.

Live audits found exactly this failure, more than once:

| Recorded failure | Why one number decided badly |
|---|---|
| A target at 0.72 was accepted on weak corroboration | corroboration itself only reached 0.5, but the single bar on the target let it through anyway |
| `done` scored 0.78 on one rendering of a screen and 0.90 on another | a pass just barely over the bar, on one particular way of showing the same screen |
| Lookalike "Select" buttons split the vote | probability spread thinly over near-identical options reads as low confidence in each one, even when one really is the right one |
| A Pay button scored 0.44 first in the list and 0.01 fifth | position in the list moved the answer more than the actual page did |
| `enter` pressed a refused city row, and typed "Mumbai" into the wrong place | a wrong press was only ever noticed once the whole step failed |
| An undo was one press of Escape | a wrong navigation, a flipped toggle, or a typed value was never actually put back |

Deliberation, on by default (`deliberation: "deep"`; `"standard"` and
`"off"` are the other two levels), replaces the single bar with layers.
Most of the layers are ordinary deterministic Rust; Jev is only asked
again where the evidence is genuinely thin. The full research background
and the case-by-case audit trail live in
[`docs/technical/specs/jev-deliberation.md`](../../../technical/specs/jev-deliberation.md);
this page is the shape of it.

## 0. Attention comes first

Before any of this runs, attention clears whatever is actively in the way.
See [Attention](attention.md).

## 1. The evidence gate

Every framing of a voted decision keeps its own separate answer, called
the question's **ballot**. From a ballot, the gate (`evidence/`) reads
three numbers:

- `p`: the winner's mean probability across framings (or the mean belief,
  for a yes/no question);
- `margin`: how far ahead the winner is of the runner-up;
- `agreement`: what share of framings picked the winner (or landed on the
  same side of the threshold, for a yes/no question).

From those three numbers, a **verdict** follows:

- **Accept**: the winner clears its own floor, the margin is at least 0.25
  (`ACCEPT_MARGIN`), and agreement is at least 0.80 (`ACCEPT_AGREEMENT`).
  Act on it, no further questions.
- **Abstain**: `none` won outright, or the winner is under 0.20
  (`ABSTAIN_FLOOR`) with agreement under 0.40 (`ABSTAIN_AGREEMENT`).
  Nothing on screen serves; leave it.
- **Deliberate**: anything in between. Ask more before acting.

A yes/no judgement (a step's completion, a condition) is accepted as read
when it sits at least 0.12 (`UNDECIDED_BAND`) away from its threshold and
its framings agree with each other; otherwise it is deliberated. A yes/no
judgement is never abstained from: there is always an answer to a
yes/no question, even a weak one.

The gate replaces the single bar at exactly the places that single bar
used to be: grounding's final target choice, the `do` judge's completion
check, and every condition check (`verify`, `wait_for`, `if`,
`repeat_until`, and `stop_before`'s "has this happened" check).

## 2. The escalation ladder

A deliberated decision climbs a ladder, one rung at a time, and stops the
moment a rung settles it.

1. **More framings.** The same request is asked in whichever framings it
   has not been asked in yet, up to the run's vote cap (9,
   `MAX_VOTES`), and the new answers simply join the existing ballot.
2. **A duel** (for a target choice only). The top few finalists (up to 4,
   `MAX_FINALISTS`, each needing at least 0.05, `FINALIST_FLOOR`, to
   qualify) are compared two at a time. Every pair is asked "A or B?" with
   A listed first, and again with B listed first, and the results are
   counted Copeland-style: one finalist beats another when it takes more
   than half of their combined share across both orderings. A finalist
   that beats every other finalist, taking at least 0.60 of each pairing
   (`DUEL_WIN`), becomes the champion. Asking both orders cancels out
   position bias, and comparing lookalikes two at a time, rather than
   splitting a vote across all of them at once, is exactly what keeps two
   near-identical options from drowning each other out.
3. **Contrast** (deep level only, when the duel could not agree on a
   champion). The two leaders are each asked "is this the element?"
   alongside "is this only something similar to it, or just next to it?",
   calibrated as a pair. One is taken as the answer if its belief reaches
   0.65 (`CONTRAST_ACCEPT`) with a lead of at least 0.20
   (`CONTRAST_LEAD`) over the other.
4. **Views** (deep level only, and only for a judgement that would
   otherwise pass). The same yes/no is asked again, but over different
   renderings of the screen: the screen alone with no history, and just
   what changed since the step began. The readings are combined by their
   **median**, not their minimum, so one dissenting view can neither pass
   a judgement on its own nor veto one on its own. This matters because
   some views are structurally blind to some evidence: live on IndiGo, a
   screen-only view read "choose One Way" at 0.2 after the option had
   already been pressed, because a radio button that is already selected
   shows no visible sign of *who* selected it or *when*. A minimum rule
   would have let that one blind view fail the step outright; the median
   keeps it as one voice among three instead.

**A close call that no rung settles is still acted on**, at whatever its
best available ranking is (the duel's champion, failing that the duel's
leader, failing that the original pick as read), with the runners-up kept
for a possible backtrack later. Deliberation is there to change which pick
gets acted on, and to refuse a pick only when the evidence genuinely says
nothing on screen serves; it is not there to make the run stop and do
nothing on a hard case. Live on IndiGo, abstaining after an inconclusive
contrast once left a `do` step pressing nothing at all until it stalled
out, in a situation where pressing something, checking its effect, and
undoing it if wrong would have recovered fine on its own.

At the `standard` deliberation level, a target's climb stops at the duel;
contrast and views are deep-level only. See the table below.

Every rung of the ladder is one more call through `FlowRun::ask`, so the
budget, briefing, masking, and journal all apply to it exactly as they do
to the first request, and every rung checks the budget before spending
anything. A run that is short on calls simply stops climbing and decides
with whatever evidence it already has, rather than failing outright for
lack of deliberation.

## 3. Tree grounding

Covered on its own page: [Grounding](grounding.md#the-tree-beam).

## 4. Denoising

Before anything is shown to Jev, `denoise/` removes what should never have
cost a question in the first place:

- disabled elements, and elements with zero area on screen, never reach
  Jev at all;
- a control that is exposed twice, such as a link that wraps its own
  visible label as a separate child element, is offered once;
- the remaining pool is ranked the way a person actually sees a page: what
  is currently in view first, then what is scrolled out of view
  (`offscreen`), then what is behind something else (`covered`, typically
  a dialog or drawer sitting in front of it), keeping the original order
  within each tier;
- a screen that returns to exactly where it was two turns ago is
  recognised as an oscillation, and both presses that caused the back-and-
  forth are banned;
- a repeated history line is shown once, with a count, instead of once per
  repetition.

Off-screen elements are demoted in the ranking, not dropped outright: when
one is actually pressed, the browser scrolls it into view first.

At the source, the browser's own page observer (`sight.js`) does an
earlier pass of the same kind, dropping ads, empty containers, and hidden
content before any of this even runs; see
[`docs/technical/specs/browser-sight.md`](../../../technical/specs/browser-sight.md).

## Levels

| | `off` | `standard` | `deep` (default) |
|---|---|---|---|
| Attention (clear distractions first) | no | yes | yes |
| Evidence gate | no | yes | yes |
| More framings | no | yes | yes |
| Duel | no | yes | yes |
| Contrast, views, wider cross-check | no | no | yes |
| Tree beam | no | yes | yes |
| Denoising | no | yes | yes |
| Expectation questions | no | on a miss | on a miss |
| Checkpoints and verified undo | no | yes | yes |
| Backtracks per step | 0 | 1 | 3 |
| Irreversible-press bar | 0.50 (`LOCATE_FLOOR`) | 0.50 | 0.85 (`IRREVERSIBLE_FLOOR`) |

Every layer can also be switched off individually through
`disabled_loops`: `attention`, `evidence`, `escalation`, `duel`,
`tree_grounding`, `denoise`, `expectation`, `checkpoint`, `backtrack`. See
[Budgets and switches](budgets-and-switches.md).

## What it costs

Clear evidence costs nothing extra: when every gate accepts a decision as
it first reads it, a deep run makes exactly as many Jev calls as an `off`
run would, with one small addition (the expectation yes/no questions,
which ride along inside the judge's request it already sends, so they add
no extra round trip). Spending grows only where uncertainty actually is: a
fully deliberated target can cost up to 8 framings, one duel request, and
one contrast request, each multiplied by the number of votes. To leave
room for that without running dry mid-task, the default votes went from 5
to 7, the default flow call budget from 1500 to 3000, the task call budget
from 3000 to 6000, and the module's own cap from 5000 to 10000.

See [Undo and backtracking](undo-and-backtracking.md) for what happens
after a press, and
[`docs/technical/decision-thresholds.md`](../../../technical/decision-thresholds.md)
for every constant named on this page.
