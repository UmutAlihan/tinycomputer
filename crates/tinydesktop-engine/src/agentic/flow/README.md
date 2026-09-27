# `agentic::flow` — intent flows run by Jev decision loops

A flow ([`tinydesktop_bus::Flow`]) says what to accomplish in one application,
step by step, with no UI knowledge. This module grounds each step on the live
screen by composing small Jev questions in deterministic Rust. The design and
its rationale are in `docs/specs/jev-intent-flows.md`.

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
| `view/` | re-exports the screen model from `tinydesktop-core::surface`; keeps the flow's policy: the act threshold and which controls it must not press |
| `backend/` | `AgentBackend` (the core `Surface` trait, which `Desktop` implements in `tinydesktop-desktop/src/surface/`) and the async wrappers that call it off the executor |
| `test.rs` | a simulated mail app and an oracle Jev that answers from its state |

## Operational constraints

- Every Choice offers at most 20 options plus `none`; a pool larger than that
  is narrowed, never silently truncated.
- Budgets: `max_actions` (≤ 120) and `max_model_calls` (≤ 300) per run; a `do`
  step takes at most 8 turns.
- Irreversible controls are pressed only by `stop_before` with
  `allow_destructive`.
- The runtime holds no files and no state between runs; grounding hints travel
  in the request and the result.
