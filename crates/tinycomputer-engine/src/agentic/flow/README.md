# `agentic::flow` — intent flows run by Jev decision loops

A flow ([`tinycomputer_bus::Flow`]) says what to accomplish in one application,
step by step, with no UI knowledge. This module grounds each step on the live
screen by composing small Jev questions in deterministic Rust. The design and
its rationale are in `docs/specs/jev-intent-flows.md`, and
`docs/decision-loops.md` walks through every loop and question
(`docs/decision-thresholds.md` lists the thresholds), and
`docs/specs/jev-wide-turns.md` specifies the wide strategy.

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
| `wide.rs` | the wide strategy: one request per `do` turn over the screen digest (judgement, `dismiss`, every move's target), the wide state, resolving prepared targets |
| `survey.rs` | the wide strategy's attention pass: which regions of a crowded screen matter, which distract |
| `ledger.rs` | the working memory wide questions see: finished steps, recent actions across steps, tried and failed, next step |
| `view/` | re-exports the screen model and digest from `tinycomputer-core::surface`; keeps the flow's policy: the act threshold, which controls it must not press, and `named_first` |
| `backend/` | `AgentBackend` (the core `Surface` trait, which `Desktop` implements in `tinycomputer-desktop/src/surface/`) and the async wrappers that call it off the executor |
| `test.rs` | a simulated mail app and an oracle Jev that answers from its state |

## Operational constraints

- Every Choice offers at most 20 options plus `none`; a pool larger than that
  is narrowed, never silently truncated.
- Budgets: `max_actions` (≤ 120) and `max_model_calls` (≤ 5000, every voted framing
  counting as one) per run; a `do`
  step takes at most 8 turns.
- Irreversible controls are pressed only by `stop_before` with
  `allow_destructive`.
- The runtime holds no state between runs; grounding hints travel in the
  request and the result. The only files it writes are the opt-in debug
  journal's (`../journal/`), best effort, never read back.
