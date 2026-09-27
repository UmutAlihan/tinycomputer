---
name: tinydesktop
description: Hand a task to the tinydesktop module and follow it to completion across web pages and desktop applications. Use when a person asks you to do something on their computer or on the web for them, such as finding and booking travel up to payment, filling a form, or drafting an email in Mail.
---

# tinydesktop

tinydesktop runs a task for you on the person's computer: in a browser, in desktop applications, or across both. You describe *what* should happen; it works out *how* on the live screen, and pauses only for what you or the person must decide.

## The loop

1. `Describe` once. It returns what is available (desktop, browser, planner), the flow guide, every member's input schema, and worked examples.
2. `StartTask` with the task. It returns at once with a task id.
3. `AwaitTask` until the status is not `running`.
4. Act on the status (below), then go back to step 3 until the status is final.
5. `TaskReport` for the details: every step, what was read, and the extracted records.

Every reply is `{ok, data}` or `{ok: false, error: {code, message, hint, recoverable}}`. When a call fails, follow the `hint`. A `TaskView` always carries a one-sentence `summary` and `next`: the calls that make sense now.

## Starting a task

- **With a planner configured** (`Describe` says `planner_configured`): pass `task` in plain words.
- **Without one:** write a `flow` from the guide and pass that. A flow says what to accomplish, step by step, and never how: no button names, menus, or shortcuts.
- **Facts:** pass the person's details (names, date of birth, email, phone) as `facts`, and refer to them in a flow as `${name}`. They brief the module's decision model so it knows whom it is acting for, and are typed on the person's machine.
- **Secrets stay templates.** A card number, CVV, passport number, password, or one-time code is secret: the decision model only ever sees `${name}`, and a flow may use it only as an `enter` value. Name any other secret in `secret_facts`.
- **Never pay on the person's behalf.** By default the task stops at payment for the person to finish. With `constraints.payment` set to `fill_then_approve` (and `origins` listed) it fills the card form from secret facts, then waits for `ContinueTask.approve` before pressing pay — ask the person before approving.
- **Constraints:** `constraints.surfaces` limits where the task may act; `constraints.origins` limits which sites it may load. `allow_destructive` lets it send, delete, or confirm without asking; leave it off unless the person said so.

## Statuses and what to do

| Status | Meaning | Do |
|---|---|---|
| `running` | working | `AwaitTask` again |
| `needs_input` | values it does not have | ask the person for `fields`, then `ContinueTask` with `inputs` |
| `needs_approval` | about to do something irreversible (`action`, `target`) | ask the person; `ContinueTask` with `approve: true` or `false` |
| `needs_human` | a captcha, a one-time code, or a login only a person can pass | tell the person `reason`; once they are done, `ContinueTask` with `answer: "done"` |
| `checkpoint` | stopped, usually at a payment page | tell the person the `summary` and where it stopped; they finish from there |
| `needs_plan` | plain-language task, no planner | write a flow with `guide` and start again with `flow` |
| `done` | finished | report `answer`; `records` holds anything read or extracted |
| `failed` | could not finish | explain `reason`; if `recoverable`, try again changed as `hint` says |

## Writing flows that work

- **One idea per step.** "search for flights", then "open the cheapest result", not both in one step.
- **`browse`** opens a web address; **`open`** switches to a desktop application.
- **`enter`** fills several fields at once, keyed by what each field is for: `{"enter": {"where from": "Delhi", "email": "${email}"}}`.
- **`pick`** chooses from a list and opens the choice. Prices, times, durations, and stops are compared exactly: `{"pick": {"from": "the flight results", "by": "lowest price", "into": "flight"}}`.
- **`extract`** captures a whole list.
- **`read`** captures one piece of text.
- **Always end a purchase or booking with `stop_before`** paying. End a message you draft with `stop_before` sending.

## Example

```json
{"member": "StartTask", "args": [{
  "flow": {"app": "browser", "steps": [
    {"browse": "https://www.google.com/travel/flights"},
    {"enter": {"where from": "${from}", "where to": "${to}", "departure date": "${date}"}},
    "search for flights",
    {"wait_for": "flight results are listed"},
    {"pick": {"from": "the flight results", "by": "lowest price", "into": "cheapest"}},
    "continue to booking",
    {"enter": {"first name": "${first name}", "last name": "${last name}", "email": "${email}", "phone": "${phone}"}},
    {"stop_before": "paying for the booking"}
  ]},
  "facts": {"from": "Delhi", "to": "Srinagar", "date": "14 October",
            "first name": "Asha", "last name": "Raina", "email": "asha@example.com"}
}]}
```

This pauses with `needs_input` for `phone`, runs to the payment page, and stops at a `checkpoint` naming the cheapest flight.
