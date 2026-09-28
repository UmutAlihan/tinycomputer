# Task output: saved values, remembered and shaped

Status: implemented. Recorded run: the WhatsApp example
(`crates/tinycomputer-examples/tasks/whatsapp/`).

## Why

A flow saves what it finds with `read`, `extract`, and `pick`. Two things
went wrong with that on a task that walks a list (reading five WhatsApp
chats one by one):

- **The run forgot what it had saved.** Jev's state held the screen and the
  recent actions ("read … into chat_1"), never the values. A step such as
  "open the next chat not yet read" could not be judged, and it looped over
  the same chats. A rescue started a new run, which forgot even the names.
- **The caller got raw screen text.** `done.records` holds whatever the
  screen showed: duplicates, chrome, text a screen reader adds ("message,
  …, Received from …"), in screen order, with no fixed shape. A caller that
  wants "five chats, ten messages each, as JSON" had to parse it.

## What

### Memory: what the run has saved

- Every state Jev is shown, narrow or wide, carries `already_collected` once
  anything is saved: each saved variable's value (an `extract`'s as its row
  count and first row), wrapped as `untrusted_accessibility_data`, the last
  12 variables at 120 characters each (`MAX_COLLECTED`, `COLLECTED_CHARS`).
  It is masked like the rest of the state, so a secret's value never shows.
- `RunFlowRequest.collected` carries what earlier runs of the same task
  saved. A run starts with those as variables (a caller's `vars` win a
  clash) and recalls them as already collected. The task controller fills
  it on every run, so a rescued, approved, or resumed task remembers.
- The rescuer's briefing lists what the task has saved, fact values
  redacted, 200 characters each, and its steps may use those names.

### Choosing the list a step means

- On a screen where nothing repeats by ordinal (a desktop tree), a run of
  three or more same-role leaf siblings is a list (`MIN_FLAT_ITEMS`), so
  `extract` and `pick` work on native applications.
- When several lists show, `extract`, and a `pick` whose criterion is judged,
  ask Jev which list their `what`/`from` names (the `list` question), and
  fall back to the longest when it is unsure.
- A `read` of an element whose name and value differ (a chat's button named
  for the chat, holding its last message) offers the name and the value as
  separate sources.

### Shape: one pass that returns the caller's JSON

- `StartTask.output` is `{instructions, schema?}`. When every step has
  finished, one reasoning-model pass (`output_model`, default
  `openai/gpt-6-luna`) gets the goal, the instructions, the schema, and the
  saved values (fact values redacted, wrapped as untrusted data, 60,000
  characters at most) and answers with one JSON object.
- The object is checked against the schema. A failing one goes back with
  every violation, by path, up to twice. What passes is `done.result`; the
  raw `records` stay beside it.
- The schema is a subset of JSON Schema: `type`, `properties`, `required`,
  `additionalProperties` (a boolean), `items`, `enum`, `minItems`,
  `maxItems`, `description`, `title`, with an `object` at the top. Anything
  else is refused when the task starts (`INVALID_OUTPUT`), so every rule
  given is enforced rather than half-checked.
- The model is told to select, clean, reorder, and restructure only, never
  to invent a value.

## Constraints

- A result that never fits fails the task, not recoverable, with its
  records in `TaskReport`; `done` is never returned without a result that
  was asked for.
- `output` needs the planner configured (`OUTPUT_UNAVAILABLE` otherwise);
  `Describe` reports `output_configured`.
- A plain `RunFlow` has no shaping pass: it belongs to the task controller.
  It does take `collected`.
- Contract 2.5 adds `StartTask.output`, `done.result`,
  `Capabilities.output_configured`, and `RunFlowRequest.collected`, all
  optional.

## Not done

- A `press` step for an application's own shortcuts (WhatsApp's ⌘1–⌘9 open
  the Nth chat) would make "open the Nth item" deterministic; list picks and
  the memory made it unnecessary for the WhatsApp run, so it was left out.
- `extract` reads what is on screen; it does not scroll for more.
