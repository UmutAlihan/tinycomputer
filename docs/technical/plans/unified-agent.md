# Unified agent — implementation plan

Implements [`../specs/unified-agent.md`](../specs/unified-agent.md). Each phase
is a PR against `tinyhumansai/tinycomputer`, test-first, and keeps the four
contract commands and the per-file coverage gate green.

## Phase 0 — docs and harness

- [x] Spec and plan.
- [x] Docker lab (`docker/lab/`, `scripts/docker-lab`, `docs/technical/docker-lab.md`)
      for anything that launches Chromium.

## Phase 1 — crate split, no behavior change

- [x] `tinycomputer-desktop`: move `src/desktop/` and `src/error/`; the
      `tinycomputer` crate re-exports `Desktop`, `Error` and `Result`.
- [x] `tinycomputer-engine`: move `src/agentic/`; expose `JevRuntime`,
      `run_goal`, `resolve_intent`, `run_flow`, `validate_flow` and
      `flow_guide`.
- [x] `tinycomputer-core::surface`: the flow's `Screen`, `Candidate`, `Depth`,
      fingerprints, change notes and verified text delivery, behind the
      `Surface` trait (the flow's former `AgentBackend`). `Desktop` implements
      it in `tinycomputer-desktop/src/surface/`; the engine's `view` and
      `backend` keep only flow policy and the async wrappers.
- [ ] Replace the flow's `cmd+…` strings with `core::Key` (PR #21's
      `platform_combo` covers Windows and Linux meanwhile).
- [ ] `RunGoal`'s private backend is ported onto `core::Surface`, and the
      duplicate `agentic/screen.rs` pair is collapsed.
- [ ] `tinycomputer` keeps only `tinybus_module/`.

## Phase 2 — link agent-browser

- [x] `tinycomputer-bus::browser` ported from `tinybrowser-bus`; the Agent
      interface contract in `tinycomputer-bus::agent`.
- [x] `tinycomputer-browser`: sessions, conversion, error mapping, and held
      outputs over an `Engine` seam, tested with a scripted engine.
- [x] Upstream: `lib.rs`, `StateOptions` and `DaemonState::with_options`
      (vercel-labs/agent-browser#2008); the `tinyhumansai/agent-browser` fork
      is pinned meanwhile.
- [x] `vendor/agent-browser` submodule; the linked `AgentBrowser` engine over
      `execute_command`; `BrowserSurface`; each task gets its own session.
- [x] Upstream: trim agent-browser's dependencies so `cargo deny` passes
      (`image` codecs, `rustls-pemfile`), folded into the same upstream PR.
      The fork's `library-target` branch is that PR's head and is pinned.
- [ ] The `Browser` interface in the cdylib, plus tabs, cookies, storage
      state, upload, dialog, find, and wait members.
- [ ] Browser tests in the Docker lab; a CI job on the Playwright image;
      `cargo deny`.

## Phase 3 — deterministic components

- [x] `tinycomputer-core`: platform keymap, action consequences and payment
      detection, record parsers and ranking, facts with card-data refusal.
- [ ] Workspace, record extraction from screens, form model, obstacle
      heuristics, settle.

## Phase 4 — new Jev loops

- [ ] Page kind, pick and rank, combobox, date picker, upsell decline, missing
      detail, validation error. Each has a simulator test and can be disabled.

## Phase 5 — task controller and Agent interface

- [x] `browse` step; `Workspace` routing a flow between the desktop and the
      browser; `run_flow` generic over surfaces.
- [x] `tinycomputer-engine::Tasks`: start, long-poll, continue (inputs and
      approvals), cancel, report, list; `needs_input` for missing facts,
      `needs_approval` for irreversible actions, a final checkpoint at payment.
- [x] Agent members served by the module (contract 1.7, 67 members), with
      `Describe` returning schemas, the guide, and working examples.
- [x] `pick {from, by, into}`: result cards grouped from ordinal-labelled
      containers (`core::surface::result_groups`), ranked exactly for prices,
      times, durations, and stops, judged by Jev otherwise, then opened.
- [x] `needs_human`: a recoverable failure in front of a captcha, one-time
      code, two-factor prompt, or login wall pauses for a person and retries
      the step; each task keeps its own workspace across runs and releases it
      when it ends (not at a payment checkpoint, which the person finishes).
- [x] A static travel fixture (`crates/tinycomputer-examples/fixtures/travel`)
      for end-to-end runs in the Docker lab.
- [x] `extract {what, into}`: every card of a list as JSON rows, surfaced as
      structured records in the task report.
- [x] `tinycomputer-skills`: an agent-facing `SKILL.md` and `StartTask`
      schema, tested against the contract.
- [ ] `in` step (switching surfaces already works through `open` and
      `browse`).

## Phase 6 — planner

- [x] `planner` feature and private `planner` config (OpenRouter); plan with
      validation repairs; plain-language `StartTask` and `PlanTask`.
- [ ] Replan after a failed step; summarize the answer from records.

## Phase 7 — lab

- [x] `browser_fixture` and `task_fixture` pass on real Chromium in the Docker
      lab; the booking task stops at the payment checkpoint
      (`docs/technical/evals/2026-09-27-unified-fixture.md`).

- [ ] Fixture travel site; scenarios `flight-fixture`, `flight-google`,
      `kashmir-booking` and `cross-surface`; results in `docs/technical/evals/`.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
.github/scripts/check-file-coverage.sh 90 coverage.json
scripts/docker-lab -- cargo test --all-features
```
