# Reading a journal

The debug journal writes down every Jev exchange a run makes: the exact
request, the raw answer, the latency, the retries, the token counts, plus
how long every observation, action, decision, and step took. It is off by
default, changes nothing about how a run behaves, and exists for exactly
two jobs: working out why a run did what it did, and finding where a slow
loop's time actually goes.

Full detail, including the event schema, lives in
[`docs/technical/jev-journal.md`](../../technical/jev-journal.md). This page
covers the `jev_journal` binary that reads it back.

## Turning the journal on

```sh
TINYCOMPUTER_JEV_JOURNAL=1 scripts/lab run mail-compose --mode flow
```

Set the variable before whatever loads the module — the lab, `task_live`,
your own host embedding it — starts. It is read once, when Jev is
configured. Any other truthy value (`true`, `on`, `yes`) also just turns it
on and writes under `.jev-journal/` in the current directory; anything else
is treated as the directory name to write under instead. `.jev-journal/` is
git-ignored: it can contain screen text and other things a run read, which
may be personal, so never commit one or paste one into an issue.

## `jev_journal`

```sh
cargo run -p tinycomputer-examples --bin jev_journal                        # list runs
cargo run -p tinycomputer-examples --bin jev_journal -- latest              # summary
cargo run -p tinycomputer-examples --bin jev_journal -- a1b2c3 --transcript # every answer
cargo run -p tinycomputer-examples --bin jev_journal -- latest --json       # summary as JSON
cargo run -p tinycomputer-examples --bin jev_journal -- latest --calibration
```

A run can be named as `latest`, any unique fragment of its run id, or the
run's directory path outright. With no name at all, it lists every run it
finds, newest information first, with a wall-clock time and a call count
for each.

| Flag | What you get |
|---|---|
| (none) | a human-readable summary: wall time, how much of it was Jev vs. observing vs. acting, per-step timings, the slowest calls |
| `--transcript` | every exchange, in order: the state Jev saw and the answer it gave, for reading a run step by step |
| `--json` | the summary as JSON, for scripting or comparing two runs |
| `--calibration` | how deliberation's verdicts (accept, deliberate, abstain) lined up with how each step actually ended, with the rungs it climbed and any undo — this is how the constants in [`decision-thresholds.md`](../../technical/decision-thresholds.md) get tuned from real runs, not guessed |

A plain summary looks roughly like this (the exact numbers will differ):

```text
run      flow: Mail: move Thursday's sync to Friday
wall       41.2s
jev        29.8s  72%  46 decisions, 230 calls (0 failed); latency p50 540 ms, p90 910 ms
turns      12 do turns; decisions in sequence per turn: mean 2.40, most 5
observe     6.1s  14%  61 reads
act         3.9s   9%  14 actions, plus 2.8s settling
```

If Jev's share of the wall time dominates, look at how many decisions each
step made and at the slowest individual calls. If `observe` dominates, look
at how many candidates each read returned. If `act` dominates, look at the
settling time after each action. [`docs/technical/jev-harness.md`](../../technical/jev-harness.md)
lists the levers for each of those.

## When the summary is not enough

The journal file itself is plain JSON Lines, one event per line, so
anything the binary does not already show you is one `jq` command away:

```sh
# the ten slowest calls, with their step and size
jq -r 'select(.event=="exchange")
  | "\(.latency_ms)\t\(.step)\t\(.request_bytes)\t\(.questions | join(","))"' \
  .jev-journal/<run>/journal.jsonl | sort -rn | head

# exactly what Jev saw on step 3
jq 'select(.event=="exchange" and .step=="3") | .request.state' \
  .jev-journal/<run>/journal.jsonl
```

## Journal versus trace

These two things look similar and answer different questions. `trace: true`
on a `RunFlow` request returns one exchange per *decision* in the result
itself, with the answers already merged — this is what the lab writes to
`jev.jsonl` next to every run, and what
[`the lab`](the-lab.md#what-a-run-leaves-behind) covers. The journal instead
records every *framing* actually sent and received, with real timings, for
any caller at all, and changes nothing about the request or the result.
Reach for the trace to see what a run decided; reach for the journal to see
what actually went over the wire and what it cost.
