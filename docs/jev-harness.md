# The Jev harness

This page is the map. It shows every layer between a caller's request and a
Jev evaluation, what each layer adds to the request, where the time goes, and
how each layer is tested and observed. [`decision-loops.md`](decision-loops.md)
goes deep on the flow runtime's loops and thresholds; this page is what to
read first, and what to read when you are trying to make the loops faster.

## Jev in one paragraph

Jev is TypeSafe's decision model. The engine reaches it through the
`tinyinference_decisions` client in `vendor/tinyinference` (fix the client
there, not here). A request carries one shared `state` value and a map of
questions, each a **Noul** (yes/no, answered as a probability), a **Score**
(an ordered scale, answered as a distribution over levels), or a **Choice**
(labelled options, answered as a pick plus a distribution). Jev never writes
text and never plans. Every question in one request is answered from the
same state, independently, in one round trip.

## The stack

```text
caller (agent, host, lab)
  │  TinyBus member: RunFlow / RunGoal / ResolveIntent / StartTask …
  ▼
crates/tinycomputer/src/tinybus_module/     dispatch.rs, runner.rs
  │  holds the configured JevRuntime; tasks get a per-task Workspace
  ▼
crates/tinycomputer-engine/src/
  task/                  the task controller: runs flows in the background,
  │                      pauses for input and approval, splits the budget
  ▼
  agentic/flow/mod.rs    run_flow → FlowRun: the step driver, budgets,
  │                      look / explore / act, and ask()
  ├─ steps.rs            one function per step kind
  ├─ act.rs              the `do` loop: judge, recover, move
  ├─ ground.rs           one element for a purpose
  ├─ enter.rs            slots to fields, verified delivery
  └─ ask.rs, vote.rs     question builders; framings and merging
  ▼
  agentic/mod.rs         JevRuntime::evaluate — the one door every call
  │                      goes through; the debug journal hooks in here
  ▼
vendor/tinyinference     tinyinference_decisions::Client: HTTP, retries,
                         timeout, response validation
```

`RunGoal` and `ResolveIntent` (`agentic/mod.rs`, `agentic/task.rs`) sit beside
the flow runtime rather than under it; they share only `JevRuntime` and its
error mapping.

## The three loops

| Loop | Entry | Shape | Read more |
|---|---|---|---|
| Intent | `resolve_intent` | one observation, one decision (operation + target), an optional rerank, an optional action | `agentic/README.md` |
| Goal | `run_goal` | per turn: observe, check the caller's visible success predicates, decide one operation and target, re-observe the target, act once, observe again; consequential actions return a one-use confirmation handle | `agentic/README.md` |
| Flow | `run_flow` | per step: the step kind's own logic, built from many small decisions — the `do` loop, grounding, slot matching, condition checks | [`decision-loops.md`](decision-loops.md) |

The goal loop asks one large question per turn and never lets Jev judge its
own completion; only the caller's accessibility predicates end it. The flow
loop asks many small questions and lets calibrated Jev judgements end steps.
Flows are where the product is going; the goal loop is the baseline the lab
measures them against.

## The life of one flow decision

Every flow question reaches Jev through `FlowRun::ask`
(`agentic/flow/mod.rs`). In order:

1. **Build.** A step's code builds the questions (`ask.rs`) and the shared
   state (`ask::state`: app, window, surface, current step, visible text,
   up to 120 elements, the last eight history lines, and field contents when
   `include_values` is set). Screen text is always wrapped as
   `untrusted_accessibility_data`.
2. **Budget.** The run's call budget is checked first; a spent budget stops
   the run with `ModelBudget`.
3. **Page kind.** On the browser, a `page_kind` Choice rides along, and its
   answer briefs the *next* request ("a results page", "a form").
4. **Brief.** Questions that *choose* — which element, move, field, record,
   and the `confirm` Noul — get the run's brief: the goal, whom it is for, the
   plan with the current step marked, what has been chosen so far, the page
   kind. Questions that *judge* the screen are left unbriefed; measured, a
   brief pulled "is the search done?" from 0.75 to 0.39 because Jev judged the
   step against the whole task.
5. **Mask.** Every secret's value is replaced by `${name}` anywhere in the
   state or the questions. Nothing after this point, including the journal,
   sees a secret.
6. **Fit.** A request over 100 KB of JSON (`MAX_REQUEST_BYTES`) is shrunk:
   the brief is kept on one question only, then the longest element and text
   lists lose their tails. Jev rejects requests past its token limit outright.
7. **Frame.** `vote::framings` makes `votes` copies (default 5, at most 9):
   label-keyed Choices are shuffled and relabelled, and each copy gets a
   different one-line perspective. Framing 0 is the request as built.
8. **Evaluate.** All framings go to `JevRuntime::evaluate` at once, each on
   its own task. Each call is charged to the budget.
9. **Merge.** Answers are mapped back to the original keys and averaged per
   question (`vote::merge`). A Choice's `confidence` becomes the share of
   framings that agreed with the winner.
10. **Record.** The merged exchange goes into the result's `trace` when the
    request set `trace: true`; each raw framing, and the decision's wall
    time, go to the debug journal when it is on.

The step's code then thresholds the merged answers (see the table at the end
of [`decision-loops.md`](decision-loops.md)).

## Where the time goes

A step is a sequence of waits; nothing inside one step runs in parallel
except the framings of a single decision. One `do` turn is roughly:

```text
look (observe)  →  judge (1 decision)  →  [ground (1+ decisions)]  →  act  →  settle
```

So a turn's wall time is the observation, plus each decision's *slowest*
framing, plus the action, plus settling. The levers:

| Lever | Effect on latency | Effect on accuracy |
|---|---|---|
| `votes` | a decision waits for its slowest framing: more framings, longer tail | more framings average out position and phrasing bias |
| request size | Jev's latency grows with input tokens; a big element list is the usual cause | trimming can drop the element that was needed |
| grounding memory | a remembered element is confirmed with one Noul instead of narrowing | none when the hint is right |
| `disabled_loops` | each loop off removes a question or a whole decision | measure it before shipping it off |
| `settle` | fixed per action on the desktop, network-idle on the browser | too short and the next look sees the old screen |
| observation | an accessibility snapshot of a large window is slow; `explore` adds more | a budgeted view can miss the target |

Measure before changing any of them. The debug journal
([`jev-journal.md`](jev-journal.md)) times every observation, action,
decision, and call, and `jev_journal` summarises a run into exactly this
breakdown.

## Configuration

`JevRuntime::configure` builds the client from the module's private `jev`
configuration: the provider (`type_safe`, `open_router`, or
`tinyhumans_openrouter`), the key, an optional endpoint (only the approved
URL for that provider is accepted), `timeout_ms`, `max_retries`, and the
model, `jev-latest` by default. The runtime is cheap to clone: the client,
the pending-confirmation table, and the journal are shared behind `Arc`s.

## How it is tested

| Harness | Where | What it is for |
|---|---|---|
| Flow simulator | `agentic/flow/test.rs` | a scripted mail app and booking widgets, plus an *oracle* Jev that answers from the simulator's true state; every loop behaviour and regression lands here first |
| Mock evaluator | `agentic/test.rs` | queued, canned `EvaluationResult`s for the goal and intent loops; records every request for assertions |
| Fake runners | `task/test.rs` | a `FlowRunner` that returns scripted replies, so the task controller is tested without a surface or Jev |
| The lab | `crates/tinycomputer-examples`, `scripts/lab` | the built module, loaded like production, driving real applications with real Jev; see [`lab.md`](lab.md) |

The simulator and the mock both implement the private `Evaluator` trait that
`JevRuntime` holds, so every test goes through the same `evaluate` door as
production, journal included.

## How it is observed

| Level | Turned on by | Holds | Lives |
|---|---|---|---|
| Step reports | always | per step: outcome, note, turns, calls, actions, loops, lowest confidence | the `RunFlow` result |
| Trace | `trace: true` on the request | per decision: the state, the questions, the *merged* answers | the `RunFlow` result; the lab writes it to `jev.jsonl` |
| Debug journal | `TINYCOMPUTER_JEV_JOURNAL`, or `JevRuntime::with_journal` | per call: the exact request and raw answers, latency, attempts, tokens; per decision, observation, action, and step: wall time | `.jev-journal/<run id>/journal.jsonl`, git-ignored |

The trace answers "what did the run decide?"; the journal answers "what
exactly went over the wire, and how long did everything take?".
