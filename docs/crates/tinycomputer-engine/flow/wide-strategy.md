# The wide strategy

The default way the flow runtime works through a `do` step asks a few
small questions per turn, one after another: judge, then choose an
element, then maybe re-ask because the pick was not confident, then narrow
a crowded screen region by region. This is the **narrow strategy**, and it
is fine for most applications.

Real web pages are often hostile in a specific way: cookie walls, upsells,
pop-ups, a footer full of links, twenty identical "Select" buttons. A
person filters all of that out without thinking about it. The narrow
strategy has to step through it with several small decisions in sequence,
each shown a flat list of up to 120 elements, and a normal `do` turn on a
crowded page can cost anywhere from two to seven round trips.

Set `strategy: "wide"` on a request to change how decisions are asked,
without changing any threshold: the wide strategy asks the judging
question, the obstacle question, and every move's target all in one
request per turn, over a richer view of the screen. It is opt-in; the
narrow strategy stays the default until more live runs confirm the wide
one is at least as good everywhere.

## What Jev is shown, differently

Under `"wide"`, the shared state replaces a flat element list and a short
history with two richer sections, built by `wide.rs`, `survey.rs`, and
`ledger.rs`:

### The digest

A **digest** of the screen (`tinycomputer_core::surface::digest`) instead
of a flat list: what is in front, elements grouped into named regions,
lists shown as one line per card, noise and distraction collapsed to a
single summary line, all kept within a 24,000-byte budget (`DIGEST_BUDGET`)
so a single crowded page cannot eat most of Jev's context window by
itself. An element with no name of its own is shown together with the
nearest named container it sits in, so "combobox in button
'destinationCity picker'" reads clearly even when the combobox itself is
unnamed.

### The ledger: working memory

Instead of a flat scroll of the last several history lines, every wide
question sees a **ledger** (`ledger.rs`): the run's working memory, built
to look like what a person doing the task would actually keep in mind.

- `steps_done`: one line per finished step, so far.
- `recent_actions`: the last 24 history lines, reaching back across step
  boundaries, not just within the current step. This turns out to matter:
  measured live, a payment page judged "seat selection was skipped" at
  0.95 when the previous step's clicks were still in view, and at 0.48
  when they were not, because the click that left a page is often the
  only evidence that the page was actually dealt with.
- `tried_and_failed`: notes for this step alone, so the runtime does not
  try the same dead end twice.
- `next_step`: so "is this step done" is judged against this step, not
  against something the next step is going to do anyway.
- `variables_read`, and `budget_left`.

The ledger never carries the goal-level brief; that stays where briefing
already puts it (see [Voting and briefing](voting-and-briefing.md)),
because judging a step against the whole task, rather than against itself,
is exactly the failure briefing exists to avoid.

## The survey: triaging a crowded screen

When a `do` turn's screen has more than 40 actionable elements
(`CROWDED`) and at least two regions, one request surveys it first, before
anything else is asked. For each region (up to 24), it asks a five-level
relevance score and a yes/no for whether the region is a distraction
(advertising, an upsell, decoration). A region judged a distraction at 0.7
or above (`DISTRACTION`) is collapsed and ranked last for the rest of the
step.

The answers are kept, by region name, for as long as the step runs. The
page is surveyed again only about regions it has not seen before, and only
when those unseen regions make up more than three tenths of the page
(`NEW_TENTHS`); opening a single dropdown is not worth another full
survey. A screen small enough to fit in the turn's own request without a
survey is never surveyed at all: there is nothing left to rank that the
turn cannot already see for itself.

## One request per turn

With the digest and, if needed, the survey in hand, one request asks
everything the turn might need:

- the same judging questions as the narrow strategy (`done`, `not_done`,
  `progress`, `blocked`, `helped`, `move`, `shortcut`);
- `dismiss`, a Choice over the safe controls of whatever region is in
  front, plus Escape, so an obstacle can be cleared without a second round
  trip; `dismiss_known` for a control an earlier run already learned
  closes this kind of obstacle;
- for every move that needs an element (`activate`, `expand`, `scroll`), a
  `target_<move>` Choice over up to 40 candidates ranked by relevance
  (`WIDE_POOL`), a reversed and relabelled `again_<move>` for the same
  consistency check narrowing uses, a `known_<move>` confirmation for a
  remembered element, and, when the pool is bigger than one Choice can
  hold, `group_<move>_<n>` knockout Choices instead.

The runtime then applies exactly the same thresholds the narrow strategy
uses, just to answers it already has in hand. A second request only goes
out when the chosen target is not confident enough on its own (one
`confirm` yes/no), or when a knockout left more than one winner (one final
Choice). A move whose target came back `none` is simply not re-asked that
same turn; the next turn looks at the screen again instead.

## Remembering what was already saved

Alongside the ledger's `variables_read` (just names and a budget), every
state, wide or narrow, also carries `already_collected`: the actual values
a `read`, `extract`, or `pick` step has saved so far, the twelve most
recent (`MAX_COLLECTED`), each clipped to 120 characters (`COLLECTED_CHARS`).
This is what lets a step such as "open the next chat not yet read" be
judged against what the run remembers, not just against a list of names.
It also carries across a task's separate runs, and reaches the rescuer
too. See [`../output.md`](../output.md) for the whole picture.

## Remembering obstacles

The control that dismissed an overlay is learned as a grounding hint,
keyed `obstacle <region>`, the same way any other click is remembered. A
later run of the same scenario confirms it with `dismiss_known` instead of
choosing among the region's controls again.

## Why bother

Jev calls are cheap and its context window is large; round trips and
missing context are what actually cost time and accuracy. The wide
strategy trades "many tiny questions, each seeing very little" for "one
big question, seeing a lot," on the theory that a Jev call has more than
enough room to take in a whole crowded page at once, if the page is
presented well. Measured on the lab's travel fixture, the wide strategy
took 1.00 decisions per turn against the narrow strategy's 1.33, for the
same outcome.

See [`docs/technical/specs/jev-wide-turns.md`](../../../technical/specs/jev-wide-turns.md)
for the full specification, and
[`docs/technical/decision-thresholds.md`](../../../technical/decision-thresholds.md)
for `CROWDED`, `DISTRACTION`, `DIGEST_BUDGET`, `WIDE_POOL`, `NEW_TENTHS`,
`MAX_FINISHED`, `MAX_RECENT`, `MAX_TRIED`, and `MAX_LINE`.
