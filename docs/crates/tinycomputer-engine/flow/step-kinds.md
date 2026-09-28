# Step kinds

A flow's `steps` list mixes plain strings (each one a `do` step) with a
handful of typed steps for the things a plain string cannot say precisely:
typing exact text, picking one item out of a list, or checking a condition.
This page covers what each kind does and roughly what it costs in Jev calls.
The authoring guide (`crates/tinycomputer-bus/src/flow/guide.md`) covers the
JSON shape of each one; this page covers how the runtime carries it out.

## The cost table

Each decision below is one Jev request, asked in as many framings as the
run votes with (seven by default, running at once, so voting does not add
wall time by itself). See [Voting and briefing](voting-and-briefing.md).

| Step | Decisions when things go well | Where |
|---|---|---|
| `open` | 0 | launch, then up to ten looks for a readable window |
| `browse` | 0 | navigate |
| `do` (a plain string) | 2 per turn: judge and ground; usually 2 to 3 turns | [The do loop](the-do-loop.md) |
| `do`, taken by a shortcut | 1 per turn: judge only | the `shortcut` answer |
| `enter` | 1 for all slots at once, plus 1 to check for field errors | [Filling in forms](filling-forms.md) |
| `choose` | 1 to 2 to ground the option; more if it has to be revealed first | [Grounding](grounding.md) |
| `read` | 1 per 60 pieces of readable text | below |
| `extract` | 0, or 1 to choose which list, when more than one shows | below |
| `pick` | 0 when the criterion parses (price, time, duration, stops); 1 to judge the item otherwise, plus 1 more to choose which list, when several show | below |
| `verify`, `if` | 1 | below |
| `wait_for` | 1 per check, up to 10 | below |
| `repeat_until` | 1 per round, plus the body's own steps | below |
| `stop_before` | 1 to 2 to ground the control | [Grounding](grounding.md) |

Grounding memory turns most `do` and `enter` groundings, on a second run of
the same scenario, into a single confirmation question. That is why running
a flow twice is usually cheaper than running it once.

## `open` and `browse`

`open` launches the named application, or brings it to the front if it is
already running, then checks up to ten times for a readable window, pausing
between checks. `browse` switches the run to the browser, opens a session
if it needs one, and navigates to the given URL. Neither makes a Jev call:
there is nothing to decide.

## `do`

A plain string, or `{"do": "..."}`, runs the loop most people mean when they
say "the loop": look, judge, decide whether to clear something out of the
way or undo a mistake, then make one move, for up to eight turns. It is
covered on its own page: [The do loop](the-do-loop.md).

## `enter`

Takes a map of slot names to text, such as
`{"recipient": "sam@example.com", "subject": "Friday"}`, matches each slot
to a field on screen, and types the text in with a read-back check that it
actually landed. Covered on its own page:
[Filling in forms](filling-forms.md).

## `choose`

Grounds the named option among the clickable, non-destructive elements on
screen (favouring ones inside whatever region the step names, when the page
carries one) and clicks it. When the option is not there yet, `choose` runs
a short three-turn `do` loop to reveal it first — opening a dropdown, say.
Once revealed, two further tricks apply, depending on what kind of control
it is:

- a date picker's calendar is paged forward to the requested day;
- an autocomplete field has the option typed into it (the field that just
  gained focus), and the runtime picks the suggestion that then appears,
  retrying up to four times.

A private option is one whose value `enter` could not safely type, such as
a password confirmation choice built from a secret. It is picked the same
way, but Jev is never asked about it: the runtime matches it locally so the
secret's shape never reaches a question.

Once a `choose` step has pressed something, the runtime looks again to
check the screen actually shows the choice it asked for, and repairs once
if it does not. See [Reflection](reflection.md).

## `read`

Offers every readable piece of text and context line on screen as options
in a Choice, sixty at a time, and stores whichever one Jev is at least 0.5
sure is the right answer into the named variable. That variable's value is
page text, and page text is data: it is never treated as an instruction,
even in a later step that uses it.

## `extract`

Finds the repeated list items on screen ("cards": search results, list
rows, anything that repeats the same shape) using `result_families`, and
stores them as JSON rows of their visible text. Usually no Jev call: the
shape of the list is enough. If more than one list shows at once, one
Choice asks which list `extract`'s own `what` names first (see "Choosing
the list a step means" below).

## `pick`

Also finds the repeated cards, then either ranks them exactly or asks Jev,
depending on whether its `by` text parses into something exact:

- lowest or highest price,
- earliest or latest time,
- fewest stops,
- shortest duration.

When it parses ("lowest price"), the ranking costs nothing: the parsers in
`tinycomputer-core/src/records/` read prices with currency symbols, clock
times, durations like `2h 35m`, and stop counts like `non-stop` or `1
stop`, and the winner is picked exactly from whichever list on screen
actually has that measure; no separate question about which list is meant
is needed. When it does not parse ("a morning flight with at most one
stop"), and more than one list shows, one Choice first asks which list
`from` names, then Jev gets one Choice over that list's cards. Either way,
the winner's text goes into the named variable, capped at 400 characters,
and its primary control (whatever looks most like "open this": select,
book, choose, view, details, continue, reserve, deal, see) is clicked,
after the same destructive check any other click gets.

## Choosing the list a step means

A results page usually repeats its cards under a container the page itself
labels by ordinal (`listitem #3`), and `result_families` picks the list
with the most cards. A native desktop tree labels nothing that way, so
there `result_families` instead reads a run of three or more same-role
leaf siblings (`MIN_FLAT_ITEMS`) as a list, one record per element; this is
what lets `extract` and `pick` work on a desktop application's flat
accessibility tree, not only on a web page's own list markup.

When more than one list shows on screen at once, such as a chat list
beside the open chat's own messages, `extract` and an unranked `pick` ask
Jev which one is meant: each of the first six lists (`MAX_LISTS`) is shown
by its first three items (`LIST_PREVIEW`), and a clear winner at 0.5
confidence or above is used; anything less clear falls back to the longest
list, the one that would have been used before the question was asked at
all. See [`../output.md`](../output.md) for the full detail, including how
this interacts with the run's own memory of what it has already saved.

## `verify`, `wait_for`, `if`, `repeat_until`

All four go through the same condition check, `holds`, which asks three
questions in one request: the condition as a yes/no, its negation as a
yes/no, and a five-level coverage score from "none of the condition holds"
to "all of the condition holds." The calibrated yes/no and the top coverage
level are averaged. The coverage question exists because a condition that
lists several things at once ("the draft shows the recipient, the subject,
and the body") tends to get a hedged yes/no but a crisp coverage answer.

- `verify` checks the condition once and fails the step if it is under
  0.75.
- `wait_for` checks up to ten times, waiting between checks, which is how a
  flow waits for a page to finish loading without guessing how long that
  takes.
- `if` checks the condition and runs `then` at 0.75 or above, `else`
  otherwise. Its children show up in the step report as `3.1`, `3.2`, and
  so on, nested under the parent.
- `repeat_until` checks the condition, runs its body, and checks again, up
  to a caller-set maximum number of rounds.

## `stop_before`

The only step allowed to reach an irreversible control, and only when the
request explicitly sets `allow_destructive`. It grounds the control (it
must be at least 0.5 sure which one it is), and:

- without `allow_destructive`, records it as the run's pending target and
  stops the run with `StoppedBeforeDestructive`;
- with `allow_destructive`, clicks it, then asks whether the action has
  actually happened, and fails if that belief is under 0.75.

The task controller turns a gated `stop_before` into a `needs_approval`
pause, or into a final checkpoint when the control is a payment. See
[Rescue](../../../rescue.md) and [Giving it a task](../../../giving-it-a-task.md).

## Facts, variables, and what Jev never sees

A flow's own `vars` and the caller's `vars` merge when a run starts.
`${name}` gets expanded in step text, with one exception: names the request
lists as `facts` are the caller's private values, and they are only ever
expanded into an `enter` step's typed text. Everywhere else Jev might see
(step text, `open` and `browse` targets, conditions, slot names, the
history a later step's questions carry) the runtime uses a substitution
that leaves a fact reference unexpanded, so `${email}` stays literally
`${email}` in anything Jev reads. Validation rejects a flow that tries to
use a fact anywhere else; the runtime's own substitution rule is a backstop
behind that, not the main defence.

`read` and `pick` write page text into variables, so a later `enter` step
can type a price or a booking reference read earlier in the run. That text
came from the page, so it is treated as data there too.
