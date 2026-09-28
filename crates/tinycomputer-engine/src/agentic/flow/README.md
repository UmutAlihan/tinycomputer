# `agentic::flow` — intent flows run by Jev decision loops

A flow ([`tinycomputer_bus::Flow`]) says what to accomplish in one application,
step by step, with no UI knowledge. This module grounds each step on the live
screen by composing small Jev questions in deterministic Rust. The design and
its rationale are in `docs/technical/specs/jev-intent-flows.md`, and
`docs/technical/decision-loops.md` walks through every loop and question
(`docs/technical/decision-thresholds.md` lists the thresholds), and
`docs/technical/specs/jev-wide-turns.md` specifies the wide strategy, and
`docs/technical/specs/jev-deliberation.md` how a decision is deliberated on its
evidence, checked after acting, and undone and retried when wrong.

## Layout

| File | Responsibility |
|---|---|
| `mod.rs` | `run_flow`, `validate_flow`, `flow_guide`; `FlowRun` state, budgets, the step driver, the Jev and action wrappers, `look` and `explore` |
| `validate.rs` | parsing and per-step validation; `${name}` substitution |
| `ask.rs` | question builders (completion, negation, progress, coverage, obstacle, element choices) and answer readers; the shared state, including `field_contents` |
| `ground.rs` | one element for a purpose: memory, region narrowing, knockout, relabelled re-ask, corroboration |
| `act.rs` | the `do` loop: judge, move, act, observe; obstacles, undo, stall, creation intents |
| `enter.rs` | slot matching and verified delivery, top to bottom |
| `steps.rs` | `open`, `choose`, `read`, `verify`, `wait_for`, `stop_before`, `if`, `repeat_until` |
| `memory.rs` | grounding hints: remember, recall, learn |
| `reflect.rs` | after a `choose` presses something: does the screen show its choice? repair once, else fail |
| `wide.rs` | the wide strategy: one request per `do` turn over the screen digest (judgement, `dismiss`, every move's target), the wide state, resolving prepared targets |
| `survey.rs` | the wide strategy's attention pass: which regions of a crowded screen matter, which distract |
| `ledger.rs` | the working memory wide questions see: finished steps, recent actions across steps, tried and failed, next step |
| `attention/` | the root of every turn: what needs attention first, the step or a distraction; clears one with its least-committal control |
| `evidence/` | a question's ballot read into accept, deliberate, or abstain |
| `escalate/` | the ladder a deliberated decision climbs: more framings, duel, contrast, views; `vouch` for irreversible presses |
| `duel/` | pairwise duels in both orders, counted Copeland-style |
| `denoise/` | inert and doubled elements left out, the pool ranked by what is in view, oscillations, compact history |
| `expect/` | a press's effect predicted, then checked against the screen |
| `checkpoint/` | checkpoints, reversibility, and the verified undo ladder |
| `view/` | re-exports the screen model and digest from `tinycomputer-core::surface`; keeps the flow's policy: the act threshold, which controls it must not press, and `named_first` |
| `backend/` | `AgentBackend` (the core `Surface` trait, which `Desktop` implements in `tinycomputer-desktop/src/surface/`) and the async wrappers that call it off the executor |
| `vote.rs` | framings, ballots, and their tally |
| `test.rs` | a simulated mail app and web shop, and an oracle Jev that answers from their state; `test/deliberation.rs` holds deliberation's scenarios |

## Operational constraints

- Every Choice offers at most 20 options plus `none`; a pool larger than that
  is narrowed, never silently truncated.
- Budgets: `max_actions` (≤ 120) and `max_model_calls` (≤ 10000, every voted framing
  counting as one) per run; a `do`
  step takes at most 8 turns.
- Irreversible controls are pressed only by `stop_before` with
  `allow_destructive`, and a deep run needs 0.85 belief first.
- Deliberation spends nothing when every gate accepts as it reads; a failed
  undo that claimed to restore fails the step closed.
- The runtime holds no state between runs; grounding hints travel in the
  request and the result. The only files it writes are the opt-in debug
  journal's (`../journal/`), best effort, never read back.
