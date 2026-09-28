# Plan: wide turns

Implements [`../specs/jev-wide-turns.md`](../specs/jev-wide-turns.md).

## Tasks

1. **Baseline.** Journal the travel fixture; count decisions per turn from
   the code; record both in
   [`../evals/2026-09-28-jev-call-audit.md`](../evals/2026-09-28-jev-call-audit.md).
2. **Contract 2.1.** `FlowStrategy` and `RunFlowRequest.strategy`,
   `TaskBudget.strategy`, `FlowLoop::{Survey, Digest}` in
   `crates/tinycomputer-bus/src/flow/types.rs` and `agent/types.rs`; wire-form
   tests; version pins.
3. **Digest.** `crates/tinycomputer-core/src/surface/digest/`: regions,
   front, lists as cards (`groups::cards_at`), noise, budgeted rendering,
   ranking; `element_line` moves to `screen.rs`. Tests first in
   `digest/test.rs`.
4. **Ledger.** `agentic/flow/ledger.rs`; begun and finished by
   `FlowRun::run_step`; tried-and-failed notes from `act.rs`.
5. **Survey.** `agentic/flow/survey.rs`, cached per step and layout.
6. **Wide turn.** `agentic/flow/wide.rs`: `judge_wide`, `Prepared`,
   `resolve`, `dismiss`, the wide state; `act.rs` split into
   `judge_questions` and `Judgement::read`; `ground::state` chooses the
   state by strategy; `named_first` in `view/mod.rs`.
7. **Simulator tests** in `agentic/flow/test.rs`, each written against the
   oracle before the behaviour.
8. **Journal.** `turn` and `survey` events, `request_bytes` on `decision`;
   `jev_journal` shows window use and decisions per turn.
9. **Lab.** `--strategy`, and `TINYCOMPUTER_FLOW_STRATEGY` for the live
   examples.
10. **Live A/B** on the travel fixture and the Kashmir task; results in the
    eval.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
scripts/docker-lab -- env TINYCOMPUTER_JEV_JOURNAL=1 TINYCOMPUTER_FLOW_STRATEGY=wide \
  crates/tinycomputer-examples/fixtures/run task_fixture
scripts/docker-lab -- env TINYCOMPUTER_FLOW_STRATEGY=wide \
  crates/tinycomputer-examples/tasks/run kashmir
```

## Checklist

- [x] 1–9
- [x] travel fixture passes under both strategies
- [ ] the default flips to wide after more live scenarios agree
