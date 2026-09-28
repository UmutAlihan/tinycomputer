# Budgets and switches

Every run is bounded, and almost every loop described in this section can
be turned off individually. This page is the reference for both.

## The two hard budgets

A run is capped on two things, whatever the request asks for:

- **actions**: at most 120 (`MAX_ACTIONS`), whatever `max_actions` in the
  request asks for; the default is 60.
- **Jev calls**: at most 10,000 (`MAX_CALLS`), whatever
  `max_model_calls` asks for; the default is 3,000. Every framing of a
  voted decision counts as one call, so voting with more framings spends
  this budget faster (see [Voting and briefing](voting-and-briefing.md)).

Every action goes through `FlowRun::act`, and every Jev request goes
through `FlowRun::ask`, and both check their budget before doing anything
at all, so no loop, however deep in the ladder, can spend past the
caller's limit. When a budget runs out mid-step, the run stops with
`ActionBudget` or `ModelBudget`, and the step report shows exactly where
it was when that happened.

Deliberation raised these defaults from where they used to sit, because
climbing the escalation ladder on a hard case can cost several extra
requests: the default number of votes went from 5 to 7, the default flow
call budget from 1,500 to 3,000, the task call budget from 3,000 to 6,000,
and the module's own hard cap from 5,000 to 10,000. See
[Deliberation](deliberation.md#what-it-costs).

## Other budgets worth knowing

| What | Limit | Why |
|---|---|---|
| Turns in a `do` step | 8 (`DO_TURNS`) | a step that needs more presses than this should be split into more than one step |
| Options in one Choice | 20 plus `none` (`CAP`) | a bigger pool is narrowed, never silently truncated |
| Obstacles dismissed per step | 2 (`MAX_OBSTACLES`) | |
| Undos from the `do` loop's own regression check | 2 per step (`MAX_UNDOS`) | shared between "progress dropped" and "didn't help" |
| Idle waits in a row | 2 (`MAX_IDLE_WAITS`) | after that, Jev is not let choose `wait` again that step |
| Repairs after a failed reflection | 1 per step, at most 4 turns each | reflection never loops (see [Reflection](reflection.md)) |
| Backtracks per step | 3 deep, 1 standard, 0 off (`MAX_BRANCHES`) | see [Undo and backtracking](undo-and-backtracking.md) |
| Request size | 100,000 bytes (`MAX_REQUEST_BYTES`) | Jev refuses a request past its own token limit outright |
| `so_far` entries in the brief | 12 (`MAX_SO_FAR`) | oldest dropped first |

## `disabled_loops`

Every loop can be switched off for a run, individually, through the
request's `disabled_loops`. This exists mainly so the lab can measure what
each loop is actually worth, by turning it off and comparing outcomes, but
it is also useful for narrowing down where a bug lives: turn off
everything except the loop you suspect, and see if the failure still
happens.

The loops that can be named:

`completion`, `progress`, `moves`, `narrowing`, `corroboration`,
`consistency`, `obstacles`, `undo`, `memory`, `vote`, `page_kind`,
`validation`, and the deliberation-specific ones: `attention`,
`evidence`, `escalation`, `duel`, `tree_grounding`, `denoise`,
`expectation`, `checkpoint`, `backtrack`.

One loop cannot be turned off: `slots`. `enter` needs it to match text to
fields at all, so disabling it would leave `enter` with no way to work.

With every judging loop disabled at once, the `do` loop reduces to its
simplest possible shape: ground something, press it, and look again, with
none of the checks that would normally decide whether that was the right
thing to press.

## The three deliberation levels

`deliberation` (`off`, `standard`, or `deep`, the default) is a single
dial that turns whole groups of the loops above on or off together,
rather than one at a time. See the levels table in
[Deliberation](deliberation.md#levels) for exactly what each level turns
on.

Turning `deliberation` to `off` is a coarser, cheaper version of turning
off the individual `disabled_loops` named `attention`, `evidence`,
`escalation`, `duel`, `tree_grounding`, `denoise`, `expectation`,
`checkpoint`, and `backtrack` all at once, and falling back to the single
fixed thresholds from before deliberation existed. Use `disabled_loops`
when you want to isolate one mechanism; use `deliberation` when you want
the older, simpler behaviour wholesale.

## Where to check the exact numbers

Every constant on this page, and on every other page in this section, has
an exact value, a file it lives in, and a one-line meaning in
[`docs/technical/decision-thresholds.md`](../../../technical/decision-thresholds.md).
That table is the single place to check a number before relying on it;
this page only explains what each budget or switch is for.
