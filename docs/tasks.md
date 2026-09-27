# Tasks: handing the module a whole job

The Agent members (`Describe`, `PlanTask`, `StartTask`, `AwaitTask`,
`ContinueTask`, `CancelTask`, `TaskReport`, `ListTasks`) are the API meant for
outside agents. A caller hands over a job such as "find the cheapest flight
from Delhi to Srinagar on 14 October and fill in my details up to payment",
and gets back a task that runs in the background and stops only when it needs
something from the caller.

The controller lives in `crates/tinydesktop-engine/src/task/`. It runs flows
through the flow runtime described in [`decision-loops.md`](decision-loops.md);
this page covers what sits on top: task state, pausing and resuming, private
values, budgets, and the planner.

## A session, end to end

A caller that has never used the module starts with `Describe`. It returns the
contract version, which surfaces are available (and why one is not), whether
Jev and a planner are configured, the step kinds, the flow guide, a JSON Schema
for every Agent member's request and reply, and worked examples.

Then it starts the task. With a planner configured, plain language is enough:

```json
{"member": "StartTask", "args": [{
  "task": "Find the cheapest one-way flight from Delhi to Srinagar on 14 October and fill in my details up to payment",
  "facts": {"first name": "Asha", "last name": "Raina", "email": "asha@example.com",
            "mobile number": "9876543210"},
  "constraints": {"surfaces": ["browser"], "origins": ["https://.goindigo.in", "https://.google.com"]},
  "budget": {"max_actions": 200, "max_model_calls": 400}
}]}
```

Without a planner, the caller passes `flow` instead of `task`, written with the
guide `Describe` returned. Either way `StartTask` returns at once with a
`TaskView`:

```json
{
  "id": "task-1",
  "status": {"state": "running"},
  "summary": "Planning the task.",
  "step": null,
  "progress": 0.0,
  "next": ["AwaitTask", "CancelTask"]
}
```

The caller then long-polls `AwaitTask({id, timeout_ms})`. It returns as soon
as the task stops running, or when the timeout passes (60 seconds at most,
`MAX_AWAIT_MS`). Each view carries a one-sentence `summary`, the current step
(index, kind, intent, surface), `progress` as the fraction of top-level steps
finished, and `next`, the calls that make sense from here. A caller that just
follows `next` cannot get the protocol wrong.

A booking like the one above usually ends like this:

```json
{
  "status": {"state": "checkpoint",
             "reason": "reached the payment step (Pay now); payment is always left to you",
             "location": "Pay now", "continuable": false,
             "summary": "done: https://www.google.com/travel/flights…; the flight results by lowest price; …"},
  "summary": "Stopped: reached the payment step (Pay now); payment is always left to you.",
  "next": ["CancelTask", "TaskReport"]
}
```

The browser session stays open on the payment page so a person can finish.
`CancelTask` releases it afterwards. `TaskReport` returns every step report,
the records the task read or extracted, the trace, and the grounding hints it
learned.

## Statuses

| State | Means | What to call next |
|---|---|---|
| `running` | the task is working | `AwaitTask`, `CancelTask` |
| `needs_input{fields}` | the flow uses a `${name}` no fact supplies, or the planner asked for a value | `ContinueTask` with `inputs` |
| `needs_approval{action, target}` | a `stop_before` found an irreversible control and the task may not press it on its own | `ContinueTask` with `approve: true` or `false` |
| `needs_human{reason}` | a step failed in front of a captcha, a one-time code, two-factor authentication, or a login wall | a person does it, then `ContinueTask` |
| `checkpoint{reason, location, summary, continuable}` | the task stopped where a person must take over; a payment checkpoint is final | `CancelTask` to release it, `TaskReport` |
| `needs_plan{guide}` | a plain-language task arrived and no planner is configured | `StartTask` again with a `flow` |
| `done{answer, records}` | every step finished | `TaskReport` |
| `failed{step, reason, hint, recoverable}` | a step failed, a budget ran out, or the flow was invalid | `TaskReport`, then a new `StartTask` |
| `cancelled` | the caller cancelled, or declined an approval | `TaskReport` |

## How a task runs

`Tasks::start` validates the request, registers a `Cell` (the task's published
view plus its working state), and spawns a worker. The controller holds at most
32 tasks (`MAX_TASKS`), dropping finished ones first.

The worker runs a list of flow runs in order (`drive`). Usually that list has
one entry, the whole flow. For each run it:

1. builds a `RunFlowRequest` (`run_request`) with the facts as variables, the
   fact names marked private, `include_values` off, the task's grounding memory,
   and what is left of the budget;
2. hands it to the `FlowRunner`, which runs it on the task's workspace;
3. reads the result (`interpret::run_outcome`) and decides whether to carry
   on or stop with a status;
4. adds the run's steps, trace, learned hints, and spend to the task, and
   keeps any values the run read that were not facts or flow variables. Those
   become the `records` in the final answer.

The `FlowRunner` trait is what makes the controller testable: tests script the
runs, and the module plugs in `WorkspaceRunner`
(`crates/tinydesktop/src/tinybus_module/runner.rs`), which gives each task its
own `Workspace` of the desktop and a fresh browser session. Runs of one task
share that workspace, so a resumed task picks up on the page the last run left.

### Stopping and resuming

`interpret.rs` maps how a run stopped to what the task does next:

- **Completed.** On to the next run, or `done` if there is none. The answer
  lists what was read, for example `Finished all 9 steps. cheapest: IndiGo ·
  08:00 · ₹7,346`.
- **Stopped before an irreversible action.** If the control or the phrase is a
  payment (`consequence` says `Payment`), the task ends at a final
  `checkpoint`. Otherwise it pauses at `needs_approval` and remembers how to
  resume: the `stop_before` phrase, the app the flow was on, and the steps
  after it. Approving runs a one-step flow with that `stop_before` and
  `allow_destructive` on, then the rest with the task's own setting. Declining
  cancels the task. A `stop_before` nested inside an `if` or `repeat_until`
  resumes from the start of the containing top-level step, because which
  branch or round it was in cannot be recovered from the step path.
- **A step failed.** The task fails, marked recoverable, and remembers the
  failed step and everything after it. Then `human_wall` reads the surface's
  visible text. If it shows a captcha, "verify you are human", a one-time code,
  two-factor authentication, or "sign in to continue", the status becomes
  `needs_human` and `ContinueTask` reruns the failed step and the rest once
  the person has got past it. If not, it stays `failed`.
- **A budget ran out, or the flow was invalid.** `failed`, with a hint such as
  "raise budget.max_actions".

### Budgets across runs

A task's `TaskBudget` (`max_actions`, `max_model_calls`, `max_elapsed_ms`)
bounds the whole task, not one run. An approval or a human step splits a task
into several runs, and each run gets only what the task has not spent yet, so
resuming never refills the budget. Unset fields default to 120 actions and 300
Jev calls. The flow runtime has no clock, so `max_elapsed_ms` is enforced by
the controller, which times out a run that would go past what is left. Time
spent waiting for the caller does not count.

### Constraints

`TaskConstraints` narrows what a task may touch:

- `surfaces`: `browser`, `desktop`, or both (empty means both). A task confined
  to one side gets a workspace without the other, so it cannot reach it even by
  mistake.
- `origins`: the sites a browser session may load, such as
  `https://.goindigo.in` for a site and its subdomains. agent-browser's domain
  filter enforces it. It is a guard rail, not a sandbox.
- `allow_destructive`: press irreversible controls without pausing. Payment is
  still a checkpoint.
- `browser_endpoint`: attach to a running Chrome at this DevTools address
  instead of launching one. Booking sites often turn away a fresh headless
  browser but serve a person's own. Closing an attached session only
  disconnects; it never closes the person's browser.
- `headed`: show the browser.

## Private values

Facts are the caller's own details: names, email, phone, date of birth. The
controller keeps them in a `Facts` store (`tinydesktop-core/src/facts/`) and
follows three rules:

1. **Values stay local.** Jev and the planner see fact names only. A value is
   looked up at the moment it is typed into a field. Summaries, step intents
   in the view, and the final answer go through `Facts::redact`, which
   replaces each value with `‹name›`.
2. **Facts are only typed.** A flow may use `${fact}` only as an `enter` step's
   value. In any other position (step text, a condition, a slot's name, an
   `open` app, a `browse` address) validation rejects the flow before anything
   runs. `open` and `browse` count because the app or address stays visible to
   every later question. When `ContinueTask` supplies a new value, the flow is
   checked again, since a name that looked undefined at the start may now be a
   fact used somewhere it must not be.
3. **Card data is refused.** A fact whose name labels card data (card number,
   credit or debit card, CVV, CVC, security code, card expiry, card PIN, UPI
   PIN), or any value of 13 to 19 digits that passes the Luhn check, is refused
   with `CARD_DATA_REFUSED`, both at `StartTask` and at `ContinueTask`. No task can be handed a card to
   type.

## Payment is always a checkpoint

Two independent checks stop a task before money moves, whatever a model
decided:

- `consequence(label)` in `tinydesktop-core/src/safety/` classifies a control
  by its words. "Pay", "Pay now", "Place order", "Checkout", "Buy now",
  "Confirm and pay" and similar are `Payment`. "Send", "Delete", "Publish",
  "Confirm booking" are `Irreversible`. "Book", "Select", and "Continue" are
  `Reversible` on purpose: they lead to more forms, and the payment check
  stops the run before anything is charged.
- `screen_payment_evidence(screen)` looks at the page itself for card-form
  wording (card number, CVV, expiry, cardholder). When it finds any, every
  click on that screen counts as destructive, so a "Continue" button on a card
  form is refused too.

A planned booking flow ends with `{"stop_before": "paying for the booking"}`.
The runtime finds the pay control, stops in front of it, and the controller
turns that into the final checkpoint.

## The planner

The planner (`crates/tinydesktop-engine/src/planner/`) is an optional language
model that turns a plain-language task into a flow. It never acts and never
sees the screen.

It is compiled into the module (the `planner` feature) and stays off until the
host sends a `planner` object in the module's private configuration, with an
OpenRouter `api_key` and an optional `model` (default
`anthropic/claude-sonnet-5`). Without it, a plain-language `StartTask` returns
`needs_plan` with the guide, and `PlanTask` returns an error saying no planner
is configured.

`Planner::plan` sends the model:

- a protocol: reply with one JSON flow; use `browse` for the web and `open` for
  applications; refer to personal details only as `${name}`; never invent
  them; never enter payment details; end any purchase with a `stop_before` for
  paying; guard sending, deleting, publishing, and submitting the same way;
- the full flow guide;
- the task, the available surfaces, and the fact names.

The reply is parsed and checked with the same validator `RunFlow` uses. An
invalid flow goes back to the model with the errors, up to two times
(`REPAIRS`). Variables the flow uses that no fact supplies come back as
questions, which the task reports as `needs_input` before anything runs.

`PlanTask` is the dry run: it returns the flow and the questions without
starting anything, so a caller can inspect or edit the plan first.

The planner does not rewrite the plan when a step fails partway through a
run. A failed step ends the task as `failed` (or `needs_human`), and a caller
that wants to try again starts a new task with a corrected flow. The lab's
`authored` mode does the multi-round version outside the module.

## Where the code is

| File | Holds |
|---|---|
| `task/mod.rs` | `Tasks`, the `FlowRunner` trait, the task store, `drive`, budgets, `human_wall`, publishing views |
| `task/interpret.rs` | what a finished run means: continue, pause, or stop, and how to resume |
| `task/describe.rs` | `Describe`: capabilities, schemas, and examples |
| `planner/mod.rs` | the planning protocol, validation, and repairs |
| `planner/openrouter.rs` | the OpenRouter `LanguageModel` (feature `planner`) |
| `workspace/mod.rs` | the desktop and the browser as one surface |
| `tinydesktop/src/tinybus_module/runner.rs` | the module's `FlowRunner`: one workspace and browser session per task |
