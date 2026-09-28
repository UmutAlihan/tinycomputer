# Grounding: picking one element

A flow step never names a button. It says "click to accomplish: start a
new email message," and something in `ground/` has to turn that into one
specific element on the current screen. This is grounding, and it runs
whenever `activate`, `choose`, or `stop_before` needs one element for a
purpose.

Grounding tries to find the answer with as few and as small questions as
the screen allows. Every Choice Jev is offered has at most 20 options plus
`none` (`CAP`), and a bigger pool is narrowed first rather than silently
cut down.

## The four steps

### 1. Memory

If an earlier run grounded the same step in the same application, the
remembered element is looked up by its role, name, and the last two
ancestor labels in its path, never by its ref (refs are minted fresh on
every snapshot, so they cannot be remembered across one). The remembered
element is confirmed with one yes/no question and used if the answer is at
least 0.5. This is why a second run of the same scenario is usually much
cheaper: most groundings on the second run cost one confirmation instead of
a full search.

### 2. Narrowing

A pool bigger than 20 elements is grouped by ancestor, and the runtime asks
two questions in one round trip:

- which **region** ("toolbar, 12 elements, e.g. New Message, Reply,
  Delete") holds the element, and
- a **knockout** Choice per group of 20, the groups cut along the same
  regions.

The chosen region's winners go on to the final Choice. If the region
question comes back with no clear winner, every group's winners go
forward instead, so a wrong region guess cannot lose the actual target.

### 3. Choice

One Choice over whatever survived narrowing.

### 4. Consistency and corroboration

A pick at 0.70 or above (`ACT`) is used right away, and so is one at 0.45
or above (`NAMED_FLOOR`) whose name appears word for word in the step's own
purpose text. Anything less confident than that gets a second request with
two more questions:

- the same Choice again, but with the options reversed and relabelled `A`,
  `B`, `C`, and so on (this is the **consistency** check), and
- a plain yes/no, "is this element the right one?" (this is
  **corroboration**).

The element is used only if both Choices agree and the corroboration
answer is at least 0.5, or if the corroboration answer alone reaches 0.8.
Otherwise the runtime reports that nothing clearly fits, and lets the next
turn of the `do` loop try a different move instead of guessing.

Reversing and relabelling the options is there to catch position bias. A
model that likes "the first option on the list" picks a different element
depending on which order it saw the list in, and asking both orders makes
that disagreement visible instead of hiding it inside one confident-looking
number.

## Why this exists

Jev's probability for a Choice measures how concentrated its answer is,
not how likely that answer is to be correct. A single confident-looking
number can come from a genuinely clear pick, or from a coin toss between
two nearly identical "Select" buttons. Grounding's extra rounds exist
specifically to tell those two situations apart before a click happens. The
[deliberation](deliberation.md) system takes this further: instead of one
threshold on one number, it reads the whole spread of answers from several
framings and only escalates when the evidence is genuinely thin.

## The tree beam

Under deliberation, narrowing becomes a small tree search rather than a
single fork. Whenever the region answer's lead over the runner-up region is
under 0.30 (`BRANCH_MARGIN`), the runner-up region is kept too, so a
close-run region guess does not silently drop the actual target the way a
single fork would. At the deep deliberation level, when the region cut
dropped some group winners on the floor, the final Choice is also asked
over every winner in the same round trip, and a disagreement between the
region-first pick and the all-winners pick sends both candidates to a duel
(see [Deliberation](deliberation.md)). No Choice ever grows past `CAP`
options regardless.

The runners-up of a step's grounding are kept as its **frontier**: the
ranked list of alternatives a backtrack tries next if the element that was
actually grounded turns out to be a mistake. See
[Undo and backtracking](undo-and-backtracking.md).

## Grounding memory across runs

Every successful click, choice, and fill is remembered as a grounding hint:
the application, a normalised key for the step, and the element's role,
name, and ancestor path. The runtime keeps no files of its own. It hands
back what it learned in the run's result, and the caller is expected to
pass those hints into the next run's `memory`. The lab keeps them in
`target/lab-runs/memory.json`, which is why running the same scenario twice
through the lab tends to cost noticeably fewer Jev calls the second time.

See [`docs/technical/decision-thresholds.md`](../../../technical/decision-thresholds.md)
for the exact values of `ACT`, `NAMED_FLOOR`, `CORROBORATED`, `AGREED`,
`CAP`, and `BRANCH_MARGIN`.
