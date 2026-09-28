# Levanto Sage: an alternative decision model

Every loop in this crate, narrow or wide, `RunGoal` or the flow runtime,
asks its questions through one shape: a state and a set of yes/no, choice,
and score questions, answered with a probability. Jev is one model that
answers that shape. `crates/tinycomputer-engine/src/agentic/sage/` adapts a
second one, Levanto Sage, to answer exactly the same shape, so it can sit
behind the same loops without any of them knowing the difference.

This exists to measure whether a different decision model changes a run's
accuracy, cost, or speed, not to replace Jev: the harness treats Sage as
an interchangeable evaluator, nothing more.

## How a request becomes a Sage call

`SageEvaluator` turns one `EvaluationRequest` into one Sage batch group:

- the request's `state` (compact JSON) becomes the group's content;
- a yes/no question (`Question::Noul`) becomes a Sage `yesno`; Sage's
  calibrated probability of yes is used directly as the `Noul` answer;
- a choice (`Question::Choice`) becomes a Sage `choice` over the same
  option names. Sage scores each option independently, so the adapter
  normalizes the per-option probabilities to sum to one before handing
  them back as a `ChoiceAnswer`;
- a score (`Question::Score`) becomes a Sage five-level `scale`
  (`SCALE_LEVELS`), the request's own levels sampled onto those five
  (`sampled`), and Sage's expectation is spread back over the two nearest
  of the request's own levels to build a `ScoreAnswer`.

A verdict Sage itself calls unsure still comes back as a probability: the
loops decide what to do with it using their own thresholds and
deliberation, exactly as they would a hesitant Jev answer. Nothing about
gating or voting changes because Sage answered instead of Jev.

## Still one door

`SageEvaluator` implements the same private `Evaluator` trait Jev's own
client does, and `JevRuntime::sage` builds a `JevRuntime` around it:

```rust
pub fn sage(api_key: &str, fast: bool) -> Result<Self, Box<DesktopError>>
```

Every call still goes through `JevRuntime::evaluate`, the crate's one door
to a decision model (see [jev-runtime.md](jev-runtime.md)): the budget is
still charged, secrets are still masked, and the journal still sees every
exchange, whichever model answered it. `JevConfiguration` names a
Sage-backed runtime by its model, `"levanto-sage"`, so a journal or a trace
reading `model` can tell which one ran.

`JevRuntime::sage` is not reachable over the bus. It exists for measuring
Sage from the examples crate, not as a caller-selectable option on
`StartTask` or `RunFlow`.

## Latency modes

`fast` picks between Sage's own `LatencyMode`:

- `Quality` (the default, `fast: false`): a choice is scored one option at
  a time.
- `Fast` (`fast: true`, wired to `SAGE_FAST=1` in the examples): a choice
  is scored in one pass over every option instead.

Quality mode is what the recorded eval below measured; fast mode is a
lever the eval names but has not yet measured.

## Switching an example to Sage

No example does any more. The recorded eval ran `task_live` with a
`TINYCOMPUTER_DECISIONS=sage` switch that built a `JevRuntime::sage` in
process; `task_live` now drives the loaded module over the bus, and the
module's configuration has no Sage setting, so Sage is reachable only from
code that builds a `JevRuntime` itself.

## What the live eval found

[`docs/technical/evals/2026-09-29-sage.md`](../../technical/evals/2026-09-29-sage.md)
ran the same two live bookings, replayed from the same committed plans,
once with Jev and once with Sage in quality mode, seven votes, deep
deliberation. A few numbers from that comparison:

| | Emirates · Jev | Emirates · Sage | Kashmir · Jev | Kashmir · Sage |
|---|---|---|---|---|
| Wall time | 4.4 min | 11.0 min | 2.5 min | 24.1 min |
| Decision calls | 679 | 780 | 972 | 1,419 (11 failed) |
| Cost | $0.14 | $1.85 | $0.30 | $3.95 |

Both runs reached the same payment checkpoint on Emirates; on Kashmir, the
Sage run ran out the site's own session before reaching payment, after 24
minutes on a booking Jev finished in 2.5. The eval's findings, in short:
Sage works as a drop-in behind the same loops, including the
irreversible-press gate holding a rescue back until it put a `stop_before`
back in place, but each call ran 10 to 15 times slower and the run cost
roughly 13 times more, with no accuracy gain in these two runs; Sage
needed twice the rescues on Kashmir. The eval also notes that a 7-vote
framing is largely redundant for a model that already calibrates its own
probabilities, so asking each decision once, rather than seven ways, is
the fairer comparison still to run.

## Source

- `crates/tinycomputer-engine/src/agentic/sage/mod.rs`, `SageEvaluator`,
  `batch`, `sage_question`, `answer_for`.
- `crates/tinycomputer-engine/src/agentic/runtime.rs`, `JevRuntime::sage`.
- [jev-runtime.md](jev-runtime.md), the one door every decision, Jev's or
  Sage's, goes through.
- [`docs/technical/evals/2026-09-29-sage.md`](../../technical/evals/2026-09-29-sage.md),
  the live comparison this page summarizes.
