# Memory and saving

tinycomputer keeps several kinds of memory. Some last for one step, some for
a whole task, and some carry over from one run to the next. This page covers
each one: what it holds, how long it lasts, and where it lives.

| What | Holds | Lasts | Lives |
|---|---|---|---|
| Saved values | text a flow read, extracted, or picked | the whole task, rescues included | the task, then `done.records` and `done.result` |
| The output shape | your saved values, cleaned into the JSON you asked for | once, at the end | `done.result` |
| Grounding memory | which control worked for which step on which app | across runs, if you keep it | returned to you; you pass it back |
| Checkpoints | a snapshot of the screen before a risky press | one step | inside the run |
| Working memory | finished steps, recent actions, what failed | the run | shown to Jev with each question |
| Saved plans | a flow you want to run again | as long as you keep the file | your own JSON file |
| The report and journal | everything that happened | as long as you keep them | `TaskReport`, the trace, `.jev-journal/` |

## Saved values: what the run found

Three step kinds save what they find on screen:

- `read` saves one piece of text, like the subject of the newest email;
- `extract` saves every row of a list, like all the flight results;
- `pick` saves the best item of a list, like the cheapest flight, and opens
  it.

Each one puts the text into a named variable (`into`). Later steps can use
it. A flow can read a price on an airline's site and then type
`${cheapest_flight}` into a Mail draft.

### The run remembers what it saved

Every question Jev gets includes an `already_collected` list: the values the
run has saved so far, up to the last twelve, each cut to 120 characters. An
extracted list shows as its row count and first row.

This matters for steps that walk through a list. Picture a task that reads
five WhatsApp chats one at a time. A step like "open the next chat not yet
read" can only be judged if Jev knows which chats were already read. Before
this memory existed, the run looped over the same chats again and again.

The memory survives breaks in the task. When a task pauses for your approval,
or a rescue starts a new run, the new run starts with everything saved so
far. The rescuer is shown the saved values too.

Secrets are masked in this list like everywhere else. Saved values are
labelled as untrusted screen data, since they came off a screen.

## Getting results back in your own shape

Saved values are raw screen text. They can have duplicates, button labels,
and the extra words a screen reader adds, like "message, ..., Received from
...". If you want clean JSON, ask for it:

```json
{"member": "StartTask", "args": [{
  "task": "Read my five most recent WhatsApp chats",
  "output": {
    "instructions": "One entry per chat: the contact's name and their last ten messages, newest last.",
    "schema": {
      "type": "object",
      "required": ["chats"],
      "properties": {
        "chats": {
          "type": "array",
          "maxItems": 5,
          "items": {
            "type": "object",
            "required": ["name", "messages"],
            "properties": {
              "name": {"type": "string"},
              "messages": {"type": "array", "items": {"type": "string"}}
            }
          }
        }
      }
    }
  }
}]}
```

When every step has finished, one pass of a reasoning model reads the goal,
your instructions, the schema, and the saved values. It answers with a JSON
object. That object is checked against the schema. If it doesn't fit, the
model gets the list of problems and tries again, up to twice. What passes
comes back as `done.result`, with the raw `records` beside it.

A few rules keep this honest:

- the model may select, clean, reorder, and restructure. It is told never to
  invent a value;
- your personal details are removed from what it sees;
- the schema supports a subset of JSON Schema (`type`, `properties`,
  `required`, `items`, `enum`, `minItems`, `maxItems`, and a few more).
  Anything outside that subset is refused when the task starts, so every
  rule you give is actually checked;
- if the result never fits, the task fails rather than returning something
  half-right. The raw records are still in `TaskReport`.

Output shapes need the planner configured, since the same model provider
does the shaping.

## Grounding memory: what worked last time

Every time a click, a choice, or a typed field works, tinycomputer remembers
it as a hint: the app, the step, and the control's role, name, and where it
sits on the screen ("the button New Message, inside the toolbar"). It never
remembers the temporary id, which changes every time the screen is read.

Next time the same step runs on the same app, the remembered control is
checked first with one yes/no question. If it still fits, that's the whole
search. A second run of the same job usually asks Jev far fewer questions
than the first.

tinycomputer doesn't write any files for this. The hints come back in each
result (`learned`) and in `TaskReport`, and you pass them back in the next
request as `memory`. You decide whether to keep them and where. The lab keeps
them in `target/lab-runs/memory.json`.

The wide strategy uses the same memory for pop-ups. The button that closed a
cookie banner is remembered, and the next run tries it first.

## Checkpoints: saving the screen before a risky move

Before an important press, tinycomputer saves a checkpoint: what the screen
showed and the page's web address. If the press turns out to be a mistake,
it uses the checkpoint to put things back and then checks that it really got
back there. A `choose` step also saves the text of every field before it
starts, so it can restore them if it types into the wrong one.

Checkpoints only last for the step. See
[Catching mistakes](catching-mistakes.md) for how undo works.

## Working memory: what Jev knows about the run

Each question Jev gets includes recent history: the last 20 things that
happened, with notes on what each action changed. With the wide strategy, it
also gets a fuller working memory:

- one line per finished step;
- the current step and the next one;
- the last 24 actions, across step boundaries;
- what was tried and failed in this step;
- what was saved;
- how much budget is left.

This is how Jev can tell "I already pressed that and nothing happened" from
a fresh start.

## Saved plans: running the same job again

A plan is a flow, and a flow is a JSON file. You can keep one and run it
again:

1. call `PlanTask` with your task. It returns the plan and any questions,
   without running anything;
2. save the plan to a file, and edit it if you like;
3. pass it as `flow` to `StartTask` whenever you want to run it.

A saved plan skips the planner and runs exactly the same steps each time.
That makes runs easy to compare. The repository keeps a few real ones in
`crates/tinycomputer-examples/tasks/`: an Emirates booking, an IndiGo booking
to Kashmir, and a WhatsApp reader. The `task_live` example replays one with
`FLOW_FILE`. See the [examples docs](crates/tinycomputer-examples/README.md).

## Picking up where it left off

A task keeps its browser session and desktop between runs. When it pauses
for approval, for a person to solve a captcha, or for a rescue, the next run
starts on the page the last one left. It doesn't start over.

What carries over:

- the browser session, on the same page;
- everything saved so far;
- what's left of the budget (pausing never refills it);
- the steps still to do.

What doesn't: anything after the task is cancelled or finished. `CancelTask`
releases the browser. The module holds up to 32 tasks and drops finished ones
first.

## The record of what happened

- `TaskReport` has every step and how it ended, every saved value, every
  rescue, and the hints learned.
- A trace (`trace: true` on a flow) records every decision: what Jev saw,
  what it was asked, and what it answered.
- The debug journal records every single call to Jev with its timing. It's
  off unless you turn it on, and it writes to `.jev-journal/`.

Traces and journals contain screen text, which can include personal data.
Both are kept out of git. See [Watching a run](watching-a-run.md).

## Where to find out more

- [`technical/specs/task-output.md`](technical/specs/task-output.md): saved
  values, memory, and output shapes.
- [`technical/decision-loops.md`](technical/decision-loops.md#grounding-memory):
  grounding memory.
- [`technical/specs/jev-wide-turns.md`](technical/specs/jev-wide-turns.md): the
  working memory.
