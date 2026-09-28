# Jev deliberation

Status: Implemented. On by default (`deliberation: "deep"`); `"off"` keeps the
single-threshold gates. Contract 2.3. Plan:
[`../plans/jev-deliberation.md`](../plans/jev-deliberation.md).

## Problem

Every flow decision used to hang on one number against one bar: a target at
0.72 cleared `ACT` (0.70), a judge at 0.78 cleared `DONE` (0.75). The live
audits show what that costs:

| Recorded failure | Why one number decided badly |
|---|---|
| A target at 0.72 accepted on a 0.5 `confirm` ([flow-examples](../flow-examples.md)) | weak corroboration let a borderline pick through |
| `done` 0.78 on the wide judge, 0.90 on the narrow one ([audit](../evals/2026-09-28-jev-call-audit.md)) | a pass just over the bar on one rendering of the screen |
| Lookalike "Select" buttons split the vote | probability spread over twins reads as low confidence in each |
| The Pay button at 0.44 first, 0.01 fifth | position bias moved the answer more than the page did |
| `enter` pressed a refused city row, choosing Mumbai | a wrong press was noticed only by the step failing |
| An undo is one Escape | a wrong navigation, toggle, or typed value is never put back |

Jev's own client documents the root cause: a Choice's `confidence` is the
*concentration* of its distribution, not the probability it is right. A bar on
it is uncalibrated by construction.

## What the research says

- **NumericJev** (arxiv 2609.28587) decodes numbers over a Jev-like choice
  interface as a multiway decision tree. Two findings carry over. First, error
  is dominated by the *first divergence*: a wrong early branch can never be
  recovered below it. Second, categorical probabilities are not calibrated, and
  a small K-way choice is *less order-sensitive* than one flat choice over
  many options.
- **WebOperator** (arxiv 2512.12692, 54.6% on WebArena) generates candidates
  from varied contexts, merges equivalent ones, and ranks them by reward *and*
  reversibility, deferring destructive actions. It backtracks to checkpoints
  it validates before trusting them. About 40% of its successes needed a
  backtrack.
- **WebRollback** (arxiv 2504.11788) shows that an explicit rollback to a
  recorded state beats pressing on after a bad action.
- **Tree search for LM agents** (arxiv 2407.01476) and world-model lookahead
  predict an action's effect and check it after acting.

## Decision

Deliberation replaces the single-number gate with layers. Most are
deterministic Rust, and Jev turns are spent only where the evidence is thin.

### 1. The evidence gate (`flow/evidence/`)

Every framing's own answer is kept as the question's **ballot**
(`vote::ballots`). From a ballot:

- `p` is the winner's mean probability;
- `margin` is its lead over the runner-up;
- `agreement` is the share of framings that picked it.

A Choice is **accepted** when `p` clears the site's floor, the margin reaches
`ACCEPT_MARGIN`, and agreement reaches `ACCEPT_AGREEMENT`. It is **abstained**
from when `none` wins, or when `p` is under `ABSTAIN_FLOOR` and agreement is
under `ABSTAIN_AGREEMENT`. Otherwise it is **deliberated**.

A yes/no judgement (completion, a condition) is accepted as it reads when it
lies at least `UNDECIDED_BAND` from its threshold and its framings agree.
Otherwise it is deliberated; it is never abstained from.

The gate runs where the single bars used to:

- grounding's final target (`ACT`, or `NAMED_FLOOR` for an exact name);
- the `do` judge's completion (`DONE` and `ALREADY_DONE`);
- conditions (`verify`, `wait_for`, `if`, `repeat_until`, and `stop_before`'s
  "has happened").

### 2. The escalation ladder (`flow/escalate/`, `flow/duel/`)

A deliberated decision climbs until a rung settles it:

1. **More framings.** The request is asked in the framings it has not had
   yet, up to `MAX_VOTES`, and the new answers join the ballot.
2. **A duel** (target only). The top finalists (`MAX_FINALISTS`, at least
   `FINALIST_FLOOR` each) are compared pairwise. Each pair is asked in both
   orders and counted Copeland-style. A finalist that takes at least
   `DUEL_WIN` of every pairing is the champion. Asking both orders cancels
   position bias, and two options cannot split a vote between lookalikes.
3. **Contrast** (deep only). The champion, or failing one the two leaders, is
   asked "is this the element?" beside "is this only similar to it, or next
   to it?", calibrated as a pair. A champion is kept at `CONTRAST_KEEP`. With
   no champion, a leader is taken only at `CONTRAST_ACCEPT` and a lead of
   `CONTRAST_LEAD`. Otherwise grounding abstains and nothing is pressed.
4. **Views** (deep only, judgements). The yes/no is asked again over other
   renderings: the screen alone, without the history that can lead it, and
   what changed since the step began. Views on the same side of the threshold
   are averaged; views that straddle it keep the minimum, so the step stays
   open or the condition does not hold.

At `standard`, a target stops at the duel: the champion is taken, and no
champion means abstaining.

Every rung goes through `FlowRun::ask`, so budget, masking, voting, and the
journal all apply. Every rung checks the budget first. A run short of calls
stops climbing and decides on what it has, never failing for lack of
deliberation.

### 3. Tree grounding (`ground.rs`)

Narrowing is a two-level tree: a region, then a knockout of `CAP`-sized
groups, then the final Choice. Wherever the region answer's lead is under
`BRANCH_MARGIN`, the runner-up region is kept too, a beam of two that guards
against the first divergence. At the deep level, when the region cut dropped
some group winners, the final Choice is also asked over every winner in the
same round trip. A disagreement between the two sends both picks to a duel.
No Choice ever exceeds `CAP` options.

The target's runners-up are kept as the step's **frontier**, the branches a
backtrack tries.

### 4. Denoising

At the source, the browser's `sight.js` drops ads, empty boxes, and hidden
content, and reports the counts ([`browser-sight.md`](browser-sight.md)). In
the flow (`flow/denoise/`):

- disabled and zero-area elements never reach Jev;
- a control exposed twice (a link wrapping its own label) is offered once;
- the pool is ranked by what a person sees: in view, then `offscreen`, then
  `covered` (behind a dialog or drawer), keeping order within each tier;
- a screen that returns to where it was two turns ago bans both presses (an
  oscillation);
- repeated history lines read once, with a count.

Off-screen elements are demoted, not dropped: a browser scrolls a target
into view when it is pressed.

### 5. Expectations (`flow/expect/`)

Before a `do` press, its **effect** is predicted from the move and the
element's role and state alone:

| Prediction | When |
|---|---|
| opens | `expand`, a combobox, or a collapsed control |
| selects or clears | a tab, radio, or option; a checkbox or switch |
| navigates | a link |
| closes | a Close, Cancel, or Done button |
| scrolls | a `scroll` move |

After the press the effect is checked against the screen and the address.
Only a contradiction counts, which is a **miss**. Examples: a checkbox that
does not show the state it was pressed toward, or a press that left the page
when it should have opened a menu. An effect the screen neither confirms nor
contradicts is `unclear` and changes nothing.

After a miss, and after every press at the deep level, the next judgement
carries `intended`/`unintended`, calibrated as a pair. Two cases are treated
as a mistake:

- a miss with belief under `MISTAKE`;
- any press with belief under `CLEAR_MISTAKE`.

A mistake joins the old triggers (a progress drop of `REGRESSION`, `helped`
under `UNHELPFUL`) for an undo.

### 6. Checkpoints and a verified undo (`flow/checkpoint/`)

Every deliberated press records a **checkpoint**: the ref-free marks of the
screen, and the surface's address from the last reply that carried a `url`.
Each action is classified:

| Class | Actions | Undone by |
|---|---|---|
| reversible | opens, scrolls | Escape |
| restorable | a toggle | pressing it again |
| restorable | a navigation | `Surface::back`, then loading the recorded address |
| irreversible | send, pay, delete, a `stop_before` control | nothing |

The undo is **verified**. The screen must show `RESTORED` of the
checkpoint's marks, at the recorded address. If a rung that claims to
restore (back, navigate, press again) misses, the step **fails closed**. An
Escape that misses is noted and the loop goes on, as before.

`Surface::back` is new. The browser goes back and replies with the address
it landed on; a desktop application refuses it by default.

An irreversible press is never an exploration branch. A `stop_before` that
may press its control (`allow_destructive`) needs `IRREVERSIBLE_FLOOR` at the
deep level. A pick short of that is vouched for once more ("is it?" against
"is it only similar?", widened), and the press is refused unless that
belief reaches the floor.

### 7. Backtracking (`act.rs`, `reflect.rs`)

After an undo, the step's frontier offers the best runner-up that is not
banned. On the next `activate` it is confirmed with one `confirm` and pressed
if at least `AGREED`, before anything is grounded afresh. Each step gets
`MAX_BRANCHES` backtracks: three deep, one standard.

A `choose` whose reflection fails first returns to the address the step
began at (verified) when it has left it, and only then repairs.

## Levels

| | `off` | `standard` | `deep` (default) |
|---|---|---|---|
| Evidence gate | — | yes | yes |
| More framings | — | yes | yes |
| Duel | — | yes | yes |
| Contrast, views, wider cross-check | — | — | yes |
| Tree beam | — | yes | yes |
| Denoising | — | yes | yes |
| Expectation questions | — | on a miss | every press |
| Checkpoints and verified undo | — | yes | yes |
| Backtracks per step | — | 1 | 3 |
| Irreversible-press bar | `LOCATE_FLOOR` | `LOCATE_FLOOR` | `IRREVERSIBLE_FLOOR` |

Each part can also be turned off on its own through `disabled_loops`:
`evidence`, `escalation`, `duel`, `tree_grounding`, `denoise`, `expectation`,
`checkpoint`, `backtrack`.

## Cost

Clear evidence costs nothing extra. When every gate accepts as it reads, a
deep run makes exactly as many Jev calls as an `off` run; its only addition,
the expectation Nouls, rides in the judge's existing request. The simulator
pins this. Spend grows only with uncertainty: a deliberated target costs up
to 8 framings, one duel request, and one contrast request, each multiplied by
the votes. To leave room for that, the default votes are now 7 (previously
5), the default flow budget 3000 calls (previously 1500), the task budget
6000 (previously 3000), and the module cap 10000 (previously 5000).

## Contract

- Adds `RunFlowRequest.deliberation` (`off`/`standard`/`deep`, default `deep`)
  and `TaskBudget.deliberation`.
- Adds the eight `FlowLoop`s above.
- Adds `Surface::back`, which refuses by default.
- `CONTRACT_VERSION` 2.3.

## Observability

New journal events (see [`../jev-journal.md`](../jev-journal.md)):

- `evidence` and `escalate` for the gate and each rung climbed;
- `views` and `duel` for those rungs;
- `expect` for each effect check;
- `checkpoint`, `restore`, and `backtrack` for undo and backtracking;
- `denoise` for oscillations.

`jev_journal -- <id> --calibration` tabulates each site's verdicts against how
the steps ended, so the constants can be tuned on live runs rather than
guessed.

## Out of scope

- **Reflection** for `enter`, `pick`, and `do`. `enter` already verifies each
  value by read-back and checks field errors. A failed reflection there would
  need a repair that can type, which the `do` loop cannot.
- **Checkpoints for typed text in the `do` loop.** It never types; `enter`
  re-enters a flagged field instead.
- **Predictions on the wide strategy.** Its turns get the expectation
  questions and the verified undo, but its targets keep the wide `prepare`
  path's thresholds.
- **Calibrating the constants on live data.** They are set from the recorded
  audits, and the calibration view exists to revise them.
