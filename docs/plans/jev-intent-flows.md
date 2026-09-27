# Jev intent flows — implementation plan

Implements [`../specs/jev-intent-flows.md`](../specs/jev-intent-flows.md).
Every behavior change lands behind a failing test first; the fakes are
`agentic/test.rs` (`FakeBackend`, scripted evaluations) and
`agentic/flow/test.rs` (a simulated mail app and an oracle Jev).

## Phase A — the Jev step foundation (`crates/tinydesktop/src/agentic/`)

- [x] `screen.rs`: ref-free `fingerprint`; static text as `context`;
      `truncated` marker; `Depth::Skeleton`. Tests:
      `a_fingerprint_ignores_ref_churn_between_snapshots`,
      `static_text_is_kept_as_context_and_truncation_is_reported`.
- [x] Split `mod.rs` into `backend.rs`, `resolve.rs`, and `goal.rs`.
- [x] `backend.rs`: `deliver_text` (set-value, settled read-back, paste),
      token fields, rich-text paste at the caret. Tests:
      `a_silently_ignored_set_value_falls_back_to_paste`,
      `a_field_that_commits_late_is_verified_on_the_settled_re_read`,
      `a_token_field_is_delivered_unverified_rather_than_pasted_over`.
- [x] `goal.rs`: reuse the post-action snapshot; `max_retries`; ban an element
      after two failures; change notes in history. Tests:
      `a_failed_action_is_retried_and_the_element_banned_after_two_strikes`,
      `a_low_confidence_turn_is_retried_before_the_run_gives_up`.
- [x] `policy.rs`: destructive gating on the target only; `SCROLL_UP`; remove
      the Spotify heuristics. Test:
      `a_goal_mentioning_send_does_not_make_every_click_destructive`.
- [x] Contract: `RunGoalRequest.{max_retries, skeleton}`, `JevTurn.note`,
      `JevOperation::ScrollUp`, with wire pins.

## Phase B — flows (`crates/tinydesktop-bus/src/flow/`, `agentic/flow/`)

- [x] Contract types, order-preserving step parsing, `guide.md`; members
      `RunFlow`, `ValidateFlow`, `FlowGuide`; contract version 1.2.
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
