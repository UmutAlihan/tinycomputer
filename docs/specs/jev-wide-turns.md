# Wide turns: fewer, larger, better-informed Jev decisions

**Status:** Implemented, opt-in (`strategy: "wide"`); the narrow strategy
stays the default until live runs show the wide one is no worse.
**Plan:** [`../plans/jev-wide-turns.md`](../plans/jev-wide-turns.md).
**Baseline:** [`../evals/2026-09-28-jev-call-audit.md`](../evals/2026-09-28-jev-call-audit.md).

## Problem

Real interfaces are hostile: cookie walls, upsells, pop-ups, footers of
links, twenty identical "Select" buttons. A person steps through them without
thinking; the flow runtime steps through them with many small Jev decisions,
one after another, each shown a flat list of up to 120 elements. Measured,
a call uses about a twelfth of Jev's 32K-token window, a normal `do` turn is
two round trips over the same screen, and a crowded screen up to seven. Jev
calls are cheap and effectively disposable; round trips and missing context
are what cost.

## Goals

- **Parse the screen for Jev.** Show what is in front first, group elements
  into regions, show a list of cards one line per card, collapse noise, and
  spend a byte budget on the regions that matter.
- **Triage a crowded screen.** Ask once which regions matter to the step and
  which are distraction, and use the answer to rank what is shown and what is
  offered as a target.
- **One request per turn.** Ask the judgement, the obstacle, and every move's
  target together, and act on the answers already held.
- **Give every decision a working memory.** Finished steps, recent actions
  across step boundaries, what was tried and failed, the next step, what was
  read, and the budget left.

## Non-goals

- Changing the Jev client (`vendor/tinyinference`), the thresholds, or the
  safety rules. Every threshold of the narrow strategy applies unchanged.
- Replacing the narrow strategy. Both ship; the lab compares them.
- Settling time. The baseline shows it dominates browser runs; it is a
  separate change.

## Behavior

`RunFlowRequest.strategy` is `"narrow"` (default) or `"wide"`;
`TaskBudget.strategy` carries it through the task API. Contract 2.1.

Under `"wide"`:

1. **The state.** Every question's state keeps `app`, `window`, `surface`,
   `current_step`, `visible_text`, and `field_contents` (with
   `include_values`), and replaces `elements` and `recent_actions` with:
   - `screen`: the digest (`tinycomputer_core::surface::digest`) as
     `{"untrusted_accessibility_data": {"in_front": [...], "regions": [...],
     "collapsed": [...]}}`, each element a line `eN role "name" [states]`,
     each list region one line per card, noise and distraction collapsed to
     one summary line, within `DIGEST_BUDGET` (24,000 bytes; at 40,000 a
     live booking page reached 28,000 of Jev's 32,000 tokens);
     an element with no name is shown with the nearest named container it
     sits in (`combobox in button "destinationCity …"`), in the element
     lines and in every element option (`near`);
   - `memory`: `steps_done` (one line per finished step), `now`,
     `recent_actions` (the last 24 history lines, across steps),
     `tried_and_failed`, `next_step`, `variables_read`, `budget_left`.
   The memory never holds the goal-level brief; choosing questions keep
   the brief as before.
2. **The survey.** On a `do` turn whose screen has more than `CROWDED` (40)
   actionable elements and at least two regions, one request asks, per
   region (up to 24), `relevance_<id>` (five levels) and `distraction_<id>`
   (a Noul; at `DISTRACTION`, 0.7, the region is collapsed and ranked last).
   Answers are kept by region name for the rest of the step; the page is
   surveyed again only about regions it has not seen, and only when they are
   more than three tenths of it — opening a dropdown is not worth a survey.
3. **The turn.** One request asks the judging questions and:
   `dismiss` over the safe controls of the region in front (plus
   `escape`), `dismiss_known` for a remembered control, and per move
   (`activate`, `expand`, `scroll`) over up to 40 candidates ranked by
   relevance, with elements the purpose names first and the element pressed
   last turn last (pressing a toggle again closes what it opened): `target_<move>` and
   `again_<move>`, or `group_<move>_<n>` knockout Choices, and
   `known_<move>` for a remembered element.
4. **Acting on it.** A target at `ACT`, or at `NAMED_FLOOR` when named, is
   used; a hesitant one gets one `confirm` Noul with the narrow rules; a
   knockout with several winners gets one final Choice; `none` is not
   re-asked that turn. An obstacle judged `blocked` is cleared from
   `dismiss` with no further request.
5. **Memory across runs.** The control that dismissed an overlay is learned
   as a grounding hint keyed `obstacle <region>`; a later run confirms it
   with `dismiss_known` instead of choosing again.

Changes found on the live runs apply to both strategies:

- elements Jev cannot tell apart (identical descriptions, bounds aside) are
  offered once, the first in page order, in element Choices and slot
  matching;
- an unnamed element is described by the nearest named container it sits in;
- text is typed only into a real text entry (on the web: a text-like
  `input`, a `textarea`, or `contenteditable`, whatever the ARIA role says);
  a field that refuses it is struck for the step with every element of its
  kind, and no `do` move of the step presses one;
- an unnamed control on the web is described by the text inside it, unless
  it is a text entry or holds a value;
- `pick` recovers from a covered click the way a `do` click does, and says
  why when it cannot; the browser clicks through a result card's own content
  when the exact target is in that card and no dialog is involved.

And a gated `stop_before` asks Jev to
*find, without pressing it,* the control, rather than to perform the action.
Measured on a payment page, "perform: paying for the booking" chose the Pay
button at 0.44 (the brief's rule says to stop before paying); the new
wording chose it at 1.0. Its grounding-memory key is unchanged.

## Invariants

- Every call still goes through `FlowRun::ask`: budget, brief, masking,
  fitting, voting, journal.
- Screen text in the digest and the survey is `untrusted_accessibility_data`.
- An irreversible control is never offered to dismiss with, and every click
  still passes `is_destructive`.
- A move or option Jev was not offered fails closed.

## Acceptance

- Simulator: the same outcomes as narrow on the mail flow; one request per
  turn when the pick is confident; an obstacle cleared from the turn's own
  request and remembered; a crowded screen surveyed once and never narrowed
  region by region; memory carrying the previous step's actions.
- Live: the travel fixture passes under both strategies (it does, 1.00
  decisions per turn wide against 1.33 narrow); the Kashmir task is recorded
  in the eval.

## Open questions

- Whether raising `CAP` above 20 options per Choice keeps accuracy.
- A desktop set of page kinds for the survey.
- Flipping the default, once more live scenarios agree.
