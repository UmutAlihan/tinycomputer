# Using it as an agent

This page is written for the calling agent's point of view: the model that
has `SKILL.md` loaded and is actually making the calls. If you are a person
deciding whether to hand tinycomputer a task, [Giving it a
task](../../giving-it-a-task.md) covers the same ground at a human level;
this page is closer to a checklist for the code (or model) doing the
calling.

## The loop, concretely

1. Call `Describe` once, at the start of a session. It returns what
   surfaces are available (`desktop`, `browser`), whether a planner is
   configured, the flow guide, every task member's input schema, worked
   examples, and a `catalogue` of every member the module serves (task,
   flow, desktop, and browser), each with its family and a one-sentence
   summary. Do this once, not before every task.
2. Call `StartTask` with either `task` (plain language, needs a planner) or
   `flow` (written from the guide `Describe` returned). It returns at once
   with a task id; the work happens in the background.
3. Call `AwaitTask` and wait for it to return. Keep calling it in a loop as
   long as the status is `running`.
4. When the status is anything else, act on it (see the table below), then
   go back to step 3 until the status is final (`done`, `failed`, or
   `cancelled`).
5. Call `TaskReport` for the full story: every step, anything that was
   read from the screen, and the records that were extracted.

Every reply follows one shape: `{ok, data}` on success, or
`{ok: false, error: {code, message, hint, recoverable}}` on failure. When a
call fails, the right move is almost always to follow `error.hint` rather
than to guess at a retry. Every `TaskView` (what `AwaitTask` and
`ContinueTask` return) also carries `summary`, a one-sentence description
of where things stand, and `next`, the list of calls that make sense right
now. If you only ever call what `next` suggests, you cannot get the
protocol wrong.

## Reacting to each status

| Status | What it means | What you do |
|---|---|---|
| `running` | Still working. | Call `AwaitTask` again. |
| `needs_input` | The task needs a value it was not given. | Ask the person for the named `fields`, then call `ContinueTask` with `inputs`. |
| `needs_approval` | It found something it cannot undo (`action`, `target`). | Ask the person; call `ContinueTask` with `approve: true` or `false`. |
| `needs_human` | A captcha, a one-time code, or a login only a person can pass. | Tell the person the `reason`; once they have done it, call `ContinueTask` with `answer: "done"`. |
| `checkpoint` | Stopped on purpose, usually right at payment. | Tell the person the `summary` and where it stopped; a person finishes from there. |
| `needs_plan` | Plain-language `task` was given but no planner is configured. | Write a `flow` yourself from the guide, and start again with `flow`. |
| `done` | Finished. | Report `answer`; `records` holds anything read or extracted. |
| `failed` | Could not finish; a configured rescuer already tried up to five times. | Explain `reason`; if `recoverable`, try again changed as `hint` suggests. |

## Writing your own flow

When there is no planner, or when you want more control than plain language
gives you, you write the `flow` yourself instead of `task`. The rules,
straight from `SKILL.md`:

- **One idea per step.** "search for flights", then "open the cheapest
  result", as two steps, not one.
- `browse` opens a web address; `open` switches to a desktop application.
- `enter` fills several fields at once, keyed by what each field is *for*,
  not by its exact label: `{"enter": {"where from": "Delhi", "email": "${email}"}}`.
- `pick` chooses from a list and opens the choice, comparing prices, times,
  durations, and stop counts exactly:
  `{"pick": {"from": "the flight results", "by": "lowest price", "into": "flight"}}`.
- `extract` captures a whole list; `read` captures one piece of text.
- Always end a purchase or a booking with `stop_before` paying. End a
  drafted message the same way, with `stop_before` sending.

Notice what is never in a flow: no button names, no menu paths, no
keyboard shortcuts. A flow says what should happen; working out how, on
whatever the screen actually looks like right now, is the module's job, not
yours. See [Writing flows](../../writing-flows.md) for more worked
examples.

## Facts, secrets, and the payment rule

- Pass a person's own details as `facts`, and refer to them inside a flow
  as `${name}`, for example `${first name}` or `${email}`. These brief the
  decision model so it understands whom it is acting for.
- Card numbers, CVVs, passport numbers, passwords, and one-time codes are
  secret: the decision model only ever sees the placeholder, never the
  value, and a flow may only use one as an `enter` value, never in any other
  kind of step. Name anything else that should be treated the same way in
  `secret_facts`.
- **Never pay on the person's behalf** by default: a task stops at payment
  and leaves it for a person to finish. Setting
  `constraints.payment: "fill_then_approve"` (with `constraints.origins`
  listed) lets it fill the card form from secret facts and then wait for
  `ContinueTask.approve` before actually pressing pay. Ask the person before
  approving that; do not treat it as routine.

## Constraining what the task can touch

`constraints.surfaces` limits a task to `desktop`, `browser`, or both, and
`constraints.origins` limits which web addresses it may load at all.
`allow_destructive` lets a task send, delete, or confirm without pausing
for approval; leave it off unless the person has said they want that.

## Looking closer, or driving the browser yourself

Tasks are the way in; the module also serves 13 `Browser…` primitives for
when you need to look or act directly instead of handing over a whole job.
They reply `{ok, command, data}` or
`{ok: false, error: {code, message, suggestion, recovery}}`, the same shape
on the desktop and in the browser: `STALE_REF` always means take a fresh
snapshot and choose again.

- `BrowserListSessions` shows every open browser session, a running task's
  included.
- `BrowserOpenSession`, then `BrowserNavigate` and `BrowserSnapshot` with
  `{"session": id, …}`; act on a ref with `BrowserPerform`, for example
  `{"session": "s-1", "action": "click", "target": {"kind": "ref", "value": "e3"}}`.
- `BrowserScreenshot` returns an output id, not an image: read it with
  `BrowserReadOutput` from `offset` 0 until `eof`, then
  `BrowserReleaseOutput`. The same call works for a screenshot in a task's
  `TaskReport.artifacts`; a task view never carries one.
- `BrowserCloseSession` when done with a session you opened; leave a task's
  own session to the task.

## The example, as a sanity check

`SKILL.md` ends with a full worked example: a flight search, a pick by
lowest price, an `enter` of passenger details missing a phone number, and a
`stop_before` paying. Sending exactly that request produces, in order, a
`needs_input` for the missing phone number, then continued progress to the
payment page, then a `checkpoint` naming the cheapest flight found. Reading
that example alongside the status table above is a reasonable way to check
that you have understood the loop correctly before wiring it up for real.
