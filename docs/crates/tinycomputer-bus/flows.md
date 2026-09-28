# Writing flows, the wire format

Source: [`crates/tinycomputer-bus/src/flow/`](../../../crates/tinycomputer-bus/src/flow/)
(`types.rs` for the Rust types, [`guide.md`](../../../crates/tinycomputer-bus/src/flow/guide.md)
for the authoring guide shipped as `FLOW_GUIDE`).

This page documents the *types*: what a `Flow` looks like as JSON, what
`RunFlowRequest` carries, and what a run reports back. For the ideas behind
flows (why they never name a button, how grounding works, worked examples),
see [Writing flows](../../writing-flows.md) and
[docs/flow-examples.md](../../technical/flow-examples.md). This page and that
guide overlap on purpose: this one is the reference, that one is the
narrative.

## What a `Flow` is

A `Flow` is a short, app-agnostic script: what to accomplish in one
application, never how. It has three fields:

```rust,ignore
pub struct Flow {
    pub app: String,                    // the application, or "browser"
    pub vars: BTreeMap<String, String>, // named values, referenced as ${name}
    pub steps: Vec<FlowStep>,
}
```

A step never names a button, a menu path, a keyboard shortcut, or a
coordinate. It says what state to reach ("start a new blank document") or
what to do ("enter the recipient"), and the module works out how, on the
live screen, one small decision at a time. That is the whole point of a
flow: it can be written by something that has never seen the application.

## Steps: a string, or one key naming a kind

On the wire, a step is either a bare JSON string (shorthand for `{"do":
"..."}`) or an object with exactly one key naming its kind:

```json
{
  "app": "TextEdit",
  "vars": { "greeting": "Hello from a flow." },
  "steps": [
    { "open": "TextEdit" },
    "start a new blank document",
    { "enter": { "document text": "${greeting}" } },
    { "verify": "the document shows the greeting text" }
  ]
}
```

This example is straight from the guide, and it is a real, valid `Flow`: the
whole guide is written so that every JSON block in it parses.

| Kind | Shape | Meaning |
|---|---|---|
| (bare string) | `"start a new note"` | same as `{"do": "..."}` |
| `open` | `{"open": "Mail"}` | launch the app, or bring it forward |
| `browse` | `{"browse": "https://…"}` | switch to the browser and load a URL; later steps act on the page until an `open` switches back |
| `do` | `{"do": "start a new note"}` | reach the described state |
| `enter` | `{"enter": {"subject": "Hi"}}` | put each text into the field its key describes |
| `choose` | `{"choose": {"what": "the font list", "option": "Helvetica"}}` | pick an option in a list, menu, or popup |
| `read` | `{"read": {"what": "...", "into": "subject"}}` | store visible text in a variable |
| `extract` | `{"extract": {"what": "...", "into": "flights"}}` | store every item of a list, as JSON rows, in a variable |
| `pick` | `{"pick": {"from": "...", "by": "lowest price", "into": "flight"}}` | choose the best of a list by a criterion, and open it |
| `verify` | `{"verify": "the draft shows a recipient"}` | fail the flow unless this holds |
| `wait_for` | `{"wait_for": "search results are showing"}` | wait until this holds |
| `stop_before` | `{"stop_before": "sending the email"}` | find an irreversible action and stop in front of it |
| `repeat_until` | `{"repeat_until": {"condition": "...", "steps": [...], "max": 5}}` | repeat nested steps until a condition holds |
| `if` | `{"if": {"condition": "...", "then": [...], "else": [...]}}` | branch on a condition |

`STEP_KINDS` in the crate is this same list as a `&[&str]`, used to produce a
readable "unknown step kind, expected one of…" error rather than a bare
decode failure, flows are written by people and by language models, and
both need an error that tells them what to try instead.

### `Slots`: why key order does not survive the wire

`enter`'s value is a map from a plain-language slot description to the text
that goes in it (`{"recipient": "sam@example.com", "subject": "Hi"}`). You
would expect the module to fill fields in the order you wrote them, and while
parsing text it does keep that order. But arguments travel over the bus as a
`serde_json::Value`, and a `Value`'s object does not promise to keep key
order through that round trip. So the crate never relies on it: it fills
matched fields top to bottom as they appear on screen, which happens to match
a form's own tab order and autocomplete expectations anyway.

### Secrets: named everywhere except where it counts

`RunFlowRequest.facts` names which of its `vars` are secret, a card number,
a password, a one-time code. A secret's *value* may only ever appear as an
`enter` step's text. Its name, `${name}`, may appear anywhere else in a flow
(an `open` app name, a `do`/`verify`/`wait_for`/`stop_before` text, a
`choose`'s `option`, a `pick`'s `by`, a condition) without the module ever
learning what it stands for.

`FlowValidation` rejects a flow that puts a secret's `${name}` anywhere but
an `enter` value, the runtime never expands one there even if validation
were somehow bypassed, and every question sent to the decision model has
every secret's value masked back to `${name}`, including anywhere the
screen itself happens to display it. This is the same rule described more
broadly in [Safety and privacy](../../safety-and-privacy.md); this crate is
where it is enforced at the type level.

## `RunFlowRequest`

```rust,ignore
pub struct RunFlowRequest {
    pub flow: Flow,
    pub vars: BTreeMap<String, String>,     // overrides flow.vars
    pub facts: BTreeSet<String>,            // which vars are secret
    pub allow_destructive: bool,            // may stop_before actually run?
    pub include_values: bool,               // may ordinary field values reach the decision model?
    pub max_actions: u32,                   // default 60, capped at 120
    pub max_model_calls: u32,               // default 3000, capped at 10000
    pub votes: u32,                         // default 7, capped at 9
    pub brief: FlowBrief,
    pub disabled_loops: Vec<FlowLoop>,      // empty in production; for measurement
    pub memory: Vec<GroundingHint>,
    pub trace: bool,
    pub strategy: FlowStrategy,             // Narrow by default
    pub deliberation: Deliberation,         // Deep by default
}
```

Like `RunGoal` (see [The goal loop](goal-loop.md)), this requires
confidential bus delivery, because the texts a flow enters travel with it.

`votes` is worth calling out: it is how many independently-framed ways one
decision is asked before the answers are averaged. Asking the decision model
more than once is cheap, so accuracy here is bought with more calls rather
than with a cleverer single question. `strategy` and `deliberation` tune the
same trade-off at a coarser grain, see
[docs/technical/decision-loops.md](../../technical/decision-loops.md) and
[docs/technical/decision-thresholds.md](../../technical/decision-thresholds.md)
for what each `FlowLoop` variant and each `Deliberation` level actually
changes.

`FlowBrief` is what every decision-model question is briefed with: the
overall `goal` in plain language, shared `details` by name, the `secrets`
list (names only), and standing `rules` ("stop before paying"). It exists so
a decision about one control is made knowing the whole task, not just the
one step in front of it.

## Checking a flow without running it

`ValidateFlowRequest` takes the candidate flow as raw JSON (a `Value`, not a
typed `Flow`), specifically so a malformed flow comes back as a list of
readable errors instead of a bus decode failure, which is what something
authoring flows programmatically actually needs in order to repair one:

```rust,ignore
pub struct FlowValidation {
    pub valid: bool,
    pub errors: Vec<String>,
    pub steps: usize, // steps counted, nested ones included
}
```

## What a run reports

```rust,ignore
pub struct FlowRunResult {
    pub stop: FlowStopReason,
    pub steps: Vec<StepReport>,
    pub vars: BTreeMap<String, String>,      // final vars, including what `read` set
    pub pending: Option<JevTarget>,          // the action a stop_before found and did not press
    pub learned: Vec<GroundingHint>,         // pass back as memory next time
    pub actions: u32,
    pub metrics: JevMetrics,
    pub trace: Vec<JevExchange>,             // only when RunFlowRequest.trace was set
}
```

`FlowStopReason` is one of `Completed`, `StoppedBeforeDestructive`,
`StepFailed`, `ActionBudget`, `ModelBudget`, `Invalid`. Each `StepReport`
names its position (`"3"`, or `"4.2"` for the second step nested in the
fourth), its outcome (`Done`, `AlreadyDone`, `Gated`, `Failed`), how many
decision turns and calls it spent, which desktop actions it took
(`FlowActionRecord`), and which `FlowLoop`s contributed to it. `FlowLoop` is
a long enum (`Completion`, `Progress`, `Moves`, `Narrowing`, `Corroboration`,
`Slots`, `Obstacles`, `Undo`, `Memory`, `Vote`, `Evidence`, `Escalation`,
`Duel`, and more), each one names one distinct decision loop in the engine,
and `docs/technical/decision-thresholds.md` is where each loop's tunable
thresholds are documented, kept in step with the loop's own code by
convention.

`GroundingHint` is how a later run gets faster without carrying a snapshot
ref (refs die with their snapshot): it remembers an element's role,
accessible name, and ancestor path, which is what survives between runs, and
hands that back as `RunFlowRequest.memory` next time the same flow runs.

## The flow authoring guide

`FLOW_GUIDE` is the contents of `guide.md` embedded as a `&str` with
`include_str!`, returned verbatim by the `FlowGuide` member. It exists to be
pasted straight into whatever writes flows, most often a planning language
model, and every JSON block inside it is a real, parseable `Flow`. If you
are writing flows by hand, read
[`crates/tinycomputer-bus/src/flow/guide.md`](../../../crates/tinycomputer-bus/src/flow/guide.md)
directly; it is the same text, and it includes rules of thumb this page does
not repeat (describe outcomes not clicks, one idea per step, end anything
that matters with `verify`, guard anything irreversible with `stop_before`).

## Where flows fit

A `Flow` is what a `RunFlow` desktop member and a `StartTask.flow` Agent
request both run. See [The Agent and task types](agent-and-tasks.md) for how
a task wraps a flow with budgets, pausing, and a plain-language status, and
[The goal loop](goal-loop.md) for `RunGoal`, the lower-level bounded loop a
single flow step is grounded against.
