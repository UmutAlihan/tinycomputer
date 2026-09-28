# RunGoal and ResolveIntent

Before the flow runtime existed, these two functions were the whole agentic
surface. They still exist, still work, and are still the right tool when a
caller wants direct, low-level control over one screen rather than a
multi-step plan. Both live in `crates/tinycomputer-engine/src/agentic/`
(`resolve.rs` and `goal.rs`), beside `JevRuntime` itself (`runtime.rs`), and
both share its policy module (`agentic/policy/`) with the flow runtime.

The difference between them is scope: `ResolveIntent` finds and optionally
acts on **one** element; `RunGoal` keeps going, observing and deciding again
after each action, until a visible goal is reached or something stops it.

## ResolveIntent: point at one thing

```rust
pub async fn resolve_intent(
    desktop: Desktop,
    runtime: JevRuntime,
    request: ResolveIntentRequest,
) -> DesktopResponse
```

Given an application name and a short intent ("the submit button", "the
search field"), `resolve_intent`:

1. observes the application's current screen;
2. asks Jev to choose an operation (click, type text, check, and so on) and a
   target element for it, from the elements actually on screen;
3. if `request.execute` is set and Jev's answer clears the safe-action
   threshold, performs it.

`ResolveIntentRequest` also takes `text` (what to type, if the operation
needs it), `root` (a container ref to narrow observation to, rather than the
whole window), and `include_values` (whether field values may be shown to
Jev at all, off by default, since a field's contents can be personal).

One shortcut lives ahead of the Jev call: `visible_completion` checks a
narrow class of already-satisfied intents directly from the screen,
currently "is something playing", without spending a Jev call to confirm
what is already visible. This is a small, deliberately limited optimization,
not a general pattern; everything else always asks.

## RunGoal: keep going until a goal is visibly true

```rust
pub async fn run_goal(
    desktop: Desktop,
    runtime: JevRuntime,
    request: RunGoalRequest,
) -> DesktopResponse
```

`RunGoalRequest` describes a bounded loop: an application, a goal in plain
words ("the playlist is playing"), a budget (`max_steps`, `max_elapsed_ms`,
`max_model_calls`), and optional scoping: `window` / `window_id` to stay on
one window, `allowed_operations` and `allowed_targets` to restrict what the
loop may do, and `text` values consumed in order by text-entering steps.

Each turn of the loop:

1. observes the current screen (scoped to the request's window, if one was
   named);
2. asks Jev the same operation-and-target question `ResolveIntent` asks,
   with the goal as the framing question and `DONE` / `BLOCKED` / `WAIT`
   added as possible answers;
3. gates the answer (see [how-it-decides.md](../../how-it-decides.md) for the
   plain-language version, and the gate below for the exact rule);
4. if the gate says `Act`, performs the operation, re-observes, and checks
   whether anything actually changed;
5. stops, or loops back to step 1.

### Why a loop can stop

`RunGoalRequest`'s reply carries a `JevStopReason`:

| Reason | Meaning |
|---|---|
| `Done` | Jev reported the goal is visibly satisfied. |
| `Blocked` | No offered operation can advance the goal. |
| `ConfirmationRequired` | The next step is destructive; see below. |
| `Cancelled` | The caller declined a pending confirmation. |
| `StaleTarget` | The screen changed since a confirmation was requested, and the approved target no longer matches. |
| `LowConfidence` | Jev's confidence did not clear the threshold. |
| `NeedsText` | The chosen operation types text, and no caller-supplied value remains. |
| `ActionBudget` / `ModelBudget` / `TimeBudget` | A budget ran out. |
| `ActionUncertain` / `ActionFailed` | The action itself did not clearly succeed. |
| `VerificationFailed` | The loop ended before the goal's visible conditions were actually satisfied. |
| `ScopeChanged` | The screen moved outside the request's declared window while a confirmed action was in flight. |
| `Stalled` | Three actions in a row changed nothing. |

### The gate

The gate (`agentic::policy::gate_with_evidence`) is the same threshold logic
`ResolveIntent` uses, described here once because both loops share it:

- `DONE` or `BLOCKED` need confidence at or above 0.70 (`ACT`) to be trusted;
  below that, the loop abstains rather than declaring victory or giving up on
  a guess.
- Below 0.55 (`FLOOR`) confidence, the loop abstains outright, unless the
  target's own name is an exact match for a phrase in the goal (an "exact
  named match"), which is trusted down to 0.45.
- At or above the destructive threshold (0.50), the decision becomes
  `ConfirmationRequired` regardless of how confident the target choice was:
  destructiveness overrides confidence, not the other way round.
- Otherwise, confidence below 0.70 without an exact named match still
  abstains; confidence at or above 0.70, or any confidence with an exact
  named match, is `Act`.

Destructiveness itself comes from two sources, taken as the higher of the
two: Jev's own answer to a yes/no "would this be hard to undo" question, and
a small deterministic word list (`deterministic_destructive` in
`policy/gate.rs`): "delete", "send", "purchase", "buy", "pay", "submit",
"confirm", "overwrite", "quit without saving", "empty trash", "sign out",
checked against the goal text and the target's own label. The word list
exists so that an action that is obviously irreversible is never left to a
model's judgment call alone.

### Confirmation handles

When the gate returns `ConfirmationRequired`, `run_goal` does not act. It
stores the pending decision (the screen it was taken on, the exact target,
how far the loop had already run) in the runtime's confirmation table (see
[jev-runtime.md](jev-runtime.md)) and returns a `confirmation_id` instead of
a result. The caller decides, then calls `run_goal` again with
`RunGoalRequest.continuation` set to a `GoalContinuation { id, approve }`:

```rust
RunGoalRequest {
    continuation: Some(GoalContinuation { id: "3f9a…".to_owned(), approve: true }),
    ..RunGoalRequest::default()
}
```

Approving re-observes the screen first and checks the exact target is still
there, unchanged, with the same available actions, before acting: a page
that changed underneath the confirmation fails closed with `StaleTarget`
rather than clicking whatever is now in that position. Declining returns
`Cancelled` without touching the desktop. Either way the handle is consumed:
it cannot be reused, and it expires after ten minutes even if never
answered.

A continuation resumes the loop's own history and budget rather than
starting over: turns, unchanged-count, and remaining time and step budget
all carry forward, so approving a confirmation costs one step of the
original budget, not a fresh one.

## When to use these instead of a flow

`RunGoal` and `ResolveIntent` predate flows and the task controller, and they
still suit a caller that:

- wants one action or one bounded loop, not a multi-step plan;
- is willing to hold the confirmation-id round trip itself;
- does not need pausing for missing values, payment checkpoints, or a
  rescuer: those only exist in the task controller (see
  [tasks.md](tasks.md)).

Most new work should reach for `StartTask` and a flow instead: it gets
budgets across multiple runs, human-in-the-loop pauses, and rescue for free.
`RunGoal` remains the right layer when a caller is already tracking its own
state machine and just needs one desktop loop inside it.

## Source

- `crates/tinycomputer-engine/src/agentic/resolve.rs`, `resolve_intent`;
  `goal.rs` and `task/`, `run_goal` and its loop; `continuation.rs`,
  `continue_goal`; `pending.rs`, the confirmation table.
- `crates/tinycomputer-engine/src/agentic/policy/`, the gate (`gate.rs`), the
  action space (`request.rs`), and the destructive word list (`gate.rs`).
- `crates/tinycomputer-bus/src/agentic/`, `ResolveIntentRequest`,
  `RunGoalRequest`, `GoalContinuation`, `JevRunResult`, `JevStopReason`.
