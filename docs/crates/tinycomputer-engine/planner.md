# The planner

The planner turns a plain-language task ("book the cheapest flight from
Delhi to Srinagar next Sunday") into a flow: the step-by-step plan the
task controller and flow runtime actually run. It lives in
`crates/tinycomputer-engine/src/planner/`, and it is optional: a task
started with a `flow` already written never touches it, and a module built
without one simply cannot run a plain-language `task`; it pauses with
`NeedsPlan` instead (see [tasks.md](tasks.md)).

The one thing worth remembering about the planner: **it never acts, and it
never sees the screen.** It only ever sees the task's text, the flow
authoring guide, the *names* of facts the caller can supply, and which
surfaces (desktop, browser, or both) are available. It writes a flow; Jev
and the flow runtime are what actually look at anything on screen.

## How planning works

`Planner::plan` is a short conversation with a `LanguageModel`:

1. A system turn carries the planner's protocol (what it may and may not do)
   followed by the flow authoring guide (`FLOW_GUIDE`, shared with
   `Describe` and with flow authors generally).
2. A user turn states the task, which surfaces are available, and which
   facts the caller can supply, by name only: split into ordinary facts
   (`${first name}`, `${email}`) and secret ones (`${card number}`), never by
   value.
3. The model replies with what should be exactly one JSON object: a flow.
4. That flow is validated with the very same checker (`agentic::check_flow`)
   a caller's own hand-written flow would be checked with. If it is invalid,
   the specific errors go back to the model as a fourth turn, asking for a
   corrected flow, up to `REPAIRS` (2) times.
5. If a valid flow never arrives, `plan` fails with a plain-text reason; the
   caller's `PlanTask` (or the task's `NeedsPlan` retry, if planning was
   attempted inside `StartTask`) then surfaces `PLAN_FAILED`.

A successful plan comes back as a `TaskPlan`: the `flow` itself, `questions`
(an `InputField` per `${name}` the flow uses that the caller did not
already name as a fact, so the caller can collect those before starting),
and `notes`, currently just one: whether the plan stops before something
irreversible or paid (a `stop_before` step is present).

## What the planner is told, and what it is not

The planner's protocol is explicit about the boundary. It is told:

- to use `browse` for anything on the web and `open` for a desktop
  application;
- to refer to the person's details only as `${name}` variables, using the
  fact names it was given, and inventing a clear name for anything else the
  task will need, so the caller can be asked for it;
- never to invent a personal detail itself;
- that a shared fact may appear in any step's text, written the way a
  person would ("choose ${title} in the title field");
- that a secret fact may appear **only** as an `enter` step's value, never
  in an `open` application name, a `browse` address, a `do`, `verify`,
  `wait_for`, `stop_before`, `choose`, `read`, `extract`, `pick`,
  `repeat_until`, or `if` text, and never as an `enter` slot's own name
  either;
- to enter payment details only from secret facts it was actually given,
  and to end any purchase or booking with a `stop_before` for paying;
- that a `choose` option must be the label the page shows ("Saver"), never a
  description ("the cheapest fare"), picking by a criterion is what `pick`
  is for;
- to guard sending, deleting, publishing, or submitting with a `stop_before`.

It is never told a fact's *value*, never shown a screenshot or an
accessibility snapshot, and never asked to decide anything at run time. All
of that is Jev's job, later, when the flow it wrote is actually run.

## `LanguageModel`

```rust
pub trait LanguageModel: Send + Sync + 'static {
    fn complete(&self, turns: &[Turn]) -> Completion;
}
```

`Turn` is one message (`Role::System`, `Role::User`, or `Role::Assistant`)
and its text; `Completion` is a boxed future resolving to the model's reply
text or a failure string. Keeping the model behind this trait is what lets
`planner/test.rs` script a fake model's answers deterministically, rather
than every planner test needing a live API call.

## The OpenRouter adapter

The `planner` Cargo feature adds one concrete `LanguageModel`:
`crates/tinycomputer-engine/src/planner/openrouter.rs`, built on
`tinyinference_llm`'s OpenAI-compatible client pointed at OpenRouter. This is
the only file in the crate that links a text-generating model at all, the
key it is given in the module's private configuration never leaves this one
adapter.

```rust
pub fn open_router(config: &PlannerConfig) -> Result<Planner, String>
```

`PlannerConfig` holds the OpenRouter `api_key`, an optional `model`
(`PLANNER_MODEL`, `anthropic/claude-sonnet-5`, when unset), and an optional
`rescue_model` (see [rescue.md](rescue.md), the same file also builds the
rescuer's model, since both are just OpenRouter chat completions with
different settings). The planner's own model is asked for a JSON object
response format, at a low sampling temperature (0.2), because a plan should
be reproducible rather than creative.

## Errors a caller sees

| Code | Cause |
|---|---|
| `PLANNER_NOT_CONFIGURED` | `PlanTask` was called on a module with no planner. |
| `PLAN_FAILED` | The model failed outright, or its answer stayed invalid after every repair. |

Both hints point the same direction: reword the task, or write the flow by
hand from `Describe`'s guide and pass it as `flow` instead of `task`.

## Source

- `crates/tinycomputer-engine/src/planner/mod.rs`, `Planner`, the protocol
  text, `plan_for` (turning a flow into a `TaskPlan`).
- `crates/tinycomputer-engine/src/planner/openrouter.rs`, the OpenRouter
  `LanguageModel`, `PlannerConfig`.
- `crates/tinycomputer-engine/src/agentic/flow/`, `check_flow` and
  `missing_inputs`, which the planner reuses unchanged.
- [writing-flows.md](../../writing-flows.md), the flow language the
  planner (and every human flow author) writes in.
