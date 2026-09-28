# Live tasks

`task_live` runs a whole plain-language task the same way an outside host
would through the task API: give it a task and some facts, and it plans a
flow, starts it, follows it until it stops, and reports what happened. The
two examples saved under `tasks/`, booking a flight to Kashmir, and one to
Dubai on Emirates, are real websites, not a fixture, so this is the closest
thing in the repository to what tinycomputer looks like doing actual work
for someone. Neither example ever pays for anything: both stop at the
payment checkpoint by design, and `task_live` treats reaching that
checkpoint as its own success condition.

If you have not read [giving it a task](../../giving-it-a-task.md) yet,
start there: it explains the task API (`PlanTask`, `StartTask`,
`AwaitTask`, checkpoints, rescues) that this binary is just a command-line
face on.

## Running a saved task

```sh
scripts/docker-lab -- crates/tinycomputer-examples/tasks/run kashmir
```

`tasks/run` is a small wrapper: it points at `tasks/kashmir/task.md` and
`tasks/kashmir/facts.json`, sets a realistic browser user agent (booking
sites are quick to turn away anything that announces itself as headless),
and builds and runs `task_live`. Swap `kashmir` for `emirates` to run the
other one. The task name becomes a path, so it is restricted to letters,
digits, `_`, and `-`, so nothing can climb out of `tasks/` with `/` or `..`.

Because this launches a real browser, run it through `scripts/docker-lab`,
never directly on the host; see [Docker lab](docker-lab.md) for why.

## What's saved under `tasks/`

Each task folder holds three files:

| File | What it is |
|---|---|
| `task.md` | the task in plain language, plus notes from researching the real site with `site_probe` beforehand: which sites actually serve results to an automated browser, what the booking widget's controls are called, what to decline |
| `facts.json` | the facts the task needs, by name: a plain string, or `{"value": "...", "secret": true}` for one that should stay out of what Jev is shown as text (a card or passport number is treated as secret regardless of that flag) |
| `plan.json` | the flow a run actually followed, saved from a real, working run (see the next section) |

Reading `tasks/kashmir/task.md` is worth doing once even if you never run
the example: it shows what research a task needs before you point an agent
at a site it has not tried, including which travel sites simply refuse to
serve an automated browser at all.

## Replaying a saved plan instead of planning a new one

Planning costs an LLM call and is not perfectly repeatable: the planner
can write a slightly different flow each time. Set `FLOW_FILE` to skip
planning and run an exact, saved flow instead:

```sh
FLOW_FILE=crates/tinycomputer-examples/tasks/kashmir/plan.json \
  scripts/docker-lab -- crates/tinycomputer-examples/tasks/run kashmir
```

Each task's `plan.json` is the actual flow its recorded evaluation runs
replayed; see
[`docs/technical/evals/2026-09-28-rescue.md`](../../technical/evals/2026-09-28-rescue.md)
for what those runs found. Use `FLOW_FILE` when you are trying to isolate
whether a change affected *running* a flow versus *planning* one. Replay
the same flow before and after your change, and any difference in outcome
is not the planner's doing.

## Attaching to your own Chrome

Some booking sites treat a fresh, unauthenticated headless browser with
suspicion but behave normally for a real signed-in session. Point
`task_live` at a Chrome you already have running, with remote debugging
enabled, instead of launching a fresh one:

```sh
# start Chrome with remote debugging once, on your Mac
open -a "Google Chrome" --args --remote-debugging-port=9222

TINYCOMPUTER_BROWSER_ENDPOINT=http://127.0.0.1:9222 \
  TASK_FILE=crates/tinycomputer-examples/tasks/kashmir/task.md \
  FACTS_FILE=crates/tinycomputer-examples/tasks/kashmir/facts.json \
  cargo run -p tinycomputer-examples --bin task_live
```

Because the browser here is your own, already-running Chrome, this runs on
the host, not in the Docker lab: the container has no browser to attach to
and nothing to display. Closing the run only disconnects from your browser;
it does not close it.

## Every environment variable `task_live` reads

All of these are documented with placeholders in
[`.env.example`](../../../.env.example); the list below groups them by what
they control.

**Required:**

| Variable | For |
|---|---|
| `OPENROUTER_API_KEY` | Jev and the planner |
| `TASK_FILE` | the task, in plain language |
| `FACTS_FILE` | the JSON facts file described above |

**Optional, how the task runs:**

| Variable | Default | For |
|---|---|---|
| `FLOW_FILE` | unset | run this flow instead of asking the planner for one |
| `TASK_OUT` | `target/task-live` | where the plan, report, and a final screenshot are written |
| `TINYCOMPUTER_FLOW_STRATEGY` | `narrow` | `narrow` or `wide` asking; see [`specs/jev-wide-turns.md`](../../technical/specs/jev-wide-turns.md) |
| `TINYCOMPUTER_FLOW_DELIBERATION` | `deep` | `deep`, `standard`, or `off` |
| `TASK_MAX_MINUTES` | `20` | the task is cancelled after this long |
| `TASK_RESCUES` | `5` | how many failed steps a reasoning model may rescue (`0` turns rescues off); see [rescue](../../rescue.md) |
| `TINYCOMPUTER_RESCUE_MODEL` | `openai/gpt-6-luna` | the OpenRouter model that performs a rescue |
| `TINYCOMPUTER_PLANNER_MODEL` | the engine's default | the OpenRouter model asked to plan the flow |

**Optional, the browser:**

| Variable | For |
|---|---|
| `TINYCOMPUTER_BROWSER_EXECUTABLE` | which browser binary to launch |
| `TINYCOMPUTER_BROWSER_USER_AGENT` | the user agent it announces |
| `TINYCOMPUTER_BROWSER_ARGS` | space-separated extra launch arguments |
| `TINYCOMPUTER_BROWSER_ENDPOINT` | attach to a running Chrome (e.g. `http://127.0.0.1:9222`) instead of launching one |

**Optional, the cursor:**

| Variable | Default | For |
|---|---|---|
| `TASK_CURSOR` | `natural` | the on-screen cursor's pace: `off`, `brisk`, `natural`, `calm` |

The cursor is drawn by the `tinycomputer-cursor-overlay` helper, which
`task_live` expects to find beside itself once it has been built with
`cargo build -p tinycomputer-cursor --features overlay`.

## What it writes and how it ends

Under `TASK_OUT` (`target/task-live/<name>` when run through `tasks/run`),
you get `plan.json` (the flow the planner wrote, if you didn't supply
`FLOW_FILE`), `report.json` (every step, its outcome, and its note), and
`final.png` (a screenshot of wherever the browser ended up). `task_live`
prints the task's status as it runs and treats stopping at a checkpoint
whose reason mentions payment as success (`PASS stopped at payment`);
anything else, including finishing without ever reaching that checkpoint,
is reported as a failure.
