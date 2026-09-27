# Unified agent — implementation plan

Implements [`../specs/unified-agent.md`](../specs/unified-agent.md). Each phase
is a PR against `tinyhumansai/tinydesktop`, test-first, and keeps the four
contract commands and the per-file coverage gate green.

## Phase 0 — docs and harness

- [x] Spec and plan.
- [x] Docker lab (`docker/lab/`, `scripts/docker-lab`, `docs/docker-lab.md`)
      for anything that launches Chromium.

## Phase 1 — crate split, no behavior change

- [x] `tinydesktop-desktop`: move `src/desktop/` and `src/error/`; the
      `tinydesktop` crate re-exports `Desktop`, `Error` and `Result`.
- [x] `tinydesktop-engine`: move `src/agentic/`; expose `JevRuntime`,
      `run_goal`, `resolve_intent`, `run_flow`, `validate_flow` and
      `flow_guide`.
- [x] `tinydesktop-core::surface`: the flow's `Screen`, `Candidate`, `Depth`,
      fingerprints, change notes and verified text delivery, behind the
      `Surface` trait (the flow's former `AgentBackend`). `Desktop` implements
      it in `tinydesktop-desktop/src/surface/`; the engine's `view` and
      `backend` keep only flow policy and the async wrappers.
- [ ] Replace the flow's `cmd+…` strings with `core::Key` (PR #21's
      `platform_combo` covers Windows and Linux meanwhile).
- [ ] `RunGoal`'s private backend is ported onto `core::Surface`, and the
      duplicate `agentic/screen.rs` pair is collapsed.
- [ ] `tinydesktop` keeps only `tinybus_module/`.

## Phase 2 — link agent-browser

- [x] `tinydesktop-bus::browser` ported from `tinybrowser-bus`; the Agent
      interface contract in `tinydesktop-bus::agent`.
- [x] `tinydesktop-browser`: sessions, conversion, error mapping, and held
      outputs over an `Engine` seam, tested with a scripted engine.
- [ ] Upstream: `lib.rs`, `StateOptions` and `DaemonState::with_options` in
      `vercel-labs/agent-browser`. The `tinyhumansai/agent-browser` fork is
      pinned meanwhile.
- [ ] `vendor/agent-browser` submodule; `tinydesktop-browser` (session,
      convert, reply, policy, outputs, `BrowserSurface`).
- [ ] `tinydesktop-bus/src/browser/` from `tinybrowser-bus`, plus the new
      members; the `Browser` interface.
- [ ] Browser tests in the Docker lab; a CI job on the Playwright image;
      `cargo deny`.

## Phase 3 — deterministic components

- [x] `tinydesktop-core`: platform keymap, action consequences and payment
      detection, record parsers and ranking, facts with card-data refusal.
- [ ] Workspace, record extraction from screens, form model, obstacle
      heuristics, settle.

## Phase 4 — new Jev loops

- [ ] Page kind, pick and rank, combobox, date picker, upsell decline, missing
      detail, validation error. Each has a simulator test and can be disabled.

## Phase 5 — task controller and Agent interface

- [ ] Task store, long-poll, continuation, cancel; grammar additions;
      `Describe` with schemas; `tinydesktop-skills`.

## Phase 6 — planner

- [ ] `planner` feature and confidential config; plan, repair, replan and
      summarize against `MockModel`.

## Phase 7 — lab

- [ ] Fixture travel site; scenarios `flight-fixture`, `flight-google`,
      `kashmir-booking` and `cross-surface`; results in `docs/evals/`.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
.github/scripts/check-file-coverage.sh 90 coverage.json
scripts/docker-lab -- cargo test --all-features
```
