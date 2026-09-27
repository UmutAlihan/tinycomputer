# The Jev debug journal

The journal writes down every Jev exchange a run makes — the exact request,
the raw answers, the latency, the retries, the tokens — and how long every
observation, action, decision, and step took. It exists for two jobs: reading
a run afterwards to see why it did what it did, and measuring where a loop's
wall time goes so it can be made faster. It is off by default and never
changes what a run does.

## Turning it on

```sh
# any run in this process: the lab, an example, a host embedding the module
TINYDESKTOP_JEV_JOURNAL=1 scripts/lab run mail-compose --mode flow
```

| Value | Effect |
|---|---|
| unset, `0`, `false`, `off`, `no` | off |
| `1`, `true`, `on`, `yes` | writes under `.jev-journal/` in the working directory |
| anything else | the directory to write under |

The variable is read when the module configures Jev (`JevRuntime::configure`),
so set it before the host loads the module. In code, a host or test can call
`JevRuntime::with_journal(dir)` instead.

`.jev-journal/` is git-ignored. A journal holds what Jev was shown — screen
text, element names, the goal — which can be personal data. Secrets are
masked to `${name}` before a request is built, so their values are never in
it. Do not commit a journal or paste one into an issue.

## Layout

```text
.jev-journal/
├── 20260928T101530Z-flow-a1b2c3/journal.jsonl   # one RunFlow call
├── 20260928T101902Z-goal-0f9e8d/journal.jsonl   # one RunGoal, continuations included
├── 20260928T102210Z-intent-77aa01/journal.jsonl # one ResolveIntent
└── task-<task id>/journal.jsonl                  # every flow run of one task
```

A run id starts with its UTC start time, so directories sort by time. A
task's runs — the first flow, and each run after an approval or a supplied
value — share one `task-…` file, so a task reads as one story. A `RunGoal`
continuation appends to the journal of the run it continues.

## Events

Each line of `journal.jsonl` is one JSON object. Every event has `event` (its
kind), `seq` (0, 1, 2, … within the file), `at` (RFC 3339 UTC), and
`elapsed_ms` (since the file was opened in this process). `step` is the flow
step path (`"3"`, `"4.1"`) the event served; the launch before the first step
has `""`, and goal and intent runs carry their goal or intent text.

| `event` | When | Fields |
|---|---|---|
| `run` | a run begins | `kind` (`flow`, `goal`, `goal-continuation`, `intent`), `label`, `model`, `pid` |
| `exchange` | every Jev call, one per framing | `step`, `questions` (ids), `request_bytes`, `request` (the exact `EvaluationRequest`), `ok`, `latency_ms`, `attempts`; on success `request_id`, `model`, `input_tokens`, `output_tokens`, `answers`; on failure `error` |
| `decision` | a flow decision is merged | `step`, `questions`, `framings`, `answered`, `wall_ms` — what the step actually waited |
| `observe` | a flow reads the screen | `step`, `part` (`screen` or `subtree`), `wall_ms`, `ok`, `candidates`, `unexplored` |
| `action` | a flow acts | `step`, `action`, `target`, `ok`, `note`, `wall_ms`, `settle_ms` |
| `step` | a flow step ends | `step`, `kind`, `text`, `outcome`, `note`, `turns`, `jev_calls`, `actions`, `loops`, `confidence`, `wall_ms` |
| `end` | a flow run ends | `stop`, `wall_ms`, `actions`, `metrics`, `learned` |

A voted decision writes one `exchange` per framing and then one `decision`.
The framings run concurrently, so a decision's `wall_ms` is close to its
slowest framing's `latency_ms`, not their sum. A parent step (`if`,
`repeat_until`) ends after its children, and its `wall_ms` includes theirs.

## Reading a run

```sh
cargo run -p tinydesktop-examples --bin jev_journal                        # list runs
cargo run -p tinydesktop-examples --bin jev_journal -- latest              # summary
cargo run -p tinydesktop-examples --bin jev_journal -- a1b2c3 --transcript # every answer
cargo run -p tinydesktop-examples --bin jev_journal -- latest --json       # summary as JSON
```

A run is named by `latest`, any unique part of its id, or its directory. The
binary reads `TINYDESKTOP_JEV_JOURNAL` to find the journal the same way the
module does. The summary looks like:

```text
run      flow: Mail: move Thursday's sync to Friday
wall       41.2s
jev        29.8s  72%  46 decisions, 230 calls (0 failed); latency p50 540 ms, p90 910 ms, max 2210 ms; mean request 14022 B; tokens 812000 in, 3100 out
observe     6.1s  14%  61 reads
act         3.9s   9%  14 actions, plus 2.8s settling
other       1.4s   3%

path   outcome       wall      jev  observe      act calls  step
1      done          2.9s     0.0s     0.4s     2.4s     0  open Mail
2      done          9.8s     7.1s     1.2s     1.3s    45  do start a new email message
…

slowest Jev calls
  #212     2210 ms  step 3     31877 B  slot_recipient, slot_subject, slot_body
```

`other` is the wall time no event accounts for: validation, merging,
building requests, and the gaps between events.

For anything the summary does not cover, the file is plain JSON Lines:

```sh
# the ten slowest calls, with their step and size
jq -c 'select(.event=="exchange") | [.latency_ms, .step, .request_bytes, .questions]' \
  .jev-journal/<run>/journal.jsonl | sort -rn | head
# what Jev saw on step 3
jq 'select(.event=="exchange" and .step=="3") | .request.state' .jev-journal/<run>/journal.jsonl
```

## Using it to make loops faster

1. Journal a baseline: the same scenario, a few trials.
2. Read the summary's split. If `jev` dominates, look at `decisions` per
   step and at the slowest calls; if `observe` does, look at `candidates` and
   `part: subtree` reads; if `act` does, look at `settle_ms`.
3. Check whether latency tracks `request_bytes` (big element lists) or
   `framings` (voting tail). Retries (`attempts > 1`) show provider trouble,
   not a loop problem.
4. Change one lever from the table in [`jev-harness.md`](jev-harness.md) —
   `votes`, a `disabled_loops` entry, memory — and journal again.
5. Compare the `--json` summaries, and check the lab's checker verdicts did
   not get worse: a faster loop that clicks the wrong thing is not faster.

## Relation to the trace

`trace: true` on a `RunFlow` request returns one `JevExchange` per *decision*
in the result, with merged answers; the lab writes those to `jev.jsonl`. The
journal writes every *framing* as sent and received, with timings, for any
caller, without changing the request or the result. Use the trace to see
what a run decided; use the journal to see what went over the wire and what
it cost.
