# Jev intent flows — implementation plan

Implements [`../specs/jev-intent-flows.md`](../specs/jev-intent-flows.md).
Every behavior change lands behind a failing test first; the fakes are
`agentic/test.rs` (`FakeBackend`, scripted evaluations) and
`agentic/flow/test.rs` (a simulated mail app and an oracle Jev).

## Phase A — the Jev step foundation (superseded)

This branch first reworked the `RunGoal` loop (ref-free stall detection,
context, read-back, retries). Upstream redesigned `RunGoal` in parallel as a
scoped task with visible success predicates, exact window binding, and
confirmation handles (tinydesktop #11–#17), which covers the same ground. The
merge keeps upstream's `RunGoal` and `ResolveIntent` unchanged; the flow
runtime keeps its own observation and delivery in `agentic/flow/view/` and
`agentic/flow/backend/`, with their tests beside them.

## Phase B — flows (`crates/tinydesktop-bus/src/flow/`, `agentic/flow/`)

- [x] Contract types, order-preserving step parsing, `guide.md`; members
      `RunFlow`, `ValidateFlow`, `FlowGuide`; contract version 1.6.
- [x] `validate.rs`, `ask.rs`, `ground.rs`, `act.rs`, `enter.rs`, `steps.rs`,
      `memory.rs`; dispatch wiring; manifest and sweep updates.
- [x] Negation-calibrated conditions and completion; coverage and progress
      Scores combined in; `field_contents` including rich text and tokens.
- [x] Creation intents cannot be complete before acting; Return refused under
      a sheet; blind looks; opt-in Jev trace.

## Phase C — the lab (`crates/tinydesktop-examples`, `scripts/lab`)

- [x] `lab::host` loads the attested module over a real bus.
- [x] `lab::scenario`: eight scenarios with briefs, flows, goals, and checkers.
- [x] `lab::record`: run directories, timelines, `jev.jsonl`, memory, scorecard.
- [x] `bin/lab.rs`: `list`, `guide`, `run`, `eval`, `report`, `validate`,
      `call`.
- [x] `scripts/lab` builds the module and the macOS clipboard helper into
      `target/lab/` with a `modules.toml`.

## Phase D — the optional author (lab only)

- [x] Vendor `vendor/tinyinference`; `lab::author` behind the `inference`
      feature: authoring, validation repair, round trips, screenshot yes/no.

## Packaging

- [x] Ship `agent-desktop-macos-helper` beside the module in macOS release
      archives; without it `ClipboardSet` and the paste fallback fail with
      `ACTION_NOT_SUPPORTED`.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
.github/scripts/check-file-coverage.sh 90 coverage.json
scripts/lab eval all --modes flow,goal --trials 3
```
