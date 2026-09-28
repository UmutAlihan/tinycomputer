# Watching a run

When a run does something unexpected, you want to know why. tinycomputer
gives you three levels of detail, from a quick summary to every single
question it asked. This page covers each one and how to use them to find the
problem.

## Level 1: step reports (always on)

Every flow result and every `TaskReport` has one report per step:

- how the step ended: `Done`, `AlreadyDone`, `Failed`, or `Gated` (stopped
  in front of an irreversible control);
- a note saying why, such as "the last three actions changed nothing on
  screen" or "not accomplished after 8 turns";
- how many turns, actions, and questions to Jev it took;
- which parts of the decision loop took part;
- the lowest confidence of any decision in the step.

The note on a failed step is usually enough to start with.

A task also has a running `summary` you can read while it works, like
"Step 13 failed; asking for guidance (rescue 1 of 5)."

## Level 2: the trace (ask for it)

Set `trace: true` on a flow, and the result includes every decision: what
Jev was shown, what it was asked, and the combined answer. This answers "what
did it decide, and based on what?".

The lab writes the trace to `jev.jsonl`, next to a readable `timeline.txt`.

## Level 3: the debug journal (turn it on)

Set `TINYCOMPUTER_JEV_JOURNAL=1` in the process that loads the module. Every
call to Jev is recorded with the exact request, the raw answers, how long it
took, retries, and token counts. Every observation, action, decision, and step
is timed too. It's written to `.jev-journal/<run id>/journal.jsonl`.

The journal is off unless you ask for it, and it can never change or fail a
run. Read it with the `jev_journal` tool:

```sh
cargo run -p tinycomputer-examples --bin jev_journal                  # list runs
cargo run -p tinycomputer-examples --bin jev_journal -- latest        # where the time went
cargo run -p tinycomputer-examples --bin jev_journal -- <id> --transcript
cargo run -p tinycomputer-examples --bin jev_journal -- <id> --json   # for comparing runs
cargo run -p tinycomputer-examples --bin jev_journal -- <id> --calibration
```

There's also a small web app in `scripts/debug-ui/` for browsing journal
files.

## Finding the fault

When a run goes wrong, work out which of four places the problem is in
before changing anything:

| Fault | Means | Look for |
|---|---|---|
| **What it saw** | Jev was shown the wrong thing | the right button missing from the element list, or named oddly |
| **What it was asked** | the question was wrong | a question that doesn't match the step |
| **The plan** | the flow asked for the wrong thing | a step that can't be true on that screen, or a `choose` option the page doesn't show |
| **The engine** | the click reached the wrong element | the right element was chosen, but something else happened |

The last one is a bug in the vendored desktop or browser engine, and it's
fixed there, not in this repository.

## When a run is slow

A step is a sequence of waits: reading the screen, asking Jev (all the
framings of one question go out together, so a question waits for its
slowest copy), acting, and letting the screen settle. `jev_journal -- latest`
shows the split. Change one thing at a time, and measure again. The levers
are listed in [`technical/jev-harness.md`](technical/jev-harness.md#where-the-time-goes).

## Running things for real: the lab

The lab loads the module exactly as a real host would and drives real apps:

```sh
scripts/lab run mail-compose                  # a hand-written flow
scripts/lab run mail-compose --mode goal      # the older single-goal loop
scripts/lab run mail-compose --mode authored  # a language model writes the flow
scripts/lab eval all --modes flow,goal --trials 3
```

Anything that launches Chromium runs in a Linux container instead:

```sh
scripts/docker-lab -- crates/tinycomputer-examples/fixtures/run task_fixture
```

Recorded results of live runs, with costs and what went wrong, live in
[`technical/evals/`](technical/evals/).

## Keep it private

Traces and journals contain screen text, and screen text can contain personal
data. Both are kept out of git. Never paste one into an issue or a pull
request.

## Where to find out more

- [`technical/jev-journal.md`](technical/jev-journal.md): the journal and its
  events.
- [`technical/lab.md`](technical/lab.md) and
  [`technical/docker-lab.md`](technical/docker-lab.md): live runs.
- [`technical/flow-examples.md`](technical/flow-examples.md): common failure
  notes and what causes them.
- [The examples crate docs](crates/tinycomputer-examples/README.md).
