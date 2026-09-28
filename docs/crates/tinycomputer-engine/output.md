# Task output: shaping the answer, remembering what was saved

A flow saves what it finds with `read`, `extract`, and `pick` steps, into
named variables. A finished task hands those back as `records`: whatever
text the screen showed, in screen order, with its duplicates and its
chrome. That is enough for a caller who only wants to look at what a task
did, but two things go wrong once a task walks a list of several similar
items, such as reading five chats one by one.

First, the run forgets what it already saved. Jev's state carried the
screen and the recent actions ("read … into chat_1"), never the values
themselves, so a step like "open the next chat not yet read" had nothing
to judge against and looped over the same chats. A rescue made this worse:
starting a fresh run forgot even the names.

Second, the caller got raw screen text back. `records` holds duplicates,
interface chrome, and text a screen reader adds ("message", "Received
from …"), in whatever order the screen showed it. A caller that wants
"five chats, ten messages each, as JSON" had to parse that itself.

This page covers the two fixes: a run remembers what it has saved, and a
finished task can shape its records into the JSON its caller actually
asked for. Both were added together and recorded on the same live run, the
WhatsApp example in `crates/tinycomputer-examples/tasks/whatsapp/`; the
formal spec is
[`docs/technical/specs/task-output.md`](../../technical/specs/task-output.md).

## Remembering what a run has saved

Every state Jev is shown, whether the run asks narrow or wide, carries an
`already_collected` field once anything has been saved. It is built by
`collected()` in `agentic/flow/wide.rs` and wired into the shared state
function in `agentic/flow/mod.rs`, so both strategies get it the same way:

- the last `MAX_COLLECTED` (12) variables read, most recent first;
- each value clipped to `COLLECTED_CHARS` (120) characters;
- an `extract`'s saved rows shown as their count and their first row, not
  the whole list.

It is wrapped as `untrusted_accessibility_data` and goes through the same
masking as the rest of the state, so a secret fact saved into a variable
never shows there either. A step such as "open the next chat not yet
read" can now be judged against what the run actually remembers, not
against a bare list of action names.

### Carrying memory across runs

A task rarely finishes in one flow run: an approval, a missing value, or a
rescue each split it into another one. `RunFlowRequest.collected` is how
what an earlier run saved reaches the next one. The task controller fills
it on every run it starts (`run_request` in `task/mod.rs`), so:

- the new run's variables start with `collected`'s values, though the
  caller's own `vars` win any clash with the same name;
- `read` in the new run's state starts with `collected`'s names, so
  `already_collected` recalls them from the very first turn, even before
  this run has saved anything itself.

A plain `RunFlow` call, made directly rather than through a task, also
takes `collected`; it is simply empty unless the caller passes something in.

### The rescuer sees it too

A step failure hands the task's rescuer a `Briefing` that lists everything
the task has saved so far (`Briefing::collected`, fact values redacted, 200
characters each), and the rescuer's own protocol tells it these are
variables its guidance may use and that it should not redo what is already
there. See [rescue.md](rescue.md) for the whole rescue path; this is the
one place its briefing overlaps with this page.

## Choosing the list a step means

`extract` and `pick` both find a screen's repeated items with
`result_families` (in `tinycomputer-core`). A results page usually repeats
its cards under a container labelled by ordinal (`listitem #3`), and the
list with the most cards is picked. A desktop tree labels nothing that
way; there, a run of `MIN_FLAT_ITEMS` (3) or more same-role leaf siblings
is read as a list instead, one record per element, which is what makes
`extract` and `pick` work on a native application's flat accessibility
tree at all, not only on a web page's own list markup.

When more than one list shows at once, such as a chat list beside the
open chat's own messages, the runtime asks Jev which one the step means
(the `list` question in `steps.rs`): each of the first `MAX_LISTS` (6)
lists is shown by its first `LIST_PREVIEW` (3) items, and a clear winner
at or above `LOCATE_FLOOR` (0.5) is used. Anything less clear falls back
to the longest list, the one an `extract` or an unranked `pick` would have
used before it asked. A `pick` whose criterion parses exactly (price,
time, duration, stop count) skips this question outright: it ranks
whichever list has that measure and never asks which list is meant.

A `read` of an element whose name and value differ, such as a chat button
named for the chat but holding its last message as a separate value,
offers both the name and the value to Jev as separate sources rather than
picking one for it.

## Shaping the answer

`StartTask.output` (`TaskOutput`) is how a caller asks for something more
than raw records:

```rust
pub struct TaskOutput {
    pub instructions: String,      // what to return, in plain language
    pub schema: Option<Value>,     // the result's JSON Schema; None means any object
}
```

Once every step in the flow has finished, if `output` was given, the task
runs one more pass before it reports `Done`: `Shaper::shape`, in
`crates/tinycomputer-engine/src/shape/mod.rs`, asks a reasoning language
model for the caller's answer, built only from what the run saved.

### What the shaper is given, and told

The shaper is handed a `Harvest`: the task's goal in the caller's own
words, the `output` it asked for, and every `(name, value)` the flow read,
with every fact value already redacted by the task controller before it
gets there. It never acts and it never sees a screen, only that text,
wrapped as untrusted data. Its protocol is explicit that it may select,
clean up, deduplicate, reorder, split, and restructure what the records
hold, but must never invent, guess, or complete a value that is not in
them: it leaves out what is missing, or uses `null` where the schema
allows it.

The records handed to it are capped at `RECORDS_CHARS` (60,000)
characters; anything cut past that is noted in the prompt rather than
silently dropped.

### Checking and repairing the answer

The model's reply must be exactly one JSON object. If a schema was given,
`schema::violations` (in `shape/schema.rs`) checks the reply against it and
lists every violation by path ("the result is missing `email`", "the
result.age must be of type integer, not string"). A reply that fails goes
back as another turn with those violations, and the model gets up to
`REPAIRS` (2) more tries. An answer that still does not fit after that
fails the task outright, not recoverable: `TaskReport`'s records are still
there for the caller to read by hand, but `Done` is never returned without
a result that actually satisfies what was asked for.

What passes becomes `done.result`, a field on `TaskStatus::Done` alongside
the existing `answer` and `records`; the raw records are still returned
too, so a caller that also wants to double-check the shaped answer can.

### The schema subset

`TaskOutput.schema` is not full JSON Schema. It supports exactly:
`type`, `properties`, `required`, `additionalProperties` (a boolean),
`items`, `enum`, `minItems`, `maxItems`, `description`, and `title`, with
an `object` at the top level (a model is asked for JSON, and a caller
reads named fields, so the result is always an object). Anything outside
that list is refused before the task ever starts, with `INVALID_OUTPUT`,
rather than being half-checked and silently ignored. `schema::supported`
is what enforces the subset; `schema::violations` is what checks a result
against it once the task is running.

### Errors

| Code | Cause |
|---|---|
| `OUTPUT_UNAVAILABLE` | `output` was given but no shaper is configured (the same `planner` feature that builds the planner and rescuer builds the shaper). |
| `INVALID_OUTPUT` | `output.schema` uses a keyword or type outside the supported subset. |

`Describe`'s `Capabilities.output_configured` reports whether a module can
honor `output` at all, so a caller can check before it asks.

### Configuring the shaper

Like the planner and the rescuer, the shaper sits behind the `LanguageModel`
trait (see [planner.md](planner.md)), and the same OpenRouter adapter
builds it:

```rust
pub fn open_router_shaper(config: &PlannerConfig) -> Result<Shaper, String>
```

using `config.output_model` (`OUTPUT_MODEL`, `openai/gpt-6-luna` when
unset), a reasoning model with room to reason briefly before it answers,
the same setup as the rescuer.

## Not done

- A `press` step for an application's own shortcuts (WhatsApp's ⌘1 through
  ⌘9 open the Nth chat) would make "open the Nth item" deterministic. The
  list memory and the list question made it unnecessary for the WhatsApp
  run, so it was left out.
- `extract` reads what is on screen as it stands; it does not scroll for
  more.

## Source

- `crates/tinycomputer-engine/src/shape/mod.rs`, `Shaper`, `Harvest`,
  `judge`, the shaping protocol.
- `crates/tinycomputer-engine/src/shape/schema.rs`, the schema subset,
  `supported`, `violations`.
- `crates/tinycomputer-engine/src/agentic/flow/wide.rs`, `collected`,
  `MAX_COLLECTED`, `COLLECTED_CHARS`.
- `crates/tinycomputer-engine/src/agentic/flow/steps.rs`, `judge_list`,
  `judge_pick`, `MAX_LISTS`, `LIST_PREVIEW`.
- `crates/tinycomputer-core/src/surface/groups.rs`, `result_families`,
  `flat_lists`, `MIN_FLAT_ITEMS`.
- `crates/tinycomputer-engine/src/task/mod.rs`, `finish`, `run_request`,
  where a task's `output` and `collected` are actually wired in.
- `crates/tinycomputer-bus/src/agent/types.rs`, `TaskOutput`, the `result`
  field on `TaskStatus::Done`.
- `crates/tinycomputer-bus/src/flow/types.rs`, `RunFlowRequest::collected`.
- [`docs/technical/specs/task-output.md`](../../technical/specs/task-output.md),
  the formal spec this page follows.
- [tasks.md](tasks.md) and [rescue.md](rescue.md), the task controller and
  rescuer that carry this memory between runs.
