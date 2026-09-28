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
| `mod.rs` | `run_flow`, `validate_flow`, `flow_guide`; `FlowRun` state and budgets |
| `run.rs` | starting a run, the step driver, and the run's result |
| `decide.rs` | `FlowRun::ask`, the one door to Jev: brief, mask, fit to size, vote |
| `brief.rs` | the run's brief added to every choosing question |
| `action.rs`, `look.rs` | one budgeted desktop action; one budgeted look and `explore` |
| `validate/` | parsing and per-step validation (`rules.rs`); `${name}` substitution (`substitution.rs`) |
| `ask/` | question builders (`questions.rs`: completion, negation, progress, coverage, obstacle, element choices), answer readers (`answers.rs`), and the shared state, including `field_contents` (`screen_state.rs`) |
| `ground/` | one element for a purpose: memory and region narrowing (`narrow.rs`), knockout, relabelled re-ask, corroboration (`decide.rs`) |
| `act/` | the `do` loop: judge (`judge.rs`), move and clear obstacles (`moves.rs`), the turn loop and stall (`turns.rs`), undo and backtracking (`recover.rs`) |
| `enter/` | slot matching (`assign.rs`) and verified delivery, top to bottom (`fill.rs`) |
| `steps/` | one file per step kind: `launch.rs` (`open`, `browse`), `choose.rs`, `reveal.rs`, `read.rs`, `list.rs` (`pick`, `extract`), `condition.rs` (`verify`, `wait_for`, `if`, `repeat_until`), `stop.rs` (`stop_before`); option matching in `matching.rs`, dates in `date.rs` |
| `memory.rs` | grounding hints: remember, recall, learn |
| `reflect.rs` | after a `choose` presses something: does the screen show its choice? repair once, else fail |
| `wide/` | the wide strategy: one request per `do` turn over the screen digest (judgement, `dismiss`, every move's target) in `judge.rs`; the wide state (`state.rs`); resolving prepared targets (`resolve.rs`) |
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
| `flow_tests.rs` | the harness every flow test runs through; `flow_tests/` holds a simulated mail app and web shop (`simulator.rs`, `screens.rs`), an oracle Jev that answers from their state (`oracle.rs`), and each topic's tests in `<topic>_tests.rs`, deliberation's scenarios in `deliberation_tests.rs` |

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
